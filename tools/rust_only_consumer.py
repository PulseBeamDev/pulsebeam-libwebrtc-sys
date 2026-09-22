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
COORDINATES = {
    ("linux-x86_64", "core"): ("x86_64-unknown-linux-gnu", "webrtc-core-linux-x86_64.tar.gz"),
    ("linux-x86_64", "native"): ("x86_64-unknown-linux-gnu", "webrtc-native-linux-x86_64.tar.gz"),
    ("linux-arm64", "core"): ("aarch64-unknown-linux-gnu", "webrtc-core-linux-arm64.tar.gz"),
    ("linux-arm64", "native"): ("aarch64-unknown-linux-gnu", "webrtc-native-linux-arm64.tar.gz"),
}

sys.path.insert(0, str(ROOT))
from tools import write_artifact_lock


class ProofError(Exception):
    pass


def coordinate(target: str, flavor: str) -> tuple[str, str]:
    try:
        return COORDINATES[(target, flavor)]
    except KeyError:
        raise ProofError(f"unsupported Linux selection: {target} {flavor}") from None


def render_consumer_manifest(repository: str, revision: str, target: str, flavor: str) -> str:
    coordinate(target, flavor)
    if not REVISION.fullmatch(revision):
        raise ProofError(f"revision is not an exact 40-character lowercase Git revision: {revision}")
    features = ', features = ["native"]' if flavor == "native" else ""
    return (CONSUMER / "Cargo.toml.in").read_text(encoding="utf-8").replace(
        "@REPOSITORY@", repository
    ).replace("@REVISION@", revision).replace("@FEATURES@", features)


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
        if not source.is_file():
            continue
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, output)
    lock_path = destination / "artifacts.lock.json"
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    assets = []
    for index, entry in enumerate(lock["artifacts"]):
        if entry["artifact_target"] != "linux-x86_64":
            continue
        assets.append({
            "name": entry["asset_name"],
            "sha256": digest if entry["asset_name"] == selected_asset else "1" * 64,
            "native_configuration_sha256": f"{index + 1:064x}",
            "license_inventory_sha256": f"{index + 101:064x}",
            "source_state": "pristine",
            "source_patch_sha256": "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037",
        })
    report = {
        "schema_version": 1,
        "scope": "linux",
        "bridge": {"identity": lock["bridge_identity"]},
        "sources": {"webrtc": {"repository": "fixture", "revision": "fixture", "patch_sha256": "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037"}, "depot_tools": {"repository": "fixture", "revision": "fixture"}},
        "assets": assets,
    }
    report_path = destination / "linux-audit.json"
    report_path.write_text(json.dumps(report), encoding="utf-8")
    lock_path.write_bytes(write_artifact_lock.render(lock_path, None, "consumer-proof", report_path))
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    for entry in lock["artifacts"]:
        if entry["artifact_target"] == "linux-x86_64":
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
    wanted = {"manifest.json", "lib/libwebrtc.a"}
    with tarfile.open(source_artifact, "r:gz") as archive:
        members = {member.name: member for member in archive.getmembers() if member.name in wanted}
        if set(members) != wanted:
            raise ProofError("checked-in host artifact is missing its manifest or native archive")
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


def cold_consumer_environment(marker: Path, sentinel_directory: Path, work: Path, cargo_target: str) -> dict[str, str]:
    sentinel_directory.mkdir()
    sentinels = {}
    for language in ("c", "cxx"):
        sentinel = sentinel_directory / language
        sentinel.write_text(
            "#!/bin/sh\n"
            f"printf '%s\\n' \"$0 $*\" >> {str(marker)!r}\n"
            "echo 'C/C++ compiler invocation is forbidden in the Rust-only consumer proof' >&2\n"
            "exit 97\n",
            encoding="utf-8",
        )
        sentinel.chmod(0o755)
        sentinels[language] = str(sentinel)
    environment = os.environ.copy()
    environment.pop("PULSEBEAM_WEBRTC_SYS_SKIP_LINK", None)
    for tool in ("gn", "ninja"):
        sentinel = sentinel_directory / tool
        sentinel.write_text(
            "#!/bin/sh\n"
            f"printf '%s\\n' \"$0 $*\" >> {str(marker)!r}\n"
            f"echo '{tool} invocation is forbidden in the Rust-only consumer proof' >&2\n"
            "exit 97\n",
            encoding="utf-8",
        )
        sentinel.chmod(0o755)
    environment["PATH"] = str(sentinel_directory) + os.pathsep + environment["PATH"]
    environment["WEBRTC_WORK"] = str(work)
    for compiler, language in (("CC", "c"), ("CXX", "cxx")):
        environment[compiler] = sentinels[language]
        environment[f"HOST_{compiler}"] = sentinels[language]
        environment[f"TARGET_{compiler}"] = sentinels[language]
        for suffix in (cargo_target, cargo_target.replace("-", "_")):
            environment[f"{compiler}_{suffix}"] = sentinels[language]
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


def write_consumer(destination: Path, repository: str, revision: str, target: str, flavor: str, cargo_config: str) -> None:
    shutil.copytree(CONSUMER / "src", destination / "src")
    (destination / "Cargo.toml").write_text(render_consumer_manifest(repository, revision, target, flavor), encoding="utf-8")
    (destination / ".cargo").mkdir()
    (destination / ".cargo/config.toml").write_text(cargo_config, encoding="utf-8")


def require_host(target: str) -> None:
    expected = {"linux-x86_64": {"x86_64", "AMD64"}, "linux-arm64": {"aarch64", "arm64"}}[target]
    if platform.system() != "Linux" or platform.machine() not in expected:
        raise ProofError(f"host does not match requested target {target}")


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
            # The image's default Cargo home can be root-owned; proof cache must be writable and isolated.
            bootstrap_environment["CARGO_HOME"] = str(temporary_root / "cargo-home")
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

            # The release proof starts with no override, artifact cache, or target cache.
            run(["cargo", "run", "--locked"], cwd=consumer, env=environment)
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
                ["cargo", "run"],
                cwd=unavailable_consumer,
                env=unavailable_environment,
                message="unavailable in this release scope",
            )
            expect_failure(
                [str(next((temporary_root / "target" / "debug" / "build").glob("pulsebeam-webrtc-sys-*/build-script-build")))],
                cwd=consumer,
                env={**environment, "TARGET": "x86_64-unknown-linux-musl"},
                message="unsupported Cargo target x86_64-unknown-linux-musl",
            )

            environment["PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR"] = str(artifact)
            environment["CARGO_NET_OFFLINE"] = "true"
            run(["cargo", "run", "--locked", "--offline"], cwd=consumer, env=environment)

            cache = temporary_root / "warm-cache"
            cache.mkdir()
            shutil.copy2(source_artifact, cache / asset_name)
            environment.pop("PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR")
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(cache)
            run(["cargo", "run", "--locked", "--offline"], cwd=consumer, env=environment)

            download_cache = temporary_root / "second-download-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(download_cache)
            environment.pop("CARGO_NET_OFFLINE")
            run(["cargo", "run", "--locked"], cwd=consumer, env=environment)

            bad_cache = temporary_root / "bad-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(bad_cache)
            server.payload = b"not the locked artifact"
            expect_failure(
                ["cargo", "run", "--locked"],
                cwd=consumer,
                env=environment,
                message="checksum mismatch",
            )

            miss_cache = temporary_root / "miss-cache"
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(miss_cache)
            environment["CARGO_NET_OFFLINE"] = "true"
            expect_failure(
                ["cargo", "run", "--locked", "--offline"],
                cwd=consumer,
                env=environment,
                message="offline artifact cache miss",
            )

            for field, wrong, expected_message in [
                ("bridge_identity", "wrong-bridge", "wrong bridge identity"),
                ("flavor", "core" if flavor == "native" else "native", "wrong flavor"),
                ("target", "linux-arm64" if target == "linux-x86_64" else "linux-x86_64", "wrong target"),
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
                    ["cargo", "run", "--locked", "--offline"],
                    cwd=consumer,
                    env=environment,
                    message=expected_message,
                )

            if marker.exists():
                invocations = marker.read_text(encoding="utf-8").strip()
                raise ProofError(f"C/C++ compiler sentinel was invoked:\n{invocations}")


def validate_released(repository: str, revision: str, target: str, flavor: str, lock_path: Path = ROOT / "artifacts.lock.json") -> None:
    _, asset_name = coordinate(target, flavor)
    if repository != PUBLIC_REPOSITORY:
        raise ProofError(f"released repository must be canonical: {PUBLIC_REPOSITORY}")
    if not REVISION.fullmatch(revision):
        raise ProofError(f"revision is not an exact 40-character lowercase Git revision: {revision}")
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    if lock.get("schema_version") != 2 or lock.get("release_scope") not in {"linux", "complete"}:
        raise ProofError("checked-in public lock does not provide a Linux release scope")
    entry = next((item for item in lock.get("artifacts", []) if item.get("asset_name") == asset_name), None)
    if not isinstance(entry, dict) or not isinstance(entry.get("url"), str) or not isinstance(entry.get("sha256"), str):
        raise ProofError(f"selected {asset_name} is unavailable in the checked-in public lock")
    expected_url = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys/releases/download/"
    if not entry["url"].startswith(expected_url) or not entry["url"].endswith("/" + asset_name):
        raise ProofError(f"selected {asset_name} has a noncanonical release URL")
    if not SHA256.fullmatch(entry["sha256"]):
        raise ProofError(f"selected {asset_name} has an invalid digest")


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

    The public CLI validates its canonical repository and checked-in public lock
    before calling any proof mechanics. This helper intentionally accepts the
    temporary fixture coordinates used by focused tests.
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
        metadata = run(
            ["cargo", "metadata", "--format-version=1"],
            cwd=consumer,
            env=environment,
            capture=True,
        ).stdout
        validate_consumer_metadata(metadata, temporary_root)
        expect_failure(["cargo", "run", "--locked"], cwd=consumer, env=environment, message=message)
        if (work / "checkout").exists() or (work / "depot_tools").exists():
            raise ProofError("Rust-only consumer created a Chromium checkout or depot_tools worktree")
        if marker.exists():
            raise ProofError(f"C/C++ compiler sentinel was invoked:\n{marker.read_text(encoding='utf-8').strip()}")


def prove_released(repository: str, revision: str, target: str, flavor: str) -> None:
    cargo_target, asset_name = coordinate(target, flavor)
    require_host(target)
    validate_released(repository, revision, target, flavor)
    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-released-") as temporary:
        temporary_root = Path(temporary)
        vendor = temporary_root / "registry"
        bootstrap_environment = os.environ.copy()
        bootstrap_environment["CARGO_HOME"] = str(temporary_root / "cargo-home")
        cargo_config = run(["cargo", "vendor", "--locked", str(vendor)], cwd=ROOT, env=bootstrap_environment, capture=True).stdout
        consumer = temporary_root / "consumer"
        write_consumer(consumer, repository, revision, target, flavor, cargo_config)
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
        run(["cargo", "run", "--locked"], cwd=consumer, env=environment)
        work = Path(environment["WEBRTC_WORK"])
        if (work / "checkout").exists() or (work / "depot_tools").exists():
            raise ProofError("Rust-only consumer created a Chromium checkout or depot_tools worktree")

        # A released archive must be reusable offline after its verified initial download.
        environment["CARGO_NET_OFFLINE"] = "true"
        environment["CARGO_TARGET_DIR"] = str(temporary_root / "warm-target")
        run(["cargo", "run", "--locked", "--offline"], cwd=consumer, env=environment)

        cold_environment = environment.copy()
        cold_environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-offline-cache")
        cold_environment["CARGO_TARGET_DIR"] = str(temporary_root / "cold-offline-target")
        expect_failure(
            ["cargo", "run", "--locked", "--offline"],
            cwd=consumer,
            env=cold_environment,
            message="offline artifact cache miss",
        )

        # A corrupt archive is discarded and downloaded again when the network is available.
        cache = Path(environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"])
        (cache / asset_name).write_bytes(b"corrupt released artifact")
        environment.pop("CARGO_NET_OFFLINE")
        environment["CARGO_TARGET_DIR"] = str(temporary_root / "repeat-download-target")
        run(["cargo", "run", "--locked"], cwd=consumer, env=environment)

        build_scripts = temporary_root / "repeat-download-target" / "debug" / "build"
        build_script = next(build_scripts.glob("pulsebeam-webrtc-sys-*/build-script-build"), None)
        if build_script is None:
            raise ProofError("released consumer did not produce a pulsebeam-webrtc-sys build script")
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
    released.add_argument("--revision", required=True)
    for selection in (candidate, released):
        selection.add_argument("--target", required=True, choices=("linux-x86_64", "linux-arm64"))
        selection.add_argument("--flavor", required=True, choices=("core", "native"))
    args = parser.parse_args()
    try:
        if args.mode == "candidate":
            if not SHA256.fullmatch(args.sha256):
                raise ProofError(f"expected SHA-256 is invalid: {args.sha256}")
            prove_candidate(args.artifact.resolve(), args.sha256, args.target, args.flavor)
        else:
            prove_released(args.repository, args.revision, args.target, args.flavor)
    except (OSError, subprocess.CalledProcessError, ProofError, json.JSONDecodeError) as error:
        print(f"Rust-only consumer proof: {error}", file=sys.stderr)
        return 1
    print(f"Rust-only {args.mode} consumer passed for {args.target} {args.flavor}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
