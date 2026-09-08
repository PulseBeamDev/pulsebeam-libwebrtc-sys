from __future__ import annotations

import argparse
import json
import shutil
import sys

from .contract import ROOT, canonical, load_json, target_config, validate
from .errors import ContractError
from .runner import run
from .workspace import bootstrap, gn_value, locations, sync


def build(target: str, profile: str) -> None:
    lock = validate()
    config = target_config(target, profile)
    checkout = sync()
    _work, _cache, output = locations()
    out = output / lock["identity"] / target / profile
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    args = " ".join(f"{key}={gn_value(value)}" for key, value in sorted(config["gn_args"].items()))
    depot = bootstrap()
    gn = checkout / "src" / "buildtools" / "linux64" / "gn"
    ninja = checkout / "src" / "third_party" / "ninja" / "ninja"
    if not gn.exists() or not ninja.exists():
        raise ContractError("locked GN/Ninja tools are missing after sync")
    run([str(gn), "gen", str(out), f"--args={args}"], cwd=checkout / "src")
    run([str(ninja), "-C", str(out), config["gn_target"]], cwd=checkout / "src")
    archive = out / config["archive"]
    if not archive.is_file():
        raise ContractError(f"build completed without expected archive: {archive}")
    record = {
        "schema_version": 1, "identity": lock["identity"], "recipe_sha256": lock["recipe_sha256"],
        "source": lock["source"], "dependency_manifest": lock["dependency_manifest"],
        "toolchain_lock": load_json(ROOT / "config/toolchains.lock.json"),
        "target": target, "profile": profile, "gn_args": config["gn_args"],
        "gn_target": "//:webrtc", "archive": str(archive),
    }
    (out / "build-record.json").write_bytes(canonical(record))
    print(archive)


def main() -> None:
    parser = argparse.ArgumentParser(description="Private implementation; use just recipes")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("bootstrap")
    sub.add_parser("validate")
    sub.add_parser("sync")
    sub.add_parser("establish-lock", help=argparse.SUPPRESS)
    build_parser = sub.add_parser("build")
    build_parser.add_argument("target")
    build_parser.add_argument("profile")
    args = parser.parse_args()
    try:
        if args.command == "bootstrap":
            print(bootstrap())
        elif args.command == "validate":
            lock = validate()
            print(lock["identity"])
        elif args.command == "sync":
            validate()
            print(sync())
        elif args.command == "establish-lock":
            print(sync(establish=True))
        else:
            build(args.target, args.profile)
    except ContractError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
