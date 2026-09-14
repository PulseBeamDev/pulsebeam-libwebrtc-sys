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

sys.path.insert(0, str(ROOT))
from tools import write_artifact_lock


class ProofError(Exception):
    pass


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None, capture: bool = False):
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )


def clean_snapshot(destination: Path, download_url: str, digest: str) -> None:
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
        if entry["artifact_target"] not in {"linux-x86_64", "linux-arm64"}:
            continue
        assets.append({
            "name": entry["asset_name"],
            "sha256": digest if entry["asset_name"] == "webrtc-core-linux-x86_64.tar.gz" else "1" * 64,
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
    entry = next(item for item in lock["artifacts"] if item["asset_name"] == "webrtc-core-linux-x86_64.tar.gz")
    entry["url"] = download_url
    lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
    run(["git", "init", "--quiet"], cwd=destination)
    run(["git", "config", "user.name", "Rust-only consumer proof"], cwd=destination)
    run(["git", "config", "user.email", "rust-only@example.invalid"], cwd=destination)
    run(["git", "add", "."], cwd=destination)
    run(["git", "commit", "--quiet", "-m", "snapshot"], cwd=destination)


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


def cold_consumer_environment(marker: Path, sentinel_directory: Path, work: Path) -> dict[str, str]:
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
    target = "x86_64-unknown-linux-gnu"
    for compiler, language in (("CC", "c"), ("CXX", "cxx")):
        environment[compiler] = sentinels[language]
        environment[f"HOST_{compiler}"] = sentinels[language]
        environment[f"TARGET_{compiler}"] = sentinels[language]
        for suffix in (target, target.replace("-", "_")):
            environment[f"{compiler}_{suffix}"] = sentinels[language]
    return environment


class ArtifactServer:
    def __init__(self, payload: bytes):
        self.payload = payload
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                if self.path != "/webrtc-core-linux-x86_64.tar.gz":
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
        return f"http://127.0.0.1:{self.server.server_port}/webrtc-core-linux-x86_64.tar.gz"

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


def write_consumer(destination: Path, repository: Path, cargo_config: str) -> None:
    shutil.copytree(CONSUMER / "src", destination / "src")
    manifest = (CONSUMER / "Cargo.toml.in").read_text(encoding="utf-8")
    (destination / "Cargo.toml").write_text(
        manifest.replace("@REPOSITORY@", repository.resolve().as_uri()), encoding="utf-8"
    )
    (destination / ".cargo").mkdir()
    (destination / ".cargo/config.toml").write_text(cargo_config, encoding="utf-8")


def prove(source_artifact: Path, expected_digest: str) -> None:
    if platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}:
        raise ProofError("the Rust-only artifact proof requires Linux x86_64")
    if not source_artifact.is_file():
        raise ProofError(f"missing core Linux x86_64 artifact: {source_artifact}")
    actual_digest = hashlib.sha256(source_artifact.read_bytes()).hexdigest()
    if actual_digest != expected_digest:
        raise ProofError(f"core Linux x86_64 checksum mismatch: expected={expected_digest} actual={actual_digest}")

    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-") as temporary:
        temporary_root = Path(temporary)
        payload = source_artifact.read_bytes()
        with ArtifactServer(payload) as server:
            repository = temporary_root / "repository"
            repository.mkdir()
            clean_snapshot(repository, server.url, expected_digest)

            artifact = temporary_root / "artifact"
            extract_artifact(source_artifact, artifact)

            vendor = temporary_root / "registry"
            cargo_config = run(
                ["cargo", "vendor", "--quiet", "--locked", "--offline", str(vendor)],
                cwd=ROOT,
                capture=True,
            ).stdout
            consumer = temporary_root / "consumer"
            write_consumer(consumer, repository, cargo_config)

            marker = temporary_root / "compiler-invocations"
            work = temporary_root / "no-producer-work"
            environment = cold_consumer_environment(marker, temporary_root / "consumer-sentinels", work)
            environment["CARGO_TARGET_DIR"] = str(temporary_root / "target")
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(temporary_root / "cold-cache")

            metadata = run(
                ["cargo", "metadata", "--format-version=1"],
                cwd=consumer,
                env=environment,
                capture=True,
            ).stdout
            metadata_path = temporary_root / "metadata.json"
            metadata_path.write_text(metadata, encoding="utf-8")
            run(
                [sys.executable, str(ROOT / "tools/check_consumer_metadata.py"), str(metadata_path)],
                cwd=ROOT,
            )

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
            write_consumer(unavailable_consumer, unavailable_repository, cargo_config)
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
            shutil.copy2(source_artifact, cache / "webrtc-core-linux-x86_64.tar.gz")
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
                ("flavor", "native", "wrong flavor"),
                ("target", "linux-arm64", "wrong target"),
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()
    try:
        if not re.fullmatch(r"[0-9a-f]{64}", args.sha256):
            raise ProofError(f"core Linux x86_64 expected SHA-256 is invalid: {args.sha256}")
        prove(args.artifact.resolve(), args.sha256)
    except (OSError, subprocess.CalledProcessError, ProofError) as error:
        print(f"Rust-only consumer proof: {error}", file=sys.stderr)
        return 1
    print("Rust-only Git consumer passed cold download, unavailable/unsupported selection, cache, offline, mismatch, and producer-sentinel proofs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
