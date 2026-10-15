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

Local raw-video sinks expose native `DiscardedFrame` and `ConstraintsChanged`
notifications through `VideoSink::try_next_event`; actor receivers use
`try_raw_video_sink_event`. Their separate eight-event FIFO retains the newest
notifications, with saturating overflow counts from `dropped_events` or
`dropped_raw_video_sink_events`. Constraint optionals and floating values are
copied unchanged. Local accepted notifications remain readable after sink close.
These are native callbacks, not inferred frame loss or source-state changes;
frame polling and frame-retention loss counters remain separate. Sink wants and
source constraint operations are still incomplete binding work.

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

Headless audio sources accept owned, interleaved signed 16-bit PCM with native
`sample_rate_hz / 100` samples per channel, at 100..=48000 Hz and 1, 2, 4, 6 or 8
channels. This includes 44.1 kHz without padding or truncation. The legacy
`AudioPcmFrame::timestamp_us` field is not an absolute capture clock. Use
`AudioSource::capture_time_now` to read the supported native timeline and
`push_frame_at` to supply a mapped actual capture instant, at native millisecond
precision. Custom non-controlled clocks have no implicit mapping. Capture
admission also protects the reached signed capture-difference arithmetic on
native sender resume. Native packetization and playout keep their own timing
semantics; successful source delivery is not an encode or wire acknowledgement.
`AudioTrack::attach_sink` exposes an owning local PCM queue with exact samples,
rate, channel geometry and optional capture milliseconds. Its collector implements
both native callback overloads: legacy callbacks have no capture timestamp;
timestamped callbacks preserve the full optional signed millisecond value. It does not expose the
encoded adapter's private carrier as PCM. `DecodedAudioFrame::capture_time_ms`
preserves the full native optional signed value; `capture_time_us` is a checked
projection and can be absent when that unit cannot represent it. Zero remains
distinct from absence. `RtpSender::set_audio_track` replaces or detaches one
source without rebuilding the sender; native renegotiation semantics remain
unchanged. The production actor provides `pcm_capture_time_now`, `push_pcm_at`
and `replace_audio_source` with the same contracts.

`AudioSource::options` returns owning native `AudioSourceOptions`, including
optional processing modes and binary ANA configuration bytes. `set_all` uses
native merge semantics: absent changes leave fields unchanged, while explicit
false, zero and empty configuration remain present. Editing this value does not
apply it to an engine. `AudioSource::set_options` and session
`set_audio_source_options` invoke native source mutation on signaling. Complete
input admission precedes mutation. Jitter-buffer fields retain their full
values here: source options reach the send channel, while this SDK applies
jitter-buffer configuration only on the separate receive channel. Storing a
source jitter option does not reconfigure NetEq.
Successful invocation of this void setter does not promise that every channel
accepted the options, or that its merge/default behavior cleared prior effects.
A source-only notification does not notify an unchanged live track in this SDK;
reattachment makes native SetSend consume updated source options. The existing
track processing request explicitly notifies its track but is narrower; source
options and the factory's effective processing state are distinct.
Source state/remoteness/volume and track state/kind/volume/signal-level queries
read the native objects. Missing signal-level support remains `None`, not zero.
`AudioTrack::status` and `RtpSender::audio_track_status` return owning snapshots;
a detached sender has no audio-track snapshot. `ProductionSession` exposes the
same source queries and installed-track snapshots by its owning identifiers.
Source/track volume setters preserve the native 0..=10 finite-input contract.
The pinned caller-fed source implements volume as a no-op with a reported value
of 1.0, even after requesting zero; no synthetic attenuation is applied to PCM
or to the encoded adapter's carrier. Setter completion means native invocation,
not a fabricated effective gain.

`AudioTrack::source` returns an owning native `AudioSourceView`;
`RtpReceiver::audio_track` and `RtpSender::audio_track` distinguish audio from
the existing video-track accessor. `AudioSource::as_source_view` captures the
same source interface without making another producer. Source-view aliases
retain their origin track/producer, peer where applicable, and factory;
`same_resource` compares actual native source identity. Remote views do not
invent PCM submission or a stop method. Attachments that would create an
owning peer dependency cycle are rejected before native mutation.

Source/track `observe_changes` and source-view `observe_volume` register native
observers with bounded owning event queues. Callbacks copy notifications without
calling Rust on native callback stacks. Overflow drops the oldest event and has
a consumable counter. `close` unregisters synchronously on signaling while
leaving accepted events consumable; aliases share registration and queue state.
The local source's native no-op volume observer produces no invented events;
remote-source volume events carry the actual native values. Session source-view
and observer identifiers provide the same operations and independent alias
retirement. Releasing an outer view/producer identifier does not invalidate an
observer or discard its queued callbacks. Polling notifications does not pump
media or advance a controlled clock.

`VideoSource::as_view` and `VideoTrack::source` retain owning video-source
interfaces without creating another frame producer. Views query native state,
remote/screencast/denoising metadata, encoded-output support and optional source
statistics. Stock push and RTP sources return no source statistics at this pin,
even after frames arrive; requested geometry is not substitute native output.
`same_interface` compares the actual SDK interface: different track-source
proxies can wrap the same producer without being the same interface.
`VideoTrack` exposes native kind, receive intent and all four content hints.
Track/source change observers are bounded, unregister synchronously on close,
and retain accepted events and overflow counts. Source views and observers retain
the exact producer/track/peer origin through final native release. Production
sessions provide opaque view/observer IDs and use the existing track IDs for
controls, preserving their captive single-owner migration contract.

`PeerConnectionFactory::create_media_stream` creates an owning native track
container, not SDP routing or receive synchronization. Get/Find return the exact
accepted track owners in native insertion order. Native Add rejects duplicate
IDs per media kind; Remove matches by ID even when passed a different track.
Successful membership retains each track's actual source/factory or received
peer origin, including across factories. Track aliases do not retain the stream.
Bounded stream change observers unregister synchronously on close and keep
accepted notifications and overflow counts consumable. `ProductionSession`
provides opaque stream/track/observer IDs, source/sender/receiver acquisition,
Get/Find/Add/Remove and sender replacement through returned owning track aliases.
Retiring IDs does not invalidate independently retained aliases or queued output.

`AudioTrack::processor` queries the native per-track processor and returns an
owning alias only when one exists. The pinned stock tracks return `None`, even
when the shared voice-engine APM is processing audio; source options and peer
statistics are not substitute processors. `AudioProcessor::stats` forwards the
remote-track condition and preserves every optional observation, including the
deprecated voice-detection value. No new processor/APM factory is provided.

Borrowed `MonoPcm`, `InterleavedPcm` and `DeinterleavedPcm` views support signed
16-bit and float PCM without owning or copying their storage. Mutable variants
carry exclusive borrows; separate-channel views retain each channel borrow.
Native shape helpers, interleaved `copy_from`, supported `copy_samples` and
planar `clear` preserve active prefixes and leave extra storage untouched.
Interleaved mono access requires one channel; native planar mono access returns
the first channel, including for a multichannel view. No layout conversion or
channel inference is performed.

`AudioFrame` and callback `AudioFrameMut` typed interleaved accessors retain the
existing shape, initialized-prefix and mute gates. Shape-taking mutable access
uses native prefix-only mute clearing, not full-buffer clearing. Frames are not
presented as planar storage. The pinned `CopySamples` template is ill-formed
for deinterleaved operands; these calls report `NativePlanarCopyUnavailable`
rather than substituting a copy algorithm. Empty copies report `EmptyCopy`
before native helpers can form invalid element-zero addresses.

A receiver can select one decoded PCM sink or one encoded Opus sink
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
`IceConfig` keeps optional native defaults distinct from explicit timers;
`resolved_defaults()` uses the SDK helpers. Native validation errors are
`RtcError` values. A successful void configuration setter means invocation,
not that the SDK applied every field; read `config()` for effective state.

`TransportCandidate` owns the full SDK candidate value, distinct from a JSEP
`IceCandidate`. Copies preserve its generated ID and byte-bearing fields.
Parsing and `verify()` are separate native operations. Packet sends retain
the exact native return and last error without claiming that the last error
describes every failed operation. Empty successful P2P datagrams are unsupported.

ICE packet observers copy borrowed payload/address/timestamp metadata and use a
bounded drop-oldest queue with an observable loss counter. Writable, ready,
gathering, received/sent packet and route subscriptions retire on the network
role. Engine-owned candidate/error/removal/selected-pair wiring stays intact;
those notifications use peer events, including
`IceSelectedCandidatePairChanged`. The pin has no individual cancellation API
for receiving-state or internal ICE-state hooks, so transient observers do not
install them; `receiving()` and `ice_transport_state()` remain live reads.
Copied packet observations remain consumable after revocation. Actor/session
counterparts use retained transport IDs and the same native operations.

Channel labels, protocols and stats string values retain native bytes, including
non-UTF-8 and embedded NUL. Creation and channel errors retain the message bytes
exposed by the SDK. Use explicit UTF-8 decoding when text is required. `PeerConnection::create_data_channel_bytes()`
and `ProductionSession::create_channel_bytes()` accept an optional typed
configuration; `None` passes the native null/default configuration. The existing
UTF-8 label conveniences pass an explicit configuration. Signed optional
retransmission inputs reach native normalization and validation: negative values
disable that setting and values above 65535 are clamped before mutual-exclusion
validation. The deprecated initialization `reliable` flag is ignored upstream;
configuration snapshots expose the actual native reliability property.

Data-channel advisory events have bounded retention: each channel retains at
most one callback-time state snapshot, one buffered-amount notification and one
negotiated maximum-message-size snapshot. Intermediate states may coalesce.
Buffered-amount notifications accumulate bytes removed from the local send queue
since the preceding notification, saturating
at `u64::MAX`; they do not establish remote application delivery. Polling returns
send completions before advisory events, then messages. Each queue retains FIFO
ordering. Consuming the terminal closed snapshot discards queued message deliveries
and suppresses later observer events, but send outcomes remain available. These
advisory bounds do not bound received-message storage or establish
consumption-driven SCTP receive backpressure.

`DataChannel::send_queue_capacity()` and
`ProductionSession::channel_send_queue_capacity()` expose the native per-channel
send-queue limit. It is not a negotiated maximum message size or receive budget.
`Sent` denotes local admission only. `DataChannel::error()` and
`ProductionSession::channel_error()` copy the current native error into an owned
value, preserving its kind, diagnostic message, optional detail and optional
SCTP cause code. No error on an ordinarily closed channel is not a failure.

`DataChannel::send_async()` and `ProductionSession::send_async()` return a
channel-local `DataChannelSendId`. Consume `DataChannelEvent::SendComplete` for
exactly one owned native result, including the native error detail and SCTP
cause code when provided. Success means native local admission, not remote
application delivery. If the engine discards the callback (for example after
transport detachment), the outcome is `DataChannelSendError::Canceled`, not a
fabricated native failure. Up to 64 operations per channel can be outstanding,
including completed outcomes not yet consumed. Admission exhaustion or a
payload larger than the native send queue returns `ResourceExhausted` without
allocating an id. Native negotiated size/state errors arrive asynchronously.
Close does not discard admitted outcomes while the channel remains retained.
Dropping the channel releases application observability; in-flight callbacks
retain only their owned completion storage, not a raw handle or application
owner. Completions still notify readiness with observer registration disabled.
Controlled sends post tasks and require the caller's driver pump.

`DataChannel::counters()` copies cumulative native message/byte counters; the
fields are independently sampled, not an atomic multi-field snapshot. Receipt
counts native delivery, not Rust inbox consumption. `reliable()` queries native
reliability. The session exposes equivalent owned queries together with label,
configuration, state and buffered amount. Priorities support the four convenient
presets and `DataChannelPriority::Custom(u16)` for the full native value range.
Native handling of explicitly assigned in-band ids and unassigned negotiated
ids is preserved rather than rejected by a binding policy.

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
observer activity does not notify readiness. Send completions still do.
Re-enabling drains native queued messages
when open and samples current state. This is not a consumption-credit API.

Peer state/negotiation advisories retain only their latest callback-time owned
snapshot, not a complete transition history or a later sample of engine state.
Non-advisory controls retain at most 128 records/256 KiB of string payloads,
plus four scalar advisory slots. Arrival maps share a 128-handle cap.
`PeerConnection::event_observation()` and
`ProductionSession::peer_event_observation()` expose coalescing, overflow and
current retention counters. Overflow drops a new candidate/error/arrival
observation, never an accepted operation terminal or a native SCTP channel.
This is binding observation capacity, not a transport window or a guarantee
that every remote resource is delivered to the application. Explicit close
discards queued controls/arrival handles, while preserving accepted terminals;
previously returned owned outputs remain readable.

`PeerConnection::request_stats()` returns `Result<OperationId, PeerError>`, then a typed
`PeerConnectionEvent::Stats` snapshot or a terminal operation error. The
snapshot's `report` retains the actual complete immutable SDK report, without
a 256-record truncation. Its `records` field remains a convenience projection
of candidate-pair, transport, inbound/outbound RTP and data-channel records.
Take each result event before requesting another; one pending or unconsumed
report retains admission per peer. Sender/receiver selectors and native cache
clearing are exposed locally and through `ProductionSession`. Optional selector
methods preserve the native empty-report result for null or foreign selectors;
they do not substitute a full peer report or fabricate a selector error.

`RtcStatsReport` supports native get/type filtering, ordered iteration, shared
retention, deep copying and exact native serialization. `RtcStatsReportBuilder`
owns native add/try-add/take/transfer operations and freezes without leaving
mutable aliases. Duplicate insertion/merge preconditions are guarded rather
than reaching native checks. `RtcStats` exposes all 20 pinned classes' typed
public fields, including inherited/deprecated fields, native defaults, copies
and valid base copies. Five base classes have protected SDK constructors;
constructing them directly returns `None`, while native base copying is usable.
`RtcStatsAttribute` preserves all 16 typed optional scalar/container variants,
full-width integers, byte strings and map keys. Undefined is not zero or an
empty container. Native `ToJson`/`ToString` retain upstream quoting/precision
limitations; use typed values for exact data. JSON serialization requires finite
object timestamps. Reports and owned values remain readable after peer close
and can move independently between caller threads. Upstream RTT/jitter values
are seconds; available/target bitrates are bits per second.

`request_legacy_stats(Standard | Debug)` on peers and `ProductionSession`
uses the deprecated native collector and emits a separate `LegacyStats` result.
It selects all tracks. `request_legacy_stats_with_track` also forwards an
optional owning `MediaTrack`, obtained from local audio/video tracks or actual
sender/receiver `track_reference()` getters. The actor uses opaque
`SessionMediaTrackId` resources canonicalized by actual SDK object identity,
not textual track IDs. Native false rejects synchronously without a callback;
the SDK can accept a foreign object whose ID matches a remembered report.
Accepted selections retain the track's own source and role context through
SDK task-capture retirement. Controlled close/drop cancels only the known
observer continuation without advancing time or dispatching queued work.
Owning endpoint `MediaTrack` references also expose typed source and capability
reads. Receiver `streams()` returns owning native `MediaStream` references with
byte-exact identity, audio/video traversal and native find operations. Track and
stream `observe(NonZeroUsize)` calls explicitly create independent SDK notifier
subscriptions, with bounded pending changes and observable overflow; aliases do
not subscribe as a side effect. ProductionSession canonicalizes the corresponding
reference IDs by native object identity and exposes the same reads and polling.
Remote source reads and notifier retirement run on their retained native role.
Source state is read from the SDK, including explicit receiver-stop transitions;
channel-survival policy is not a promise that peer close leaves a source live.
Absent processor/signal/stat values are not replaced with defaults.

Reports preserve native timestamps in milliseconds, report/metric names,
all seven typed value variants, byte strings and native formatting. SDK Value
references stay on signaling; copied payloads and retained immutable SDK IDs
remain usable after peer teardown and caller-thread migration. ID equality uses
the SDK comparison, not rendered strings. Legacy and modern requests share the
one-pending-or-unconsumed-stats-result admission limit.

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
returns the retained local track when present. Endpoint aliases and local-track
leases use actual native sender identity, not textual track/sender IDs. A stopped
sender retained across completed renegotiation remains distinct from a new
sender even when the SDK reuses its textual ID.

`RtpSender::dtmf_sender()` is source-wired for the native audio sender's owning
DTMF proxy (native video senders return `None`). It preserves native default
and explicit comma-delay insertion, capability/getters, and both observer
callback forms through bounded polling, with an explicit retained guard against
consumed signed-delay overflow. Repeated getters share a registration; the
handle retains its peer/factory origin. New fixtures are unrun, not qualification.
See [DTMF contracts](docs/dtmf-contracts.md) for sequence, lifetime and loss rules.

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
The encoded sink therefore makes no received-VLA claim. A test-only offline
native parser checks captured clear VLA/RID extensions against actual negotiated
IDs and native sender SSRCs after all controlled roots have been destroyed.
It proves sender wire generation, not authenticated received allocation; it
neither decrypts payload nor verifies SRTP authentication. The SDK still owns
metadata authentication. On Linux x86_64 with a matching core export:

```sh
python3 tests/build_native_vla_probe.py \
  --kit "$PWD/.work/package/webrtc-core-linux-x86_64" \
  --checkout-src "$PWD/.work/checkout/src" --output "$PWD/.work/native-probes/vla"
CARGO_HOME="$PWD/.work/cargo-home" \
PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR="$PWD/.work/package/webrtc-core-linux-x86_64" \
PULSEBEAM_NATIVE_VLA_PROBE="$PWD/.work/native-probes/vla" \
cargo test --offline --test controlled_media native_vla_wire_uses_provided_offline_parser \
  -- --ignored --nocapture --test-threads=1
```

The probe-dependent test is explicitly ignored in the default Rust suite; run
it separately for wire qualification. It checks native full three-rung
allocations, RID/index association, actual primary SSRC publication and the
full captured UDP payload-size bound (leaving IPv4/UDP overhead within 1500
bytes). Sender-only three-rung runs also exercise fragmented synthetic opaque
tails: the largest primary UDP payload observed was 1077 bytes, with full native
VLA present on all three primary SSRCs. This does not establish simultaneous
stock simulcast receipt or exhaustive MTU/retransmission behavior.

`controlled_h264_opaque_tail_survives_fragmentation_without_decode` separately
qualifies exact 4 KiB synthetic protected-tail reassembly for H264 without DD
and L1T3 with DD, on ordinary and loss/duplication/reordering links with exact
normalized replay. It retains clear Annex-B/NAL framing, valid SPS/PPS and the
slice fields consumed by the pinned native parser. Native may normalize the
unprotected SPS/VUI and framing, so only the protected VCL tail and its clear
slice prefix are compared. The SDK remains responsible for encryption,
authentication and syntax-safe protected framing; the fixture provides neither
cryptography nor a valid decodable-slice claim. Received keyframe, temporal,
dependency and SSRC metadata are checked independently. Native bitrate control
funds these large frames rather than overriding native allocation or pacing;
builtin H264 decode remains unavailable and encoded interception terminal.

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
16-capture-token/8 MiB pending queue evicts oldest complete captures under pressure, with
per-source `dropped_frames()` and `pending_frames()` observability.
`take_encoder_error()` retrieves and clears the latest source-keyed adapter
rejection or encoded callback failure; repeated errors coalesce and rejected
queued units also increment that source's drop count. This adapter
does not supply a decoder. Ordinary direct input does not support encoded
simulcast; spatial SVC and H.264 packetization mode 0 are unsupported.

`EncodedH264Input::new_simulcast()` opts into three-rung H.264 sending through
native per-image callbacks. Create a sender with three ordered RIDs, then use
`push_simulcast([unit0, unit1, unit2])` with matching capture timestamps, ascending
width/height and explicit simulcast indices 0..2. All three units are validated
and reserved as one bounded capture token; `pending_frames()` counts access
units, and eviction/drop accounting counts every unit. Images are published
separately, so a later native callback failure cannot retract an earlier image.
Native initialization must retain three ordered streams with matching active
rung geometry. Collapsed senders and ambiguous single-encoder fallback are
rejected asynchronously with `InvalidConfiguration`, including the native
single-active-rung fallback. Geometry is not substituted for encoding identity.
Inactive or unfunded rungs can be dropped observably. Native per-encoding
keyframe feedback is available via `take_encoding_keyframe_requests()`; this
has the same callback-only, non-idle guarantee. `ProductionSession::new_simulcast`
and `push_video_simulcast` expose the same bounded path in the actor.
`PeerConnection::set_bitrate` and actor `set_peer_bitrate` forward optional native
bandwidth-estimation constraints and native validation; they do not inject a
per-rung allocation. The initial native allocation can fund fewer than three
rungs. Stock libwebrtc does not receive all three simulcast rungs concurrently;
sender-only SFU-answer tests and outbound RID/SSRC statistics are not evidence
of simultaneous receipt or received DD/VLA on those rungs.

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
is explicit zero; cells are per-layer, not cumulative. This is the effective
encoder allocation after native bitrate adjustment. `target_bitrate_bps` and
`target_layer_bitrates_bps` report the separate target from the actual native
`SetRates` callback, before that adjustment. Each allocation retains its own
bandwidth-limited flag. Neither is reconstructed from the caller's last request;
native simulcast adapters can slice allocations before product delivery. These
values are rate guidance, not received or independently declared VLA. The generic
`EncodedImageCallback::emit_with_metadata()` path also accepts codec-specific
VP8/VP9 packetization fields, native H.264 `base_layer_sync`, and explicit
simulcast, spatial and temporal indices for caller-provided encoders. These
fields are passed to libwebrtc, not used for binding-generated dependencies or
VLA; `emit()` retains the ordinary
single-layer behavior. Ordinary direct inputs reject layered units; the explicit
H.264 L1T3 profile allows its three temporal indices, and the explicit simulcast
profile admits exactly three RIDs and non-scalable rung encodings. Other SVC
modes and unsupported resolution changes are rejected before sender mutation.

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
- certifying H.264 patent clearance or licensing a consuming product;
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
