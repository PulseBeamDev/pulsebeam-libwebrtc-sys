#!/usr/bin/env python3
"""Refresh and verify PulseBeam's Rust-only import of the CXX runtime."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import io
import json
import os
from pathlib import Path, PurePosixPath
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
VENDOR = Path("vendor/cxx")
PROVENANCE = VENDOR / "provenance.json"
OVERLAY = Path("tools/cxx/Cargo.toml.in")
IMPORTED_ROOT_FILES = {"LICENSE-APACHE", "LICENSE-MIT"}
IMPORTED_PREFIXES = ("include/", "src/")
GENERATED_FILES = {"Cargo.toml", "provenance.json"}
GENERATOR_STAMP = "cxxbridge-provenance.json"
DOWNLOAD_ATTEMPTS = 3
RETRYABLE_HTTP_STATUSES = {408, 429, 500, 502, 503, 504}


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
    by_name: dict[str, list[dict]] = {}
    for package in packages:
        by_name.setdefault(package.get("name", ""), []).append(package)
    pending = ["cxx"]
    reached: set[str] = set()
    while pending:
        name = pending.pop()
        if name in reached:
            continue
        reached.add(name)
        for package in by_name.get(name, []):
            pending.extend(dependency.split()[0] for dependency in package.get("dependencies", []))
    forbidden = sorted(reached & {"cc", "cxx-build"})
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
    generator = provenance.get("generator", {})
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
    if generator.get("name") != "cxxbridge-cmd":
        raise ImportError(f"{PROVENANCE}: generator package name must be cxxbridge-cmd")
    if generator.get("registry") != "https://crates.io":
        raise ImportError(f"{PROVENANCE}: generator registry must be crates.io")
    if generator.get("version") != version:
        raise ImportError(f"{PROVENANCE}: generator version disagrees with the runtime")
    if generator.get("source_revision") != upstream.get("commit"):
        raise ImportError(f"{PROVENANCE}: generator source revision disagrees with the runtime")
    if len(generator.get("checksum", "")) != 64:
        raise ImportError(f"{PROVENANCE}: generator package checksum must be SHA-256")

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


def package_identity(package: dict) -> str:
    name = package.get("name")
    version = package.get("version")
    checksum = package.get("checksum")
    if not isinstance(name, str) or not isinstance(version, str) or not isinstance(checksum, str):
        raise ImportError("package provenance is missing name, version, or checksum")
    if len(checksum) != 64:
        raise ImportError("package provenance checksum must be SHA-256")
    return f"{name}-{version}-{checksum}"


def package_url(package: dict) -> str:
    name = package["name"]
    version = package["version"]
    return f"https://static.crates.io/crates/{name}/{name}-{version}.crate"


def cached_archive(root: Path, package: dict) -> Path:
    return root / ".work" / "downloads" / f"{package_identity(package)}.crate"


def verified_cache_state(archive: Path, checksum: str) -> str:
    if not archive.is_file():
        return "missing"
    if archive.is_symlink():
        return "corrupt"
    try:
        return "verified" if digest(archive.read_bytes()) == checksum else "corrupt"
    except OSError:
        return "unreadable"


def retryable_transport_failure(error: BaseException) -> bool:
    if isinstance(error, urllib.error.HTTPError):
        return error.code in RETRYABLE_HTTP_STATUSES
    if isinstance(error, urllib.error.URLError):
        return retryable_transport_failure(error.reason)
    return isinstance(
        error,
        (
            socket.gaierror,
            socket.timeout,
            TimeoutError,
            ConnectionError,
            http.client.IncompleteRead,
            http.client.RemoteDisconnected,
        ),
    )


def retrieval_error(
    package: dict,
    url: str,
    attempts: int,
    cache_state: str,
    failure_class: str,
    error: BaseException,
) -> ImportError:
    return ImportError(
        f"cannot retrieve pinned input {package_identity(package)} from {url}: "
        f"attempts={attempts}, cache={cache_state}, failure={failure_class}: {error}"
    )


def download(root: Path, package: dict) -> Path:
    """Return a checksum-verified cached crate, fetching it with bounded recovery."""
    expected_checksum = package.get("checksum")
    if not isinstance(expected_checksum, str):
        raise ImportError("package provenance is missing checksum")
    archive = cached_archive(root, package)
    cache_state = verified_cache_state(archive, expected_checksum)
    if cache_state == "verified":
        return archive

    archive.parent.mkdir(parents=True, exist_ok=True)
    url = package_url(package)
    for attempt in range(1, DOWNLOAD_ATTEMPTS + 1):
        temporary: Path | None = None
        try:
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read()
            with tempfile.NamedTemporaryFile(
                prefix=f".{archive.name}.", suffix=".tmp", dir=archive.parent, delete=False
            ) as output:
                temporary = Path(output.name)
                output.write(data)
                output.flush()
                os.fsync(output.fileno())
            if digest(temporary.read_bytes()) != expected_checksum:
                raise ImportError("downloaded package checksum mismatch")
            os.replace(temporary, archive)
            return archive
        except ImportError as error:
            raise retrieval_error(
                package, url, attempt, cache_state, "checksum", error
            ) from error
        except (
            urllib.error.HTTPError,
            urllib.error.URLError,
            socket.gaierror,
            socket.timeout,
            TimeoutError,
            ConnectionError,
            http.client.IncompleteRead,
            http.client.RemoteDisconnected,
            ValueError,
            OSError,
        ) as error:
            if not retryable_transport_failure(error):
                raise retrieval_error(
                    package, url, attempt, cache_state, "deterministic-retrieval", error
                ) from error
            if attempt == DOWNLOAD_ATTEMPTS:
                raise retrieval_error(
                    package, url, attempt, cache_state, "transient-transport-exhausted", error
                ) from error
            time.sleep(0.2 * attempt)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)

    raise AssertionError("bounded download loop unexpectedly ended")


def generator_binary(install_root: Path) -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return install_root / "bin" / f"cxxbridge{suffix}"


def generator_version(binary: Path) -> str:
    try:
        result = subprocess.run(
            [binary, "--version"], check=True, capture_output=True, text=True
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise ImportError(f"cannot run generator {binary}: {error}") from error
    return result.stdout.strip()


def unpack_generator(archive: Path, destination: Path, package: dict) -> Path:
    archive_bytes = archive.read_bytes()
    actual_checksum = digest(archive_bytes)
    if actual_checksum != package.get("checksum"):
        raise ImportError(
            f"generator package checksum mismatch: expected {package.get('checksum')}, "
            f"got {actual_checksum}"
        )

    version = package["version"]
    prefix = f"cxxbridge-cmd-{version}/"
    manifest = None
    vcs = None
    try:
        with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as crate:
            for member in crate.getmembers():
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts or not member.name.startswith(prefix):
                    raise ImportError(f"unsafe or unexpected generator archive path: {member.name}")
                relative = member.name[len(prefix) :]
                if not member.isfile():
                    continue
                extracted = crate.extractfile(member)
                if extracted is None:
                    raise ImportError(f"cannot read generator archive member: {member.name}")
                data = extracted.read()
                if relative == "Cargo.toml.orig":
                    manifest = tomllib.loads(data.decode())
                elif relative == ".cargo_vcs_info.json":
                    vcs = json.loads(data)
                output = destination / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(data)
    except (tarfile.TarError, UnicodeDecodeError, tomllib.TOMLDecodeError, json.JSONDecodeError) as error:
        raise ImportError(f"invalid generator package archive: {error}") from error

    if manifest is None or manifest.get("package", {}).get("name") != "cxxbridge-cmd":
        raise ImportError("generator package manifest is missing or has the wrong name")
    if manifest["package"].get("version") != version:
        raise ImportError("generator package manifest version disagrees with provenance")
    if vcs is None or vcs.get("git", {}).get("sha1") != package.get("source_revision"):
        raise ImportError("generator package source revision disagrees with CXX provenance")
    if not (destination / "Cargo.lock").is_file():
        raise ImportError("generator package is missing Cargo.lock")
    return destination


def verify_generator(root: Path, install_root: Path) -> Path:
    verify(root)
    package = read_provenance(root)["generator"]
    binary = generator_binary(install_root)
    expected_version = f"cxxbridge {package['version']}"
    if generator_version(binary) != expected_version:
        raise ImportError(f"generator version must be {expected_version}")
    try:
        stamp = json.loads((install_root / GENERATOR_STAMP).read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ImportError(f"cannot verify installed generator provenance: {error}") from error
    if stamp != package:
        raise ImportError("installed generator provenance disagrees with the pinned package")
    return binary


def install_generator(root: Path, install_root: Path, archive: Path | None) -> Path:
    verify(root)
    package = read_provenance(root)["generator"]
    binary = generator_binary(install_root)
    if binary.exists():
        expected_version = f"cxxbridge {package['version']}"
        if generator_version(binary) != expected_version:
            raise ImportError(f"refusing generator whose version is not {expected_version}")
        try:
            if json.loads((install_root / GENERATOR_STAMP).read_text()) == package:
                return binary
        except (OSError, json.JSONDecodeError):
            pass

    if archive is None:
        archive = download(root, package)

    with tempfile.TemporaryDirectory(prefix="cxxbridge-package-") as temporary:
        source = unpack_generator(archive.resolve(), Path(temporary), package)
        command = [
            "cargo", "install", "--path", str(source), "--locked", "--root", str(install_root)
        ]
        if binary.exists():
            command.append("--force")
        try:
            subprocess.run(command, check=True)
        except (OSError, subprocess.CalledProcessError) as error:
            raise ImportError(f"cannot install checksum-verified generator: {error}") from error

    if generator_version(binary) != f"cxxbridge {package['version']}":
        raise ImportError("installed generator reports the wrong version")
    install_root.mkdir(parents=True, exist_ok=True)
    (install_root / GENERATOR_STAMP).write_text(
        json.dumps(package, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return verify_generator(root, install_root)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("verify", help="verify the import without network access")
    subparsers.add_parser("version", help="print the recorded compatible CXX version")
    refresh_parser = subparsers.add_parser("refresh", help="download and refresh the pinned import")
    refresh_parser.add_argument("--archive", type=Path, help="use an already downloaded .crate archive")
    install_parser = subparsers.add_parser(
        "install-generator", help="install the checksum-verified compatible cxxbridge"
    )
    install_parser.add_argument("--root", type=Path, required=True)
    install_parser.add_argument("--archive", type=Path)
    verify_parser = subparsers.add_parser(
        "verify-generator", help="verify an installed cxxbridge and its package provenance"
    )
    verify_parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()

    try:
        provenance = read_provenance(ROOT)
        if args.command == "version":
            print(provenance["package"]["version"])
        elif args.command == "verify":
            verify(ROOT)
        elif args.command == "refresh":
            archive = args.archive or download(ROOT, provenance["package"])
            refresh(ROOT, archive.resolve())
        elif args.command == "install-generator":
            print(install_generator(ROOT, args.root.resolve(), args.archive))
        else:
            print(verify_generator(ROOT, args.root.resolve()))
    except (ImportError, OSError, KeyError) as error:
        print(f"cxx import: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
