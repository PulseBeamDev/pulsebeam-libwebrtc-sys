#!/usr/bin/env python3
"""Prove the pinned VP8 worker/wait seam against a matching Linux x86_64 SDK.

This native-thread forwarding test is not managed-worker qualification. It uses
private headers from the verified source checkout, not a new public codec API.
Invoke through the project's verification runner, after producing the SDK.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

try:
    from tools.native_compile_commands import compilation_commands
    from tools.native_provenance import LIBVPX_SOURCE, validate_configuration
except ImportError:
    from native_compile_commands import compilation_commands
    from native_provenance import LIBVPX_SOURCE, validate_configuration

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", type=Path, required=True)
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    kit, checkout, output = args.kit.resolve(), args.checkout.resolve(), args.output.resolve()
    manifest = json.loads((kit / "manifest.json").read_text())
    validate_configuration(manifest, ROOT)
    vpx = checkout / "third_party/libvpx/source/libvpx"
    revision = subprocess.check_output(["git", "-C", str(vpx), "rev-parse", "HEAD"], text=True).strip()
    patch = subprocess.check_output(["git", "-C", str(vpx), "diff", "--no-ext-diff",
                                     "--no-color", "--binary", "--full-index"])
    if (revision != LIBVPX_SOURCE["revision"] or
            hashlib.sha256(patch).hexdigest() != LIBVPX_SOURCE["worker_patch_sha256"]):
        raise ValueError("private libvpx headers do not match the exported SDK source pin")
    subprocess.run(["git", "-C", str(vpx), "diff", "--cached", "--quiet"], check=True)
    if subprocess.check_output(["git", "-C", str(vpx), "ls-files", "--others"]):
        raise ValueError("private libvpx headers have unbound untracked inputs, including ignored files")
    source = ROOT / "consumer/vp8_runtime_workers.cc"
    archive = kit / manifest["archive"]["member"]
    archive_digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if archive_digest != manifest["archive"]["sha256"]:
        raise ValueError("SDK archive digest mismatch")
    print("native configuration", manifest["native_configuration_sha256"], flush=True)
    print("libvpx worker patch", LIBVPX_SOURCE["worker_patch_sha256"], flush=True)
    print("archive", archive_digest, "consumer", hashlib.sha256(source.read_bytes()).hexdigest(), flush=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    command = compilation_commands(ROOT, kit, checkout, ROOT / ".work/clangd/include")[0]["arguments"][:-4]
    command += ["-O1", "-g", "-UNDEBUG", "-Wall", "-Wextra", "-Werror", "-I" + str(vpx),
                str(source), str(archive), "-nostdlib++", "-fuse-ld=lld", "-pthread",
                "-ldl", "-lrt", "-lm", "-o", str(output)]
    subprocess.run(command, check=True)
    environment = dict(os.environ)
    environment["ASAN_OPTIONS"] = "detect_leaks=1:detect_stack_use_after_return=1"
    subprocess.run([str(output)], check=True, timeout=180, env=environment)


if __name__ == "__main__":
    main()
