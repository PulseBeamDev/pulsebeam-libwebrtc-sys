#!/usr/bin/env python3
"""Write the canonical native artifact manifest using only producer inputs."""

import argparse
import hashlib
import json
from pathlib import Path

from cxx_provenance import artifact_provenance, validate_artifact_provenance


TARGETS = {
    "linux-x86_64": {
        "cargo_target": "x86_64-unknown-linux-gnu",
        "minimum_runtime": "glibc 2.31 (Debian Bullseye sysroot)",
        "cxx_runtime": "bundled libc++ and libc++abi",
        "crt": "glibc",
        "archive_member": "lib/libwebrtc.a",
    },
    "linux-arm64": {
        "cargo_target": "aarch64-unknown-linux-gnu",
        "minimum_runtime": "glibc 2.31 (Debian Bullseye sysroot)",
        "cxx_runtime": "bundled libc++ and libc++abi",
        "crt": "glibc",
        "archive_member": "lib/libwebrtc.a",
    },
    "windows-x86_64": {
        "cargo_target": "x86_64-pc-windows-msvc",
        "minimum_runtime": "Windows 10",
        "cxx_runtime": "MSVC STL",
        "crt": "static multithreaded CRT (/MT)",
        "archive_member": "lib/webrtc.lib",
    },
    "macos-x86_64": {
        "cargo_target": "x86_64-apple-darwin",
        "minimum_runtime": "macOS 12.0",
        "cxx_runtime": "system libc++",
        "crt": "system",
        "archive_member": "lib/libwebrtc.a",
    },
    "macos-arm64": {
        "cargo_target": "aarch64-apple-darwin",
        "minimum_runtime": "macOS 12.0",
        "cxx_runtime": "system libc++",
        "crt": "system",
        "archive_member": "lib/libwebrtc.a",
    },
    "android-x86_64": {
        "cargo_target": "x86_64-linux-android",
        "minimum_runtime": "Android API 26",
        "cxx_runtime": "bundled libc++, libc++abi, and libunwind",
        "crt": "Android bionic",
        "archive_member": "lib/libwebrtc.a",
    },
    "android-arm64-v8a": {
        "cargo_target": "aarch64-linux-android",
        "minimum_runtime": "Android API 26",
        "cxx_runtime": "bundled libc++, libc++abi, and libunwind",
        "crt": "Android bionic",
        "archive_member": "lib/libwebrtc.a",
    },
    "ios-arm64": {
        "cargo_target": "aarch64-apple-ios",
        "minimum_runtime": "iOS 18.0",
        "cxx_runtime": "system libc++",
        "crt": "system",
        "archive_member": "lib/libwebrtc.a",
    },
    "ios-simulator-arm64": {
        "cargo_target": "aarch64-apple-ios-sim",
        "minimum_runtime": "iOS Simulator 18.0",
        "cxx_runtime": "system libc++",
        "crt": "system",
        "archive_member": "lib/libwebrtc.a",
    },
}


def expected_source_state(flavor, target):
    return "applied" if flavor == "core" and target in {"ios-arm64", "ios-simulator-arm64"} else "pristine"


def link(kind, name):
    return {"kind": kind, "name": name}


def links_for(flavor, target):
    if target.startswith("linux-"):
        link_args = ["-fuse-ld=lld"]
        if target == "linux-arm64":
            link_args.append("--rtlib=compiler-rt")
        links = [link("link_arg", arg) for arg in link_args]
        links += [link("dylib", name) for name in ("pthread", "dl", "rt", "m")]
        if flavor == "native":
            links += [
                link("dylib", name)
                for name in (
                    "X11", "gio-2.0", "glib-2.0", "gobject-2.0", "Xcomposite",
                    "Xdamage", "Xext", "Xfixes", "Xrandr", "Xrender", "Xtst",
                    "gbm", "drm",
                )
            ]
        return links
    if target == "windows-x86_64":
        return [
            link("dylib", name)
            for name in (
                "advapi32", "bcrypt", "crypt32", "d3d11", "dmoguids", "dwmapi",
                "dxgi", "iphlpapi", "msdmo", "ole32", "oleaut32", "secur32",
                "shcore", "strmiids", "user32", "winmm", "wmcodecdspuuid", "ws2_32",
            )
        ]
    if target.startswith("macos-"):
        frameworks = (
            "Foundation", "AppKit", "ApplicationServices", "CoreAudio", "CoreFoundation",
            "CoreGraphics", "CoreMedia", "CoreVideo", "AudioToolbox", "AVFoundation",
            "IOKit", "IOSurface", "OpenGL", "VideoToolbox",
        )
        return [link("dylib", "c++")] + [link("framework", name) for name in frameworks] + [
            link("weak_framework", "ScreenCaptureKit")
        ]
    if target.startswith("android-"):
        names = ["log", "android", "GLESv2", "OpenSLES", "dl", "m"]
        if flavor == "native":
            names.append("aaudio")
        return [
            link("link_arg", "-fuse-ld=lld"),
            link("link_arg", "--unwindlib=none"),
        ] + [link("dylib", name) for name in names]
    frameworks = (
        "Foundation", "CoreFoundation", "CoreGraphics", "CoreMedia", "CoreVideo",
        "AudioToolbox", "AVFoundation", "VideoToolbox", "UIKit",
    )
    return [link("dylib", "c++")] + [link("framework", name) for name in frameworks]


def digest(value):
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--flavor", choices=("core", "native"), required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--bridge-identity", required=True)
    parser.add_argument("--source-repository", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--source-patch-sha256", required=True)
    parser.add_argument("--source-state", choices=("applied", "pristine"), required=True)
    parser.add_argument("--depot-tools-repository", required=True)
    parser.add_argument("--depot-tools-revision", required=True)
    parser.add_argument("--bridge-source", type=Path, required=True)
    parser.add_argument("--generated-header", type=Path, required=True)
    parser.add_argument("--generated-source", type=Path, required=True)
    parser.add_argument("--toolchain-file", type=Path, required=True)
    parser.add_argument("--gn-args-file", type=Path, required=True)
    parser.add_argument("--defines-file", type=Path, required=True)
    parser.add_argument("--licenses", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.source_state != expected_source_state(args.flavor, args.target):
        parser.error("source state does not match target/flavor applicability")

    target = TARGETS[args.target]
    toolchain = args.toolchain_file.read_text().strip()
    cxx = artifact_provenance(
        Path(__file__).resolve().parents[1],
        args.bridge_source,
        args.generated_header,
        args.generated_source,
    )
    validate_artifact_provenance(cxx, Path(__file__).resolve().parents[1])
    native_configuration = {
        "bridge_identity": args.bridge_identity,
        "cxx": cxx,
        "source_revision": args.source_revision,
        "source_patch_sha256": args.source_patch_sha256,
        "source_state": args.source_state,
        "depot_tools_revision": args.depot_tools_revision,
        "flavor": args.flavor,
        "target": args.target,
        "cargo_target": target["cargo_target"],
        "toolchain": toolchain,
        "gn_args": args.gn_args_file.read_text().strip(),
        "cxx_defines": args.defines_file.read_text().splitlines(),
    }
    license_files = []
    for path in sorted(path for path in args.licenses.rglob("*") if path.is_file()):
        license_files.append({
            "path": (Path("LICENSES") / path.relative_to(args.licenses)).as_posix(),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        })

    manifest = {
        "schema_version": 2,
        "bridge": {"identity": args.bridge_identity, "cxx": cxx},
        "sources": {
            "webrtc": {
                "repository": args.source_repository,
                "revision": args.source_revision,
                "patch_sha256": args.source_patch_sha256,
                "state": args.source_state,
            },
            "depot_tools": {
                "repository": args.depot_tools_repository,
                "revision": args.depot_tools_revision,
            },
        },
        "artifact": {
            "flavor": args.flavor,
            "target": args.target,
            "cargo_target": target["cargo_target"],
        },
        "native_configuration_sha256": digest(native_configuration),
        "abi": {
            "toolchain": toolchain,
            "cxx_standard": "C++20",
            "cxx_runtime": target["cxx_runtime"],
            "crt": target["crt"],
            "minimum_runtime": target["minimum_runtime"],
        },
        "archive": {"member": target["archive_member"], "name": "webrtc"},
        "links": links_for(args.flavor, args.target),
        "licenses": {"sha256": digest(license_files), "files": license_files},
    }
    args.output.write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
