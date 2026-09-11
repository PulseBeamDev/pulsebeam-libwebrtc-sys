# `pulsebeam-webrtc-sys`

This repository builds and consumes PulseBeam's pinned WebRTC artifacts. It owns
the private CXX bridge, small native adapters, artifact metadata and selection,
and the low-level Rust API used by `pulsebeam-libwebrtc`.

## Commands

The complete local interface is:

```console
just check
just build <core|native> <target>
```

`just check` is fast and offline once the pinned Rust dependencies are cached.
`just build` validates the flavor, target, and host before it synchronizes the
pinned sources, configures and compiles WebRTC, exports the static closure, and
performs C++ and Rust compile/link-only consumer checks. Every flavor/target
artifact carries the same generated bridge and portable adapter sources,
compiled with that job's target toolchain and ABI configuration.

For development against a matching extracted artifact:

```console
PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR=.work/package/webrtc-core-linux-x86_64 cargo test
```

The override is intentionally local-only. Immutable release resolution and
download/cache behavior are introduced in a later implementation slice.

## Artifact flavors

Every supported target has two artifacts built from the same pinned source and
with the same public WebRTC API:

- `core` is an optimized, static, headless C++ engine for simulations, servers,
  and custom media pipelines. It retains WebRTC transport and media logic,
  built-in audio codecs, VP8, VP9, AV1, and caller-supplied devices, media
  sources, sinks, clocks, queues, sockets, and codec factories. It excludes
  platform capture, playback, and window-system integrations where upstream
  build switches allow.
- `native` adds the upstream platform audio, camera, screen/window capture, and
  native codec paths available for the target. Rendering remains a downstream
  responsibility so clients can use one consistent implementation across
  platforms.

Bundled H.264 is disabled in both flavors. H.264 signaling and the public codec
factory interfaces remain available for caller-supplied implementations.

## Target matrix

Both flavors support every target in this matrix:

| Platform | Targets | Minimum runtime |
|---|---|---|
| Linux | `linux-x86_64`, `linux-arm64` | Debian Bullseye sysroot / glibc 2.31 |
| Windows | `windows-x86_64` | Windows 10 |
| macOS | `macos-x86_64`, `macos-arm64` | macOS 12 |
| Android | `android-x86_64`, `android-arm64-v8a` | API 26; native uses AAudio |
| iOS | `ios-arm64`, `ios-simulator-arm64` | iOS 18 |

Each target is a separate archive built with its supported toolchain. Android
ABIs and Apple device/simulator builds are not combined. Linux musl, 32-bit
Android, Windows arm64, x86_64 iOS Simulator, WebAssembly, and other targets are
deferred.

## Injection and determinism boundary

The exported API retains the upstream extension points required by PulseBeam:

| Requirement | Upstream mechanism |
|---|---|
| Caller-controlled time | `EnvironmentFactory` |
| Cooperative task queues | `TaskQueueFactory` |
| Caller-owned threads | `PeerConnectionFactoryDependencies` |
| Simulated sockets/network | `PacketSocketFactory` |
| Seeded WebRTC IDs/randomness | `SetRandomGenerator` |

WebRTC has no usable runtime injection point for deterministic cryptographic
entropy. This repository does not patch WebRTC or BoringSSL to add one.
Consumers must not expect certificates, DTLS material, SRTP keys, or encrypted
bytes to repeat for the same simulation seed.

## H.264 boundary

The build always uses `rtc_use_h264=false`; it does not compile or publish the
built-in OpenH264 encoder or FFmpeg H.264 decoder, and there is no separate
H.264-enabled artifact. The consumer smoke test preserves the public
`VideoEncoderFactory` and `VideoDecoderFactory` path so downstream code can
advertise and inject H.264 implementations.

`pulsebeam-libwebrtc` owns any optional OpenH264 adapter. OpenH264 acquisition,
distribution, enablement, attribution, licensing, and version checks belong to
that adapter and the consuming product. A consumer may instead provide a
hardware-backed or separately licensed implementation. Absence of an external
implementation must leave H.264 unavailable or produce an explicit error; it
must not change these artifacts.

## Artifact and ABI contract

Each build writes one `dist/webrtc-<flavor>-<target>.tar.gz` containing:

```text
include/          same-revision source and generated headers
lib/libwebrtc.a   self-contained static library (non-Windows)
lib/webrtc.lib    self-contained static library (Windows)
link.txt          required system libraries, frameworks, and link flags
LICENSES/         applicable notices and licenses
build.txt         source revision, flavor, target, toolchain, GN arguments,
                  and exported C++ definitions
manifest.json     versioned source, bridge, native configuration, ABI, archive,
                  ordered link-input, and license-inventory identity
```

Headers and libraries always come from the same immutable WebRTC revision. The
archive contains the WebRTC-owned static link closure but does not redistribute
the operating-system runtime: Linux supplies glibc, Windows uses the static
multithreaded CRT (`/MT`), Apple targets use platform libc++ and frameworks, and
Android uses its pinned API/NDK system libraries. `link.txt` remains a
human-readable producer report. The versioned, ordered, typed entries in
`manifest.json` are the Rust crate's authoritative system-library, framework,
weak-framework, and raw-linker contract.

The generated CXX C++ half and repository-owned adapters are compiled during
artifact production with WebRTC's target compiler and ABI settings, then merged
into the single native archive. The pinned CXX Rust runtime is vendored without
its compiler-running Cargo build script; its matching C++ runtime is also part
of the native archive. Cargo consumers therefore compile only Rust. CXX and STL
types stay private; the public crate surface uses ordinary Rust values.

This repository does not produce AARs, JARs, frameworks, or XCFrameworks.

## Release contract

The immutable commit
`ba469aa2093ba950066258ca0a59a6fbd1295582` is the current WebRTC source
identity; `m150_release` is branch context, not an input to the build. The
source is built without a local patch stack, and only optimized release
artifacts are published.

Releases run only through the manual GitHub Actions workflow. It requires an
existing tag that resolves to the workflow commit, builds and compile/link-checks
all 18 archives on standard hosted runners, generates `SHA256SUMS`, attests the
final assets, and creates one GitHub Release. Publication succeeds only after
every matrix entry succeeds. Provenance, rather than byte-for-byte archive
identity across runners, is the reproducibility contract; published releases
should be treated as immutable.

The smoke test validates API availability and the exported static link closure.
It does not claim physical camera, microphone, speaker, GPU, platform UI, or
hardware-codec qualification. Those runtime tests belong to downstream
applications.

## Downstream rendering contract

This repository contains the low-level Rust bindings but no rendering code. The
intended `pulsebeam-libwebrtc` feature model uses `core` by default, lets an
additive `native` feature select the matching native artifact, and keeps
rendering independently selectable so core applications can render without
enabling platform devices.

wgpu is the intended common renderer for Android, iOS, macOS, Windows, and
Linux. The initial path uploads decoded I420 or NV12 planes and performs color
conversion, scaling, cropping, and rotation in shaders; the application owns
the window or view, surface lifetime, and UI-thread integration. Zero-copy
interop with `CVPixelBuffer`, `AHardwareBuffer`, Direct3D textures, and DMA-BUF
is deferred.

## Repository boundaries

This repository owns the immutable source/tool pins, target matrix, GN
arguments, private CXX definition, native adapters, low-level Rust package,
artifact metadata, release workflow, consumer tests, and licenses needed to
make releases trustworthy. Its own code is Apache-2.0 licensed. It must not
vendor WebRTC, depot_tools, generated build trees, output archives, application
or simulator code, production codec implementations, or a source patch stack.
Build workspaces and release artifacts are disposable local/CI output.

Non-goals include:

- maintaining a WebRTC or BoringSSL fork;
- bundling an H.264 encoder or decoder;
- deterministic cryptographic output;
- exposing CXX or libwebrtc's C++ ABI as the public Rust API;
- implementing Rust, wgpu, or zero-copy rendering here;
- publishing debug artifacts or physical-device test infrastructure;
- promising byte-identical builds across runners; and
- depending on or adapting LiveKit `webrtc-sys`.

## Upgrades

For a routine WebRTC upgrade, change only `webrtc_commit` near the top of the
`Justfile`, review the build, and run the release workflow. Change the pinned
`depot_tools` revision only when WebRTC compatibility requires it. Adapt build
arguments or target selection only when CI shows that an upstream change broke
a required PulseBeam contract.
