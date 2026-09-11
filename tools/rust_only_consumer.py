#!/usr/bin/env python3
"""Prove a clean Git consumer builds without invoking a C/C++ compiler."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile


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


def clean_snapshot(destination: Path) -> None:
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


def prove() -> None:
    if platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}:
        raise ProofError("the checked-in Rust-only artifact fixture requires Linux x86_64")
    if not FIXTURE.is_file():
        raise ProofError(f"missing checked-in host artifact: {FIXTURE}")

    with tempfile.TemporaryDirectory(prefix="pulsebeam-rust-only-") as temporary:
        temporary_root = Path(temporary)
        repository = temporary_root / "repository"
        repository.mkdir()
        clean_snapshot(repository)

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
            ["cargo", "vendor", "--locked", "--offline", str(vendor)],
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
    print("Rust-only Git consumer built and ran without invoking a C/C++ compiler")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
