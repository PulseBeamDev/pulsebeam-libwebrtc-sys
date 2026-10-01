"""Path-aware identity of repository adapters and their native build recipe.

The generated bridge and vendor CXX runtime have separate provenance. Include
all native C++ sources/headers (but not manifest fixtures) and the Justfile that
selects, compiles and links them. JSON arrays use UTF-8 and compact separators.
"""

import hashlib
import json
from pathlib import Path


def adapter_inputs(root: Path) -> list[Path]:
    return sorted(
        [root / "Justfile", *root.glob("native/*.cc"), *root.glob("native/*.h")],
        key=lambda path: path.relative_to(root).as_posix(),
    )


def adapter_digest(root: Path) -> str:
    inventory = [
        [path.relative_to(root).as_posix(), hashlib.sha256(path.read_bytes()).hexdigest()]
        for path in adapter_inputs(root)
    ]
    encoded = json.dumps(inventory, separators=(",", ":"), ensure_ascii=True).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()
