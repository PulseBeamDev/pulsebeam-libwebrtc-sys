"""Run the GitHub release boundary locally or from a thin Actions workflow.

Requires gh authentication for live operations. The pure bundle verification and
immutable upload decision remain in tools.linux_release_publication.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

from tools import linux_release_publication as publication

REPOSITORY = publication.CANONICAL_REPOSITORY
# The CLI preflight checks these exact subcommand-specific flags against gh help.
GH_OPTIONS = {
    "api": {"--paginate", "--jq"},
    "release create": {"--repo", "--draft", "--verify-tag", "--title", "--generate-notes"},
    "release upload": {"--repo"},
    "release view": {"--repo", "--json"},
    "release download": {"--repo", "--dir"},
    "release edit": {"--repo", "--draft"},
}


def gh(*arguments: str) -> str:
    return subprocess.run(["gh", *arguments], check=True, text=True, capture_output=True).stdout


def assert_repository(repository: str) -> None:
    if repository != REPOSITORY:
        raise publication.PublicationError(f"publication repository must be {REPOSITORY!r}, got {repository!r}")


def write_plan(plan: dict[str, Any], path: Path) -> None:
    path.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")


def inventory(tag: str) -> tuple[str, list[str]]:
    # gh api releases/tags returns 404 for drafts. Paginate the full inventory.
    raw = gh("api", "--paginate", f"repos/{REPOSITORY}/releases?per_page=100", "--jq", ".[] | {tag_name,draft,assets:[.assets[]|{name}]} | @json")
    matches = [item for line in raw.splitlines() if (item := json.loads(line))["tag_name"] == tag]
    if len(matches) > 1:
        raise publication.PublicationError("ambiguous release identity")
    if not matches:
        return "absent", []
    release = matches[0]
    names = [asset["name"] for asset in release["assets"]]
    return ("draft" if release["draft"] else "published"), names


def download(tag: str, names: list[str], directory: Path) -> None:
    directory.mkdir()
    if names:
        gh("release", "download", tag, "--repo", REPOSITORY, "--dir", str(directory))


def classify(tag: str, repository: str, bundle: Path, output: Path) -> dict[str, Any]:
    assert_repository(repository)  # Before even reading the remote inventory.
    state, names = inventory(tag)
    with tempfile.TemporaryDirectory(prefix="pulsebeam-release-") as temp:
        observed = Path(temp) / "observed"
        if state != "absent":
            download(tag, names, observed)
        plan = publication.plan(bundle, state, observed if state != "absent" else None, names if state != "absent" else None, repository)
    write_plan(plan, output)
    if github_output := os.environ.get("GITHUB_OUTPUT"):
        with Path(github_output).open("a", encoding="utf-8") as stream:
            stream.write(f"finalize={str(plan['finalize']).lower()}\n")
    return plan


def upload(tag: str, bundle: Path, plan_file: Path) -> None:
    plan = json.loads(plan_file.read_text(encoding="utf-8"))
    names = plan["upload"]
    if not plan["finalize"] or not isinstance(names, list) or len(names) != len(set(names)) or not set(names) <= publication.EXPECTED_BUNDLE:
        raise publication.PublicationError("unsafe release upload plan")
    for name in names:
        if not (bundle / name).is_file():
            raise publication.PublicationError(f"missing release asset: {name}")
    if plan["create_draft"]:
        gh("release", "create", tag, "--repo", REPOSITORY, "--draft", "--verify-tag", "--title", tag, "--generate-notes")
    for name in names:
        gh("release", "upload", tag, "--repo", REPOSITORY, str(bundle / name))


def verify(tag: str, bundle: Path, output: Path) -> dict[str, Any]:
    release = json.loads(gh("release", "view", tag, "--repo", REPOSITORY, "--json", "assets"))
    names = [asset["name"] for asset in release["assets"]]
    with tempfile.TemporaryDirectory(prefix="pulsebeam-release-") as temp:
        observed = Path(temp) / "observed"
        download(tag, names, observed)
        plan = publication.plan(bundle, "draft", observed, names, REPOSITORY)
    if plan["upload"]:
        raise publication.PublicationError(f"release draft is incomplete: {plan['upload']}")
    write_plan(plan, output)
    return plan


def advertise(tag: str, plan_file: Path) -> None:
    plan = json.loads(plan_file.read_text(encoding="utf-8"))
    if plan["upload"] or not plan["finalize"] or plan["create_draft"]:
        raise publication.PublicationError("refusing advertisement without a complete verified draft")
    gh("release", "edit", tag, "--repo", REPOSITORY, "--draft=false")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    for action in ("classify", "upload", "verify", "advertise"):
        selected = actions.add_parser(action)
        selected.add_argument("--tag", required=True)
        if action != "advertise": selected.add_argument("--bundle", type=Path, required=True)
        if action == "classify": selected.add_argument("--repository", required=True)
        if action in ("classify", "verify"): selected.add_argument("--output", type=Path, required=True)
        if action in ("upload", "advertise"): selected.add_argument("--plan", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.action == "classify": classify(args.tag, args.repository, args.bundle, args.output)
        elif args.action == "upload": upload(args.tag, args.bundle, args.plan)
        elif args.action == "verify": verify(args.tag, args.bundle, args.output)
        else: advertise(args.tag, args.plan)
    except (OSError, KeyError, ValueError, TypeError, subprocess.CalledProcessError, publication.PublicationError) as error:
        print(f"GitHub release: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
