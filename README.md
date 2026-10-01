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

## Actor-owned production execution

`ProductionSession` is a move-only `Send`, non-`Sync` owner for a production
resource graph. It constructs headless peers on system time and retains
WebRTC's existing network, worker and signaling threads. There is no additional
application owner executor. All mutations require `&mut self`; peers, channels,
encoded sources, RTP handles and sinks stay private. Copyable session IDs and
owned event/frame values are the application boundary, so an async actor may
retain the session across awaits and migrate between executor threads.

Use `ProductionSession::new(ProductionSessionConfig::default())` for encoded
Opus and data channels. An optional `video_format` selects direct encoded H.264
or VP8 input and encoded-only video reception, not a new H.264 decoder. Existing
input limitations still apply: this API alone does not implement layered or
opaque media. Low-level local handles and controlled execution remain non-Send
and cannot be imported into a production session. Production factories and
controlled worlds reject coexistence before starting production threads.

Drain `try_peer_event`, `try_channel_event` and any attached encoded sinks, then
await `ready()` (or call `poll_ready` from an executor). Readiness coalesces
activity, may be spurious and never enters the engine or drives a clock. A
single actor waiter is supported; its waker must schedule the actor, not run
engine operations inside a native callback. Native event locks are released
before waking. Each peer admits at most 64 asynchronous operations until their
terminal outcomes are consumed; excess admission fails synchronously with
`ResourceExhausted`. Closing retains peer observation identities so accepted
pending operations still produce one terminal outcome. Shutdown is idempotent;
owned outputs already returned remain valid.

Data channels expose native push delivery and send backpressure. Advisory
snapshots are bounded, but the Rust message inbox is not byte/count bounded and
consuming its events does not return SCTP receive-window credit. No binding-only
SID-retirement or substitute receive-credit policy is imposed. Layered and
opaque media remain unqualified; this ownership API alone is not the entire
agent-ready binding contract.

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

Headless audio sources accept owned, interleaved signed 16-bit PCM in exactly
10 ms blocks at 8, 16, 32 or 48 kHz, mono or stereo. The source's capture
microsecond value is validated but is not used as an RTP or absolute capture
clock. A receiver can select one decoded PCM sink or one encoded Opus sink
before packets arrive. The encoded sink delivers complete Opus packets without
passing them to NetEq's decoder, with RTP timestamp, payload type, SSRC and
48 kHz sample duration. It does not expose RED or other non-Opus payloads;
its bounded queue records dropped packets. The decoded headless sink pulls
10 ms of playout on demand when empty; it does not open a host speaker.
Native artifacts additionally allow opt-in platform audio with
`PeerConnectionFactory::builder().native_audio(true)`: enumerate recording
and playout devices, select them by current index, and make a microphone
track. A native peer can request recording/playout enable or disable with
`set_native_audio_enabled(recording, enabled)`. WebRTC starts and stops the
selected device as matching media streams appear or disappear; the setting is
shared by peers from the same factory. The upstream setter does not report
asynchronous device startup or playout failures. Platform-device methods and
types are not exposed without the `native` Cargo feature. Enumeration can be
empty and selection can fail while a device is active; CI smoke-tests this
path without requiring hardware.

For prerecorded Opus, select `AudioEncoderFactory::with_opus_frames()` on the
peer factory, then create an `EncodedAudioSource` and audio track with
`create_encoded_audio_source(channels)` (1 for mono, 2 for stereo) and
`create_encoded_audio_track()`. For stereo, the receiver must request
`stereo=1` in its negotiated Opus answer for that audio m-line. Selecting the
sender's advertised stereo capability with `set_audio_codec_preferences` alone
does not make the built-in receiver answer request stereo; its default answer
requests mono. Confirm the answer's fmtp before pushing stereo packets, since
a mono sender cannot consume a stereo carrier. Push complete Opus packets (without RTP
headers) with a matching mono/stereo TOC as `OpusInputFrame`, supplying the
48 kHz RTP timestamp and matching 10 to 60 ms duration. Input is limited to
1200 bytes per packet and must fit within 960 bytes per channel per 10 ms
block, minus 28 bytes for the first block's internal header. The adapter carries those
bytes to an Opus pass-through encoder without calling the Opus encoder;
WebRTC creates the RTP headers,
RTCP and encrypted transport. Each source is independent. The source admits at
most 24 outstanding 10 ms blocks and returns `Backpressure` before admitting
more, including when no track is attached. Retry without advancing the input
timestamp. This factory is Opus-only and rejects raw PCM sources; use a
separate peer factory for raw PCM tracks. The adapter does not validate that
packet bodies can be decoded. WebRTC cannot
re-encode these packets to match a changing bitrate; the caller controls
source bitrate and pacing. Controlled peers continue to reject audio transceivers because their cooperative codec queues
can deadlock; this input adapter does not change that limitation.

Native camera devices expose names, IDs, and capture capabilities through
`camera_devices()` and `camera_formats(id)`. `open_camera(id, width, height,
fps)` starts the closest upstream matching capability and owns a source for
`create_video_track(track_id, capture.source())`; an unsupported selection
returns an error. Refresh enumeration after device changes rather than caching
indices. `CameraCapture::status(timeout)` reports starting, streaming, stalled,
closed, or stopped capture; stalled means no frames arrived within the caller's
timeout, not a confirmed device-loss cause. `screen_sources()` and `window_sources()` enumerate desktop source
IDs, and `open_screen(id)` / `open_window(id)` create caller-paced captures.
Call `capture_next_frame()` to request a frame, inspect `status()` and
`failed_frames()`, and call `stop()` to end the capture and source. Wayland
portal selection may remain pending and can be cancelled or fail; an open
capture does not prove that frames were delivered. `stop()` reports failure
when a native device cannot stop or its source cannot end, while `Drop` is
best-effort. Native device smoke tests run without hardware or a display and
do not qualify actual microphone, camera, X11/Wayland portal, or speaker paths.
`examples/native_devices.rs` uses the public API for discovery and opt-in
capture. A portal response classified as cancelled by upstream can include a
denial; upstream's generic error does not distinguish missing service from
other failures. Wayland portal session closure has its own capture status.

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

## CPU video frame layout

`VideoFrame::i420` owns tightly packed planar I420 bytes. For padded planes,
`VideoFrameBuffer::i420_strided` copies the visible rows into packed I420.
`VideoFrame::nv12` accepts owned Y and interleaved UV planes with explicit
strides and converts to packed I420 before source submission; odd dimensions
round chroma sizes up. A decoded `VideoFrame` can be converted back into owned,
packed NV12 planes with `to_nv12()`. Conversions only rearrange bytes, without
scaling or color-space conversion. `VideoFrame::with_rotation` attaches
clockwise 0/90/180/270-degree metadata without rotating pixels; native source
submission, local sinks, and encoder/decoder callbacks preserve that metadata.
Each strided plane must contain exactly
`stride × rows` bytes, including padding after the final row; zero dimensions,
short/extra buffers, narrow strides, and overflow are rejected.

## Decoded video sink retention

Each attached `VideoSink` holds at most four decoded I420 frames and 16 MiB
of frame data. On overflow it drops oldest frames; frames exceeding the byte
budget are discarded. `VideoSink::dropped_frames()` reports cumulative loss,
including after explicit close. Polling `try_next_frame()` does not guarantee
receipt of every decoded frame. The cap applies per sink; applications should
close unused sinks to release retained frames.

## Audio processing

`PeerConnectionFactory::builder().audio_processing(config)` configures the
factory's WebRTC software AEC, NS and AGC module for raw PCM. A local raw
`AudioTrack` may call `set_audio_processing_options` to request disabled,
automatic, platform or software AEC, NS and AGC. The setter stores a request;
it does not fail when a platform effect is unavailable. During application,
libwebrtc may leave an unavailable platform-only effect disabled and log a
warning. Platform effects also require opt-in native audio. Inspect
`factory.audio_processing_state()` after starting capture to distinguish the
request from resolved software/platform effects and availability. A stored
request before attachment is not evidence of active processing. The voice engine is shared by tracks
in one factory, so concurrent per-track options are not independent. Encoded
Opus bypasses PCM processing; the synthetic Opus-frame adapter currently
rejects combining an encoded-input factory with PCM processing or native
microphones, and encoded tracks reject processing options. This is an adapter
constraint, not an Opus limitation.

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

`ControlledPeerDriver` borrows the caller's thread for peer network, worker and
signaling roles. Pair it with a matching manual-clock environment, cooperative
task queues, and `ControlledSimulatedNetwork` to pump multiple peers with
`run_ready` and `next_deadline`. Only one driver may own the process-global
WebRTC clock at a time; drop all controlled peers, endpoints, and networks to
release it. Native audio and independently threaded peer roles are rejected.
A seeded world's builder can opt into `controlled_media()`: pair Opus carrier
input with `builtin_opus()`, and either direct VP8 input with `builtin_vp8()` or
direct H.264 input with `input.encoded_receive_factory()`. The latter registers
the wire format only: decoder support queries return false, decoded receiver
sinks fail explicitly, and `attach_encoded_sink()` is the receive path. Ordinary
H.264 needs no DD; `new_l1t3()` requires native DD negotiation and sender L1T3
selection. The caller alone delivers packets, advances time and pumps queues.
Closing an encoded sink is terminal for that receiver; a new receiver is needed
to intercept again. It does not make H.264 decoding available.
The controlled network supports externally scheduled UDP delivery, client-side
TCP connect/data decisions, and caller-provided DNS answers for STUN/TURN
servers. Unknown names fail deterministically. A delivered TCP connect succeeds;
dropping it fails the socket, while dropping TCP data discards those bytes. The
external driver must implement any server response and inject received bytes.
TURN/TLS runs over the same caller-delivered TCP bytes with certificate
validation; TCP listeners remain unsupported. See
[`tests/controlled_driver.rs`](tests/controlled_driver.rs) for gathered-SDP
connectivity under virtual time, [`tests/controlled_turn_tls.rs`](tests/controlled_turn_tls.rs)
for TLS and authenticated TURN relay candidate gathering, and
[`docs/capability-matrix.md`](docs/capability-matrix.md) for remaining evidence gaps.

## QA responsibility

This adapter tests what it owns: public Rust/CXX construction and configuration,
representative raw/encoded payload and metadata mapping, negotiation and
rejection paths, backpressure, and resource shutdown. For simulcast/SVC and
H.264, use focused adapter integration or smoke tests, not an exhaustive
matrix of codec profiles, layers, packetization, and RTP metadata combinations.
The pinned libwebrtc project's QA is responsible for its codec, packetization,
and media-engine correctness. Physical device and desktop portal behavior remains the upstream project's
qualification responsibility; this repository does not establish that upstream
qualification for the pinned revision. Our headless smoke tests report
service-dependent scenarios they cannot exercise rather than count those
scenarios as passing.
This division does not waive missing adapter capabilities, broken supported
paths, controlled-media deadlocks, or representative adapter-side metadata
checks. See the capability matrix for tested paths and known gaps. The approved
Linux spec and its acceptance requirements are not amended by this guidance.

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
implementation must leave raw-input H.264 encoding and decoded H.264 output
unavailable or produce an explicit error; it must not change these artifacts.
Direct encoded H.264 transport uses the native packetizer with caller-provided
access units and does not require a compression or decoding implementation.

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

Schema-3 manifests also require `bridge.native_adapter_sha256`, a path-aware
SHA-256 of every `native/*.cc`/`native/*.h` input and `Justfile`. It participates
in native configuration identity and is checked against the current source
package by producer provenance, release audit and Rust-only consumers. Matching
CXX declarations alone cannot qualify an archive containing stale adapters or
build recipes; schema-2 manifests are deliberately rejected.

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

## Peer transport and stats (Linux core and native)

`PeerConfiguration` accepts STUN/TURN URLs with credentials and an explicit
`IceTransportPolicy::{All, RelayOnly}`. `turn:` URLs support UDP or TCP via
`?transport=udp` or `?transport=tcp`; `turns:` uses TLS with certificate
and hostname verification. By default, the pinned WebRTC roots are trusted.
`PeerConfiguration::turn_tls_ca_pem` can add a PEM-encoded CA for that peer's
TURN/TLS connections without disabling WebRTC's existing roots or hostname
checks. This option widens trust for all TLS servers configured on that peer;
it is not a per-server CA restriction. An invalid CA fails peer construction.
`PeerConnection::create_ice_restart_offer()` returns an
operation ID whose SDP arrives in `OperationComplete`. For non-trickle signaling,
set each local description, wait for `IceGatheringState::Complete`, then copy
its gathered SDP with `PeerConnection::descriptions()` and send that description
to the other peer. Applications own signaling; forwarding per-candidate events
is not required. `PeerConnection::descriptions()` copies current and pending
local/remote SDP in one signaling-thread snapshot; an empty-SDP `Rollback`
description restores
stable negotiation state, while a nonempty rollback SDP is rejected.

Data-channel advisory events have bounded retention: each channel retains at
most one callback-time state snapshot and one buffered-amount notification.
Intermediate states may coalesce. Buffered-amount notifications accumulate bytes
removed from the local send queue since the preceding notification, saturating
at `u64::MAX`; they do not establish remote application delivery. Polling returns
advisory events before messages, while messages retain FIFO ordering. Consuming
the terminal closed snapshot discards queued deliveries and suppresses later
events. These advisory bounds do not bound received-message storage or establish
consumption-driven SCTP receive backpressure.

`DataChannel::send_queue_capacity()` and
`ProductionSession::channel_send_queue_capacity()` expose the native per-channel
send-queue limit. It is not a negotiated maximum message size or receive budget.
`Sent` denotes local admission only. `DataChannel::error()` and
`ProductionSession::channel_error()` copy the current native error into an owned
value, preserving its kind, diagnostic message, optional detail and optional
SCTP cause code. No error on an ordinarily closed channel is not a failure.

The pinned PeerConnection data-channel path is push-based. Native dcSCTP
reassembly has its own receive window; native pre-observer/connecting delivery
also has an engine-owned queue which can close a channel with `ResourceExhausted`
on byte overflow. Neither is a Rust consumption-credit API. Once our observer
receives a message, its owned copy waits in the Rust inbox until consumed or
shutdown discards it. A paused Rust consumer can therefore accumulate message
storage; application consumption policy remains outside the binding. The
generic dcSCTP socket pull API is not exposed as a stock PeerConnection capability.
Native allocation, reset and stream-ID reuse behavior are preserved.
`DataChannel::set_event_observation` and the session counterpart expose native
observer registration/unregistration. Disabling quiesces new channel callbacks,
not transport; already-owned Rust events remain available. While disabled, the
engine can buffer messages or fail on its own overflow limit, and channel
activity does not notify readiness. Re-enabling drains native queued messages
when open and samples current state. This is not a consumption-credit API.

`PeerConnection::request_stats()` returns an operation ID, then a typed
`PeerConnectionEvent::Stats` snapshot or a terminal operation error. The
snapshot contains candidate-pair, transport, inbound/outbound RTP and data
channel records with object IDs and optional metrics; up to 256 records are
accepted per request. Take each snapshot event before requesting another.
Upstream RTT and jitter values are seconds; available/target bitrates are
bits per second. Metrics that WebRTC did not supply remain `None`.

`PeerConnection::video_transceivers()`, `video_senders()`, and
`video_receivers()` return owned video-only handles from snapshots of upstream
transceivers. `mid()` is absent before negotiation and
may become absent after rollback. `stop()` initiates standard stopping; a
subsequent negotiation completes it, after which upstream removes the
transceiver from enumeration. An existing handle remains readable.
`RtpSender::set_track(Some(&track))` replaces a live local video track from
the same peer factory without replacing the sender; `set_track(None)` detaches
it. The peer retains local tracks and their sources until replacement, removal,
close, or drop, even if caller-owned track and source handles are dropped.
Renegotiate if the remote track identity must change. A sender's `track()`
returns the retained local track when present.

`video_sender_capabilities()` and `video_receiver_capabilities()` expose
actual video RTP codec profiles, FMTP parameters, RTCP feedback and scalability
modes for each peer's factory. Select and order discovered sender capabilities
with `RtpTransceiver::set_codec_preferences()` before negotiating. Empty
preferences restore upstream defaults; unknown or repeated profiles fail.
Discovery includes resiliency codecs such as RTX, and does not install missing
H.264 encoder or decoder implementations.

`VideoCodecFormat::scalability_modes` preserves the native SDP format's declared
capabilities through custom encoder/decoder factories. Construct a recognized
mode with `VideoScalabilityMode::parse()` and add it using
`with_scalability_mode()`. Recognition is not codec support or selection: the
encoder must truthfully advertise it, and sender parameters select the mode.
Native `SetParameters` validates the capability list, so answering true in a
factory's support query alone does not enable a temporal profile. This format
API does not generate a GOP, dependency metadata or VLA.

`VideoEncoderSettings` reports the selected native `scalability_mode` and the
raw native `h264_temporal_layers` and per-stream `simulcast_temporal_layers`
counts independently at initialization. None is inferred from capability
declarations or submitted frames; an absent mode is preserved. The H.264 count
is absent for other codecs and may differ from the per-stream counts, which
also affect native H.264 initialization. An empty stream list preserves native
zero-stream configuration. Settings are `Clone`, not `Copy`.
Exposing these initialization facts alone does not select a temporal mode.

`RtpReceiver::attach_encoded_sink()` exclusively intercepts depacketized
encoded video access units before decoding. Its bounded four-frame/4 MiB queue
reports drops. Access units include optional RID, capture/receive timing, and
negotiated dependency-descriptor frame/layer/decode-target metadata; absent
metadata is not fabricated. For H.264, `data` is a depacketized Annex-B
access unit with start-code-delimited NAL units, not RTP payload fragments.
Closing the sink clears queued frames and resumes normal WebRTC decoding; this
pinned upstream receiver cannot safely be attached a second time, even after
close. This is not a raw RTP payload API.

`RtpTransceiver::header_extensions_to_negotiate()` returns the native complete
ordered extension snapshot. Change only each entry's `direction`, then apply
that full vector with `set_header_extensions_to_negotiate()` before the next
SDP negotiation. The engine validates the list and mandatory MID extension;
IDs and encryption remain native-owned. DD and VLA are disabled by default in
this pinned revision. `negotiated_header_extensions()` reports native negotiated
IDs and directions; stopped entries are not negotiated, including stopped
snapshots returned before negotiation. Negotiation does not prove that a frame
carried an extension. Libwebrtc computes and transmits VLA, but its pinned public
receiver, transformer and stats APIs do not expose received VLA allocations.
The encoded sink therefore makes no received-VLA claim.

For direct encoded sending, create `EncodedVideoInput::new_for_format()` with
an actual VP8, VP9, AV1 or H265 format (or `EncodedH264Input::new()` for the
constrained-baseline H.264 format), configure its `encoder_factory()` on the
peer factory builder, then call `create_source()` and `create_track()` on that
factory. Feed codec-matched `EncodedVideoAccessUnit` values with explicit
`EncodedVideoMetadata` via `push_encoded()`. Formats are advertised by the
supplied encoder factory; they are not claims that a bundled encoder exists.
A bare VP9, AV1 or H265 format uses the pinned upstream default fmtp; pass
explicit parameters when advertising another compatible profile or level.
The encoded bytes must match the negotiated format and the artifact must
include that codec's RTP packetizer. For H.264 the original `push()` method
accepts `H264AccessUnit` values with strictly increasing nonnegative
microsecond timestamps. Each value is
one Annex-B access unit (three- or four-byte start codes), with explicit
keyframe, dimensions and optional QP. Keyframes must contain SPS, PPS and IDR;
delta frames contain non-IDR slices. The advertised format is constrained
baseline, packetization mode 1, level 3.1; SPS must signal constrained
baseline profile and at most level 3.1. The native sender constructs a private
raw trigger solely to drive WebRTC's video stream scheduling. Caller-supplied
access-unit bytes bypass encoding, and WebRTC derives the RTP timestamp from
its capture clock. Each source has a stable `stream_id`; a bounded shared
16-frame/8 MiB pending queue evicts oldest frames under pressure, with
per-source `dropped_frames()` and `pending_frames()` observability.
`take_encoder_error()` retrieves and clears the latest source-keyed adapter
rejection or encoded callback failure; repeated errors coalesce and rejected
queued units also increment that source's drop count. This adapter
does not supply a decoder. The direct input source does not support encoded
simulcast, spatial SVC or H.264 packetization mode 0.

`EncodedH264Input::new_l1t3([64, 128, 255])` opts into single-encoding H.264
three-temporal-layer input. The supplied nonzero, nondecreasing cumulative
fractions must end at 255 and are passed to native encoder capabilities, not
used to construct a GOP or VLA. Negotiate native DD on both peers, then select
`L1T3` on the retained sender's parameters. Selection without negotiated DD
fails before native parameter mutation. Submit explicit temporal indices 0..=2
and H.264 `base_layer_sync` metadata; keyframes use index 0. Native initialization
must actually select L1T3 with one three-temporal-layer stream, or queued input
is rejected observably rather than emitted unlayered. Libwebrtc infers frame
dependencies from the supplied codec metadata. Its pinned H.264 fallback DD
structure has four decode targets, which are preserved, not rewritten to three.
Producers remain responsible for truthful compressed references and signaling.
`ProductionSession::new_l1t3(config, fps)` exposes the same profile in the movable
actor with owned extension snapshots and source-keyed encoder errors. `take_keyframe_request()` reports
and clears sender feedback after
WebRTC asks the adapter to encode a frame; a delta frame submitted during a
keyframe request is rejected. Idle sources have no keyframe-notification
promise: polling does not schedule `Encode`, synthesize frames or substitute
observed RTCP for native encoder feedback. `latest_rate_control()` exposes the last
native rate update for the encoder associated with the source's presentation
token. Later native rate callbacks update it without another submitted unit.
For `ProductionSession`, newly recorded keyframe/rate feedback and asynchronous
encoder errors notify actor readiness; unchanged rate and pending keyframe
snapshots coalesce. Notification does not dispatch engine work or create an
idle-source keyframe guarantee. Standalone sources retain polling access.
`VideoRateControl::layer_bitrates_bps` preserves the native five-by-four
spatial/simulcast and temporal allocation matrix: `None` is unset and `Some(0)`
is explicit zero; cells are per-layer, not cumulative. This is rate guidance,
not a received or independently declared VLA. The generic
`EncodedImageCallback::emit_with_metadata()` path also accepts codec-specific
VP8/VP9 packetization fields, native H.264 `base_layer_sync`, and explicit
simulcast, spatial and temporal indices for caller-provided encoders. These
fields are passed to libwebrtc, not used for binding-generated dependencies or
VLA; `emit()` retains the ordinary
single-layer behavior. Ordinary direct inputs reject layered units; the explicit
H.264 L1T3 profile allows its three temporal indices. Multiple RIDs, other SVC
modes and resolution scaling on a direct input source are rejected
explicitly before changing a sender.

`RtpReceiver::request_keyframe()` submits an RTCP keyframe request for a live
remote video receiver without guaranteeing that a remote sender honors it.
`RtpSender::request_keyframe(&rids)` submits a keyframe request for an
active sending video stream. An empty RID list targets all encodings; invalid
RIDs fail. Success means that the request was accepted, not that a keyframe
was emitted or that received video can be requested through this API.

`PeerConnection::add_video_transceiver_with_rids` configures explicit,
unique encoding RIDs before negotiation. Invalid RID identifiers and duplicate
RIDs fail before modifying the peer; upstream validation may reject additional
combinations. An empty list retains the default single encoding. Creation
configures identity, not a guarantee that the selected codec can emit every
encoding or that simulcast is negotiated with a remote peer.

A video transceiver's `RtpSender::parameters()` returns an owned encoding
snapshot. Edit `encodings` and pass the snapshot to `set_parameters()` on the
same sender handle. Supported fields are active state, maximum bitrate and
framerate, relative and absolute resolution scale, and scalability mode; RID
is inspectable but cannot change without renegotiation. Unsupported or invalid
values fail explicitly. A successful update consumes the snapshot transaction,
so obtain fresh parameters before another update. Absolute resolution scale
supersedes relative scale when both are set. Encoding order and count are
negotiated, not mutable here. This does not establish simulcast negotiation or
SVC interoperability.

The Linux artifact tests validate configuration, two-peer ICE restart through
renewed credentials and packet delivery, connected-peer stat relationships,
simulated IPv4 and IPv6 STUN Binding responses represented as server-reflexive
candidates in gathered SDP, authenticated TURN UDP/TCP relay through a local
hostname-resolved coturn fixture, authenticated TURN/TLS relay using an
additional fixture CA, rollback and pending/current SDP transitions, and
Rust-only linking. An explicitly invoked IPv6 coturn fixture also verifies
relay-only UDP connection over the host's global IPv6 interface, using gathered
SDP rather than individual candidate forwarding. This is host-dependent, so the
IPv6 test is ignored by the default suite and fails explicitly without a global
IPv6 interface. Run it inside the Linux runtime image with `cargo test --test turn
relay_only_ipv6_udp_with_gathered_sdp -- --ignored`, setting the same artifact,
Cargo home, and target-directory environment as `_runtime-test`. This establishes
one real Linux IPv6 path, not qualification for all production networks.
The UDP fixture checks that bad TURN credentials produce no relay candidate and
report a candidate error. The TLS fixture rejects an untrusted CA and a
hostname mismatch. Terminating the TCP relay after connection exposes
transport loss to the peer. Trickle ICE and remote end-of-candidates are outside
this non-trickle signaling contract; no upstream patch is required.

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

See [the capability-to-API/test matrix](docs/capability-matrix.md) for the
approved Linux scope, the public Rust surfaces, proven paths, and open gaps.
The table below describes repository jobs, not proof of every capability.

| Contract | Automated job or release check |
|---|---|
| Offline schemas, extraction, target/flavor substitution, link translation, source cleanliness, CXX inventory, and Rust-only dependency graph | **CI and release / Source checks** (`just check`) |
| Complete bridge and extracted-artifact C++/Rust compile/link for Linux x86_64 core and native | **CI and release / Build** plus `audit_release.py` |
| Deterministic execution/network, signaling, data channel, injected video path, and ordered teardown for both Linux flavors | Two **Runtime** matrix checks |
| Observer, callback, partial-construction, close/drop, and provider lifetime under instrumentation | Two **ASan tests** jobs |
| Fresh Git dependency, cold artifact and target caches, identity probe, host runtime smoke, and forbidden C/C++ compiler sentinels | **CI and release / Released consumer** |
| Mechanical CXX/generated-bridge refresh and pin changes | `just refresh-cxx` and Linux release qualification |
| Immutable URLs/checksums, complete external lock, licenses, notices, and attestations | Tag-triggered **publish** and **released-consumer** gates |
