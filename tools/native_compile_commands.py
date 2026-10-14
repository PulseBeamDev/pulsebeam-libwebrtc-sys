"""Generate ignored clangd inputs from a pinned Linux x86_64 SDK package.

This configures parsing only. It does not build, link, install, fetch or qualify
an artifact. Absolute machine paths belong in the generated database, not Git.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shlex

ROOT = Path(__file__).resolve().parents[1]


def compilation_commands(root: Path, kit: Path, checkout: Path, overlay: Path) -> list[dict]:
    manifest = json.loads((kit / "manifest.json").read_text(encoding="utf-8"))
    artifact = manifest["artifact"]
    if artifact["target"] != "linux-x86_64" or artifact["flavor"] not in ("core", "native"):
        raise ValueError("clangd input generation currently supports Linux x86_64 core/native kits")
    bridge = manifest["bridge"]["cxx"]
    if hashlib.sha256((root / "src/lib.rs").read_bytes()).hexdigest() != bridge["bridge_source_sha256"]:
        raise ValueError("kit generated bridge does not match current src/lib.rs; rebuild matching inputs")
    fields = dict(line.split("=", 1) for line in (kit / "build.txt").read_text(encoding="utf-8").splitlines() if "=" in line)
    defines = shlex.split(fields["cxx_defines"])
    if not defines or any(not value.startswith("-D") for value in defines):
        raise ValueError("expected exported -D compilation definitions")
    compiler = checkout / "third_party/llvm-build/Release+Asserts/bin/clang++"
    sysroot = checkout / "build/linux/debian_bullseye_amd64-sysroot"
    include = kit / "include"
    generated_header = include / "pulsebeam-webrtc-sys/src/lib.rs.h"
    glib_includes = [sysroot / "usr/include/glib-2.0", sysroot / "usr/lib/x86_64-linux-gnu/glib-2.0/include"]
    required = [compiler, sysroot, include / "c++/v1", include / "rust/cxx.h", generated_header]
    if artifact["flavor"] == "native":
        required += glib_includes
    for path in required:
        if not path.exists():
            raise ValueError(f"missing pinned compilation input: {path}")
    if hashlib.sha256(generated_header.read_bytes()).hexdigest() != bridge["generated_header_sha256"]:
        raise ValueError("packaged generated bridge header digest does not match manifest")
    arguments = [str(compiler), "--target=x86_64-linux-gnu", f"--sysroot={sysroot}",
                 "-std=c++20", "-fno-exceptions", "-fno-rtti", "-Wno-nullability-completeness",
                 "-nostdinc++", "-isystem", str(include / "c++/v1"), "-pthread",
                 "-fno-fast-math", "-ffp-contract=off", f"-I{overlay}", f"-I{include}", *defines]
    if artifact["flavor"] == "native":
        arguments += ["-DPULSEBEAM_WEBRTC_NATIVE_AUDIO=1", "-isystem", str(glib_includes[0]),
                      "-isystem", str(glib_includes[1])]
    sources = sorted([*root.glob("native/*.cc"), *root.glob("native/*.h"),
                      *root.glob("consumer/*.cc")])
    return [{"directory": str(root), "file": str(source),
             "arguments": [*arguments, "-x", "c++", "-c", str(source)]}
            for source in sources]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", type=Path, required=True)
    parser.add_argument("--checkout-src", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / ".work/clangd/compile_commands.json")
    args = parser.parse_args()
    try:
        overlay = args.output.resolve().parent / "include"
        commands = compilation_commands(ROOT, args.kit.resolve(), args.checkout_src.resolve(), overlay)
        native_headers = overlay / "pulsebeam-webrtc-sys/native"
        native_headers.parent.mkdir(parents=True, exist_ok=True)
        if native_headers.is_symlink():
            if native_headers.resolve() != ROOT / "native":
                raise ValueError("refusing to replace a different native-header overlay")
        elif native_headers.exists():
            raise ValueError("native-header overlay exists but is not the current-source symlink")
        else:
            native_headers.symlink_to(ROOT / "native", target_is_directory=True)
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(commands, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {len(commands)} clangd entries to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
