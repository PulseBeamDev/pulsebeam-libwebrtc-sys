"""Remove only interrupted Git checkouts that have no usable HEAD.

WebRTC's gclient can leave a freshly created dependency .git behind after a
network timeout. It then refuses to reuse that checkout on the next attempt.
This is a local workspace repair, never a source pin or release operation.
"""

import argparse
import os
import shutil
import subprocess
from pathlib import Path


def repair(root: Path) -> list[Path]:
    if not root.is_dir() or root.is_symlink():
        return []
    invalid = []
    for directory, names, _ in os.walk(root, followlinks=False):
        if ".git" not in names:
            continue
        names.remove(".git")
        path = Path(directory)
        # Never follow a symlink or delete a valid checkout, even if sync
        # was interrupted elsewhere in the same source tree.
        if path.is_symlink() or (path / ".git").is_symlink():
            continue
        result = subprocess.run(
            ["git", "-C", str(path), "cat-file", "-e", "HEAD^{commit}"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
        )
        if result.returncode:
            invalid.append(path)
    for path in sorted(invalid, key=lambda path: len(path.parts), reverse=True):
        shutil.rmtree(path)
        print(f"removed interrupted Git checkout without HEAD: {path}")
    return invalid


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="gclient's src directory")
    args = parser.parse_args()
    repair(args.source)


if __name__ == "__main__":
    main()
