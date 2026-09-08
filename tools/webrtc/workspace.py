from __future__ import annotations

import hashlib
import json
import os
import shutil
from pathlib import Path

from .contract import ROOT, canonical, load_json, patch_entries, recipe_hash, sha256
from .errors import ContractError
from .patches import apply_all
from .runner import run


def locations(root: Path = ROOT) -> tuple[Path, Path, Path]:
    work = Path(os.environ.get("WEBRTC_WORK_ROOT", root / ".work")).resolve()
    cache = Path(os.environ.get("WEBRTC_CACHE_ROOT", root / ".cache")).resolve()
    output = Path(os.environ.get("WEBRTC_OUTPUT_ROOT", root / "out")).resolve()
    return work, cache, output


def bootstrap(root: Path = ROOT) -> Path:
    _work, cache, _output = locations(root)
    cfg = load_json(root / "config/toolchains.lock.json")["depot_tools"]
    depot = cache / "depot_tools" / cfg["sha"]
    if not depot.exists():
        depot.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", "--no-checkout", cfg["url"], str(depot)])
        run(["git", "checkout", "--detach", cfg["sha"]], cwd=depot)
    actual = run(["git", "rev-parse", "HEAD"], cwd=depot, capture=True)
    dirty = run(["git", "status", "--porcelain"], cwd=depot, capture=True)
    if actual != cfg["sha"] or dirty:
        raise ContractError("cached depot_tools is not the locked clean revision; remove that cache entry")
    return depot


def _env(depot: Path, root: Path = ROOT) -> dict[str, str]:
    env = dict(os.environ)
    _work, cache, _output = locations(root)
    tracker = cache / "gsutil" / "tracker-files"
    tracker.mkdir(parents=True, exist_ok=True)
    boto = cache / "gsutil" / "boto.cfg"
    boto.write_text(f"[GSUtil]\nresumable_tracker_dir = {tracker}\n")
    env["PATH"] = f"{depot}{os.pathsep}{env.get('PATH', '')}"
    env["DEPOT_TOOLS_UPDATE"] = "0"
    env["DEPOT_TOOLS_METRICS"] = "0"
    env["CIPD_CACHE_DIR"] = str(cache / "cipd")
    env["VPYTHON_VIRTUALENV_ROOT"] = str(cache / "vpython")
    env["BOTO_CONFIG"] = str(boto)
    return env


def _run_env(argv: list[str], cwd: Path, depot: Path, capture: bool = False) -> str:
    import subprocess
    try:
        result = subprocess.run(argv, cwd=cwd, env=_env(depot), check=True, text=True,
                                stdout=subprocess.PIPE if capture else None,
                                stderr=subprocess.PIPE if capture else None)
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "stderr", None) or ""
        raise ContractError(f"command failed ({' '.join(argv)}): {detail.strip()}") from exc
    return result.stdout.strip() if capture else ""


def dependency_manifest(checkout: Path) -> dict:
    git = {}
    for marker in sorted(checkout.rglob(".git")):
        repo = marker.parent
        rel = str(repo.relative_to(checkout)) or "."
        git[rel] = run(["git", "rev-parse", "HEAD"], cwd=repo, capture=True)
    cipd = {}
    for path in sorted(checkout.rglob("*")):
        if path.is_file() and ".cipd" in path.parts and path.name in {"pins", "ensure", "ensure-file", "manifest.json"}:
            cipd[str(path.relative_to(checkout))] = sha256(path.read_bytes())
    return {"schema_version": 1, "git": git, "cipd_state_sha256": cipd}


def patch_state(checkout: Path, root: Path = ROOT) -> dict:
    """Return the exact staged state produced by the ordered patch series."""
    states = {}
    checkouts = {"src"}
    checkouts.update(meta["checkout"] for _name, meta, _data in patch_entries(root))
    for name in sorted(checkouts):
        target = checkout / name
        if run(["git", "diff", "--quiet"], cwd=target, capture=False) is None:
            pass
        # `run` only returns on success; an unstaged diff is therefore rejected above.
        diff = run(["git", "diff", "--cached", "--binary", "HEAD"], cwd=target, capture=True)
        states[name] = sha256(diff.encode())
    return states


def _require_patch_state(checkout: Path, root: Path, lock: dict) -> None:
    marker = checkout / ".pulsebeam-patches.json"
    expected = {"recipe_sha256": lock["recipe_sha256"], "states": patch_state(checkout, root)}
    if not marker.is_file() or load_json(marker) != expected:
        raise ContractError("checkout patch state differs from the locked series; remove it before sync")


def sync(root: Path = ROOT, *, establish: bool = False) -> Path:
    lock = load_json(root / "upstream.lock.json")
    depot = bootstrap(root)
    work, _cache, _output = locations(root)
    checkout = work / "checkout"
    src = checkout / "src"
    if checkout.exists():
        if not src.exists():
            raise ContractError(f"partial checkout at {checkout}; remove it before retrying")
        if run(["git", "rev-parse", "HEAD"], cwd=src, capture=True) != lock["source"]["sha"]:
            raise ContractError("source checkout is not at the locked revision; remove it before sync")
        _require_patch_state(checkout, root, lock)
    else:
        checkout.mkdir(parents=True)
        (checkout / ".gclient").write_text(
            "solutions = [{\"name\": \"src\", \"url\": \"" + lock["source"]["url"] +
            "\", \"deps_file\": \"DEPS\", \"managed\": False, \"custom_deps\": {}, \"custom_vars\": {}}]\n")
        src.mkdir()
        run(["git", "init", "-q"], cwd=src)
        run(["git", "remote", "add", "origin", lock["source"]["url"]], cwd=src)
        run(["git", "fetch", "--depth=1", "origin", lock["source"]["sha"]], cwd=src)
        run(["git", "checkout", "--detach", "FETCH_HEAD"], cwd=src)
        try:
            _run_env([str(depot / "gclient.py"), "sync", "--revision", f"src@{lock['source']['sha']}", "--no-history", "--nohooks", "--delete_unversioned_trees", "--force"], checkout, depot)
            _run_env([str(depot / "gclient.py"), "runhooks"], checkout, depot)
            apply_all(checkout, root)
            (checkout / ".pulsebeam-patches.json").write_bytes(canonical({
                "recipe_sha256": lock.get("recipe_sha256"),
                "states": patch_state(checkout, root),
            }))
        except Exception:
            shutil.rmtree(checkout, ignore_errors=True)
            raise
    manifest = dependency_manifest(checkout)
    if establish:
        lock["dependency_manifest"] = manifest
        lock["recipe_sha256"] = None
        lock["identity"] = None
        (root / "upstream.lock.json").write_bytes(canonical(lock))
        digest = recipe_hash(root)
        lock["recipe_sha256"] = digest
        lock["identity"] = f"m{lock['milestone']}-{lock['source']['sha'][:12]}-pb.{digest[:12]}"
        (root / "upstream.lock.json").write_bytes(canonical(lock))
        (checkout / ".pulsebeam-patches.json").write_bytes(canonical({
            "recipe_sha256": digest,
            "states": patch_state(checkout, root),
        }))
    elif manifest != lock.get("dependency_manifest"):
        raise ContractError("resolved dependency/package manifest differs from upstream.lock.json")
    return checkout


def gn_value(value) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, int):
        return str(value)
    raise ContractError(f"unsupported GN argument value: {value!r}")
