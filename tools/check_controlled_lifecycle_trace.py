#!/usr/bin/env python3
"""Check a continuous strace of the safe controlled-media fixture.

Run the already-built test executable directly with --test-threads=1 under:
  strace -f -s 256 -e trace=clone,clone3,fork,vfork,execve,socket,connect,bind,sendto,recvfrom,write -o TRACE EXECUTABLE --nocapture --test-threads=1
Markers encompass acquisition, all native work and destruction, including
negative configuration paths. Test-harness worker creation outside markers is
not library creation. This monitor never advances virtual time or calls peers.
"""
import argparse
import re
from pathlib import Path


def inspect_trace(text: str, minimum_scopes: int = 9) -> int:
    active = False
    scopes = 0
    forbidden = re.compile(r"\b(?:clone3?|fork|vfork|execve|socket|connect|bind|sendto|recvfrom)\(")
    for number, line in enumerate(text.splitlines(), 1):
        if '"CONTROLLED_MEDIA_BEGIN\\n"' in line:
            if active:
                raise ValueError(f"nested lifecycle at line {number}")
            active = True
            scopes += 1
        elif '"CONTROLLED_MEDIA_END\\n"' in line:
            if not active:
                raise ValueError(f"unmatched lifecycle end at line {number}")
            active = False
        elif active and forbidden.search(line):
            # Reject attempts as well as successful creation, so an unfinished
            # clone cannot evade observation by returning after the end marker.
            raise ValueError(f"native thread/process or real socket syscall inside lifecycle at line {number}: {line}")
    if active:
        raise ValueError("unterminated lifecycle (including panic or watchdog kill)")
    if scopes < minimum_scopes:
        raise ValueError(f"only {scopes} lifecycle scopes, expected at least {minimum_scopes}")
    return scopes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--minimum-scopes", type=int, default=9)
    args = parser.parse_args()
    scopes = inspect_trace(args.trace.read_text(), args.minimum_scopes)
    print(f"PASS: {scopes} complete controlled lifecycles, zero thread/helper-process creation and real socket syscalls")


if __name__ == "__main__":
    main()
