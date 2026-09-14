#!/usr/bin/env python3
"""Render immutable consumer metadata from one closed release audit."""
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
REPOSITORY = "PulseBeamDev/pulsebeam-libwebrtc-sys"
BASE_URL = f"https://github.com/{REPOSITORY}/releases/download"
LINUX_TARGETS = frozenset({"linux-x86_64", "linux-arm64"})
SUPPORTED_TARGETS = {
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-unknown-linux-gnu": "linux-arm64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-apple-darwin": "macos-arm64",
    "x86_64-linux-android": "android-x86_64",
    "aarch64-linux-android": "android-arm64-v8a",
    "aarch64-apple-ios": "ios-arm64",
    "aarch64-apple-ios-sim": "ios-simulator-arm64",
}
FLAVORS = frozenset({"core", "native"})

class LockError(Exception): pass

def read_checksums(path: Path) -> dict[str, str]:
    checksums = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2 or not SHA256.fullmatch(parts[0]): raise LockError(f"malformed SHA256SUMS line: {line!r}")
        name = parts[1].lstrip("*")
        if name in checksums: raise LockError(f"duplicate checksum: {name}")
        checksums[name] = parts[0]
    return checksums

def template(lock_path: Path) -> dict:
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    if lock.get("schema_version") != 2 or set(lock) != {"schema_version", "release_scope", "bridge_identity", "artifacts"}: raise LockError("lock template must be schema 2 with explicit release_scope")
    if lock["release_scope"] not in {"none", "linux", "complete"}: raise LockError("invalid lock release scope")
    expected = {(cargo_target, flavor) for cargo_target in SUPPORTED_TARGETS for flavor in FLAVORS}
    selections = set()
    for item in lock["artifacts"]:
        if not isinstance(item, dict) or set(item) != {"cargo_target", "artifact_target", "flavor", "asset_name", "url", "sha256"}: raise LockError("invalid lock template selection")
        cargo_target, flavor = item["cargo_target"], item["flavor"]
        artifact_target = SUPPORTED_TARGETS.get(cargo_target)
        if flavor not in FLAVORS or artifact_target is None or item["artifact_target"] != artifact_target: raise LockError("lock template contains unsupported or noncanonical selection")
        expected_asset = f"webrtc-{flavor}-{artifact_target}.tar.gz"
        if item["asset_name"] != expected_asset: raise LockError("lock template contains noncanonical asset name")
        if (cargo_target, flavor) in selections: raise LockError("lock template contains duplicate selection")
        selections.add((cargo_target, flavor))
        if (item.get("url") is None) != (item.get("sha256") is None): raise LockError(f"half-populated lock selection: {item.get('asset_name')}")
    if selections != expected: raise LockError("lock template must contain the exact supported selection universe")
    return lock

def expected_assets(lock: dict, scope: str) -> dict[str, dict]:
    return {item["asset_name"]: item for item in lock["artifacts"] if scope == "complete" or item["artifact_target"] in LINUX_TARGETS}

def validate_report(report: object, lock: dict) -> tuple[str, dict[str, str]]:
    if not isinstance(report, dict) or set(report) != {"schema_version", "scope", "bridge", "sources", "assets"}: raise LockError("invalid audit report shape")
    if report["schema_version"] != 1 or report["scope"] not in {"linux", "complete"}: raise LockError("audit report must be schema 1 with linux or complete scope")
    if not isinstance(report["bridge"], dict) or report["bridge"].get("identity") != lock["bridge_identity"]: raise LockError("wrong audit bridge identity")
    sources = report["sources"]; webrtc = sources.get("webrtc") if isinstance(sources, dict) else None; depot_tools = sources.get("depot_tools") if isinstance(sources, dict) else None
    if not isinstance(webrtc, dict) or not all(isinstance(webrtc.get(key), str) and webrtc[key] for key in ("repository", "revision", "patch_sha256")) or not isinstance(depot_tools, dict) or not all(isinstance(depot_tools.get(key), str) and depot_tools[key] for key in ("repository", "revision")): raise LockError("missing audit source identity")
    expected = expected_assets(lock, report["scope"]); assets = report["assets"]
    if not isinstance(assets, list): raise LockError("audit assets must be a list")
    seen = {}
    configurations = set()
    for asset in assets:
        required = {"name", "sha256", "native_configuration_sha256", "license_inventory_sha256", "source_state", "source_patch_sha256"}
        if not isinstance(asset, dict) or set(asset) != required: raise LockError("invalid audit asset evidence")
        name, digest = asset["name"], asset["sha256"]
        if name in seen: raise LockError(f"duplicate audit asset: {name}")
        if name not in expected: raise LockError(f"unexpected audit asset: {name}")
        if not isinstance(digest, str) or not SHA256.fullmatch(digest): raise LockError(f"invalid audit checksum: {name}")
        for field in ("native_configuration_sha256", "license_inventory_sha256", "source_patch_sha256"):
            if not isinstance(asset[field], str) or not SHA256.fullmatch(asset[field]): raise LockError(f"missing or invalid {field}: {name}")
        expected_state = "applied" if name.startswith("webrtc-core-ios-") else "pristine"
        if asset["source_patch_sha256"] != webrtc["patch_sha256"] or asset["source_state"] != expected_state: raise LockError(f"inconsistent audit source evidence: {name}")
        if asset["native_configuration_sha256"] in configurations: raise LockError(f"duplicate native configuration evidence: {name}")
        configurations.add(asset["native_configuration_sha256"])
        seen[name] = digest
    if seen.keys() != expected.keys(): raise LockError(f"audit asset set mismatch; missing={sorted(expected.keys() - seen.keys())}, extra={sorted(seen.keys() - expected.keys())}")
    return report["scope"], seen

def render(lock_path: Path, checksums_path: Path | None, tag: str, audit_path: Path | None = None) -> bytes:
    if not TAG.fullmatch(tag): raise LockError(f"invalid release tag: {tag!r}")
    lock = template(lock_path)
    if audit_path is not None:
        scope, checksums = validate_report(json.loads(audit_path.read_text(encoding="utf-8")), lock)
    else:
        if checksums_path is None: raise LockError("provide --audit-report or --checksums")
        scope, checksums = "complete", read_checksums(checksums_path)
        if checksums.keys() != expected_assets(lock, scope).keys(): raise LockError("checksum set mismatch for complete release")
    lock["release_scope"] = scope
    for item in lock["artifacts"]:
        name = item["asset_name"]
        item["url"] = f"{BASE_URL}/{tag}/{name}" if name in checksums else None
        item["sha256"] = checksums.get(name)
    return (json.dumps(lock, indent=2) + "\n").encode()

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True); parser.add_argument("--checksums", type=Path); parser.add_argument("--audit-report", type=Path); parser.add_argument("--lock", type=Path, default=LOCK); parser.add_argument("--output", type=Path, default=LOCK)
    args = parser.parse_args()
    try:
        output = render(args.lock, args.checksums, args.tag, args.audit_report); args.output.write_bytes(output)
    except (OSError, KeyError, TypeError, json.JSONDecodeError, LockError) as error:
        print(f"artifact lock: {error}", file=sys.stderr); return 1
    print(f"wrote {args.output} ({hashlib.sha256(output).hexdigest()})"); return 0

if __name__ == "__main__": raise SystemExit(main())
