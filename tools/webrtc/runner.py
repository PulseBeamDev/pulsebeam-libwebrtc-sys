from __future__ import annotations

import subprocess
from pathlib import Path

from .errors import ContractError


def run(argv: list[str], *, cwd: Path | None = None, capture: bool = False) -> str:
    try:
        result = subprocess.run(
            argv, cwd=cwd, check=True, text=True,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )
    except FileNotFoundError as exc:
        raise ContractError(f"required command not found: {argv[0]}") from exc
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or "").strip()
        suffix = f": {detail}" if detail else ""
        raise ContractError(f"command failed ({' '.join(argv)}){suffix}") from exc
    return result.stdout.strip() if capture else ""
