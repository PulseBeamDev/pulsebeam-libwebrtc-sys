"""Select a candidate WebRTC pin for a disposable upgrade rehearsal checkout."""

import argparse
import re
from pathlib import Path

PIN = re.compile(r'^webrtc_commit := "([0-9a-f]{40})"$', re.MULTILINE)
CANDIDATE = re.compile(r"[0-9a-f]{40}\Z")


def select(justfile: Path, candidate: str) -> None:
    if not CANDIDATE.fullmatch(candidate):
        raise ValueError("candidate must be a lowercase 40-character commit")
    text = justfile.read_text(encoding="utf-8")
    matches = list(PIN.finditer(text))
    if len(matches) != 1:
        raise ValueError("expected exactly one checked-in WebRTC pin")
    if matches[0].group(1) == candidate:
        raise ValueError("candidate must differ from the checked-in pin")
    updated = text[: matches[0].start(1)] + candidate + text[matches[0].end(1) :]
    justfile.write_text(updated, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate", help="40-character lowercase WebRTC commit")
    parser.add_argument("--justfile", type=Path, default=Path("Justfile"))
    args = parser.parse_args()
    try:
        select(args.justfile, args.candidate)
    except ValueError as exc:
        parser.error(str(exc))


if __name__ == "__main__":
    main()
