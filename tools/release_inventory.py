"""Closed OS release inventories and canonical source/derived tag identities."""
from __future__ import annotations

import re

OS_TARGETS = {
    "linux": frozenset({"linux-x86_64", "linux-arm64"}),
    "macos": frozenset({"macos-x86_64", "macos-arm64"}),
    "windows": frozenset({"windows-x86_64"}),
    "android": frozenset({"android-x86_64", "android-arm64-v8a"}),
    "ios": frozenset({"ios-arm64", "ios-simulator-arm64"}),
}
# Cargo's canonical version spelling, including optional prerelease/build metadata.
SOURCE_TAG = re.compile(r"^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$")


def source_tag(tag: str) -> str:
    # An OS suffix is reserved, even though it could otherwise be a prerelease.
    if not SOURCE_TAG.fullmatch(tag) or any(tag.endswith(f"-{os}") for os in OS_TARGETS):
        raise ValueError(f"not a canonical crate-version source tag: {tag!r}")
    return tag


def release_tag(source: str, scope: str) -> str:
    if scope not in OS_TARGETS:
        raise ValueError(f"not an OS release scope: {scope!r}")
    return f"{source_tag(source)}-{scope}"


def validate_release_tag(tag: str, scope: str) -> str:
    suffix = f"-{scope}"
    if scope not in OS_TARGETS or not tag.endswith(suffix):
        raise ValueError(f"release tag {tag!r} does not match OS scope {scope!r}")
    source = tag[:-len(suffix)]
    if release_tag(source, scope) != tag:
        raise ValueError(f"invalid OS release tag: {tag!r}")
    return source


def archive_names(scope: str) -> frozenset[str]:
    return frozenset(f"webrtc-{flavor}-{target}.tar.gz" for target in OS_TARGETS[scope] for flavor in ("core", "native"))


def validate_tag_identity(source_commit: str, derived_commit: str | None) -> None:
    if not isinstance(source_commit, str) or not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise ValueError("source tag must resolve to an exact commit")
    if derived_commit is not None and derived_commit != source_commit:
        raise ValueError("immutable derived tag conflict: source and OS tag identify different commits")
