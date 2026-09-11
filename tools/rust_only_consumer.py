#!/usr/bin/env python3
"""Prove a clean Git consumer builds without invoking a C/C++ compiler."""

from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/webrtc-core-linux-x86_64.tar.gz"
CONSUMER = ROOT / "consumer/rust-only"


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


def clean_snapshot(destination: Path, download_url: str) -> None:
    listing = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
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
    entry = next(
        item
        for item in lock["artifacts"]
        if item["cargo_target"] == "x86_64-unknown-linux-gnu"
        and item["flavor"] == "core"
    )
    entry["url"] = download_url
    lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
    run(["git", "init", "--quiet"], cwd=destination)
    run(["git", "config", "user.name", "Rust-only consumer proof"], cwd=destination)
    run(["git", "config", "user.email", "rust-only@example.invalid"], cwd=destination)
    run(["git", "add", "."], cwd=destination)
    run(["git", "commit", "--quiet", "-m", "snapshot"], cwd=destination)


def extract_artifact(destination: Path) -> None:
    wanted = {"manifest.json", "lib/libwebrtc.a"}
    with tarfile.open(FIXTURE, "r:gz") as archive:
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


def compiler_environment(marker: Path, sentinel_directory: Path) -> dict[str, str]:
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


def prove() -> None:
    if platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}:
        raise ProofError("the checked-in Rust-only artifact fixture requires Linux x86_64")
    if not FIXTURE.is_file():
        raise ProofError(f"missing checked-in host artifact: {FIXTURE}")

    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-") as temporary:
        temporary_root = Path(temporary)
        with ArtifactServer(FIXTURE.read_bytes()) as server:
            repository = temporary_root / "repository"
            repository.mkdir()
            clean_snapshot(repository, server.url)

            artifact = temporary_root / "artifact"
            extract_artifact(artifact)

            consumer = temporary_root / "consumer"
            shutil.copytree(CONSUMER / "src", consumer / "src")
            repository_url = repository.resolve().as_uri()
            manifest = (CONSUMER / "Cargo.toml.in").read_text(encoding="utf-8")
            (consumer / "Cargo.toml").write_text(
                manifest.replace("@REPOSITORY@", repository_url), encoding="utf-8"
            )

            vendor = temporary_root / "registry"
            cargo_config = run(
                ["cargo", "vendor", "--quiet", "--locked", "--offline", str(vendor)],
                cwd=ROOT,
                capture=True,
            ).stdout
            (consumer / ".cargo").mkdir()
            (consumer / ".cargo/config.toml").write_text(cargo_config, encoding="utf-8")

            marker = temporary_root / "compiler-invocations"
            environment = compiler_environment(marker, temporary_root / "compiler-sentinels")
            environment["CARGO_TARGET_DIR"] = str(temporary_root / "target")
            environment["PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR"] = str(artifact)

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

            environment["CARGO_NET_OFFLINE"] = "true"
            run(["cargo", "run", "--locked", "--offline"], cwd=consumer, env=environment)

            cache = temporary_root / "warm-cache"
            cache.mkdir()
            shutil.copy2(FIXTURE, cache / "webrtc-core-linux-x86_64.tar.gz")
            environment.pop("PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR")
            environment["PULSEBEAM_WEBRTC_SYS_CACHE_DIR"] = str(cache)
            run(["cargo", "run", "--locked", "--offline"], cwd=consumer, env=environment)

            download_cache = temporary_root / "download-cache"
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
    parser.parse_args()
    try:
        prove()
    except (OSError, subprocess.CalledProcessError, ProofError) as error:
        print(f"Rust-only consumer proof: {error}", file=sys.stderr)
        return 1
    print("Rust-only Git consumer passed override, cache, download, offline, and mismatch proofs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
