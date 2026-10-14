#!/usr/bin/env python3
"""Enforce the Linux x86_64 native floating-point compilation contract.

GN invokes this before the compiler, including for common_audio itself. The
wrapper identity is part of the command so changing it invalidates Ninja's
compile commands. No SDK source modification or runtime FP-mode mutation.
"""

import hashlib
import json
import os
from pathlib import Path
import shlex
import sys


FLOATING_POINT_FLAGS = ("-fno-fast-math", "-ffp-contract=off")


def source_digest():
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def gn_wrapper(cache):
    return json.dumps(shlex.join([
        sys.executable, str(Path(__file__).resolve()),
        "--contract-sha256", source_digest(), cache,
    ]), ensure_ascii=False)


def floating_point_configuration(gn_args):
    wrappers = [argument.split("=", 1)[1] for argument in shlex.split(gn_args)
                if argument.startswith("cc_wrapper=")]
    if len(wrappers) != 1:
        raise ValueError("Linux x86_64 requires the native floating-point compiler wrapper")
    command = shlex.split(wrappers[0])
    if len(command) != 5 or command != shlex.split(json.loads(gn_wrapper(command[-1]))):
        raise ValueError("native compiler wrapper identity does not match GN configuration")
    return {
        "final_compiler_flags": list(FLOATING_POINT_FLAGS),
        "compiler_wrapper_sha256": source_digest(),
    }


def compiler_command(arguments):
    if len(arguments) < 2:
        raise ValueError("expected cache executable, compiler and compiler arguments")
    # These are final driver arguments, overriding earlier fast-math/contract
    # options, including those loaded by Clang from response files.
    return [*arguments, *FLOATING_POINT_FLAGS]


def main():
    arguments = sys.argv[1:]
    if len(arguments) == 2 and arguments[0] == "--gn-wrapper":
        print(gn_wrapper(arguments[1]))
        return
    if len(arguments) < 4 or arguments[0] != "--contract-sha256":
        raise SystemExit("expected --contract-sha256 HASH CACHE COMPILER [ARGUMENT ...]")
    if arguments[1] != source_digest():
        raise SystemExit("native compiler wrapper changed; rerun GN configuration")
    command = compiler_command(arguments[2:])
    os.execv(command[0], command)


if __name__ == "__main__":
    main()
