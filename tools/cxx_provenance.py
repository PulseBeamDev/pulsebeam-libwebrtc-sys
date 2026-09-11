#!/usr/bin/env python3
"""Build and verify artifact-facing CXX producer provenance."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
PROVENANCE = Path("vendor/cxx/provenance.json")


class ProvenanceError(Exception):
    pass


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def is_digest(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def source_provenance(root: Path) -> dict:
    try:
        provenance = json.loads((root / PROVENANCE).read_text())
        package = provenance["package"]
        generator = provenance["generator"]
        imported = provenance["imported_files"]
    except (OSError, json.JSONDecodeError, KeyError) as error:
        raise ProvenanceError(f"cannot read complete CXX provenance: {error}") from error
    if provenance.get("schema") != 1:
        raise ProvenanceError("unsupported CXX source provenance schema")
    if package.get("name") != "cxx" or generator.get("name") != "cxxbridge-cmd":
        raise ProvenanceError("wrong CXX runtime or generator package")
    if package.get("registry") != "https://crates.io" or generator.get("registry") != "https://crates.io":
        raise ProvenanceError("CXX packages must come from crates.io")
    if package.get("version") != generator.get("version"):
        raise ProvenanceError("CXX runtime and generator versions disagree")
    if generator.get("source_revision") != provenance.get("upstream", {}).get("commit"):
        raise ProvenanceError("CXX runtime and generator source revisions disagree")
    for name, value in (
        ("runtime package", package.get("checksum")),
        ("generator package", generator.get("checksum")),
    ):
        if not is_digest(value):
            raise ProvenanceError(f"invalid {name} checksum")
    required = ("src/lib.rs", "include/cxx.h", "src/cxx.cc")
    for relative in required:
        expected = imported.get(relative)
        path = root / "vendor/cxx" / relative
        if not is_digest(expected) or not path.is_file() or sha256(path) != expected:
            raise ProvenanceError(f"altered or unrecorded CXX input: {relative}")
    return provenance


def artifact_provenance(root: Path, bridge_source: Path, generated_header: Path, generated_source: Path) -> dict:
    provenance = source_provenance(root)
    package = provenance["package"]
    generator = provenance["generator"]
    imported = provenance["imported_files"]
    for name, path in (
        ("bridge source", bridge_source),
        ("generated bridge header", generated_header),
        ("generated bridge source", generated_source),
    ):
        if not path.is_file():
            raise ProvenanceError(f"missing {name}: {path}")
    return {
        "version": package["version"],
        "runtime_package_sha256": package["checksum"],
        "generator_package_sha256": generator["checksum"],
        "rust_runtime_sha256": imported["src/lib.rs"],
        "header_sha256": imported["include/cxx.h"],
        "native_runtime_sha256": imported["src/cxx.cc"],
        "bridge_source_sha256": sha256(bridge_source),
        "generated_header_sha256": sha256(generated_header),
        "generated_source_sha256": sha256(generated_source),
    }


def validate_artifact_provenance(value: object, root: Path, artifact: Path | None = None) -> None:
    expected_keys = {
        "version", "runtime_package_sha256", "generator_package_sha256",
        "rust_runtime_sha256", "header_sha256", "native_runtime_sha256",
        "bridge_source_sha256", "generated_header_sha256", "generated_source_sha256",
    }
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise ProvenanceError("missing or unexpected artifact CXX provenance fields")
    source = source_provenance(root)
    expected = {
        "version": source["package"]["version"],
        "runtime_package_sha256": source["package"]["checksum"],
        "generator_package_sha256": source["generator"]["checksum"],
        "rust_runtime_sha256": source["imported_files"]["src/lib.rs"],
        "header_sha256": source["imported_files"]["include/cxx.h"],
        "native_runtime_sha256": source["imported_files"]["src/cxx.cc"],
        "bridge_source_sha256": sha256(root / "src/lib.rs"),
    }
    for name, expected_value in expected.items():
        if value.get(name) != expected_value:
            raise ProvenanceError(f"stale or inconsistent CXX provenance: {name}")
    for name in expected_keys - {"version"}:
        if not is_digest(value.get(name)):
            raise ProvenanceError(f"invalid CXX provenance digest: {name}")
    if artifact is not None:
        files = {
            "header_sha256": artifact / "include/rust/cxx.h",
            "generated_header_sha256": artifact / "include/pulsebeam-webrtc-sys/src/lib.rs.h",
        }
        for name, path in files.items():
            if not path.is_file() or sha256(path) != value[name]:
                raise ProvenanceError(f"artifact CXX file disagrees with manifest: {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path, help="extracted artifact directory")
    args = parser.parse_args()
    try:
        manifest = json.loads((args.artifact / "manifest.json").read_text())
        if manifest.get("schema_version") != 2:
            raise ProvenanceError("artifact manifest is not schema 2")
        validate_artifact_provenance(manifest.get("bridge", {}).get("cxx"), ROOT, args.artifact)
    except (OSError, json.JSONDecodeError, ProvenanceError) as error:
        print(f"CXX artifact provenance: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
