#!/usr/bin/env python3
"""Inventory the pinned native SDK, independently of the Rust binding subset.

Uses native-api.md for scope, GN for test-only ownership, and the pinned Clang
ExtractAPI frontend for declarations. A declaration is not functional coverage.
Compilation failures remain explicit unresolved headers, never exclusions.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import unquote, urlparse

from native_provenance import validate_configuration

ROOT = Path(__file__).resolve().parents[1]
TARGET = "linux-x86_64"
PROFILES = ("core", "native")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sdk_scope(sdk: Path) -> tuple[list[dict], list[str]]:
    """Read both tables, preserving the recursive/non-recursive distinction."""
    scopes = []
    headers = set()
    for directory, recursive in re.findall(
        r"^`([^`]+)`\s*\|\s*(Yes|No)\s*$", (sdk / "native-api.md").read_text(), re.M
    ):
        folder = sdk / directory
        if not folder.is_dir():
            raise ValueError(f"missing public SDK directory: {directory}")
        scopes.append({"directory": directory, "recursive": recursive == "Yes"})
        for path in folder.rglob("*") if recursive == "Yes" else folder.iterdir():
            if path.is_file() and path.suffix in (".h", ".hh", ".hpp", ".hxx"):
                headers.add(path.relative_to(sdk).as_posix())
    if not scopes or scopes[0] != {"directory": "api", "recursive": True}:
        raise ValueError("unrecognized native-api.md scope tables")
    return scopes, sorted(headers)


def gn_metadata(sdk: Path, build: Path) -> tuple[dict[str, dict[str, bool]], dict[str, list[str]]]:
    """No path/name heuristics: public headers can live in api/test."""
    gn = sdk / "buildtools/linux64/gn"
    properties = {}
    for name in ("sources", "public", "testonly", "include_dirs"):
        properties[name] = json.loads(subprocess.check_output(
            [str(gn), "desc", str(build), "*", name, "--format=json"], cwd=sdk, text=True
        ))
    owners: dict[str, dict[str, bool]] = {}
    for label, target in properties["sources"].items():
        testonly = properties["testonly"][label]["testonly"]
        public = properties["public"].get(label, {}).get("public", [])
        files = target.get("sources", []) + (public if isinstance(public, list) else [])
        for source in files:
            if source.startswith("//"):
                owners.setdefault(source[2:], {})[label] = testonly
    includes = {label: [str(sdk / directory[2:]) if directory.startswith("//") else directory
                        for directory in value.get("include_dirs", [])]
                for label, value in properties["include_dirs"].items()}
    return owners, includes


def source_reference(reference: dict, root: Path, sdk: Path) -> dict:
    tree = {"repository": root, "sdk": sdk}[reference["tree"]]
    path = Path(reference["path"])
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"non-relative source reference: {path}")
    text = (tree / path).read_text()
    anchor = reference["anchor"]
    if not anchor or text.count(anchor) != 1:
        raise ValueError(f"source anchor must match exactly once: {path}: {anchor!r}")
    return {**reference, "line": text[:text.index(anchor)].count("\n") + 1}


def conditions(path: Path) -> list[dict]:
    """Retain guards for review, including branches not active in either artifact.

    ExtractAPI describes active declarations, not every possible preprocessor
    configuration. Keeping these locations prevents an empty/disabled branch
    from being confused with evidence of upstream unavailability.
    """
    return [{"line": index, "source": line.strip()}
            for index, line in enumerate(path.read_text().splitlines(), 1)
            if re.match(r"\s*#\s*(if|ifdef|ifndef|elif|else|endif)\b", line)]


def implementation_name(name: str) -> bool:
    # Tokenize CamelCaseImpl as well as snake_case_impl, without mistaking
    # SimpleStringBuilder for an implementation-only name.
    tokens = re.findall(r"[A-Z]+(?=[A-Z][a-z]|[^A-Za-z]|$)|[A-Z]?[a-z]+|[0-9]+", name)
    return any(token.lower() in ("impl", "internal") for token in tokens)


def declarations(graph: dict, sdk: Path, header: str) -> tuple[list[dict], dict[str, int]]:
    symbols = {item["identifier"]["precise"]: item for item in graph["symbols"]}
    parents = {item["source"]: item["target"] for item in graph.get("relationships", [])
               if item["kind"] == "memberOf"}

    def exclusion(identifier: str) -> str | None:
        item = symbols.get(identifier)
        if not item:
            return None
        if item.get("accessLevel", "public") != "public":
            return "private_or_protected"
        if any(implementation_name(name) for name in item["pathComponents"]):
            return "sdk_internal_or_impl"
        parent = parents.get(identifier)
        return exclusion(parent) if parent else None

    result = []
    excluded: dict[str, int] = {}
    for identifier, item in symbols.items():
        location = item.get("location")
        if not location:
            continue
        path = Path(unquote(urlparse(location["uri"]).path)).resolve()
        if path != (sdk / header).resolve():
            continue
        reason = exclusion(identifier)
        if reason:
            excluded[reason] = excluded.get(reason, 0) + 1
            continue
        if item["kind"]["identifier"] == "c++.namespace":
            continue
        fragments = item.get("declarationFragments", [])
        result.append({
            "id": header + "#" + identifier,
            "usr": identifier,
            "name": "::".join(item["pathComponents"]),
            "kind": item["kind"]["identifier"],
            "declaration": "".join(fragment["spelling"] for fragment in fragments),
            "header": header,
            "line": location["position"]["line"] + 1,
            "column": location["position"]["character"] + 1,
            "references": sorted({fragment["preciseIdentifier"] for fragment in fragments
                                  if "preciseIdentifier" in fragment}),
        })
    return sorted(result, key=lambda item: item["id"]), excluded


def compiler_arguments(sdk: Path, artifact: Path, build: Path, manifest: dict) -> list[str]:
    compiler = sdk / "third_party/llvm-build/Release+Asserts/bin/clang++"
    version = subprocess.check_output([str(compiler), "--version"], text=True).splitlines()[0]
    if version != manifest["abi"]["toolchain"]:
        raise ValueError("SDK compiler does not match the qualified artifact toolchain")
    return [
        str(compiler),
        "-extract-api", "-x", "c++-header", "-std=c++20", "-fno-exceptions", "-fno-rtti",
        "-nostdinc++", "--target=x86_64-unknown-linux-gnu", "-msse3", "-w", "-ferror-limit=5",
        "--sysroot=" + str(sdk / "build/linux/debian_bullseye_amd64-sysroot"),
        "-isystem", str(artifact / "include/c++/v1"),
        "-I", str(sdk), "-I", str(build / "gen"),
        "-I", str(sdk / "third_party/abseil-cpp"),
        "-I", str(sdk / "third_party/boringssl/src/include"),
        "-I", str(sdk / "third_party/libyuv/include"),
        "-I", str(artifact / "include"),
        *manifest["native_configuration"]["cxx_defines"],
    ]


def extract_header(arguments: list[str], sdk: Path, header: str) -> dict:
    with tempfile.TemporaryDirectory(prefix="webrtc-sdk-header-") as temporary:
        output = Path(temporary) / "symbols.json"
        process = subprocess.run([*arguments, str(sdk / header), "-o", str(output)],
                                 text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if process.returncode:
            return {"status": "unresolved", "diagnostic": process.stderr}
        symbols, excluded = declarations(json.loads(output.read_text()), sdk, header)
        return {"status": "parsed", "declarations": symbols, "excluded_symbols": excluded}


def inventory(work: Path, metadata_build: Path, mapping: dict, jobs: int) -> dict:
    sdk = (work / "checkout/src").resolve()
    expected_pin = re.search(r'^webrtc_commit := "([0-9a-f]{40})"$',
                             (ROOT / "Justfile").read_text(), re.M)
    actual_pin = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=sdk, text=True).strip()
    if not expected_pin or actual_pin != expected_pin[1]:
        raise ValueError("SDK checkout does not match the repository pin")
    if mapping["sdk_revision"] != actual_pin:
        raise ValueError("mapping does not match the SDK pin")
    scopes, headers = sdk_scope(sdk)
    if not re.search(r"^rtc_include_tests\s*=\s*true\s*$",
                     (metadata_build / "args.gn").read_text(), re.M):
        raise ValueError("metadata build must explicitly enable rtc_include_tests")
    metadata_owners, metadata_includes = gn_metadata(sdk, metadata_build)
    header_availability = mapping.get("header_availability", {})
    if set(header_availability) - set(headers):
        raise ValueError("availability record references a header outside the scope")
    result = {"schema": 1, "sdk_revision": actual_pin,
              "scope_sha256": digest(sdk / "native-api.md"), "scope": scopes,
              "configuration_macros": re.findall(r"^## `(WEBRTC[^`]+)`", (sdk / "native-api.md").read_text(), re.M),
              "metadata_gn_args_sha256": digest(metadata_build / "args.gn"),
              "headers": {header: {"sha256": digest(sdk / header),
                                   "conditions": conditions(sdk / header), "profiles": {}}
                          for header in headers}, "profiles": {}}
    for flavor in PROFILES:
        artifact = (work / f"package/webrtc-{flavor}-{TARGET}").resolve()
        build = (work / f"out/{flavor}/{TARGET}").resolve()
        manifest = json.loads((artifact / "manifest.json").read_text())
        validate_configuration(manifest, ROOT)
        if manifest["artifact"] != {"flavor": flavor, "target": TARGET,
                                    "cargo_target": "x86_64-unknown-linux-gnu"}:
            raise ValueError(f"wrong {flavor} artifact")
        if manifest["sources"]["webrtc"]["revision"] != actual_pin:
            raise ValueError(f"{flavor} artifact SDK revision mismatch")
        owners, includes = gn_metadata(sdk, build)
        for header, targets in metadata_owners.items():
            for label, testonly in targets.items():
                existing = owners.setdefault(header, {}).get(label, True)
                owners[header][label] = existing and testonly
        arguments = compiler_arguments(sdk, artifact, build, manifest)
        selected = []
        for header in headers:
            target_owners = owners.get(header, {})
            record = {"gn_owners": target_owners}
            if header in header_availability:
                availability = header_availability[header]
                if availability["availability"] != "non_linux" or not availability["reason"]:
                    raise ValueError(f"unsupported header disposition: {header}")
                record.update(status="non_linux", reason=availability["reason"], evidence=[
                    source_reference(reference, ROOT, sdk) for reference in availability["evidence"]
                ])
                if not record["evidence"]:
                    raise ValueError(f"non-Linux disposition without evidence: {header}")
            elif target_owners and all(target_owners.values()):
                record["status"] = "gn_testonly"
            else:
                selected.append(header)
            result["headers"][header]["profiles"][flavor] = record
        def extract(header: str) -> dict:
            directories = list(dict.fromkeys(
                directory for label in sorted(owners.get(header, {}))
                for directory in includes.get(label, metadata_includes.get(label, []))
            ))
            return extract_header([*arguments, *[arg for directory in directories
                                                for arg in ("-I", directory)]], sdk, header)

        with ThreadPoolExecutor(max_workers=jobs) as pool:
            for header, extraction in zip(selected, pool.map(extract, selected)):
                result["headers"][header]["profiles"][flavor].update(extraction)
        result["profiles"][flavor] = {
            "native_configuration_sha256": manifest["native_configuration_sha256"],
            "gn_args_sha256": digest(build / "args.gn"),
            "compiler": manifest["abi"]["toolchain"],
            "cxx_defines": manifest["native_configuration"]["cxx_defines"],
        }
    return result


def check_mapping(inventory: dict, mapping: dict, root: Path, sdk: Path) -> dict:
    if mapping["schema"] != 1 or inventory["schema"] != 1:
        raise ValueError("unsupported coverage schema")
    if mapping["sdk_revision"] != inventory["sdk_revision"]:
        raise ValueError("mapping SDK revision mismatch")
    scopes, headers = sdk_scope(sdk)
    if inventory["scope_sha256"] != digest(sdk / "native-api.md") or inventory["scope"] != scopes:
        raise ValueError("native-api.md scope changed; regenerate the inventory")
    if set(headers) != set(inventory["headers"]):
        raise ValueError("public SDK header set changed; regenerate the inventory")
    if set(inventory["profiles"]) != set(PROFILES):
        raise ValueError("inventory must retain both core/native profiles")
    declared = {}
    declaration_details = {}
    native_types = set()
    native_references = set()
    unresolved = []
    for header, value in inventory["headers"].items():
        if digest(sdk / header) != value["sha256"]:
            raise ValueError(f"SDK header changed: {header}")
        if set(value["profiles"]) != set(PROFILES):
            raise ValueError(f"header must retain both core/native profile records: {header}")
        for profile, metadata in value["profiles"].items():
            status = metadata["status"]
            if status not in ("parsed", "unresolved", "gn_testonly", "non_linux"):
                raise ValueError(f"unknown header disposition: {profile}: {header}: {status}")
            if status == "parsed" and not isinstance(metadata.get("declarations"), list):
                raise ValueError(f"parsed header without declaration list: {profile}: {header}")
            if status != "parsed" and metadata.get("declarations"):
                raise ValueError(f"unparsed header claims declarations: {profile}: {header}")
            if status == "gn_testonly":
                owners = metadata.get("gn_owners", {})
                if not owners or not all(testonly is True for testonly in owners.values()):
                    raise ValueError(f"test-only header without exclusive GN test ownership: {header}")
            if status == "non_linux" and header not in mapping.get("header_availability", {}):
                raise ValueError(f"non-Linux header without availability evidence: {header}")
            if status == "unresolved":
                unresolved.append(f"{profile}: {header}")
            for declaration in metadata.get("declarations", []):
                declared.setdefault(declaration["id"], set()).add(profile)
                declaration_details.setdefault(declaration["id"], []).append(declaration)
                native_types.add(declaration.get("usr"))
                native_references.update(reference for reference in declaration.get("references", [])
                                         if reference.startswith(("c:@N@webrtc@", "c:@N@rtc@")))
    for header, entry in mapping.get("header_availability", {}).items():
        if header not in inventory["headers"] or entry["availability"] != "non_linux" or not entry["reason"]:
            raise ValueError(f"invalid header availability: {header}")
        if not entry["evidence"]:
            raise ValueError(f"missing availability evidence: {header}")
        for reference in entry["evidence"]:
            source_reference(reference, root, sdk)
        if any(profile["status"] != "non_linux"
               for profile in inventory["headers"][header]["profiles"].values()):
            raise ValueError(f"header availability differs from inventory: {header}")
    mapped = set()
    partial = []
    for entry in mapping["mappings"]:
        identifier = entry["declaration"]
        if identifier not in declared or identifier in mapped:
            raise ValueError(f"unknown or duplicate declaration mapping: {identifier}")
        if set(entry["profiles"]) != declared[identifier]:
            raise ValueError(f"wrong declaration profiles: {identifier}")
        if entry["state"] not in ("implemented", "partial") or not entry["semantics"]:
            raise ValueError(f"invalid mapping state/semantics: {identifier}")
        if entry["state"] == "partial":
            if not entry.get("gaps"):
                raise ValueError(f"partial mapping without gaps: {identifier}")
            partial.append(identifier)
        elif entry.get("gaps"):
            raise ValueError(f"implemented mapping has gaps: {identifier}")
        for field in ("rust", "installation", "verification"):
            if not entry[field]:
                raise ValueError(f"missing {field} path: {identifier}")
            for reference in entry[field]:
                source_reference(reference, root, sdk)
        mapped.add(identifier)
    unavailable = {}
    for entry in mapping.get("declaration_availability", []):
        identifier = entry["declaration"]
        if identifier not in declared or identifier in mapped or identifier in unavailable:
            raise ValueError(f"unknown, duplicate or mapped unavailable declaration: {identifier}")
        if set(entry["profiles"]) != declared[identifier]:
            raise ValueError(f"wrong unavailable declaration profiles: {identifier}")
        # ExtractAPI drops '= delete'. Recognize only an explicitly deleted
        # default constructor at the frontend's exact source coordinates. Do
        # not generalize missing implementations or disabled builds into this.
        if entry["availability"] != "deleted_default_constructor" or not entry["reason"]:
            raise ValueError(f"unsupported declaration availability: {identifier}")
        reference = entry["evidence"]
        evidence = source_reference(reference, root, sdk)
        for declaration in declaration_details[identifier]:
            names = declaration["name"].split("::")
            header = declaration["header"]
            line = declaration["line"]
            column = declaration["column"]
            source = (sdk / header).read_text().splitlines()[line - 1][column - 1:].strip()
            if (reference["tree"] != "sdk" or reference["path"] != header
                    or evidence["line"] != line or reference["anchor"] != source
                    or declaration["kind"] != "c++.method" or len(names) < 2
                    or names[-1] != names[-2]
                    or not re.fullmatch(re.escape(names[-1]) + r"\s*\(\s*\)\s*=\s*delete\s*;", source)):
                raise ValueError(f"availability is not the exact deleted default constructor: {identifier}")
        if not entry["verification"]:
            raise ValueError(f"missing unavailable declaration verification: {identifier}")
        for reference in entry["verification"]:
            source_reference(reference, root, sdk)
        unavailable[identifier] = {**entry, "evidence": evidence}
    configuration = set(inventory["configuration_macros"])
    mapped_configuration = set()
    for entry in mapping.get("configuration_mappings", []):
        macro = entry["macro"]
        if macro not in configuration or macro in mapped_configuration:
            raise ValueError(f"unknown or duplicate configuration macro: {macro}")
        if not entry["semantics"] or not entry["rust"] or not entry["installation"] or not entry["verification"]:
            raise ValueError(f"incomplete configuration mapping: {macro}")
        for field in ("rust", "installation", "verification"):
            for reference in entry[field]:
                source_reference(reference, root, sdk)
        mapped_configuration.add(macro)
    for entry in mapping.get("retained_capabilities", []):
        if not entry["name"] or not entry["semantics"]:
            raise ValueError("retained capability without name/semantics")
        for field in ("rust", "installation", "verification"):
            if not entry[field]:
                raise ValueError(f"retained capability missing {field}: {entry['name']}")
            for reference in entry[field]:
                source_reference(reference, root, sdk)
    return {"declarations": len(declared), "mapped": len(mapped),
            "partial": sorted(partial),
            "unavailable_declarations": [unavailable[key] for key in sorted(unavailable)],
            "unmapped": sorted(set(declared) - mapped - set(unavailable)),
            "unresolved_headers": unresolved,
            "external_native_references": sorted(native_references - native_types),
            "unmapped_configuration_macros": sorted(configuration - mapped_configuration)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("inventory", "check"))
    parser.add_argument("--work", type=Path, default=Path(os.environ.get("WEBRTC_WORK", ROOT / ".work")))
    parser.add_argument("--mapping", type=Path, default=ROOT / "docs/native-sdk-mapping.json")
    parser.add_argument("--metadata-build", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--inventory", type=Path)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--require-mapped", action="store_true",
                        help="reject unmapped/partial declarations; not a functional qualification gate")
    args = parser.parse_args()
    mapping = json.loads(args.mapping.read_text())
    work = args.work.resolve()
    if args.command == "check":
        if not args.inventory:
            parser.error("check requires --inventory")
        snapshot = json.loads(args.inventory.read_text())
        sdk = work / "checkout/src"
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=sdk, text=True).strip()
        if revision != snapshot["sdk_revision"]:
            raise ValueError("inventory does not match the SDK checkout revision")
        report = check_mapping(snapshot, mapping, ROOT, sdk)
        if args.output:
            args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        print(f"{report['mapped']} of {report['declarations']} active declarations have explicit mappings; "
              f"{len(report['partial'])} partial; "
              f"{len(report['unavailable_declarations'])} source-evidenced unavailable declarations; "
              f"{len(report['unresolved_headers'])} unresolved headers; "
              f"{len(report['unmapped_configuration_macros'])} configuration macros and "
              f"{len(report['external_native_references'])} external/specialized native type references require review")
        return int(bool(report["unresolved_headers"] or (args.require_mapped and (
            report["unmapped"] or report["partial"] or report["unmapped_configuration_macros"]))))
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    if not args.output or not args.metadata_build:
        parser.error("inventory requires --output and --metadata-build")
    result = inventory(work, args.metadata_build.resolve(), mapping, args.jobs)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    unresolved = []
    for flavor in result["profiles"]:
        statuses = {}
        symbols = 0
        for header, metadata in result["headers"].items():
            profile = metadata["profiles"][flavor]
            status = profile["status"]
            statuses[status] = statuses.get(status, 0) + 1
            symbols += len(profile.get("declarations", []))
            if status == "unresolved":
                unresolved.append(f"{flavor}: {header}")
        print(f"{flavor}: {len(result['headers'])} candidate headers; {statuses}; {symbols} declarations")
    for header in unresolved:
        print(f"unresolved: {header}")
    return int(bool(unresolved))


if __name__ == "__main__":
    raise SystemExit(main())
