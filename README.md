# `webrtc-build`

This repository builds pinned, raw WebRTC development artifacts for PulseBeam.
Bindings, platform package containers, rendering, and runtime integration belong
to downstream repositories.

## Commands

The complete local interface is:

```console
just check
just build <core|native> <target>
```

`just check` is fast and offline. `just build` validates the flavor, target, and
host before it synchronizes the pinned sources, configures and compiles WebRTC,
exports the static closure, and performs a compile/link-only consumer check.

## Flavors and targets

`core` is an optimized, static, headless C++ build with injectable clocks, task
queues, threads, packet sockets, devices, media endpoints, and codec factories.
`native` adds upstream platform audio, camera, screen/window capture, and native
codec paths where the target provides them. Both flavors build Opus, VP8, VP9,
and AV1. Bundled H.264 is disabled, while H.264 signaling and caller-supplied
codec factories remain available.

Both flavors support the same targets:

- `linux-x86_64` and `linux-arm64` (Bullseye/glibc 2.31)
- `windows-x86_64` (Windows 10, static `/MT` CRT)
- `macos-x86_64` and `macos-arm64` (macOS 12)
- `android-x86_64` and `android-arm64-v8a` (Android API 26; native uses AAudio)
- `ios-arm64` and `ios-simulator-arm64` (iOS 18)

## Artifacts

Each build writes one `dist/webrtc-<flavor>-<target>.tar.gz` containing:

```text
include/          same-revision source and generated headers
lib/libwebrtc.a   self-contained static library (non-Windows)
lib/webrtc.lib    self-contained static library (Windows)
link.txt          required system libraries, frameworks, and link flags
LICENSES/         applicable notices and licenses
build.txt         source revision, flavor, target, toolchain, and GN arguments
```

Android device/ABI and Apple device/simulator archives stay separate. This
repository does not produce AARs, JARs, frameworks, or XCFrameworks.

## Releases and upgrades

Releases run only through the manual GitHub Actions workflow. The workflow
requires an existing tag that resolves to the workflow commit, builds the
literal 18-job matrix on standard hosted runners, generates `SHA256SUMS`,
attests the final assets, and creates one GitHub Release.

For a routine WebRTC upgrade, change only `webrtc_commit` near the top of the
`Justfile`, review the build, and run the release workflow. Change the pinned
`depot_tools` revision only when WebRTC compatibility requires it. Adapt build
arguments or target selection only when CI shows that an upstream change broke
a required PulseBeam contract.
