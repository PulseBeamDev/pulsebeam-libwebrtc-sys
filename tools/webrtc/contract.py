from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from .errors import ContractError

ROOT = Path(__file__).resolve().parents[2]
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
REF = re.compile(r"^refs/heads/[A-Za-z0-9._/-]+$")


def load_json(path: Path):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ContractError(f"invalid JSON at {path}: {exc}") from exc


def canonical(value) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def patch_entries(root: Path = ROOT):
    manifest = load_json(root / "patches/manifest.json")
    if manifest.get("schema_version") != 1 or not isinstance(manifest.get("patches"), dict):
        raise ContractError("patches/manifest.json must be schema version 1 with a patches object")
    series = []
    for raw in (root / "patches/series").read_text().splitlines():
        item = raw.strip()
        if item and not item.startswith("#"):
            if item in series:
                raise ContractError(f"duplicate patch in series: {item}")
            series.append(item)
    metadata = manifest["patches"]
    files = {p.name for p in (root / "patches").glob("*.patch")}
    if set(series) != set(metadata) or set(series) != files:
        raise ContractError("patch series, manifest keys, and .patch files must agree one-to-one")
    out = []
    for name in series:
        meta = metadata[name]
        if set(meta) != {"checkout", "base_sha", "rationale", "sha256"}:
            raise ContractError(f"invalid metadata keys for patch {name}")
        if not FULL_SHA.fullmatch(meta["base_sha"]):
            raise ContractError(f"patch {name} has a non-full base SHA")
        data = (root / "patches" / name).read_bytes()
        if sha256(data) != meta["sha256"]:
            raise ContractError(f"patch digest mismatch: {name}")
        out.append((name, meta, data))
    return out


def recipe_hash(root: Path = ROOT) -> str:
    lock = load_json(root / "upstream.lock.json")
    lock.pop("identity", None)
    lock.pop("recipe_sha256", None)
    parts = {"upstream.lock.json": sha256(canonical(lock))}
    fixed = ["Justfile", "config/toolchains.lock.json", "config/targets.json",
             "patches/manifest.json", "patches/series"]
    for rel in fixed:
        parts[rel] = sha256((root / rel).read_bytes())
    for parent in (root / "tools", root / "tests"):
        for path in sorted(parent.rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts and not path.name.endswith(".pyc"):
                parts[str(path.relative_to(root))] = sha256(path.read_bytes())
    for name, _meta, data in patch_entries(root):
        parts[f"patch-bytes/{name}"] = sha256(data)
    return sha256(canonical(parts))


def validate(root: Path = ROOT, *, require_resolved: bool = True) -> dict:
    lock = load_json(root / "upstream.lock.json")
    if lock.get("schema_version") != 1:
        raise ContractError("unsupported upstream lock schema")
    source = lock.get("source", {})
    if not REF.fullmatch(source.get("ref", "")):
        raise ContractError("source ref must be a full refs/heads/... ref")
    if not FULL_SHA.fullmatch(source.get("sha", "")):
        raise ContractError("source SHA must contain exactly 40 lowercase hex characters")
    tools = load_json(root / "config/toolchains.lock.json")
    if not FULL_SHA.fullmatch(tools.get("depot_tools", {}).get("sha", "")):
        raise ContractError("depot_tools SHA must be full")
    targets = load_json(root / "config/targets.json")
    names = [item.get("name") for item in targets.get("targets", [])]
    if len(names) != len(set(names)):
        raise ContractError("duplicate target names")
    expected = {"linux-x86_64", "linux-arm64", "macos-arm64", "windows-x86_64", "android-arm64", "ios-arm64"}
    if set(names) != expected:
        raise ContractError("reserved target set is incomplete")
    patch_entries(root)
    if require_resolved and not isinstance(lock.get("dependency_manifest"), dict):
        raise ContractError("dependency manifest is unresolved; the checked-in lock must be established")
    actual = recipe_hash(root)
    if lock.get("recipe_sha256") != actual:
        raise ContractError(f"recipe digest mismatch: lock has {lock.get('recipe_sha256')}, computed {actual}")
    identity = f"m{lock['milestone']}-{source['sha'][:12]}-pb.{actual[:12]}"
    if lock.get("identity") != identity:
        raise ContractError(f"identity mismatch: expected {identity}")
    return lock


def target_config(name: str, profile: str, root: Path = ROOT) -> dict:
    data = load_json(root / "config/targets.json")
    matches = [item for item in data["targets"] if item["name"] == name]
    if not matches:
        raise ContractError(f"unknown target: {name}")
    profiles = matches[0]["profiles"]
    if profile not in profiles:
        raise ContractError(f"unknown profile {profile!r} for target {name}")
    config = profiles[profile]
    if not config.get("implemented"):
        raise ContractError(f"build {name}/{profile} is reserved but not implemented")
    return config
