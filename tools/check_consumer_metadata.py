#!/usr/bin/env python3
"""Reject native bridge compilation in downstream Cargo metadata."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


class MetadataError(Exception):
    pass


def _package_by_id(metadata: dict) -> dict[str, dict]:
    packages = metadata.get("packages")
    if not isinstance(packages, list):
        raise MetadataError("Cargo metadata has no package list")
    return {package["id"]: package for package in packages}


def _reachable(resolve: dict, root: str) -> set[str]:
    nodes = {
        node["id"]: [dependency["pkg"] for dependency in node.get("deps", [])]
        for node in resolve.get("nodes", [])
    }
    pending = [root]
    reached: set[str] = set()
    while pending:
        package_id = pending.pop()
        if package_id in reached:
            continue
        reached.add(package_id)
        pending.extend(nodes.get(package_id, []))
    return reached


def validate(metadata: dict) -> None:
    packages = _package_by_id(metadata)
    resolve = metadata.get("resolve")
    if not isinstance(resolve, dict):
        raise MetadataError("Cargo metadata has no resolved dependency graph")

    consumer = [
        package
        for package in packages.values()
        if package.get("name") == "pulsebeam-webrtc-sys-rust-only-consumer"
    ]
    if len(consumer) != 1:
        raise MetadataError("expected exactly one Rust-only consumer package")
    consumer_graph = _reachable(resolve, consumer[0]["id"])

    sys_packages = [
        packages[package_id]
        for package_id in consumer_graph
        if packages[package_id].get("name") == "pulsebeam-webrtc-sys"
    ]
    if len(sys_packages) != 1:
        raise MetadataError("consumer does not resolve exactly one pulsebeam-webrtc-sys package")
    bridge_graph = _reachable(resolve, sys_packages[0]["id"])

    cxx_packages = [
        packages[package_id]
        for package_id in bridge_graph
        if packages[package_id].get("name") == "cxx"
    ]
    if len(cxx_packages) != 1:
        raise MetadataError("bridge graph does not resolve exactly one local cxx package")
    cxx = cxx_packages[0]
    if cxx.get("source") != sys_packages[0].get("source"):
        raise MetadataError("bridge graph resolved a registry cxx package instead of the local import")
    if Path(cxx.get("manifest_path", "")).parts[-3:] != ("vendor", "cxx", "Cargo.toml"):
        raise MetadataError("bridge graph cxx package is not the repository-local vendor/cxx import")
    if any("custom-build" in target.get("kind", []) for target in cxx.get("targets", [])):
        raise MetadataError("local cxx package has a forbidden custom-build target")

    forbidden = sorted(
        {
            packages[package_id]["name"]
            for package_id in bridge_graph
            if packages[package_id].get("name") in {"cc", "cxx-build"}
        }
    )
    if forbidden:
        raise MetadataError(
            "bridge graph contains forbidden native build packages: " + ", ".join(forbidden)
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("metadata", type=Path, help="Cargo metadata JSON to inspect")
    args = parser.parse_args()
    try:
        validate(json.loads(args.metadata.read_text(encoding="utf-8")))
    except (OSError, json.JSONDecodeError, MetadataError) as error:
        print(f"Rust-only consumer metadata: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
