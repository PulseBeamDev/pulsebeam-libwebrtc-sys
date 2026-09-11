#!/usr/bin/env python3
"""Record an immutable GitHub release's 18 artifact hashes in the consumer lock."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "artifacts.lock.json"
TAG = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
BASE_URL = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys/releases/download"


class LockError(Exception):
    pass


def read_checksums(path: Path) -> dict[str, str]:
    checksums: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2 or not SHA256.fullmatch(parts[0]):
            raise LockError(f"malformed SHA256SUMS line: {line!r}")
        name = parts[1].lstrip("*")
        if name in checksums:
            raise LockError(f"duplicate checksum: {name}")
        checksums[name] = parts[0]
    return checksums


def render(lock_path: Path, checksums_path: Path, tag: str) -> bytes:
    if not TAG.fullmatch(tag):
        raise LockError(f"invalid release tag: {tag!r}")
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    expected = {entry["asset_name"] for entry in lock["artifacts"]}
    checksums = read_checksums(checksums_path)
    if checksums.keys() != expected:
        missing = sorted(expected - checksums.keys())
        extra = sorted(checksums.keys() - expected)
        raise LockError(f"checksum set mismatch; missing={missing}, extra={extra}")
    lock["release_ready"] = True
    for entry in lock["artifacts"]:
        name = entry["asset_name"]
        entry["url"] = f"{BASE_URL}/{tag}/{name}"
        entry["sha256"] = checksums[name]
    return (json.dumps(lock, indent=2) + "\n").encode()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--checksums", type=Path, required=True)
    parser.add_argument("--lock", type=Path, default=LOCK)
    parser.add_argument("--output", type=Path, default=LOCK)
    args = parser.parse_args()
    try:
        output = render(args.lock, args.checksums, args.tag)
        args.output.write_bytes(output)
    except (OSError, KeyError, TypeError, json.JSONDecodeError, LockError) as error:
        print(f"artifact lock: {error}", file=sys.stderr)
        return 1
    print(f"wrote {args.output} ({hashlib.sha256(output).hexdigest()})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
