# `pulsebeam-webrtc-sys`

This repository builds and consumes PulseBeam's pinned WebRTC artifacts. It owns
the private CXX bridge, small native adapters, artifact metadata and selection,
and the low-level Rust API used by `pulsebeam-libwebrtc`.

## Commands

The complete local interface is:

```console
just check
just ci-preflight
just ci --help
just ci-release --help
just verify-artifact <archive> <sha256>
just build <core|native> <target>
just refresh-cxx
```

`just ci-preflight` checks every release CLI option used by the workflows
against the locally installed `gh` (also run on the GitHub runner before the
Linux image build). It needs neither network nor a GitHub token. Run it before
starting a long local build; unlike `just check`, it requires `gh` on the host.

The single `release.yml` workflow calls repository-owned `just` commands for
validation, artifact audit, result qualification, and release publication. Its
YAML retains job dependencies, runner selection, credential transport, caching,
artifact transfer, and GitHub attestation.
Run the same operations locally with `just ci <task> ...` (see `just ci --help`),
for example `just ci revision <40-character-revision>` or `just ci audit linux
release-input release-input/SHA256SUMS release-input/LINUX-RELEASE-MANIFEST.json`.
`just ci-release --help` lists publication stages. The release commands require
an authenticated `gh` and contact GitHub; the local tests fake the CLI and
never publish a release. Bundle preparation and audit run on the host without
building a Podman image. Only the actual WebRTC builds need the Linux image.
The released-consumer job uses the published tag as a Cargo Git dependency and
proves both Linux x86_64 flavors without a local artifact override.

`just check` is the fast, offline source/control-plane gate once the pinned Rust
dependencies are cached. It does not require or extract native bytes. Native
runtime, ABI, resolver, and cold-consumer proof is explicit: pass an untracked
core Linux x86_64 archive and its trusted lowercase SHA-256 to `just
verify-artifact`. The archive can be built at this revision with `just build
core linux-x86_64`, using the producer-recorded SHA-256, or be an immutable
release/workflow artifact accompanied by its matching trusted audit checksum.

The tested Rust-only contract excludes downstream C and C++ compilation. Rust
still invokes the platform linker, and the target must provide the system
libraries and SDK inputs declared by the artifact manifest. Using a compiler
driver such as `cc` as Rust's linker is allowed; invoking that driver to compile
C or C++ source is not. The metadata check separately rejects a custom build
target on the local `cxx` runtime and any `cc` or `cxx-build` package reachable
from the bridge dependency graph. These checks are reusable by later artifact
download, cache, offline, and release consumer jobs.

`just build` validates the flavor, target, and host before it synchronizes the
pinned sources, configures and compiles WebRTC, exports the static closure, and
performs C++ and Rust compile/link-only consumer checks. After a failed
`gclient sync`, it removes only incomplete dependency Git checkouts without a
usable HEAD before retrying; a real upstream timeout can still fail the build.
Every flavor/target artifact carries the same generated bridge and portable
adapter sources, compiled with that job's target toolchain and ABI configuration.

On a version-tag push, the release workflow runs the Rust runtime suite for
both Linux x86_64 flavors. Linux x86_64 AddressSanitizer jobs rebuild both
flavors and exercise the lifetime-sensitive environment, provider, peer,
channel, video, and teardown tests; only their failure logs are retained, never
their instrumented archives. Other platform builds are not part of this release.

`just refresh-cxx` is the explicit networked maintenance operation for the
Rust-only CXX runtime. It downloads the version recorded in
`vendor/cxx/provenance.json`, verifies the crates.io package checksum before
touching the import, copies the selected upstream runtime, header, C++ runtime,
and license files byte-for-byte, and regenerates the packaging-only manifest
from `tools/cxx/Cargo.toml.in`. `just check` verifies the recorded inventory,
per-file digests, generated overlay, exact component versions, and dependency
graph without accessing the network.

Artifact builds obtain `cxxbridge-cmd` from the exact published package also
recorded in `vendor/cxx/provenance.json`. The package checksum and reported
version are verified before bridge generation. Each manifest records both CXX
package checksums plus the imported runtime/header, bridge definition, and
generated C++ output digests; export rechecks the packaged headers against that
metadata before consumer smoke tests run.

For development against a matching extracted artifact:

```console
PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR=.work/package/webrtc-core-linux-x86_64 cargo test
```

Without a local artifact, `build.rs` fetches `artifacts.lock.json` from the
GitHub release named `v<crate version>`, checks its bridge identity, scope and
exact tag-bound asset URLs, then verifies the selected archive's SHA-256 before
safe extraction. The release lock is trusted from GitHub over HTTPS and cached
by tag; unlike the archives, its digest is not pinned in the tagged source.
A replaced release asset and lock could therefore change what a cold consumer
receives. Do not move tags or replace release assets.

The cache defaults to `$CARGO_HOME/pulsebeam-webrtc-sys/artifacts-v1`; set
`PULSEBEAM_WEBRTC_SYS_CACHE_DIR` for a hermetic cache. `CARGO_NET_OFFLINE=true`,
Cargo's `--offline`, or `PULSEBEAM_WEBRTC_SYS_OFFLINE=true` disables downloads;
for offline builds, first populate both the release lock and selected archive
while online. The checked-in lock is a producer/candidate template with all 18
selections; it is not used by normal Git consumers. Only Linux x86_64 core and
native are currently released. Other targets require a matching extracted
artifact through `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR`.

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
build.txt         source repository/revision, flavor, target, toolchain, GN arguments,
                  and exported C++ definitions
manifest.json     versioned source and CXX producer provenance, native
                  configuration, ABI, archive, ordered link-input, and license
                  inventory identity
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
source is built without a local patch stack except for the immutable
`patches/core-ios-remove-framework-objc.patch`: core iOS device and simulator
artifacts apply that pinned delta to remove `sdk:framework_objc`; native iOS
and every other artifact use pristine upstream source. `build.txt` and
`manifest.json` record the patch digest and applied/pristine source state, and
only optimized release artifacts are published.

Linux releases use two explicit phases. The independently qualified primary
Linux group is exactly `core` and `native` for `linux-x86_64`; Linux arm64 is a
secondary tier. Registry publication remains deferred (`pulsebeam-webrtc-sys`
is not a crates.io dependency).

## Linux container development

Native Linux builds run in the locally built, repository-owned development image. Build the
image explicitly with Podman, then run the normal build or runtime recipe
inside it; the checkout is mounted read/write so generated outputs stay owned
by the calling developer.

```bash
just ci-preflight
just linux-image pulsebeam-linux
just linux-run pulsebeam-linux just build core linux-x86_64
just linux-run pulsebeam-linux just _runtime-test core linux-x86_64 dist/webrtc-core-linux-x86_64.tar.gz
just linux-run pulsebeam-linux just build native linux-x86_64
just linux-run pulsebeam-linux just _runtime-test native linux-x86_64 dist/webrtc-native-linux-x86_64.tar.gz
```

For a manual upgrade rehearsal, run `python3 tools/select_upgrade_pin.py
<40-character-commit>` in a disposable checkout before building the image.
This changes the local Justfile pin; the single hosted workflow has no separate upgrade-rehearsal mode.

After publication, use the same version tag that published the release
(replace `<tag>` with that tag, for example `v0.5.6`):

```toml
[dependencies]
pulsebeam-webrtc-sys = { git = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", tag = "<tag>" }
# For native capture/playback, add: features = ["native"]
```

On Linux x86_64, a normal `cargo run` then downloads and verifies the selected
artifact automatically. No separate setup is needed.

1. **One CI workflow** runs fast checks on pull requests. Push a new `v*`
   version tag to run the full Linux x86_64 core and native qualification, with
   runtime tests, ASan, audit, and cold candidate consumers. Publication starts
   automatically only if all qualification gates pass, the tag resolves to
   the checked-out commit, and it equals `v<package.version>` in `Cargo.toml`.
   Bump the crate version before creating a new tag. The resulting bundle contains two archives,
   `SHA256SUMS`, `LINUX-RELEASE-MANIFEST.json`, the repository license, and a
   schema-2 `artifacts.lock.json` with two Linux x86_64 URLs/digests and sixteen
   explicitly unavailable selections. Other targets are not released.
2. The workflow first creates a draft if no release exists, downloads and hashes
   any existing assets, and uploads only missing byte-verified assets. It never
   deletes, replaces, or clobbers an asset; a conflicting or unexpected asset
   fails closed. It rechecks the complete inventory, attests the closed bundle,
   and only then advertises the release. An interrupted draft is recoverable by
   rerunning the exact same tag with identical bytes.
3. After publication, CI tests both flavors using the tag itself as the Cargo
   Git dependency, with fresh checkouts and cold release-lock and archive caches.
   It reports the usable tag in the workflow summary. The tag works only after
   the matching release and its lock have been published.

Never replace an asset on an existing release. If publication is incomplete or
incorrect, use a new producer tag. Provenance, rather than byte-for-byte archive
identity across runners, is the reproducibility contract.

The automated host suite validates the portable API and exported static link
closure. It does not claim physical camera, microphone, speaker, GPU, platform
UI, or hardware-codec qualification; physical-device qualification remains a
downstream application responsibility.

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
or simulator code, production codec implementations, or a source patch stack
beyond the documented immutable core-iOS closure delta.
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

For a routine WebRTC upgrade, rehearse the proposed 40-character commit in a
disposable checkout using `tools/select_upgrade_pin.py`, then refresh the CXX
bridge and compile/link the core Linux adapter locally. Once reviewed, change
only `webrtc_commit` near the top of the `Justfile`, run `just check`, and push
a new version tag to run the qualified release workflow.
Change `depot_tools_commit` only when WebRTC compatibility requires it. Change
build arguments or target selection only to repair an observed required-matrix
failure.

Upgrade CXX as one reviewable change:

1. From the crates.io index and the matching upstream tag, update both the
   `cxx` and `cxxbridge-cmd` package versions and checksums plus the repository
   tag and commit in `vendor/cxx/provenance.json`.
2. Run `just refresh-cxx`. Review the imported inventory and regenerated
   per-file digests; do not edit any imported file.
3. Pin the same exact version in the root `Cargo.toml`, then update
   `cxxbridge-macro` and `Cargo.lock` with Cargo. Change the overlay only for an
   intentional packaging requirement and describe that delta in provenance.
4. Run `just check`, run the applicable native artifact build, then run
   `just refresh-cxx` once more and confirm that it produces no diff.

## Acceptance coverage

| Contract | Automated job or release check |
|---|---|
| Offline schemas, extraction, target/flavor substitution, link translation, source cleanliness, CXX inventory, and Rust-only dependency graph | **CI and release / Source checks** (`just check`) |
| Complete bridge and extracted-artifact C++/Rust compile/link for Linux x86_64 core and native | **CI and release / Build** plus `audit_release.py` |
| Deterministic execution/network, signaling, data channel, injected video path, and ordered teardown for both Linux flavors | Two **Runtime** matrix checks |
| Observer, callback, partial-construction, close/drop, and provider lifetime under instrumentation | Two **ASan tests** jobs |
| Fresh Git dependency, cold artifact and target caches, identity probe, host runtime smoke, and forbidden C/C++ compiler sentinels | **CI and release / Released consumer** |
| Mechanical CXX/generated-bridge refresh and pin changes | `just refresh-cxx` and Linux release qualification |
| Immutable URLs/checksums, complete external lock, licenses, notices, and attestations | Tag-triggered **publish** and **released-consumer** gates |
