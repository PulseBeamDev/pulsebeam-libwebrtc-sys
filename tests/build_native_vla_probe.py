"""Build the test-only offline parser against a matching Linux x86_64 SDK.

This does not alter artifact inputs or run an engine/controlled lifecycle.
The SDK's exported definitions and libc++ headers preserve its C++ ABI.
"""
import argparse
from pathlib import Path
import platform
import shlex
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", type=Path, required=True)
    parser.add_argument("--checkout-src", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--asan", action="store_true")
    args = parser.parse_args()
    if sys.platform != "linux" or platform.machine() != "x86_64":
        parser.error("this test-only builder supports Linux x86_64")
    kit = args.kit.resolve()
    src = args.checkout_src.resolve()
    output = args.output.resolve()
    definitions = next(
        line.split("=", 1)[1]
        for line in (kit / "build.txt").read_text().splitlines()
        if line.startswith("cxx_defines=")
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    command = [
        str(src / "third_party/llvm-build/Release+Asserts/bin/clang++"),
        "-std=c++20", "-fno-exceptions", "-fno-rtti",
        "-Wno-nullability-completeness", *shlex.split(definitions),
        "--target=x86_64-linux-gnu",
        f"--sysroot={src / 'build/linux/debian_bullseye_amd64-sysroot'}",
        "-nostdinc++", "-isystem", str(kit / "include/c++/v1"),
        f"-I{kit / 'include'}", str(Path(__file__).with_name("native_vla_probe.cc")),
        str(kit / "lib/libwebrtc.a"), "-fuse-ld=lld", "--rtlib=compiler-rt",
        "--unwindlib=none", "-nostdlib++", "-pthread", "-ldl", "-lrt", "-lm",
        # iostreams pull exception support. Host unwinding is only for this
        # offline executable, not a change to the native artifact closure.
        "-lgcc_s", "-o", str(output),
    ]
    if args.asan:
        command.append("-fsanitize=address")
    subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
