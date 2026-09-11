# `webrtc-build`

`webrtc-build` builds release artifacts from a pinned `webrtc-sdk/webrtc`
source revision and publishes them on GitHub. It is build infrastructure only:
Rust bindings, codec adapters, rendering, applications, and simulator scenarios
belong to consuming repositories.

## Artifact flavors

Every supported target has two artifacts built from the same pinned source and
with the same public WebRTC API:

- `core` is the portable, headless engine for simulations, servers, and custom
  media pipelines. It retains WebRTC transport and media logic, built-in audio
  codecs, VP8, VP9, AV1, and caller-supplied devices, media sources, sinks,
  clocks, queues, sockets, and codec factories. It excludes platform capture,
  playback, and window-system integrations where upstream build switches allow.
- `native` is a platform-specific strict superset for complete clients. It adds
  the upstream camera, microphone, speaker/playback, and screen/window capture
  integrations available for that target. Rendering is supplied uniformly by
  the downstream Rust integration described below rather than by mixing
  incompatible Android, Apple, Windows, and Linux UI implementations here.

The intended platform families are Android, iOS, macOS, Windows, and Linux.
Each artifact is built on a supported native toolchain and released separately;
one host build is not treated as portable across operating systems.

## Initial target matrix

Both flavors are built for every target. Platform-native containers may combine
compatible build variants, but their manifest must identify every included
slice.

| Platform | Build variants | Minimum runtime |
|---|---|---|
| Linux | x86_64 GNU, arm64 GNU | Debian Bullseye sysroot / glibc 2.31 |
| Windows | x86_64 MSVC | Windows 10 |
| macOS | arm64, x86_64 | macOS 12 |
| Android | arm64-v8a, x86_64 emulator | API 26 with AAudio |
| iOS | arm64 device, arm64 simulator | iOS 18 |

Linux musl, 32-bit Android, Windows arm64, x86_64 iOS Simulator, WebAssembly,
and other targets are deferred.

## Required capabilities

Released headers and libraries must expose the ordinary upstream libwebrtc APIs
needed by downstream integrations:

| Requirement | Existing upstream mechanism | Patch needed? |
|---|---|---:|
| Caller-controlled time | `EnvironmentFactory` | No |
| Cooperative task queues | `TaskQueueFactory` | No |
| Caller-owned threads | `PeerConnectionFactoryDependencies` | No |
| Simulated sockets/network | `PacketSocketFactory` | No |
| Seeded WebRTC IDs/randomness | `SetRandomGenerator` | No |
| Deterministic cryptographic entropy | No usable runtime injection point | Yes |

The initial release does not provide deterministic cryptographic entropy and
must not carry a libwebrtc or BoringSSL patch. Consumers must not expect
certificates, DTLS material, SRTP keys, or encrypted bytes to repeat for the
same simulation seed.

## Build and release contract

- Pin `webrtc-sdk/webrtc` commit
  `ba469aa2093ba950066258ca0a59a6fbd1295582` from `m150_release`; the moving
  branch name is context, never the source identity.
- Build the ordinary upstream targets needed by each flavor without modifying
  the pinned source.
- Obtain matching headers from that same revision; never combine headers and
  libraries from different revisions.
- Publish a self-contained, per-target archive with the library closure,
  required headers, upstream notices/licenses, a machine-readable build
  manifest, and SHA-256 checksums.
- Publish `core` as a static C++ link kit on every target. Publish `native` as a
  link kit on Linux and Windows, Android AAR/JAR plus ABI-specific JNI library,
  and Apple framework/XCFramework packages with their required metadata.
- Record the upstream revision, resolved dependency revisions, GN arguments,
  runner image, toolchain/SDK identity, target ABI and runtime floor, and
  build-recipe identity in the manifest.
- Verify the archive in a clean consumer-style link test before publication.
- Treat provenance, not byte-for-byte archive identity, as the reproducibility
  contract.
- Publish optimized release artifacts only. Debug builds are local-only.
- Use pinned standard GitHub-hosted runners available to public repositories;
  paid and self-hosted runners are not implicit fallbacks. Each job must measure
  disk capacity and fail clearly when it cannot meet its build budget.
- Initiate source revision updates and immutable GitHub Releases manually
  through explicit `just` commands. Never follow or publish from a moving
  upstream branch automatically.
- Name releases `m<milestone>-r<revision>`. Any source, flag, recipe, or package
  correction produces a new revision; published releases are never replaced.
- Publish only after every required target and flavor builds and verifies. Every
  asset requires a SHA-256 checksum and GitHub/Sigstore build-provenance
  attestation.

## Release operations

Releases are manual and immutable. Review source changes before dispatching CI:

```console
just update-source <40-character-webrtc-commit> <milestone> <recipe-revision>
git diff -- Justfile
```

`update-source` verifies that the exact commit is fetchable from
`webrtc-sdk/webrtc`, changes only the authoritative identity constants, and
prints the resulting source and recipe identity. It does not sync, build, tag,
or publish anything. Any source, flag, packaging, or recipe correction after a
release requires a new recipe revision.

After committing, reviewing, and pushing the clean recipe commit, dispatch the
only publishing workflow with:

```console
just release m150-r1
```

The command checks tag syntax and identity agreement, the clean pushed recipe
commit, GitHub credentials, source availability, and the absence of an existing
tag or release. The workflow is `workflow_dispatch`-only. It builds and verifies
the full matrix through `just`, aggregates only the expected verified files,
creates the release checksum index and manifest, generates GitHub/Sigstore
provenance, and uploads a draft. The draft becomes public only after a clean
download passes the same release checks. A failed draft is never advertised as
a final release and must not be repaired in place; increment the recipe
revision.

Anyone with GitHub CLI credentials that can read the repository attestations
can independently download and verify a published release without local build
outputs:

```console
just verify-release m150-r1
```

This checks the exact asset inventory, release-wide and per-asset SHA-256
checksums, manifest/source/recipe agreement, all target/flavor slices, archive
structure, licenses and third-party notices, codec disclosures, absence of a
bundled OpenH264 or FFmpeg H.264 implementation, the immutable release tag, and
GitHub/Sigstore build provenance.

"Static" applies to the WebRTC library, not to every operating-system runtime.
The Linux kits use the Bullseye ABI floor, dynamically supplied glibc, and the
packaged pinned libc++/libc++abi static runtime; Windows retains the pinned
upstream static CRT (`/MT`) contract; Apple uses the platform libc++ and
frameworks; and Android records and retains its pinned NDK/libc++ contract,
with JNI shared libraries allowed in `native`. Every manifest must state the
exact runtime and linker contract.

Downstream Rust/CXX bindings must expose opaque handles or copied plain data,
keep STL types private to C++, and destroy WebRTC-owned objects through the same
C++ runtime that created them. Consumer bridge code must match the artifact's
C++ ABI and runtime settings. A complete link kit contains all WebRTC-owned
inputs needed to consume it, but does not redistribute the platform runtime.

Release verification runs the control and codec smoke test on runnable desktop
targets, exercises native packaging on the Android x86_64 emulator and iOS
arm64 simulator, and performs structural/ABI checks on cross-compiled and
device-only artifacts. Physical camera, microphone, speaker, GPU, and codec
hardware qualification belongs to downstream applications and is not implied
by a release.

## H.264 boundary

H.264 signaling and caller-supplied codec factories are required, but this
repository must build with `rtc_use_h264=false`. It must not compile or publish
the built-in OpenH264 encoder or FFmpeg H.264 decoder, and it must not publish a
separate H.264-enabled libwebrtc variant.

The release must preserve and verify the native `VideoEncoderFactory` and
`VideoDecoderFactory` integration needed for a consumer to advertise and supply
H.264.

`pulsebeam-libwebrtc` owns the optional OpenH264 factory adapter. OpenH264 itself
is not part of this repository or its release assets; its acquisition,
distribution, enablement, attribution, and licensing remain the responsibility
of that adapter and the consuming product. Production consumers may instead
supply a hardware-backed or separately licensed H.264 factory.

The future `pulsebeam-libwebrtc` integration may expose an optional `openh264`
Cargo feature. The adapter may load an externally installed OpenH264 shared
library at runtime, validate its supported version, and inject its codec
factories. Absence of that library must leave H.264 unavailable or produce an
explicit error when H.264 is required; it must not change the contents of these
release artifacts.

## Downstream Rust and rendering contract

This repository does not contain Rust bindings or rendering code. Its artifacts
must support a downstream `pulsebeam-libwebrtc` feature model in which `core` is
the default, additive `native` selects the matching native artifact and wgpu
renderer, and an independently selectable renderer feature can render frames
from the core artifact without enabling platform devices.

wgpu is the common rendering implementation for Android, iOS, macOS, Windows,
and Linux. The initial renderer uploads decoded I420 or NV12 planes to GPU
textures and performs color conversion, scaling, cropping, and rotation in
shaders. The application still owns the platform window/view, surface lifetime,
and UI-thread integration.

Zero-copy interop with platform decoder buffers such as `CVPixelBuffer`,
`AHardwareBuffer`, Direct3D textures, or DMA-BUF is deferred. Those paths may be
added later without changing the portable frame-rendering contract.

## Keep this repository tiny

The repository owns the pin/manifest, `Justfile` command surface, GitHub release
workflow, and only the minimal validation needed to make releases trustworthy.
All human and CI entry points go through `just`.

Pins, the target matrix, GN arguments, orchestration, packaging, and public
commands live in `Justfile`. Small C++, Java, or Objective-C consumer fixtures
may be committed when they are needed to verify an artifact clearly; they must
not grow into another tooling framework. This repository's own code is licensed
under Apache-2.0.

It must not vendor WebRTC source, depot_tools, generated build trees, output
archives, bindings, codec implementations, application code, simulator code, or
a patch stack. Build workspaces and release artifacts are disposable CI/local
outputs.

## Non-goals

- Maintaining a libwebrtc or BoringSSL fork.
- Source patches in the initial release.
- A bundled H.264 encoder or decoder.
- Deterministic cryptographic output.
- A public Rust or C++ binding API.
- A Rust or wgpu implementation in this repository.
- Zero-copy video rendering in the initial integration.
- Published debug artifacts or physical-device test infrastructure.
- Byte-identical builds across runners.
- Depending on or adapting LiveKit `webrtc-sys`.
