#!/usr/bin/env python3
"""Audit the complete release matrix and write its attested manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import tarfile

from tools.write_artifact_lock import LockError, read_checksums, template
from tools.release_inventory import OS_TARGETS
from tools.native_provenance import LIBVPX_SOURCE, adapter_digest, configuration_digest, validate_configuration


ROOT = Path(__file__).resolve().parents[1]
SCOPES = {**OS_TARGETS, "complete": None}
CORE_IOS_PATCH_SHA256 = "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037"
ENCODER_SWITCH_PATCH_SHA256 = "8206418589692b8f5724ce9bad820fcd2d5a5b1d531e6bff8f64b53ca601d55a"
RUNTIME_RESOURCES_PATCH_SHA256 = "0746c63e599fd0bedde9f41c3be254f20c9ca028f37250278acdeb392e685e02"


class AuditError(Exception):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def manifest_digest(value: object) -> str:
    return configuration_digest(value)


def read_member(archive: tarfile.TarFile, name: str) -> bytes:
    try:
        member = archive.getmember(name)
    except KeyError as error:
        raise AuditError(f"missing {name}") from error
    if not member.isfile():
        raise AuditError(f"release member is not a file: {name}")
    source = archive.extractfile(member)
    if source is None:
        raise AuditError(f"cannot read release member: {name}")
    return source.read()


def audit(archives: Path, checksums_path: Path, lock_path: Path, scope: str = "complete") -> dict:
    try:
        targets = SCOPES[scope]
    except KeyError as error:
        raise AuditError(f"unknown release scope: {scope}") from error
    try:
        lock = template(lock_path)
    except LockError as error:
        raise AuditError(str(error)) from error
    expected = {
        entry["asset_name"]: entry
        for entry in lock["artifacts"]
        if targets is None or entry["artifact_target"] in targets
    }
    selected = [entry for entry in lock["artifacts"] if targets is None or entry["artifact_target"] in targets]
    if len(expected) != len(selected) or len({(entry["cargo_target"], entry["flavor"]) for entry in selected}) != len(selected):
        raise AuditError("duplicate artifact lock selection or asset name")
    checksums = read_checksums(checksums_path)
    found = {path.name: path for path in archives.glob("webrtc-*.tar.gz")}
    if found.keys() != expected.keys() or checksums.keys() != expected.keys():
        raise AuditError("release archive, checksum, and lock asset sets differ")

    expected_adapter = adapter_digest(ROOT)
    common_bridge = None
    common_sources = None
    configurations: set[str] = set()
    assets = []
    for name in sorted(expected):
        path = found[name]
        archive_digest = digest(path.read_bytes())
        if checksums[name] != archive_digest:
            raise AuditError(f"checksum mismatch: {name}")
        with tarfile.open(path, "r:gz") as archive:
            manifest = json.loads(read_member(archive, "manifest.json"))
            if manifest.get("schema_version") != 4:
                raise AuditError(f"artifact manifest is not schema 4: {name}")
            entry = expected[name]
            identity = manifest.get("artifact", {})
            for field in ("cargo_target", "artifact_target", "flavor"):
                manifest_field = "target" if field == "artifact_target" else field
                if identity.get(manifest_field) != entry[field]:
                    raise AuditError(f"wrong {field} in {name}")
            bridge = manifest.get("bridge")
            sources = manifest.get("sources")
            if not isinstance(bridge, dict) or bridge.get("identity") != lock["bridge_identity"]:
                raise AuditError(f"wrong bridge identity in {name}")
            if bridge.get("native_adapter_sha256") != expected_adapter:
                raise AuditError(f"stale native adapter/build input provenance in {name}")
            webrtc = sources.get("webrtc") if isinstance(sources, dict) else None
            depot_tools = sources.get("depot_tools") if isinstance(sources, dict) else None
            expected_state = "applied" if entry["flavor"] == "core" and entry["artifact_target"] in {"ios-arm64", "ios-simulator-arm64"} else "pristine"
            if not isinstance(webrtc, dict) or webrtc.get("state") != expected_state:
                raise AuditError(f"wrong WebRTC source state in {name}")
            source_identity = {
                "webrtc": {key: webrtc.get(key) for key in ("repository", "revision", "patch_sha256", "encoder_switch_patch_sha256", "runtime_resources_patch_sha256")},
                "depot_tools": depot_tools,
                "libvpx": sources.get("libvpx"),
            }
            if not all(source_identity["webrtc"].values()) or not isinstance(depot_tools, dict):
                raise AuditError(f"missing WebRTC source identity in {name}")
            if webrtc["patch_sha256"] != CORE_IOS_PATCH_SHA256:
                raise AuditError(f"wrong WebRTC patch identity in {name}")
            if webrtc["encoder_switch_patch_sha256"] != ENCODER_SWITCH_PATCH_SHA256:
                raise AuditError(f"wrong WebRTC encoder-switch patch identity in {name}")
            if webrtc["runtime_resources_patch_sha256"] != RUNTIME_RESOURCES_PATCH_SHA256:
                raise AuditError(f"wrong WebRTC runtime-resources patch identity in {name}")
            if common_bridge is None:
                common_bridge, common_sources = bridge, source_identity
            elif bridge != common_bridge or source_identity != common_sources:
                raise AuditError(f"bridge or source pin differs in {name}")
            configuration = manifest.get("native_configuration_sha256")
            if not isinstance(configuration, str) or configuration in configurations:
                raise AuditError(f"missing or duplicate native configuration in {name}")
            try:
                validate_configuration(manifest, ROOT)
            except (ValueError, KeyError, TypeError) as error:
                raise AuditError(f"invalid native configuration in {name}: {error}") from error
            configurations.add(configuration)
            native_archive = manifest["archive"]
            if digest(read_member(archive, native_archive["member"])) != native_archive["sha256"]:
                raise AuditError(f"native archive digest mismatch in {name}")
            unwind = manifest.get("unwind_archive")
            if unwind and digest(read_member(archive, unwind["member"])) != unwind["sha256"]:
                raise AuditError(f"Android unwind archive digest mismatch in {name}")

            licenses = manifest.get("licenses", {})
            files = licenses.get("files", []) if isinstance(licenses, dict) else []
            if not files:
                raise AuditError(f"missing license inventory in {name}")
            if licenses.get("sha256") != manifest_digest(files):
                raise AuditError(f"wrong license inventory identity in {name}")
            for license_file in files:
                path = license_file.get("path") if isinstance(license_file, dict) else None
                if (not isinstance(path, str) or not path.startswith("LICENSES/")
                        or "\\" in path or any(part in ("", ".", "..") for part in path.split("/"))):
                    raise AuditError(f"invalid license inventory path in {name}: {path!r}")
                contents = read_member(archive, path)
                if digest(contents) != license_file["sha256"]:
                    raise AuditError(f"license checksum mismatch in {name}: {license_file['path']}")

            assets.append(
                {
                    "name": name,
                    "sha256": archive_digest,
                    "native_configuration_sha256": configuration,
                    "license_inventory_sha256": licenses.get("sha256"),
                    "source_state": expected_state,
                    "source_patch_sha256": webrtc["patch_sha256"],
                }
            )
    return {
        "schema_version": 1,
        "scope": scope,
        "bridge": common_bridge,
        "sources": common_sources,
        "assets": assets,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--checksums", type=Path, required=True)
    parser.add_argument("--lock", type=Path, default=ROOT / "artifacts.lock.json")
    parser.add_argument("--scope", choices=sorted(SCOPES), default="complete")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = audit(args.archives, args.checksums, args.lock, args.scope)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, tarfile.TarError, AuditError) as error:
        print(f"release audit: {error}", file=sys.stderr)
        return 1
    print(f"audited {len(report['assets'])} release artifacts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
