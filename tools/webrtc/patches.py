from __future__ import annotations

import shutil
from pathlib import Path

from .contract import ROOT, patch_entries
from .errors import ContractError
from .runner import run


def apply_all(checkout_root: Path, root: Path = ROOT) -> None:
    try:
        for name, meta, _data in patch_entries(root):
            target = checkout_root / meta["checkout"]
            actual = run(["git", "rev-parse", "HEAD"], cwd=target, capture=True)
            if actual != meta["base_sha"]:
                raise ContractError(f"patch {name} expected base {meta['base_sha']}, found {actual}")
            patch = root / "patches" / name
            run(["git", "apply", "--check", "--index", str(patch)], cwd=target)
            run(["git", "apply", "--index", str(patch)], cwd=target)
    except Exception:
        shutil.rmtree(checkout_root, ignore_errors=True)
        raise
