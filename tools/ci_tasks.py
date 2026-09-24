"""Repository-owned commands shared by local runs and all CI workflows.

Only the workflow graph, artifact transport, credentials and attestation belong
in GitHub Actions. Run individual tasks with `just ci <task> ...`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

from tools import audit_release, linux_release_publication, write_artifact_lock


class TaskError(Exception):
    pass


def podman() -> None:
    subprocess.run(["podman", "--version"], check=True)
    result = subprocess.run(["podman", "info", "--format", "{{.Host.Security.Rootless}}"], check=True, text=True, capture_output=True)
    if result.stdout.strip() != "true":
        raise TaskError("rootless Podman is required")


def revision(value: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise TaskError("revision must be exactly 40 lowercase hexadecimal characters")


def release_tag(tag: str, commit: str, marker: Path) -> None:
    if not write_artifact_lock.TAG.fullmatch(tag) or tag == "v0.5.0":
        raise TaskError("invalid or prohibited tag")
    revision(commit)
    result = subprocess.run(["git", "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}"], check=True, text=True, capture_output=True)
    if result.stdout.strip() != commit:
        raise TaskError("tag does not resolve to the workflow commit")
    marker.write_text(tag, encoding="utf-8")


def require_success(results: list[str]) -> None:
    if not results:
        raise TaskError("at least one job result is required")
    for result in results:
        print(result)
        if "=" not in result or result.split("=", 1)[1] != "success":
            raise TaskError(f"qualification failed: {result}")


def audit(scope: str, archives: Path, checksums: Path, output: Path) -> None:
    names = sorted(path.name for path in archives.glob("webrtc-*.tar.gz") if path.is_file())
    if scope == "linux" and names != sorted(linux_release_publication.ARCHIVES):
        raise TaskError(f"expected exactly the two primary Linux archives; found {names}")
    # Reject unexpected files rather than silently publishing an incomplete
    # or expanded bundle. The audit checks each archive's contents and digest.
    if scope == "complete" and len(names) != 18:
        raise TaskError(f"expected 18 complete-matrix archives; found {len(names)}")
    checksums.write_text("".join(f"{_sha256(archives / name)}  {name}\n" for name in names), encoding="utf-8")
    report = audit_release.audit(archives, checksums, write_artifact_lock.LOCK, scope)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def candidate(flavor: str, target: str, checksums: Path) -> None:
    if flavor not in write_artifact_lock.FLAVORS or target not in write_artifact_lock.SUPPORTED_TARGETS.values():
        raise TaskError("unsupported candidate flavor or target")
    name = f"webrtc-{flavor}-{target}.tar.gz"
    digest = write_artifact_lock.read_checksums(checksums).get(name)
    if digest is None:
        raise TaskError(f"missing checksum for {name}")
    subprocess.run([sys.executable, "-m", "tools.rust_only_consumer", "candidate", "--artifact", str(Path("dist") / name), "--sha256", digest, "--target", target, "--flavor", flavor], check=True)


def install_target(target: str) -> None:
    cargo_targets = {artifact: cargo for cargo, artifact in write_artifact_lock.SUPPORTED_TARGETS.items()}
    if target not in cargo_targets or target.startswith("linux-"):
        raise TaskError(f"unsupported non-Linux Rust target: {target}")
    subprocess.run(["rustup", "target", "add", cargo_targets[target]], check=True)


def asan(image: str, flavor: str) -> None:
    if flavor not in write_artifact_lock.FLAVORS:
        raise TaskError(f"unsupported flavor: {flavor}")
    subprocess.run(["just", "linux-run", image, "just", "build", flavor, "linux-x86_64"], check=True)
    subprocess.run(["just", "linux-run", image, "just", "_runtime-test", flavor, "linux-x86_64", f"dist/webrtc-{flavor}-linux-x86_64.tar.gz"], check=True)


def upgrade(image: str, build: bool) -> None:
    log = Path("upgrade-rehearsal.log")
    commands = (
        [["just", "linux-run", image, "just", "build", "core", "linux-x86_64"]]
        if build else [
            ["just", "linux-run", image, "just", "refresh-cxx"],
            ["just", "linux-run", image, "git", "diff", "--exit-code", "--", "vendor/cxx", "Cargo.toml", "Cargo.lock"],
        ]
    )
    with log.open("a", encoding="utf-8") as output:
        for command in commands:
            with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) as process:
                assert process.stdout is not None
                for line in process.stdout:
                    sys.stdout.write(line)
                    output.write(line)
                if process.wait():
                    raise TaskError(f"upgrade command failed: {command[:4]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="task", required=True)
    commands.add_parser("podman")
    for task in ("revision", "rust-target", "upgrade-pin"):
        commands.add_parser(task).add_argument("value")
    tag = commands.add_parser("release-tag")
    tag.add_argument("tag")
    tag.add_argument("commit")
    tag.add_argument("marker", type=Path)
    commands.add_parser("require-success").add_argument("results", nargs="+")
    audit_parser = commands.add_parser("audit")
    audit_parser.add_argument("scope", choices=("linux", "complete"))
    audit_parser.add_argument("archives", type=Path)
    audit_parser.add_argument("checksums", type=Path)
    audit_parser.add_argument("output", type=Path)
    candidate_parser = commands.add_parser("candidate")
    candidate_parser.add_argument("flavor")
    candidate_parser.add_argument("target")
    candidate_parser.add_argument("checksums", type=Path)
    prepare = commands.add_parser("release-prepare")
    prepare.add_argument("archives", type=Path)
    prepare.add_argument("checksums", type=Path)
    prepare.add_argument("audit", type=Path)
    prepare.add_argument("marker", type=Path)
    prepare.add_argument("output", type=Path)
    for task in ("upgrade-refresh", "upgrade-build"):
        commands.add_parser(task).add_argument("image")
    asan_parser = commands.add_parser("asan")
    asan_parser.add_argument("image")
    asan_parser.add_argument("flavor")
    args = parser.parse_args()
    try:
        if args.task == "podman": podman()
        elif args.task == "revision": revision(args.value)
        elif args.task == "rust-target": install_target(args.value)
        elif args.task == "upgrade-pin": subprocess.run([sys.executable, "tools/select_upgrade_pin.py", args.value], check=True)
        elif args.task == "release-tag": release_tag(args.tag, args.commit, args.marker)
        elif args.task == "require-success": require_success(args.results)
        elif args.task == "audit": audit(args.scope, args.archives, args.checksums, args.output)
        elif args.task == "candidate": candidate(args.flavor, args.target, args.checksums)
        elif args.task == "release-prepare": linux_release_publication.prepare(args.archives, args.checksums, args.audit, args.marker.read_text(encoding="utf-8"), args.output)
        elif args.task == "asan": asan(args.image, args.flavor)
        elif args.task == "upgrade-refresh": upgrade(args.image, False)
        elif args.task == "upgrade-build": upgrade(args.image, True)
    except (OSError, ValueError, TaskError, subprocess.CalledProcessError, audit_release.AuditError, linux_release_publication.PublicationError, write_artifact_lock.LockError) as error:
        print(f"CI task: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
