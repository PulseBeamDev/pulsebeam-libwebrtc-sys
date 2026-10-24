#!/usr/bin/env python3
"""Prove a clean Git consumer builds without invoking a C/C++ compiler."""

from __future__ import annotations

import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import re
import shutil
import shlex
import subprocess
import sys
import tarfile
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]
CONSUMER = ROOT / "consumer/rust-only"
PUBLIC_REPOSITORY = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git"
REVISION = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"[0-9a-f]{64}")
sys.path.insert(0, str(ROOT))
from tools import write_artifact_lock
from tools.release_inventory import OS_TARGETS, release_tag, source_tag

COORDINATES = {
    (artifact, flavor): (cargo, f"webrtc-{flavor}-{artifact}.tar.gz")
    for cargo, artifact in write_artifact_lock.SUPPORTED_TARGETS.items()
    for flavor in ("core", "native")
}


class ProofError(Exception):
    pass


def coordinate(target: str, flavor: str) -> tuple[str, str]:
    try:
        return COORDINATES[(target, flavor)]
    except KeyError:
        raise ProofError(f"unsupported artifact selection: {target} {flavor}") from None


def render_consumer_manifest(repository: str, revision: str, target: str, flavor: str, *, tag: bool = False) -> str:
    coordinate(target, flavor)
    if tag:
        try:
            source_tag(revision)
        except ValueError as error:
            raise ProofError(f"invalid release tag: {revision}") from error
    elif not REVISION.fullmatch(revision):
        raise ProofError(f"revision is not an exact 40-character lowercase Git revision: {revision}")
    features = ', features = ["native"]' if flavor == "native" else ""
    return (CONSUMER / "Cargo.toml.in").read_text(encoding="utf-8").replace(
        "@REPOSITORY@", repository
    ).replace("@SELECTOR@", "tag" if tag else "rev").replace("@REVISION@", revision).replace("@FEATURES@", features)


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None, capture: bool = False):
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )


def clean_snapshot(destination: Path, download_url: str, digest: str, target: str, flavor: str) -> str:
    _, selected_asset = coordinate(target, flavor)
    listing = subprocess.check_output(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
    )
    for encoded in listing.split(b"\0"):
        if not encoded:
            continue
        relative = Path(os.fsdecode(encoded))
        source = ROOT / relative
        if source.is_symlink() or not source.is_file():
            raise ProofError(f"snapshot input is not a regular file: {relative}")
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, output)
    lock_path = destination / "artifacts.lock.json"
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    scope = target.split('-')[0]
    assets = []
    for index, entry in enumerate(lock["artifacts"]):
        if entry["artifact_target"] not in OS_TARGETS[scope]:
            continue
        assets.append({
            "name": entry["asset_name"],
            "sha256": digest if entry["asset_name"] == selected_asset else "1" * 64,
            "native_configuration_sha256": f"{index + 1:064x}",
            "license_inventory_sha256": f"{index + 101:064x}",
            "source_state": "applied" if entry["flavor"] == "core" and scope == "ios" else "pristine",
            "source_patch_sha256": "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037",
        })
    report = {
        "schema_version": 1,
        "scope": scope,
        "bridge": {"identity": lock["bridge_identity"]},
        "sources": {"webrtc": {"repository": "fixture", "revision": "fixture", "patch_sha256": "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037", "encoder_switch_patch_sha256": "3efb4195c6fe89e7a120cfa9ded6d8fccd99ffacf85148ff0c79e9b34550f99a", "runtime_resources_patch_sha256": "937d2c42fe925b7703b0319c0eb80d6dfdf09194005790520d211ed00e4454d8", "recordable_identity_patch_sha256": write_artifact_lock.RECORDABLE_IDENTITY_PATCH_SHA256}, "depot_tools": {"repository": "fixture", "revision": "fixture"}, "libvpx": dict(write_artifact_lock.LIBVPX_SOURCE)},
        "assets": assets,
    }
    report_path = destination / f"{scope}-audit.json"
    report_path.write_text(json.dumps(report), encoding="utf-8")
    lock_path.write_bytes(write_artifact_lock.render(lock_path, None, release_tag("v0.0.0", scope), report_path))
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    for entry in lock["artifacts"]:
        if entry["artifact_target"] in OS_TARGETS[scope]:
            entry["url"] = download_url.rsplit("/", 1)[0] + "/" + entry["asset_name"]
            entry["sha256"] = digest if entry["asset_name"] == selected_asset else "1" * 64
    lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
    run(["git", "init", "--quiet"], cwd=destination)
    run(["git", "config", "user.name", "Rust-only consumer proof"], cwd=destination)
    run(["git", "config", "user.email", "rust-only@example.invalid"], cwd=destination)
    run(["git", "add", "."], cwd=destination)
    run(["git", "commit", "--quiet", "-m", "snapshot"], cwd=destination)
    return run(["git", "rev-parse", "HEAD"], cwd=destination, capture=True).stdout.strip()


def extract_artifact(source_artifact: Path, destination: Path) -> None:
    wanted = {"manifest.json"}
    with tarfile.open(source_artifact, "r:gz") as archive:
        manifest_member = archive.getmember("manifest.json")
        if not manifest_member.isfile():
            raise ProofError("artifact manifest is not a regular file")
        manifest_source = archive.extractfile(manifest_member)
        if manifest_source is None:
            raise ProofError("cannot read artifact manifest")
        manifest = json.load(manifest_source)
        archive_member = manifest.get("archive", {}).get("member")
        if archive_member not in {"lib/libwebrtc.a", "lib/webrtc.lib"}:
            raise ProofError("invalid native archive member")
        wanted.add(archive_member)
        if manifest.get("unwind_archive") is not None:
            if manifest["unwind_archive"].get("member") != "lib/libunwind.a":
                raise ProofError("invalid Android unwind archive member")
            wanted.add("lib/libunwind.a")
        for notice in manifest["licenses"]["files"]:
            path = Path(notice["path"])
            if "\\" in notice["path"] or path.is_absolute() or not path.parts or path.parts[0] != "LICENSES" or ".." in path.parts:
                raise ProofError("invalid artifact notice path")
            wanted.add(path.as_posix())
        selected = [member for member in archive.getmembers() if member.name in wanted]
        members = {member.name: member for member in selected}
        if len(members) != len(selected):
            raise ProofError("artifact contains duplicate consumer payload members")
        if set(members) != wanted:
            raise ProofError("artifact is missing its manifest, native archive or inventoried notices")
        for name in sorted(wanted):
            member = members[name]
            if not member.isfile():
                raise ProofError(f"checked-in host artifact member is not a file: {name}")
            source = archive.extractfile(member)
            if source is None:
                raise ProofError(f"cannot read checked-in host artifact member: {name}")
            output = destination / name
            output.parent.mkdir(parents=True, exist_ok=True)
            with output.open("wb") as sink:
                shutil.copyfileobj(source, sink)


def configure_target_environment(environment: dict[str, str], cargo_target: str) -> None:
    target = write_artifact_lock.SUPPORTED_TARGETS[cargo_target]
    flags = environment.get("CARGO_ENCODED_RUSTFLAGS")
    flags = flags.split("\x1f") if flags is not None else shlex.split(environment.get("RUSTFLAGS", ""))
    if target == "linux-arm64":
        linker_key = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"
        if not environment.get(linker_key):
            linker = shutil.which("clang-19") or shutil.which("clang")
            if not linker:
                raise ProofError("Linux ARM64 consumer requires Clang with compiler-rt and LLD")
            environment[linker_key] = linker
        flags += ["-C", "link-arg=-fuse-ld=lld", "-C", "link-arg=--rtlib=compiler-rt", "-C", "link-arg=--unwindlib=none"]
    elif target.startswith("android-"):
        linker_key = f"CARGO_TARGET_{cargo_target.upper().replace('-', '_')}_LINKER"
        if not environment.get(linker_key):
            ndk = environment.get("ANDROID_NDK_HOME")
            if not ndk:
                raise ProofError("Android consumer requires ANDROID_NDK_HOME or its Cargo target linker")
            triple = "x86_64" if target == "android-x86_64" else "aarch64"
            linker = Path(ndk) / f"toolchains/llvm/prebuilt/linux-x86_64/bin/{triple}-linux-android26-clang"
            if not linker.is_file():
                raise ProofError(f"missing Android API 26 linker: {linker}")
            environment[linker_key] = str(linker)
        flags += ["-C", "link-arg=--unwindlib=none"]
    elif target.startswith(("macos-", "ios-")):
        if target.startswith("macos-"):
            environment["MACOSX_DEPLOYMENT_TARGET"] = "12.0"
            sdk = "macosx"
        else:
            environment["IPHONEOS_DEPLOYMENT_TARGET"] = "18.0"
            sdk = "iphonesimulator" if target == "ios-simulator-arm64" else "iphoneos"
        environment["SDKROOT"] = run(["xcrun", "--sdk", sdk, "--show-sdk-path"], cwd=ROOT, capture=True).stdout.strip()
        if not environment["SDKROOT"] or not Path(environment["SDKROOT"]).is_dir():
            raise ProofError(f"missing {sdk} SDK")
    elif target == "windows-x86_64":
        flags += ["-C", "target-feature=+crt-static"]
    environment.pop("RUSTFLAGS", None)
    environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)


def write_sentinel(directory: Path, name: str, marker: Path) -> Path:
    windows = platform.system() == "Windows"
    sentinel = directory / (name + ".cmd" if windows else name)
    if windows:
        contents = f'@echo off\necho %0 %* >> "{marker}"\necho {name} invocation is forbidden in the Rust-only consumer proof 1>&2\nexit /b 97\n'
    else:
        contents = ("#!/bin/sh\n"
                    f"printf '%s\\n' \"$0 $*\" >> {shlex.quote(str(marker))}\n"
                    f"echo '{name} invocation is forbidden in the Rust-only consumer proof' >&2\n"
                    "exit 97\n")
    sentinel.write_text(contents, encoding="utf-8")
    sentinel.chmod(0o755)
    return sentinel


def cold_consumer_environment(marker: Path, sentinel_directory: Path, work: Path, cargo_target: str) -> dict[str, str]:
    sentinel_directory.mkdir()
    sentinels = {language: str(write_sentinel(sentinel_directory, language, marker)) for language in ("c", "cxx")}
    environment = os.environ.copy()
    for name in ("PULSEBEAM_WEBRTC_SYS_SKIP_LINK", "PULSEBEAM_WEBRTC_SYS_CANDIDATE_LOCK", "PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR", "PULSEBEAM_WEBRTC_SYS_OFFLINE", "CARGO_NET_OFFLINE"):
        environment.pop(name, None)
    for tool in ("gn", "ninja"):
        write_sentinel(sentinel_directory, tool, marker)
    environment["PATH"] = str(sentinel_directory) + os.pathsep + environment["PATH"]
    environment["WEBRTC_WORK"] = str(work)
    for compiler, language in (("CC", "c"), ("CXX", "cxx")):
        environment[compiler] = sentinels[language]
        environment[f"HOST_{compiler}"] = sentinels[language]
        environment[f"TARGET_{compiler}"] = sentinels[language]
        for suffix in (cargo_target, cargo_target.replace("-", "_")):
            environment[f"{compiler}_{suffix}"] = sentinels[language]
    configure_target_environment(environment, cargo_target)
    return environment


class ArtifactServer:
    def __init__(self, payload: bytes, asset_name: str):
        self.payload = payload
        self.asset_name = asset_name
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                if self.path != "/" + owner.asset_name:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Length", str(len(owner.payload)))
                self.end_headers()
                self.wfile.write(owner.payload)

            def log_message(self, _format, *_args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.server.server_port}/{self.asset_name}"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_args):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()


def expect_failure(command: list[str], *, cwd: Path, env: dict[str, str], message: str) -> None:
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True)
    if result.returncode == 0:
        raise ProofError(f"expected failure containing {message!r}")
    output = result.stdout + result.stderr
    if message not in output:
        raise ProofError(f"failure did not contain {message!r}:\n{output}")


def validate_consumer_metadata(metadata: str, temporary_root: Path) -> None:
    """Run the repository's downstream consumer graph policy on Cargo output."""
    metadata_path = temporary_root / "metadata.json"
    metadata_path.write_text(metadata, encoding="utf-8")
    run(
        [sys.executable, str(ROOT / "tools/check_consumer_metadata.py"), str(metadata_path)],
        cwd=ROOT,
    )


def write_consumer(destination: Path, repository: str, revision: str, target: str, flavor: str, cargo_config: str, *, tag: bool = False) -> None:
    shutil.copytree(CONSUMER / "src", destination / "src")
    (destination / "Cargo.toml").write_text(render_consumer_manifest(repository, revision, target, flavor, tag=tag), encoding="utf-8")
    (destination / ".cargo").mkdir()
    (destination / ".cargo/config.toml").write_text(cargo_config, encoding="utf-8")


def require_host(target: str) -> None:
    hosts = {
        "linux-x86_64": ("Linux", {"x86_64", "AMD64"}),
        "linux-arm64": ("Linux", {"aarch64", "arm64"}),
        "windows-x86_64": ("Windows", {"AMD64", "x86_64"}),
        "macos-x86_64": ("Darwin", {"x86_64"}),
        "macos-arm64": ("Darwin", {"arm64"}),
        "ios-arm64": ("Darwin", {"arm64"}),
        "ios-simulator-arm64": ("Darwin", {"arm64"}),
        "android-x86_64": ("Linux", {"x86_64"}),
        "android-arm64-v8a": ("Linux", {"x86_64"}),
    }
    system, machines = hosts[target]
    if platform.system() != system or platform.machine() not in machines:
        raise ProofError(f"host does not match requested target {target}")


def consumer_command(target: str, *flags: str) -> list[str]:
    cargo_target, _ = coordinate(target, "core")
    # Mobile runtime is a separate emulator/simulator operation. This boundary
    # must still produce a fully linked target executable, not cargo check.
    action = "build" if target.split('-')[0] in {"android", "ios"} else "run"
    return ["cargo", action, "--target", cargo_target, *flags]


def find_build_script(target_root: Path) -> Path:
    name = "build-script-build.exe" if platform.system() == "Windows" else "build-script-build"
    scripts = sorted((target_root / "debug" / "build").glob(f"pulsebeam-webrtc-sys-*/{name}"))
    if not scripts:
        raise ProofError("consumer did not produce a pulsebeam-webrtc-sys build script")
    return scripts[0]


def prove_candidate(source_artifact: Path, expected_digest: str, target: str, flavor: str) -> None:
    cargo_target, asset_name = coordinate(target, flavor)
    require_host(target)
    if not source_artifact.is_file():
        raise ProofError(f"missing {asset_name} artifact: {source_artifact}")
    actual_digest = hashlib.sha256(source_artifact.read_bytes()).hexdigest()
    if actual_digest != expected_digest:
        raise ProofError(f"{asset_name} checksum mismatch: expected={expected_digest} actual={actual_digest}")

    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-") as temporary:
        temporary_root = Path(temporary)
        payload = source_artifact.read_bytes()
        with ArtifactServer(payload, asset_name) as server:
            repository = temporary_root / "repository"
            repository.mkdir()
            revision = clean_snapshot(repository, server.url, expected_digest, target, flavor)

            artifact = temporary_root / "artifact"
            extract_artifact(source_artifact, artifact)

            vendor = temporary_root / "registry"
            bootstrap_environment = os.environ.copy()
            # Reuse an explicitly supplied writable dependency cache for
            # vendoring. The consumer itself still starts with a fresh home.
            consumer_cargo_home = str(temporary_root / "cargo-home")
            bootstrap_environment["CARGO_HOME"] = os.environ.get("CARGO_HOME", consumer_cargo_home)
            cargo_config = run(
                ["cargo", "vendor", "--locked", str(vendor)],
                cwd=ROOT,
                env=bootstrap_environment,
                capture=True,
            ).stdout
            consumer = temporary_root / "consumer"
            write_consumer(consumer, repository.resolve().as_uri(), revision, target, flavor, cargo_config)

            marker = temporary_root / "compiler-invocations"
            work = temporary_root / "no-producer-work"
            environment = cold_consumer_environment(marker, temporary_root / "consumer-sentinels", work, cargo_target)
            environment["CARGO_HOME"] = consumer_cargo_home
            environment["CARGO_TARGET_DIR"] = str(temporary_root / "target")
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-cache")
            # The candidate fixture exercises the checked-in mock lock; real Git consumers fetch the tag's release lock.
            environment["PULSEBEAM_WEBRTC_SYS_CANDIDATE_LOCK"] = "1"

            metadata = run(
                ["cargo", "metadata", "--format-version=1"],
                cwd=consumer,
                env=environment,
                capture=True,
            ).stdout
            validate_consumer_metadata(metadata, temporary_root)

            # The release proof starts with no override, artifact cache, or target cache.
            run(consumer_command(target, "--locked"), cwd=consumer, env=environment)
            if (work / "checkout").exists() or (work / "depot_tools").exists():
                raise ProofError("Rust-only consumer created a Chromium checkout or depot_tools worktree")

            unavailable_repository = temporary_root / "unavailable-repository"
            shutil.copytree(repository, unavailable_repository, ignore=shutil.ignore_patterns(".git"))
            unavailable_lock = unavailable_repository / "artifacts.lock.json"
            unavailable = json.loads(unavailable_lock.read_text(encoding="utf-8"))
            for entry in unavailable["artifacts"]:
                entry["url"] = entry["sha256"] = None
            unavailable["release_scope"] = "none"
            unavailable_lock.write_text(json.dumps(unavailable, indent=2) + "\n", encoding="utf-8")
            run(["git", "init", "--quiet"], cwd=unavailable_repository)
            run(["git", "config", "user.name", "Rust-only consumer proof"], cwd=unavailable_repository)
            run(["git", "config", "user.email", "rust-only@example.invalid"], cwd=unavailable_repository)
            run(["git", "add", "."], cwd=unavailable_repository)
            run(["git", "commit", "--quiet", "-m", "unavailable"], cwd=unavailable_repository)
            unavailable_consumer = temporary_root / "unavailable-consumer"
            unavailable_revision = run(["git", "rev-parse", "HEAD"], cwd=unavailable_repository, capture=True).stdout.strip()
            write_consumer(unavailable_consumer, unavailable_repository.resolve().as_uri(), unavailable_revision, target, flavor, cargo_config)
            unavailable_environment = environment.copy()
            unavailable_environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "unavailable-cache")
            expect_failure(
                consumer_command(target),
                cwd=unavailable_consumer,
                env=unavailable_environment,
                message="unavailable in this release scope",
            )
            expect_failure(
                [str(find_build_script(temporary_root / "target"))],
                cwd=consumer,
                env={**environment, "TARGET": "x86_64-unknown-linux-musl"},
                message="unsupported Cargo target x86_64-unknown-linux-musl",
            )

            environment["PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR"] = str(artifact)
            environment["CARGO_NET_OFFLINE"] = "true"
            run(consumer_command(target, "--locked", "--offline"), cwd=consumer, env=environment)

            cache = temporary_root / "warm-cache"
            cache.mkdir()
            shutil.copy2(source_artifact, cache / asset_name)
            environment.pop("PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR")
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(cache)
            run(consumer_command(target, "--locked", "--offline"), cwd=consumer, env=environment)

            download_cache = temporary_root / "second-download-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(download_cache)
            environment.pop("CARGO_NET_OFFLINE")
            run(consumer_command(target, "--locked"), cwd=consumer, env=environment)

            bad_cache = temporary_root / "bad-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(bad_cache)
            server.payload = b"not the locked artifact"
            expect_failure(
                consumer_command(target, "--locked"),
                cwd=consumer,
                env=environment,
                message="checksum mismatch",
            )

            miss_cache = temporary_root / "miss-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(miss_cache)
            environment["CARGO_NET_OFFLINE"] = "true"
            expect_failure(
                consumer_command(target, "--locked", "--offline"),
                cwd=consumer,
                env=environment,
                message="offline artifact cache miss",
            )

            for field, wrong, expected_message in [
                ("bridge_identity", "wrong-bridge", "wrong bridge identity"),
                ("flavor", "core" if flavor == "native" else "native", "wrong flavor"),
                ("target", next(item for item in write_artifact_lock.SUPPORTED_TARGETS.values() if item != target), "wrong target"),
            ]:
                mismatch = temporary_root / f"mismatch-{field}"
                shutil.copytree(artifact, mismatch)
                manifest_path = mismatch / "manifest.json"
                value = json.loads(manifest_path.read_text(encoding="utf-8"))
                if field == "bridge_identity":
                    value["bridge"]["identity"] = wrong
                else:
                    value["artifact"][field] = wrong
                manifest_path.write_text(json.dumps(value), encoding="utf-8")
                environment["PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR"] = str(mismatch)
                expect_failure(
                    consumer_command(target, "--locked", "--offline"),
                    cwd=consumer,
                    env=environment,
                    message=expected_message,
                )

            if marker.exists():
                invocations = marker.read_text(encoding="utf-8").strip()
                raise ProofError(f"C/C++ compiler sentinel was invoked:\n{invocations}")


def validate_released(repository: str, tag: str, target: str, flavor: str) -> None:
    coordinate(target, flavor)
    if repository != PUBLIC_REPOSITORY:
        raise ProofError(f"released repository must be canonical: {PUBLIC_REPOSITORY}")
    try:
        source_tag(tag)
    except ValueError as error:
        raise ProofError(f"invalid release tag: {tag}") from error


def prove_released_fixture(
    repository: str,
    revision: str,
    target: str,
    flavor: str,
    message: str,
    cargo_config: str | None = None,
    cargo_target_dir: Path | None = None,
) -> None:
    """Exercise released-consumer mechanics against an isolated Git/HTTP fixture.

    The public CLI requires the canonical repository and a release tag. This
    helper intentionally accepts temporary fixture coordinates for negative tests
    of the candidate lock without needing a published release.
    """
    cargo_target, _ = coordinate(target, flavor)
    require_host(target)
    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-released-fixture-") as temporary:
        temporary_root = Path(temporary)
        bootstrap_environment = os.environ.copy()
        bootstrap_environment["CARGO_HOME"] = os.environ.get("CARGO_HOME", str(temporary_root / "cargo-home"))
        if cargo_config is None:
            vendor = temporary_root / "registry"
            cargo_config = run(
                ["cargo", "vendor", "--locked", str(vendor)],
                cwd=ROOT,
                env=bootstrap_environment,
                capture=True,
            ).stdout
        consumer = temporary_root / "consumer"
        write_consumer(consumer, repository, revision, target, flavor, cargo_config)
        marker = temporary_root / "compiler-invocations"
        work = temporary_root / "no-producer-work"
        environment = cold_consumer_environment(marker, temporary_root / "consumer-sentinels", work, cargo_target)
        environment["CARGO_HOME"] = bootstrap_environment["CARGO_HOME"]
        environment["CARGO_TARGET_DIR"] = str(cargo_target_dir or temporary_root / "target")
        environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-cache")
        environment["PULSEBEAM_WEBRTC_SYS_CANDIDATE_LOCK"] = "1"
        metadata = run(
            ["cargo", "metadata", "--format-version=1"],
            cwd=consumer,
            env=environment,
            capture=True,
        ).stdout
        validate_consumer_metadata(metadata, temporary_root)
        expect_failure(consumer_command(target, "--locked"), cwd=consumer, env=environment, message=message)
        if (work / "checkout").exists() or (work / "depot_tools").exists():
            raise ProofError("Rust-only consumer created a Chromium checkout or depot_tools worktree")
        if marker.exists():
            raise ProofError(f"C/C++ compiler sentinel was invoked:\n{marker.read_text(encoding='utf-8').strip()}")


def prove_released(repository: str, tag: str, target: str, flavor: str) -> None:
    cargo_target, asset_name = coordinate(target, flavor)
    require_host(target)
    validate_released(repository, tag, target, flavor)
    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-released-") as temporary:
        temporary_root = Path(temporary)
        vendor = temporary_root / "registry"
        bootstrap_environment = os.environ.copy()
        bootstrap_environment["CARGO_HOME"] = str(temporary_root / "cargo-home")
        cargo_config = run(["cargo", "vendor", "--locked", str(vendor)], cwd=ROOT, env=bootstrap_environment, capture=True).stdout
        consumer = temporary_root / "consumer"
        write_consumer(consumer, repository, tag, target, flavor, cargo_config, tag=True)
        marker = temporary_root / "compiler-invocations"
        environment = cold_consumer_environment(marker, temporary_root / "consumer-sentinels", temporary_root / "no-producer-work", cargo_target)
        environment["CARGO_HOME"] = bootstrap_environment["CARGO_HOME"]
        environment["CARGO_TARGET_DIR"] = str(temporary_root / "target")
        environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-cache")
        metadata = run(
            ["cargo", "metadata", "--format-version=1"],
            cwd=consumer,
            env=environment,
            capture=True,
        ).stdout
        validate_consumer_metadata(metadata, temporary_root)
        run(consumer_command(target, "--locked"), cwd=consumer, env=environment)
        work = Path(environment["WEBRTC_WORK"])
        if (work / "checkout").exists() or (work / "depot_tools").exists():
            raise ProofError("Rust-only consumer created a Chromium checkout or depot_tools worktree")

        # A released archive must be reusable offline after its verified initial download.
        environment["CARGO_NET_OFFLINE"] = "true"
        environment["CARGO_TARGET_DIR"] = str(temporary_root / "warm-target")
        run(consumer_command(target, "--locked", "--offline"), cwd=consumer, env=environment)

        cold_environment = environment.copy()
        cold_environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-offline-cache")
        cold_environment["CARGO_TARGET_DIR"] = str(temporary_root / "cold-offline-target")
        expect_failure(
            consumer_command(target, "--locked", "--offline"),
            cwd=consumer,
            env=cold_environment,
            message="offline release lock cache miss",
        )

        # A corrupt archive is discarded and downloaded again when the network is available.
        cache = Path(environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"])
        os_tag = release_tag(tag, target.split('-')[0])
        (cache / os_tag / asset_name).write_bytes(b"corrupt released artifact")
        environment.pop("CARGO_NET_OFFLINE")
        environment["CARGO_TARGET_DIR"] = str(temporary_root / "repeat-download-target")
        run(consumer_command(target, "--locked"), cwd=consumer, env=environment)

        build_script = find_build_script(temporary_root / "repeat-download-target")
        expect_failure(
            [str(build_script)],
            cwd=consumer,
            env={**environment, "TARGET": "x86_64-unknown-linux-musl"},
            message="unsupported Cargo target x86_64-unknown-linux-musl",
        )
        if marker.exists():
            raise ProofError(f"C/C++ compiler sentinel was invoked:\n{marker.read_text(encoding='utf-8').strip()}")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_subparsers(dest="mode", required=True)
    candidate = mode.add_parser("candidate")
    candidate.add_argument("--artifact", type=Path, required=True)
    candidate.add_argument("--sha256", required=True)
    released = mode.add_parser("released")
    released.add_argument("--repository", required=True)
    released.add_argument("--tag", required=True)
    for selection in (candidate, released):
        selection.add_argument("--target", required=True, choices=sorted(set(write_artifact_lock.SUPPORTED_TARGETS.values())))
        selection.add_argument("--flavor", required=True, choices=("core", "native"))
    args = parser.parse_args()
    try:
        if args.mode == "candidate":
            if not SHA256.fullmatch(args.sha256):
                raise ProofError(f"expected SHA-256 is invalid: {args.sha256}")
            prove_candidate(args.artifact.resolve(), args.sha256, args.target, args.flavor)
        else:
            prove_released(args.repository, args.tag, args.target, args.flavor)
    except (OSError, subprocess.CalledProcessError, ProofError, json.JSONDecodeError) as error:
        print(f"Rust-only consumer proof: {error}", file=sys.stderr)
        return 1
    boundary = "compile/link" if args.target.split("-")[0] in {"android", "ios"} else "runtime"
    print(f"Rust-only {args.mode} consumer {boundary} passed for {args.target} {args.flavor}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
