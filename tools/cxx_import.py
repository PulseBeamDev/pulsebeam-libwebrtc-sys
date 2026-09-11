#!/usr/bin/env python3
"""Refresh and verify PulseBeam's Rust-only import of the CXX runtime."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tarfile
import tempfile
import tomllib
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
VENDOR = Path("vendor/cxx")
PROVENANCE = VENDOR / "provenance.json"
OVERLAY = Path("tools/cxx/Cargo.toml.in")
IMPORTED_ROOT_FILES = {"LICENSE-APACHE", "LICENSE-MIT"}
IMPORTED_PREFIXES = ("include/", "src/")
GENERATED_FILES = {"Cargo.toml", "provenance.json"}


class ImportError(Exception):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_provenance(root: Path) -> dict:
    try:
        data = json.loads((root / PROVENANCE).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ImportError(f"cannot read {PROVENANCE}: {error}") from error
    if data.get("schema") != 1:
        raise ImportError(f"unsupported provenance schema: {data.get('schema')!r}")
    return data


def render_manifest(root: Path, version: str) -> bytes:
    template = (root / OVERLAY).read_text(encoding="utf-8")
    if template.count("@CXX_VERSION@") != 2:
        raise ImportError(f"{OVERLAY} must contain exactly two @CXX_VERSION@ placeholders")
    return template.replace("@CXX_VERSION@", version).encode()


def imported_path(path: str) -> bool:
    return path in IMPORTED_ROOT_FILES or path.startswith(IMPORTED_PREFIXES)


def regular_files(directory: Path) -> set[str]:
    return {
        path.relative_to(directory).as_posix()
        for path in directory.rglob("*")
        if path.is_file() or path.is_symlink()
    }


def exact_dependency(value: object, version: str, *, path: str | None = None) -> bool:
    if not isinstance(value, dict) or value.get("version") != f"={version}":
        return False
    return path is None or value.get("path") == path


def validate_cargo_state(root: Path, version: str) -> list[str]:
    errors: list[str] = []
    try:
        local = tomllib.loads((root / VENDOR / "Cargo.toml").read_text())
        workspace = tomllib.loads((root / "Cargo.toml").read_text())
        lock = tomllib.loads((root / "Cargo.lock").read_text())
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"Cargo metadata is unreadable: {error}"]

    if local.get("package", {}).get("version") != version:
        errors.append(f"{VENDOR / 'Cargo.toml'}: package version is not {version}")
    macro = local.get("dependencies", {}).get("cxxbridge-macro")
    if macro != f"={version}":
        errors.append(f"{VENDOR / 'Cargo.toml'}: cxxbridge-macro is not pinned to ={version}")
    cxx = workspace.get("dependencies", {}).get("cxx")
    if not exact_dependency(cxx, version, path="vendor/cxx"):
        errors.append(f"Cargo.toml: local cxx dependency is not pinned to ={version}")

    packages = lock.get("package", [])
    locked_cxx = [package for package in packages if package.get("name") == "cxx"]
    locked_macro = [package for package in packages if package.get("name") == "cxxbridge-macro"]
    if len(locked_cxx) != 1 or locked_cxx[0].get("version") != version or "source" in locked_cxx[0]:
        errors.append(f"Cargo.lock: cxx {version} is not resolved as one local package")
    if len(locked_macro) != 1 or locked_macro[0].get("version") != version:
        errors.append(f"Cargo.lock: cxxbridge-macro is not resolved exactly once at {version}")
    forbidden = sorted(
        package.get("name") for package in packages if package.get("name") in {"cc", "cxx-build"}
    )
    if forbidden:
        errors.append(f"Cargo.lock: forbidden consumer native build packages: {', '.join(forbidden)}")
    if (root / VENDOR / "build.rs").exists():
        errors.append(f"{VENDOR / 'build.rs'}: native build script must not exist")
    return errors


def verification_errors(root: Path) -> list[str]:
    provenance = read_provenance(root)
    version = provenance.get("package", {}).get("version")
    expected = provenance.get("imported_files")
    if not isinstance(version, str) or not isinstance(expected, dict):
        raise ImportError(f"{PROVENANCE}: missing package version or imported_files")

    package = provenance["package"]
    upstream = provenance.get("upstream", {})
    if package.get("name") != "cxx":
        raise ImportError(f"{PROVENANCE}: package name must be cxx")
    if package.get("registry") != "https://crates.io":
        raise ImportError(f"{PROVENANCE}: package registry must be crates.io")
    if len(package.get("checksum", "")) != 64:
        raise ImportError(f"{PROVENANCE}: package checksum must be SHA-256")
    if upstream.get("repository") != "https://github.com/dtolnay/cxx":
        raise ImportError(f"{PROVENANCE}: unexpected upstream repository")
    if upstream.get("tag") != version or len(upstream.get("commit", "")) != 40:
        raise ImportError(f"{PROVENANCE}: upstream tag or commit disagrees with the package")

    vendor = root / VENDOR
    actual = regular_files(vendor)
    expected_paths = set(expected) | GENERATED_FILES
    errors = [f"unexpected imported file: {path}" for path in sorted(actual - expected_paths)]
    errors.extend(f"missing imported file: {path}" for path in sorted(expected_paths - actual))
    for path, expected_digest in sorted(expected.items()):
        source = vendor / path
        if source.is_file() and not source.is_symlink():
            actual_digest = digest(source.read_bytes())
            if actual_digest != expected_digest:
                errors.append(f"altered imported file: {path}")
        elif source.is_symlink():
            errors.append(f"altered imported file: {path} (symlink)")

    manifest = vendor / "Cargo.toml"
    if manifest.is_file() and manifest.read_bytes() != render_manifest(root, version):
        errors.append("generated overlay differs: vendor/cxx/Cargo.toml")
    errors.extend(validate_cargo_state(root, version))
    return errors


def verify(root: Path) -> None:
    errors = verification_errors(root)
    if errors:
        raise ImportError("\n".join(errors))


def archive_payload(archive: Path, package: dict) -> dict[str, bytes]:
    archive_bytes = archive.read_bytes()
    actual_checksum = digest(archive_bytes)
    expected_checksum = package.get("checksum")
    if actual_checksum != expected_checksum:
        raise ImportError(
            f"package checksum mismatch: expected {expected_checksum}, got {actual_checksum}"
        )

    version = package["version"]
    prefix = f"cxx-{version}/"
    payload: dict[str, bytes] = {}
    upstream_manifest: dict | None = None
    try:
        with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as crate:
            for member in crate.getmembers():
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts or not member.name.startswith(prefix):
                    raise ImportError(f"unsafe or unexpected archive path: {member.name}")
                relative = member.name[len(prefix) :]
                if not member.isfile():
                    continue
                extracted = crate.extractfile(member)
                if extracted is None:
                    raise ImportError(f"cannot read archive member: {member.name}")
                data = extracted.read()
                if relative == "Cargo.toml.orig":
                    upstream_manifest = tomllib.loads(data.decode())
                if imported_path(relative):
                    payload[relative] = data
    except (tarfile.TarError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ImportError(f"invalid CXX package archive: {error}") from error

    required = IMPORTED_ROOT_FILES | {"include/cxx.h", "src/cxx.cc", "src/lib.rs"}
    missing = required - set(payload)
    if missing:
        raise ImportError(f"package is missing required imports: {', '.join(sorted(missing))}")
    if upstream_manifest is None:
        raise ImportError("package is missing Cargo.toml.orig")
    if upstream_manifest.get("package", {}).get("name") != "cxx":
        raise ImportError("package manifest is not for cxx")
    if upstream_manifest.get("package", {}).get("version") != version:
        raise ImportError("package manifest version disagrees with recorded CXX version")
    macro = upstream_manifest.get("dependencies", {}).get("cxxbridge-macro", {})
    if macro.get("version") != f"={version}":
        raise ImportError("upstream cxxbridge-macro version disagrees with recorded CXX version")
    return payload


def refresh(root: Path, archive: Path) -> None:
    provenance = read_provenance(root)
    package = provenance["package"]
    payload = archive_payload(archive, package)
    version = package["version"]
    manifest = render_manifest(root, version)

    # Complete all package checks and stage the replacement before touching the import.
    with tempfile.TemporaryDirectory(prefix="cxx-import-", dir=root / "vendor") as temporary:
        staged = Path(temporary) / "cxx"
        for path, data in payload.items():
            destination = staged / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        (staged / "Cargo.toml").write_bytes(manifest)
        provenance["imported_files"] = {path: digest(data) for path, data in sorted(payload.items())}
        (staged / "provenance.json").write_text(
            json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        errors = verification_errors_for_staged(root, staged, provenance)
        if errors:
            raise ImportError("\n".join(errors))

        destination = root / VENDOR
        backup = Path(temporary) / "previous"
        os.replace(destination, backup)
        try:
            os.replace(staged, destination)
        except BaseException:
            os.replace(backup, destination)
            raise


def verification_errors_for_staged(root: Path, staged: Path, provenance: dict) -> list[str]:
    expected = provenance["imported_files"]
    actual = regular_files(staged)
    expected_paths = set(expected) | GENERATED_FILES
    errors = [f"unexpected staged file: {path}" for path in sorted(actual - expected_paths)]
    errors.extend(f"missing staged file: {path}" for path in sorted(expected_paths - actual))
    for path, expected_digest in expected.items():
        if path in actual and digest((staged / path).read_bytes()) != expected_digest:
            errors.append(f"altered staged file: {path}")
    if (staged / "Cargo.toml").read_bytes() != render_manifest(root, provenance["package"]["version"]):
        errors.append("generated staged overlay differs")
    return errors


def download(root: Path, package: dict) -> Path:
    version = package["version"]
    url = f"https://static.crates.io/crates/cxx/cxx-{version}.crate"
    destination = root / ".work" / "downloads" / f"cxx-{version}.crate"
    destination.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url) as response:
        data = response.read()
    temporary = destination.with_suffix(".tmp")
    temporary.write_bytes(data)
    os.replace(temporary, destination)
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("verify", help="verify the import without network access")
    subparsers.add_parser("version", help="print the recorded compatible CXX version")
    refresh_parser = subparsers.add_parser("refresh", help="download and refresh the pinned import")
    refresh_parser.add_argument("--archive", type=Path, help="use an already downloaded .crate archive")
    args = parser.parse_args()

    try:
        provenance = read_provenance(ROOT)
        if args.command == "version":
            print(provenance["package"]["version"])
        elif args.command == "verify":
            verify(ROOT)
        else:
            archive = args.archive or download(ROOT, provenance["package"])
            refresh(ROOT, archive.resolve())
    except (ImportError, OSError, KeyError) as error:
        print(f"cxx import: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
