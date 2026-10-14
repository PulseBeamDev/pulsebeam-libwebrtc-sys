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

Schema 4 manifests carry the canonical native configuration as well as its
SHA-256, including compiler, GN/protobuf, floating-point, bridge and source
identities. Consumers reject inconsistent configurations, stale adapters, altered
static archives and missing or altered inventoried notices. These checks detect
mismatches; they do not authenticate a local artifact supplied by its owner.
Older manifests require a matching rebuild, not relabeling.

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

### Native diagnostics

Generate ignored clangd inputs from a matching extracted Linux x86_64 kit and
the pinned SDK checkout:

```console
python3 tools/native_compile_commands.py \
  --kit .work/package/webrtc-core-linux-x86_64 \
  --checkout-src .work/checkout/src --check
```

The SDK's Chromium Clang, Bullseye sysroot, exported definitions and bundled
libc++ headers are required. `--check` parses the real codec adapter and verifies
that an intentionally invalid SDK member produces a type diagnostic, rather
than a missing-include cascade. Omit it to generate editor inputs only. This does
not replace native compile/link or runtime validation.

### Protobuf configurations and SDK execution

Linux defaults to `rtc_enable_protobuf=true`. Set
`PULSEBEAM_WEBRTC_PROTOBUF=false` when building either flavor to select the
pinned no-protobuf configuration. Use a separate `WEBRTC_WORK` and `WEBRTC_DIST`
for configurations that must coexist. The manifest records the selection. The
built-in event logger writes native encoded events with protobuf enabled; with
protobuf disabled the upstream factory produces its no-op logger and rejects
output start. Ordinary event values, caller-provided loggers, RTP-extension
helpers, software codecs and APM remain available in both configurations.

AddressSanitizer Linux kits require the pinned Clang/lld linker and sanitizer
runtime, including when linking a Rust-only consumer. Set
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER` to the checkout's
`third_party/llvm-build/Release+Asserts/bin/clang++`, and use
`RUSTFLAGS="-C link-arg=-fsanitize=address -C link-arg=-fuse-ld=lld"`.
A host GNU linker cannot consume the LLVM objects in those kits. This does not
require downstream C or C++ compilation.

`just _runtime-test <core|native> linux-x86_64 <archive>` executes the SDK C++
consumer, the real Rust-linked companion, and the Rust runtime suite against the
matching archive. The SDK consumer exercises every advertised built-in audio
format, VP8/VP9/AV1/H264 encode/decode, native video metadata, processed APM PCM,
and configuration-specific event/RTP helpers. Building alone only compile-links
these consumers. Physical device behavior remains explicitly opt-in and is not
certified by software codec or APM execution.

## Actor-owned production execution

`ProductionSession` is a move-only `Send`, non-`Sync` owner for a production
resource graph. It constructs headless peers on system time and retains
WebRTC's existing network, worker and signaling threads. There is no additional
application owner executor. All mutations require `&mut self`; peers, channels,
encoded sources, RTP handles, DTMF senders, transports and sinks stay private. Copyable session IDs and
owned event/frame values are the application boundary, so an async actor may
retain the session across awaits and migrate between executor threads.
Final drop from an SDK callback moves the entire private resource graph to a
cleanup caller, allowing the callback to return before its native roles retire.
That cleanup is asynchronous; ordinary caller-thread drop remains synchronous.
The cleanup caller does not execute application operations or controlled work.

Use `ProductionSession::new(ProductionSessionConfig::default())` for encoded
Opus and data channels. For caller-fed PCM, select
`ProductionSession::builder(config).pcm_audio()` and optionally
`.audio_processing(settings)` before building. The actor owns PCM sources,
sender-track processing choices and exclusive encoded/decoded receive sinks;
headless PCM playout remains caller-paced. Platform devices additionally require
the `native` feature and explicit `.native_audio(true)` selection. See
[`docs/audio-runtime-contracts.md`](docs/audio-runtime-contracts.md) for receive
mode, processing and device contracts. An optional `video_format` selects direct encoded H.264
or VP8 input and encoded-only video reception, not a new H.264 decoder. Existing
input limitations still apply: this API alone does not implement layered or
opaque media. Low-level local handles and controlled execution remain non-Send
and cannot be imported into a production session. Production factories and
controlled worlds reject coexistence before starting production threads.

Raw media uses explicit factory selection without changing those encoded-input
defaults. For PCM, select compatible stock audio factories on the builder:

```rust,no_run
use pulsebeam_webrtc_sys::*;
let encoder = AudioEncoderFactory::builtin()?;
let decoder = AudioDecoderFactory::builtin()?;
let mut session = ProductionSession::builder(ProductionSessionConfig::default())
    .audio_encoder_factory(&encoder)
    .audio_decoder_factory(&decoder)
    .build()?;
let source = session.create_pcm_source()?;
let peer = session.create_peer(PeerConfiguration::default())?;
let transceiver = session.publish_pcm(
    peer, source, "voice", RtpTransceiverDirection::SendOnly,
)?;
let frame = AudioPcmFrame::i16_interleaved(48000, 1, 0, vec![0; 480])?;
session.push_pcm(source, &frame)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

For raw video, `create_raw_video_source`, `publish_video` and `push_raw_video`
use the selected video encoder. Selecting only an encoder or decoder factory
keeps the stock default for the omitted side; an explicit factory's null or
failed product is never replaced by a stock product. Supply actual capture timestamps mapped into
`clock_timestamp`'s native timeline, not elapsed time from an unrelated epoch.
`replace_video_source` accepts either raw or supported encoded video sources.
After negotiation, select `attach_pcm` or `attach_raw_video` on the receiving
endpoint and consume owned values with `try_pcm_frame` or
`try_raw_video_frame`. Raw sinks require explicit polling; in headless mode a
PCM poll may pull one native playout block. Readiness alone does not drive
headless audio playout or guarantee raw-video notification. One receive mode
is active per actor receiver. `close_media_sink` detaches it before reattachment
or an audio-mode switch; encoded-video interception retains its existing
receiver-lifetime restriction. Returned frames own their data across source
replacement, sink closure, session migration and shutdown.

Drain `try_peer_event`, `try_channel_event`, registered DTMF/DTLS/SCTP observer
queues and any attached encoded sinks, then await `ready()` (or call `poll_ready` from an executor). Readiness coalesces
activity, may be spurious and never enters the engine or drives a clock. A
single actor waiter is supported; its waker must schedule the actor, not run
engine operations inside a native callback. Native event locks are released
before waking. Each peer admits at most 64 asynchronous operations until their
terminal outcomes are consumed; excess admission fails synchronously with
`ResourceExhausted`. Closing retains peer observation identities so accepted
pending operations still produce one terminal outcome. Shutdown is idempotent;
owned outputs already returned remain valid. Transport IDs refer to retained
native public facades; each ICE-internal operation reacquires its revocable
pointer on the network role and reports `InternalUnavailable` after native
teardown. DTLS information and certificate copies are independent owned values;
SCTP snapshots contain the exact callback-time DTLS ID. Repeated endpoint and
transport lookups share actual native identities and observer queues, not string
ID joins. Explicit `release_*_transport` and `release_dtmf_sender` calls retire
an individual actor entry and invalidate copies of that ID. `close_source(id)` idempotently
retires an individual producer and removes its actor-owned entry; existing
native track references can remain until the sender is detached/stopped.

Data channels expose native push delivery and send backpressure. The owned
message inbox is bounded by `DataChannel::receive_queue_limits()` (also available
on `ProductionSession`). Newest deliveries exceeding its message or byte limit
are dropped before copying and reported through `DataChannelEvent::MessagesDropped`.
Overflow does not fabricate a native error or close the channel. Aliases share
one observer, inbox and send-completion budget. Consuming events does not return
SCTP receive-window credit. No binding-only SID-retirement or substitute
receive-credit policy is imposed. Layered and
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

Linux x86_64 build configuration enables bundled OpenH264 encoding and FFmpeg
H.264 decoding in both flavors. Explicit `VideoEncoderFactoryHandle::builtin()`
and `VideoDecoderFactoryHandle::builtin()` selections expose the artifact's
native discovery/support. Other target configurations still disable bundled
H.264. Codec availability comes from the matching native artifact, not from a
Rust source update alone.

Headless audio sources accept owned, interleaved signed 16-bit PCM in exactly
10 ms blocks at 8, 16, 32 or 48 kHz, mono or stereo. The source's capture
microsecond value is validated but is not used as an RTP or absolute capture
clock. A receiver can select one decoded PCM sink or one encoded Opus sink
before packets arrive. Audio sink close releases its reservation for reattachment
or a mode change. Encoded close restores native decoding for future packets;
stop reception before closing when opaque packets must never reach a decoder,
and resume only after a new encoded sink is attached. Dropping an already closed
sink does not affect a replacement. The encoded sink delivers complete Opus packets without
passing them to NetEq's decoder, with RTP timestamp, payload type, SSRC and
48 kHz sample duration, native sequence number, received audio level and optional
RFC6464 voice-activity bit. The pinned native incoming frame's `Type()` projects
the received V bit when `AudioLevel()` is present; absent extension means unknown,
not silence. This is not inferred from carrier PCM or decoder speech decisions.
It does not expose RED or other non-Opus payloads;
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
block, minus 36 bytes per block for the internal envelope. The adapter carries those
bytes to an Opus pass-through encoder without calling the Opus encoder;
WebRTC creates the RTP headers,
RTCP and encrypted transport. Each source is independent. The source admits at
most 24 outstanding 10 ms input copies across all attached senders and returns
`Backpressure` before admitting more, including when no sender is attached.
Each native encoder consumption releases only its own input slot. Detaching a
sender does not release its still-queued copies; if native input is discarded
before encoding, capacity is conservatively retained until source close.
Retry without advancing the input timestamp. This factory is Opus-only and rejects raw PCM sources; use a
separate peer factory for raw PCM tracks. The adapter does not validate that
packet bodies can be decoded. For independent producer loudness and V metadata,
construct `OpusAudioLevel::new(level_dbov, voice_activity)` and call
`push_opus_with_audio_level()` (or the explicit mapped capture-time variant).
Level is validated in 0..=127, independently of V, and is never inferred from
Opus bytes or the adapter's private PCM carrier. Native encoder speech and the
supported sender-frame level setter feed native RFC6464 serialization. If the
extension is not negotiated, received level/V remain absent. The ordinary
`push_opus()` API remains unchanged and does not declare a producer level.
`ProductionSession::push_opus_with_audio_level()` exposes the same owned-value
submission without leaking sequence-bound handles. For explicit capture stamps,
`EncodedAudioSource::capture_time_now()` reads the qualified native capture
timeline: controlled world time or default system-clock production time.
`push_opus_at()` and `push_opus_at_with_audio_level()` require the actual capture
instant mapped into that timeline, not an unrelated clock epoch or an assumed
enqueue time. Native forwarding has millisecond precision; received capture
values use the negotiated native NTP domain. Custom non-controlled clock domains
remain rejected, and future capture instants fail before admission in both profiles.
The actor exposes `opus_capture_time_now(id)` and both explicit-time submission
variants without exposing local handles. Legacy unstamped production submission
is unchanged and does not invent a producer capture instant.
Missing or unsupported native handoffs drop the frame and increment `AudioEncoderFactory::opus_frame_handoff_failures()`;
that counter is an adapter failure observation, not a delivery receipt. The
handoff keeps at most one emission per native encoder and uses the provided
encoder queue, not RTP timestamps, SSRCs, or payload hashes, to consume it.
WebRTC cannot
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
to intercept again. This controlled selection does not admit H.264 decoding,
even when the artifact bundles a decoder.
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

Linux x86_64 core and native configurations use `proprietary_codecs=true`,
`ffmpeg_branding="Chrome"`, `rtc_use_h264=true` and
`rtc_system_openh264=false`. The FFmpeg branding is required for actual H.264
decoding, not just advertised formats. Use matching exports built with these
arguments, not archives produced with the historical disabled configuration.
The shipped-archive C++ smoke fixture checks actual H.264 encode/decode output;
`tests/video.rs` exercises Rust peer transport with the built-in factories.

Use explicit `VideoEncoderFactoryHandle::builtin()` and
`VideoDecoderFactoryHandle::builtin()` peer-builder selections for the stock
video codecs. Omitted selections do not silently install video factories, and
custom creation failure does not fall back to a stock codec. Controlled media
still admits only its documented selections; packaged H.264 is not a proof of
caller-pumped decoding with no extra threads.

The export includes dependency notices, WebRTC PATENTS and configuration
provenance. OpenH264/FFmpeg licenses and H.264 patent obligations must be reviewed
for the consuming product's use and distribution. Technical enablement and
passing tests are not patent clearance or publication authorization. Consumers
may instead explicitly select a hardware-backed or separately licensed factory.
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
   runtime tests, ASan, audit, and cold candidate consumers. Before building any
   container or native code, the first job checks that the repository is the
   canonical producer, the tag resolves to the checked-out commit, and it equals
   `v<package.version>` in `Cargo.toml`. Pull requests skip this tag-only check.
   Bump the crate version (and update `Cargo.lock`) before creating a new tag;
   for example, `v0.6.0` cannot release a crate still declaring `0.5.6`.
   `just ci release-tag <tag> <commit>` runs the same check locally without a
   container, GitHub authentication, or marker files. The tag must already exist
   in the local checkout. Publication starts automatically only if all
   qualification gates pass. The resulting bundle contains two archives,
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

## Factory configuration (Linux core and native)

`PeerConnectionFactory::builder().options(FactoryOptions { .. })` applies
native factory options before peer creation. `factory.set_options(options)`
replaces options for subsequent peers; existing peers retain their construction
options. `ProductionSession::set_factory_options()` provides the same operation
under exclusive actor access, including before the first peer and after caller
migration. Native `SetOptions` has no validation outcome or readback getter;
the actor method can still reject a closed session.

The defaults preserve encryption, the native network-monitor choice, a
loopback-only ignored-adapter mask, and the native DTLS 1.3 maximum.
`NetworkAdapterMask` supports bitwise composition and retains unknown native
bits. `SslProtocolVersion` includes the native unspecified and legacy values,
with `DTLS_10`, `DTLS_12` and `DTLS_13` aliases. The pinned engine has a DTLS 1.2
minimum, so legacy selectors do not enable obsolete protocols. DTLS 1.3 remains
subject to native build support. These options do not configure TURN TLS.
Setting `disable_encryption` is insecure and prevents DTLS-dependent SCTP
channels. See [factory configuration contracts](docs/factory-configuration-contracts.md).

## Crypto option values (Linux core and native)

`CryptoOptions` provides owned native crypto settings and native SRTP cipher-order
queries. `KeyExchangeGroup` queries the selected binary's supported groups and
names while retaining open protocol identifiers. `KeyExchangeGroups::update`
applies native field-trial defaults through a synchronously borrowed environment;
the resulting values can be moved into actor configuration. Querying an empty
SRTP profile selection returns an error instead of triggering native `RTC_CHECK`.
These helpers do not install frame providers or configure a peer by themselves.
See [crypto option contracts](docs/crypto-options-contracts.md).

## Native certificate resources (Linux core and native)

`Certificate` provides owned native generation, PEM/chain import and export,
expiration queries, chain snapshots, and native fingerprint lookup.
`CertificateKeyParams` preserves native RSA parameter validation and P-256
generation. Clones retain native identity; imported PEM creates a distinct
resource with native value equality. Immutable resources may cross actor
caller threads under the qualified pinned BoringSSL backend. Synchronous
generation uses native system epoch time and crypto randomness, not a manual
environment clock. Exported PEM contains sensitive private-key material.
Retained resources can be supplied through `RtcConfiguration::options.certificates`;
this does not expose an async generator. See
[certificate contracts](docs/certificate-contracts.md).

## Peer configuration values (Linux core and native)

`RtcConfiguration` exposes owned native safe/aggressive presets, full
`RtcIceServer` values, certificates, crypto, SDP/ICE policies, timing,
network/VPN, allocator, SCTP and media-related configuration values.
`create_peer_connection_with_configuration` and actor
`create_peer_with_configuration` accept it without changing existing
`PeerConfiguration` and `IceServer` literals. Native `configuration` and
`set_configuration` operations have actor counterparts and preserve native
update errors and certificate resource identity. The candidate-pool range is
0..=65535, with invalid creation input rejected before allocator work.
Provider and environment dependencies are separate remaining parity work.
See [peer configuration contracts](docs/peer-configuration-contracts.md).

## Peer transport and stats (Linux core and native)

`PeerConfiguration` accepts STUN/TURN URLs with credentials and an explicit
`IceTransportPolicy::{All, RelayOnly}`. `turn:` URLs support UDP or TCP via
`?transport=udp` or `?transport=tcp`; `turns:` uses TLS with certificate
and hostname verification. By default, the pinned WebRTC roots are trusted.
`PeerConfiguration::turn_tls_ca_pem` can add a PEM-encoded CA for that peer's
TURN/TLS connections without disabling WebRTC's existing roots or hostname
checks. This option widens trust for all TLS servers configured on that peer;
it is not a per-server CA restriction. An invalid CA fails peer construction.
`PeerConnection::create_offer()`, `create_ice_restart_offer()`, `create_answer()`,
description setters and `add_ice_candidate()` return
`Result<OperationId, PeerError>`. Admission retains at most 64 pending operations
plus unconsumed terminal outcomes per peer, also in the native adapter.
Only one SDP operation (offer/answer/description setter) may be pending per peer.
A second SDP operation fails with `ResourceExhausted` until the native callback
completes, avoiding the pinned engine's queued-SDP shutdown callback hazard.
Completed outcomes still share the 64-slot admission budget until consumed.
Exhaustion fails synchronously with `ResourceExhausted`, and closed submission
fails with `Closed`; neither creates another queued outcome. Native shutdown
may report its own error before binding cancellation; accepted outcomes are not
guaranteed to have the `Closed` error kind. An accepted
offer's SDP arrives in `OperationComplete`. For non-trickle signaling,
set each local description, wait for `IceGatheringState::Complete`, then copy
its gathered SDP with `PeerConnection::descriptions()` and send that description
to the other peer. Applications own signaling; forwarding per-candidate events
is not required. `PeerConnection::descriptions()` copies current and pending
local/remote SDP in one signaling-thread snapshot; an empty-SDP `Rollback`
description restores
stable negotiation state, while a nonempty rollback SDP is rejected. The
snapshot's `local()` and `remote()` accessors prefer pending SDP over current
SDP, matching native effective-description access.

`create_offer_with_options()` and `create_answer_with_options()` take
`OfferAnswerOptions`, including native receive counts, voice-activity detection,
ICE restart, BUNDLE, packetization, legacy SCTP syntax, and Plan B simulcast
options. Defaults match the engine. Counts outside native integer range fail
before admission; native option validation otherwise arrives in the operation
completion. `set_local_description_auto()` lets signaling state choose,
generate and apply an offer or answer without manual SDP handling. Its success
completion carries no description; copy the resulting `descriptions()` snapshot.
It shares ordinary SDP admission, cancellation and controlled-media restrictions.

`restart_ice()` marks future offers for restart without generating SDP. Use
`should_fire_negotiation_needed(event_id)` to validate an observed negotiation
notification before delivering it: native operations or later state changes can
make a queued ID stale. `add_ice_candidate_sync()` and `remove_ice_candidate()`
preserve the native synchronous choices. They do not create operation IDs or
wait behind the asynchronous SDP operations chain. Candidate parsing errors
carry `PeerErrorKind::Syntax`; native acknowledgment or rejection is an
`IceCandidateDisposition`, since this native API supplies no error detail for
rejection. Acceptance is not connectivity evidence or proof that a removed
candidate previously existed. All these operations also have owned-ID
`ProductionSession` counterparts and retain native sequencing after actor
migration.

Existing senders also support generic `set_media_track()`, including audio
replacement and `None` detachment. `ProductionSession::set_sender_track()` uses
owned track IDs. Native boolean rejection remains distinct from binding errors;
accepted replacement retains the original source dependencies across aliases.
The existing video convenience shares its admitted-source/layout checks rather
than providing a bypass for controlled-media profiles.

`reconfigure_bandwidth_estimation()` accepts typed
`BandwidthEstimationSettings`, including `allow_probe_without_media`. It invokes
the SDK's asynchronous worker reconfiguration, which can restart estimation.
Success is not proof of a probe or a bandwidth allocation. Padding probes still
require an appropriate sending RTX-capable native transceiver.

`set_data_channel_message_observation()` installs or replaces the SDK's
peer-wide message observer independently of individual channel observers.
`DataChannelMessage` peer events retain native Unix-millisecond timestamps,
native SCTP channel IDs, label bytes, send/receive direction, text/binary kind
and payload bytes. A native channel ID is not a `SessionChannelId`; no identity
join is guessed from IDs or labels. Copied observations use the existing bounded
peer queue and its loss counters, not transport buffering or consumption credit.
Disabling from an ordinary caller fences SDK network-thread replacement while
retaining queued copies. Caller-owned controlled roles must execute their own
posted SDK task; the binding does not pump for them. Replacement from an SDK
network callback is rejected to avoid a signaling/network blocking-invoke cycle.
SDK-owned callback state remains valid until native replacement or teardown.

Peer events also expose actual legacy `RenegotiationNeeded` callbacks and native
`InterestingUsage` integers. The latter uses the SDK's configured reporting delay
and heuristics, not a binding-generated report. The
`IceConnectionReceivingChanged` hook is wired, but this SDK pin has no source
invoker for it; use live ICE receiving-state reads rather than expecting a
synthetic callback. Renegotiation/receiving advisories are bounded and coalesced
with observable accounting. Interesting-usage log occurrences instead retain
distinct records subject to the bounded control queue and its drop counters.

The deprecated description observer overloads are available as
`set_local_description_legacy()`, `set_remote_description_legacy()` and
`set_local_description_auto_legacy()`. They invoke the SDK's delayed observer
path and share the same bounded admission and exactly-one terminal result
contract. A completion can describe an earlier state; use current description
snapshots for current state.

Factories can create owning byte-ID `MediaStream` values. Native track
add/remove results remain distinct from peer `add_stream()` acceptance and the
void `remove_stream()` invocation. The latter operations and local/remote peer
collections require Plan B semantics; Unified Plan is rejected before the SDK's
fatal precondition. Source leases survive aliases, stream getters and accepted
peer retention without peer-retaining track cycles. `StreamAdded`,
`StreamRemoved` and `TrackAdded { receiver, streams }` preserve actual callback
references, including the deprecated callbacks alongside `Track`. Arrivals
share the existing bounded queue; oversized stream-association vectors are
dropped observably rather than truncated. Actor counterparts retain IDs for the
same native objects, and baseline Opus/video track constructors do not require
implicitly publishing an auxiliary transceiver. `senders()`, `receivers()`
and `transceivers()` copy one actual native collection snapshot; the latter
requires Unified Plan. Stream source leases follow native collection and sender
identity, including the SDK's ID-based track removal semantics.

Retained ICE access is revocable: `IceTransportInternal` reacquires the internal
transport inside each network-role call. It exposes actual configuration,
credentials/role/gathering controls, candidate mutation, candidate-pair and
connection statistics, packet send/options/errors, and network routes.

[Showing lines 1-837 of 1324 (50.0KB limit). Use offset=838 to continue.]
