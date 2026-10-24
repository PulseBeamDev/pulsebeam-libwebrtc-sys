"""Reindex Apple build archives as GNU archives before LLVM consumes them.

GNU thin archives may carry a Darwin __.SYMDEF child on Apple hosts. LLD's
Mach-O reader treats that child as an object, not a symbol index. Remove only
those metadata children and rebuild the GNU index. llvm-ar preserves thinness,
object contents, path-qualified members and duplicate object basenames.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import subprocess


DARWIN_INDEXES = {"__.SYMDEF", "__.SYMDEF SORTED", "__.SYMDEF_64", "__.SYMDEF_64 SORTED"}


def normalize(ar: str, archive: Path) -> None:
    members = subprocess.run([ar, "t", str(archive)], check=True, capture_output=True, text=True).stdout.splitlines()
    metadata = [member for member in members if Path(member).name in DARWIN_INDEXES]
    if metadata:
        subprocess.run([ar, "--format=gnu", "d", str(archive), *metadata], check=True)
    subprocess.run([ar, "--format=gnu", "s", str(archive)], check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ar", required=True)
    parser.add_argument("--list", type=Path, dest="listing")
    parser.add_argument("archives", type=Path, nargs="*")
    args = parser.parse_args()
    archives = args.archives
    if args.listing:
        archives.extend(Path(line) for line in args.listing.read_text().splitlines() if line)
    for archive in archives:
        try:
            normalize(args.ar, archive)
        except (OSError, subprocess.CalledProcessError) as error:
            detail = getattr(error, "stderr", None) or str(error)
            parser.exit(1, f"cannot normalize Apple archive {archive}: {detail}\n")


if __name__ == "__main__":
    main()
