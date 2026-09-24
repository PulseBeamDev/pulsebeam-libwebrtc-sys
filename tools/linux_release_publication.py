#!/usr/bin/env python3
"""Prepare and immutably plan publication of one closed Linux release bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime
from pathlib import Path
import shutil
import sys
from typing import Any

from tools import audit_release, write_artifact_lock


ROOT = Path(__file__).resolve().parents[1]
ARCHIVES = frozenset({
    "webrtc-core-linux-x86_64.tar.gz",
    "webrtc-native-linux-x86_64.tar.gz",
})
METADATA = frozenset({"SHA256SUMS", "LINUX-RELEASE-MANIFEST.json", "artifacts.lock.json", "PULSEBEAM-APACHE-2.0.txt"})
EXPECTED_BUNDLE = ARCHIVES | METADATA
CANONICAL_REPOSITORY = write_artifact_lock.REPOSITORY


class PublicationError(Exception):
    pass


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bundle_inventory(bundle: Path) -> dict[str, str]:
    found = {path.name for path in bundle.iterdir() if path.is_file()}
    if found != EXPECTED_BUNDLE:
        raise PublicationError(f"closed Linux bundle mismatch; missing={sorted(EXPECTED_BUNDLE - found)}, extra={sorted(found - EXPECTED_BUNDLE)}")
    return {name: sha256(bundle / name) for name in sorted(EXPECTED_BUNDLE)}


def prepare(archives: Path, checksums: Path, audit: Path, tag: str, output: Path) -> dict[str, str]:
    if output.exists():
        raise PublicationError(f"publication output already exists: {output}")
    # Re-audit rather than trusting an adjacent report, then require byte-for-byte
    # agreement so the lock, checksum index, and manifest have one authority.
    report = audit_release.audit(archives, checksums, ROOT / "artifacts.lock.json", "linux")
    encoded_report = json.dumps(report, indent=2) + "\n"
    if audit.read_text(encoding="utf-8") != encoded_report:
        raise PublicationError("Linux audit report does not match the closed archive audit")
    lock = write_artifact_lock.render(ROOT / "artifacts.lock.json", None, tag, audit)
    output.mkdir()
    for name in ARCHIVES:
        shutil.copyfile(archives / name, output / name)
    shutil.copyfile(checksums, output / "SHA256SUMS")
    shutil.copyfile(audit, output / "LINUX-RELEASE-MANIFEST.json")
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


def prior_automatic_success(current: object, pages: object, repository: str, sha: str, run_id: int) -> str:
    if repository != CANONICAL_REPOSITORY or not isinstance(run_id, int) or run_id <= 0:
        raise PublicationError("invalid publication repository or run ID")
    if not isinstance(current, dict):
        raise PublicationError("invalid dispatch run")
    def field(run: dict, key: str, expected: object) -> None:
        if run.get(key) != expected:
            raise PublicationError(f"run {key} does not match dispatch")
    field(current, "id", run_id)
    field(current, "head_sha", sha)
    field(current, "event", "workflow_dispatch")
    if not isinstance(current.get("repository"), dict):
        raise PublicationError("missing run repository")
    field(current["repository"], "full_name", repository)
    workflow_id = current.get("workflow_id")
    if not isinstance(workflow_id, int) or workflow_id <= 0 or current.get("path", "").split("@", 1)[0] != ".github/workflows/linux.yml":
        raise PublicationError("dispatch is not the Linux workflow")
    def timestamp(value: object) -> datetime:
        if not isinstance(value, str):
            raise PublicationError("missing run timestamp")
        try:
            parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
            if parsed.tzinfo is None:
                raise ValueError("timezone missing")
            return parsed
        except ValueError as error:
            raise PublicationError("invalid run timestamp") from error
    cutoff = timestamp(current.get("created_at"))
    if not isinstance(pages, list) or not pages:
        raise PublicationError("missing workflow run pages")
    qualifying = []
    for page in pages:
        if not isinstance(page, dict) or not isinstance(page.get("workflow_runs"), list):
            raise PublicationError("invalid workflow run page")
        for run in page["workflow_runs"]:
            if not isinstance(run, dict):
                raise PublicationError("invalid workflow run")
            if run.get("head_sha") != sha:
                raise PublicationError("workflow run query returned a different SHA")
            if run.get("workflow_id") != workflow_id or not isinstance(run.get("repository"), dict) or run["repository"].get("full_name") != repository:
                raise PublicationError("workflow run query returned a different workflow or repository")
            updated = timestamp(run.get("updated_at"))
            if run.get("id") != run_id and run.get("event") in {"push", "pull_request"} and run.get("status") == "completed" and run.get("conclusion") == "success" and updated < cutoff:
                url = run.get("html_url")
                if not isinstance(url, str) or not url.startswith(f"https://github.com/{repository}/actions/runs/"):
                    raise PublicationError("invalid qualifying run URL")
                qualifying.append(url)
    if not qualifying:
        raise PublicationError("no earlier completed automatic qualification for this SHA")
    return qualifying[0]


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
    qualify_parser = commands.add_parser("qualify")
    qualify_parser.add_argument("--current", type=Path, required=True)
    qualify_parser.add_argument("--runs", type=Path, required=True)
    qualify_parser.add_argument("--repository", required=True)
    qualify_parser.add_argument("--sha", required=True)
    qualify_parser.add_argument("--run-id", type=int, required=True)
    args = parser.parse_args()
    try:
        if args.command == "qualify":
            print(prior_automatic_success(json.loads(args.current.read_text()), json.loads(args.runs.read_text()), args.repository, args.sha, args.run_id))
            return 0
        remote_names = None if args.command == "prepare" or args.remote_names is None else json.loads(args.remote_names.read_text(encoding="utf-8"))
        result = prepare(args.archives, args.checksums, args.audit, args.tag, args.output) if args.command == "prepare" else plan(args.bundle, args.release_state, args.observed, remote_names, args.repository)
        if args.command == "plan":
            args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        else:
            print(json.dumps(result, sort_keys=True))
    except (OSError, ValueError, audit_release.AuditError, write_artifact_lock.LockError, PublicationError) as error:
        print(f"Linux publication: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
