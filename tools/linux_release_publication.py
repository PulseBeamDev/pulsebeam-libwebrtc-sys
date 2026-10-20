#!/usr/bin/env python3
"""Prepare and immutably plan publication of one closed OS release bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
from typing import Any

from tools import audit_release, write_artifact_lock
from tools.release_inventory import OS_TARGETS, archive_names, validate_release_tag


ROOT = Path(__file__).resolve().parents[1]
def expected_bundle(scope: str) -> frozenset[str]:
    return archive_names(scope) | {"SHA256SUMS", f"{scope.upper()}-RELEASE-MANIFEST.json", "artifacts.lock.json", "PULSEBEAM-APACHE-2.0.txt"}


def bundle_scope(bundle: Path) -> str:
    scope = write_artifact_lock.template(bundle / "artifacts.lock.json")["release_scope"]
    if scope not in OS_TARGETS:
        raise PublicationError("publication bundle must identify exactly one OS")
    return scope

CANONICAL_REPOSITORY = write_artifact_lock.REPOSITORY


class PublicationError(Exception):
    pass


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_bundle_tag(bundle: Path, tag: str) -> None:
    scope = bundle_scope(bundle)
    validate_release_tag(tag, scope)
    lock = write_artifact_lock.template(bundle / "artifacts.lock.json")
    for entry in lock["artifacts"]:
        available = entry["artifact_target"] in OS_TARGETS[scope]
        expected = f"{write_artifact_lock.BASE_URL}/{tag}/{entry['asset_name']}" if available else None
        if entry["url"] != expected or (available and not write_artifact_lock.SHA256.fullmatch(entry["sha256"] or "")):
            raise PublicationError("bundle lock has missing, foreign or tag-inconsistent selections")


def bundle_inventory(bundle: Path) -> dict[str, str]:
    expected = expected_bundle(bundle_scope(bundle))
    found = {path.name for path in bundle.iterdir()}
    if found != expected or any(not path.is_file() or path.is_symlink() for path in bundle.iterdir()):
        raise PublicationError(f"closed OS bundle mismatch; missing={sorted(expected - found)}, extra={sorted(found - expected)}")
    return {name: sha256(bundle / name) for name in sorted(expected)}


def prepare(archives: Path, checksums: Path, audit: Path, tag: str, output: Path) -> dict[str, str]:
    if output.exists():
        raise PublicationError(f"publication output already exists: {output}")
    # Re-audit rather than trusting an adjacent report, then require byte-for-byte
    # agreement so the lock, checksum index, and manifest have one authority.
    scope = json.loads(audit.read_text(encoding="utf-8"))["scope"]
    if scope not in OS_TARGETS:
        raise PublicationError("publication requires an OS-scoped audit")
    validate_release_tag(tag, scope)
    report = audit_release.audit(archives, checksums, ROOT / "artifacts.lock.json", scope)
    encoded_report = json.dumps(report, indent=2) + "\n"
    if audit.read_text(encoding="utf-8") != encoded_report:
        raise PublicationError("OS audit report does not match the closed archive audit")
    lock = write_artifact_lock.render(ROOT / "artifacts.lock.json", None, tag, audit)
    output.mkdir()
    for name in archive_names(scope):
        shutil.copyfile(archives / name, output / name)
    shutil.copyfile(checksums, output / "SHA256SUMS")
    shutil.copyfile(audit, output / f"{scope.upper()}-RELEASE-MANIFEST.json")
    (output / "artifacts.lock.json").write_bytes(lock)
    shutil.copyfile(ROOT / "LICENSE", output / "PULSEBEAM-APACHE-2.0.txt")
    return bundle_inventory(output)


def plan(bundle: Path, release_state: str, observed: Path | None, remote_names: list[str] | None = None, repository: str = CANONICAL_REPOSITORY) -> dict[str, Any]:
    if repository != CANONICAL_REPOSITORY:
        raise PublicationError(
            f"publication repository must be {CANONICAL_REPOSITORY!r}, got {repository!r}"
        )
    intended = bundle_inventory(bundle)
    actual = {} if observed is None else {path.name: sha256(path) for path in observed.iterdir() if path.is_file()}
    if remote_names is not None:
        if not all(isinstance(name, str) for name in remote_names) or len(remote_names) != len(set(remote_names)):
            raise PublicationError("ambiguous remote inventory: duplicate or invalid asset name")
        if set(remote_names) != set(actual):
            raise PublicationError(f"remote inventory/download mismatch; inventory={sorted(remote_names)}, downloaded={sorted(actual)}")
    present = sorted(actual)
    verified = sorted(name for name in intended if actual.get(name) == intended[name])
    missing = sorted(name for name in intended if name not in actual)
    conflicting = sorted(name for name in actual if name not in intended or actual[name] != intended.get(name))
    diagnostics = {"present": present, "verified": verified, "missing": missing, "conflicting": conflicting}
    if release_state not in {"absent", "draft", "published"}:
        raise PublicationError(f"ambiguous release state: {release_state}")
    if release_state == "absent" and actual:
        raise PublicationError(f"absent release has observed assets: {json.dumps(diagnostics, sort_keys=True)}")
    if conflicting:
        raise PublicationError(f"immutable release conflict: {json.dumps(diagnostics, sort_keys=True)}")
    if release_state == "published" and missing:
        raise PublicationError(f"published release is incomplete: {json.dumps(diagnostics, sort_keys=True)}")
    return {
        "create_draft": release_state == "absent",
        "upload": missing,
        "finalize": release_state != "published",
        "diagnostics": diagnostics,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--archives", type=Path, required=True)
    prepare_parser.add_argument("--checksums", type=Path, required=True)
    prepare_parser.add_argument("--audit", type=Path, required=True)
    prepare_parser.add_argument("--tag", required=True)
    prepare_parser.add_argument("--output", type=Path, required=True)
    plan_parser = commands.add_parser("plan")
    plan_parser.add_argument("--bundle", type=Path, required=True)
    plan_parser.add_argument("--release-state", required=True)
    plan_parser.add_argument("--observed", type=Path)
    plan_parser.add_argument("--remote-names", type=Path)
    plan_parser.add_argument("--repository", default=CANONICAL_REPOSITORY)
    plan_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        remote_names = None if args.command == "prepare" or args.remote_names is None else json.loads(args.remote_names.read_text(encoding="utf-8"))
        result = prepare(args.archives, args.checksums, args.audit, args.tag, args.output) if args.command == "prepare" else plan(args.bundle, args.release_state, args.observed, remote_names, args.repository)
        if args.command == "plan":
            args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        else:
            print(json.dumps(result, sort_keys=True))
    except (OSError, ValueError, audit_release.AuditError, write_artifact_lock.LockError, PublicationError) as error:
        print(f"OS publication: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
