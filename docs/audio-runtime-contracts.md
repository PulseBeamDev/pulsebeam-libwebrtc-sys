# Audio runtime contracts

## Caller codec installation

Caller audio factory, encoder, decoder and parsed-frame handles are standalone
owners. They are not selectable peer or shared `Session<R>` codec dependencies.
The pinned SDK does not provide the recoverable error paths required to install
arbitrary fallible caller encoders safely:

- The voice engine repeats `QueryAudioEncoder` and consumes an optional result
  after only a DCHECK. A caller that previously reported support can subsequently
  reject the same format, return an error, panic or reenter. Returning nullopt
  cannot safely reject that later native operation.
- `AudioEncoder::Encode` returns `EncodedInfo`, not an error. Native sample and
  appended-byte invariants use fatal checks. ACM treats empty output as buffering
  or an empty packet, so empty metadata is not an encoding-failure channel.

Standalone wrappers preserve current Query answers and typed callback failures.
They do not authorize installed use, cache a prior success, fabricate supported
metadata, substitute stock products, or turn encoding errors into successful
buffering. The installed caller-codec consumer contract remains an open
binding/qualification gap, not evidence that caller codecs are upstream
unavailable. Runtime-injection changes do not authorize rewriting codec error
semantics or treating standalone forwarding as installed-consumer proof.

## Common standalone construction

Generic `R: SessionRuntime` applications configure the existing native factory
with `runtime.environment_factory()`, then call
`runtime.create_environment(&factory)` to attach ownership to the completed
selection. Existing APM, codec, NetEq and logging constructors consume that
same environment. Managed contexts retain creator-side retirement authority;
no peer factory, separate clock, cleanup worker or media profile is required.
Release products and SDK environment aliases before the final caller-owned
runtime scope. Context-free native mixers keep their ordinary constructor.

`tests/runtime_standalone.rs` exercises actual APM processing, source-backed
mixing, G711 encode/decode and stock log output with one procedure for threaded
and managed runtimes, including release of configuring owners before use and
failed-configuration recovery. Core/native and core ASan with leak detection
passed. These are bounded construction/consumer results, not complete NetEq,
VoIP or all-software-codec deterministic execution qualification.

### Active standalone APM and live configuration

The generic standalone application also creates a second actual builtin module
with `AudioProcessingModule::builtin_with_config` through the selected runtime
environment. It releases the configuring factory and original environment before
initialization, processing and configuration replacement. The module retains its
real creation context; no peer factory, physical device or alternate DSP is used.

At 16 kHz mono, native pre-amplifier gain 2 and capture-level pre/post gains
3/0.5 transform the first 160 input samples from 1000 to 3000. The same module's
`apply_config` then changes capture pre-gain to 2 and the next block becomes
2000. Each caller buffer has nine extra samples that must remain unchanged.
These sample-level results witness the active native effect and live replacement,
not just construction or config readback. Initialization and both processing
statuses must be native `OK`; actual configurations must equal the requested
complete values.

The full input/output buffers, statuses and typed configuration snapshots are
part of the existing standalone observation. Its JSON trace renders each full
plain-value Rust config snapshot, rather than the SDK's shorter `ToString`
subset. All observations match threaded/managed execution, repeated managed
scopes, fresh processes and both Linux artifact flavors. Core ASan with leak and
stack-use-after-return checks passed alongside the existing native config tests.
Eight complete managed application lifecycles passed continuous monitoring with
no physical thread/helper-process creation or real socket syscalls. Managed time
remains frozen. The existing `PULSEBEAM_STANDALONE_MIXING_TRACE_PATH` includes
these observations alongside default APM, G711, mixer and logger consumers.

This is bounded standalone effect/control/ownership qualification, separate from
the installed peer pre-amplifier case. It does not qualify all adaptive/noise,
render/capture, echo, multichannel or configuration-rejection paths.

### Distinct-rate multi-source mixing

The same standalone application now calls `tests/support/standalone_mixing.rs`,
using the ordinary stock mixer alongside its environment-owned APM, G711 and
logger products. The SDK mixer has no environment constructor; no extra runtime
plumbing, executor or replacement mixing algorithm is introduced.

Two actual source providers prefer 8 kHz mono and 16 kHz stereo. They generate
caller PCM in the format requested by the native mixer. Twenty native mixes
alternate mono/stereo output while both sources are registered. Both providers
are requested at 16 kHz; steady stereo output must contain both distinct source
contributions. The stereo source is then removed through an alias, repeated
removal must report absence, and releasing the alias must destroy that provider.
The remaining mono source supplies twenty more alternating mono/stereo outputs
at 8 kHz, then its provider is released with the mixer. No removed-source callback
may occur after release.

Replay retains all forty output blocks, source callback order and input frames,
source drops, active samples, public frame fields, layout, mute state and optional
absolute capture timestamps. Local inputs supply no RTP packet infos; the corpus
asserts they remain empty rather than silently omitting unexpected metadata.
Startup/ramp output is retained without filtering or amplitude normalization.

Current core/native observations match across threaded/managed execution, two
managed repetitions and fresh processes. Both complete standalone test cases also
pass core ASan with leak and stack-use-after-return checks. Continuous monitoring
covers eight complete managed application acquisition-through-final-drop scopes,
including the original APM/G711/logging consumers, with no physical thread/helper
process creation or real socket syscalls. Managed time remains frozen. The
threaded baseline is outside managed lifecycle markers.

`PULSEBEAM_STANDALONE_MIXING_TRACE_PATH` writes the full JSON tuple
`[threaded_observation, [managed_first, managed_second]]` after native lifecycles
end. This qualifies the stated two-source rate/channel/removal consumer, not all
source statuses, source-selection/limiter settings, reentry or concurrent callbacks,
RTP packet-info merging or peer-installed mixing paths.

## Standalone NetEq decoder selection

`NetEq::stock_with_decoder_factory` and
`NetEqFactory::create_with_decoder_factory` preserve the supplied optional
factory. `None` means a genuine absent SDK factory, not a builtin default.
The original `stock` and `create` methods select the explicitly supplied factory.
Creation contexts retain the actual environment and optional factory independently
of the registration and outer owners.

Stock registration remains lazy even with no decoder factory. Ordinary codec
creation and packet consumption are refused before the pinned SDK dereferences
the missing factory. Codec registration and removal remain available. Empty
playout still uses the actual stock engine. Internally handled CN, DTMF and RED
retain their native behavior; RED does not supply a missing ordinary decoder.
The deprecated stock decoder-format getter returns None in this pin. Current
format inspection also returns None for internally handled formats, whose native
getter otherwise dereferences a nonexistent AudioDecoder.

Factory callback reentry on the same thread returns `ReentrantCall` before
locking. An ordinary rejection or reentry does not disable later factory calls;
callback panic does. Product callback reentry latches an observable failure and
fails closed without inventing initialized audio or output-slot writes. Different
registrations remain independently callable, and concurrent factory callbacks
are serialized.

### Initialized RTP-header inputs

RtpHeaderValues owns fully initialized input fields, including raw inactive
video-timing values. Its Default is an initialized caller input, not a snapshot
of a bare SDK RTPHeader constructor: that constructor leaves six inactive timing
deltas uninitialized. The unsafe private constructor-observation probe is removed,
not replaced by zero-valued native metadata. Valid caller timing fields remain
verbatim even when has_video_timing is false.

All four standalone caller-NetEq insertion callbacks receive headers reconstructed
from these initialized owned inputs. The private native-to-Rust conversion requires
that provenance; the factory is not installed into an SDK receive channel and no
arbitrary external header is assumed initialized. A future SDK-owned ingress route
would need its own initialization proof or explicit unavailable-state model.
Malformed private round-trip inputs return before serializing the uninitialized
native local, with a failed admission flag rather than a successful observation.

## Actor-owned PCM and processing

The shared session's absent codec choices select stock PCM encoding and decoding.
Use `Session::with_runtime(runtime, config)` with either runtime; `.pcm_audio()`
is an explicit selection of the same PCM behavior. Prerecorded Opus carrier input
requires `.encoded_audio()`. Explicit codec selections are preserved, including native
rejection when the selected encoder does not support the requested source type.
Software processing is separately selected with `audio_processing(config)`;
it does not process already encoded Opus packets or activate a physical device.
Headless custom PCM passes through the exact selected native module on the
factory worker before delivery to its own source sinks. It does not enter the
microphone transport's broadcast-to-all-senders path. A native processing error
refuses delivery and retains its error code; callback reentry is refused before
source locks. Echo cancellation and adaptive analog gain require render/delay
or microphone gain feedback that the custom PCM interface does not supply;
such capture requests return `UnsupportedProcessing`, not unchanged successful
processed audio. Custom PCM cannot share this processing capture thread with a
native device module. Use a microphone track or a separate headless factory.

Create an actor-owned source with `create_pcm_source`, publish it with
`publish_pcm`, and submit validated 10-ms `AudioPcmFrame` blocks using `push_pcm`.
Publication accepts optional track-level processing choices; subsequent
`set_audio_processing_options` calls target the retained sender track. The
voice engine shares processing across tracks, so those choices are not per-track
DSP isolation. At the custom capture boundary the selected noise/gain algorithms
and source enable/disable choices are applied through native `ApplyConfig` when
they change. This prevents Linux voice-engine defaults from silently replacing
the selected digital algorithm. Other native stream options can still change
the shared module's settings; `audio_processing_state` reports its current state,
including unknowns.
The raw source's input timestamp is not forwarded as an absolute capture clock
or RTP timestamp.

`attach_pcm_audio` selects decoded reception and `try_pcm_audio_frame` returns
owned initialized PCM. Headless polling pulls one native playout block when the
queue is empty; readiness notifications are not an audio playout timer. Encoded
and decoded receive modes are mutually exclusive. Use `detach_audio` before
selecting another mode. Stop reception before detaching an encoded sink when
opaque packets must never reach a native decoder. Source close, peer close and
shutdown retire the captive graph without invalidating already returned frames.

With a native artifact and the `native` Cargo feature, `native_audio(true)`
explicitly selects the platform device module. Device enumeration and index
selection are actor operations; re-enumerate before selecting a current index.
`publish_microphone` separately requests a microphone track, requiring a
PCM-capable encoder. `set_native_audio_enabled` forwards the peer's native
recording/playout request. Empty discovery, unavailable initialization and
failed selection remain native outcomes, not proof of physical capture or
playout. These operations are not exposed by the headless core feature set.

### Caller-driven stock PCM baseline

`tests/managed_pcm.rs` uses a generic `Session<R>` application with stock Opus
selected as an explicit codec preference. It submits six mono 48 kHz/480-sample
PCM blocks per sender at 10 ms virtual intervals and receives raw PCM in both
directions. The answering source attaches to the transceiver established by the
offer, rather than a separate pre-created m-line. No prerecorded Opus input,
custom codec or optional source processor is installed.

The outer caller pumps native queues, delivers packets and advances virtual time.
It polls each raw sink once per 10 ms step, because polling an empty live sink
can drive headless playout. An unbounded drain is not a passive observation. The
fixture requires at least three native Opus RTP packets per sender and three
source-correlated tone blocks per receiver, while retaining every returned PCM
block, including initial silence. Explicit sink/peer/session teardown must not
advance virtual time.

`PULSEBEAM_PCM_TRACE_PATH` writes the first run's JSON at the supplied path and
the second run at that path with extension `second.json`, after both native
lifecycles end. Compare the complete `observation` objects: native PCM samples
and optional capture times, logical events, virtual capture/playout/delivery
times, packet metadata, clear Opus RTP headers/sizes and actual plaintext
encoded frames. The separate `wire` array retains unmodified packet bytes. Cryptographic identities/ciphertext are
not replay criteria and are not rewritten or made deterministic by the fixture.

### Explicit builtin source processing

The same application and outer harness live in `tests/support/managed_pcm.rs`.
`tests/managed_pcm_processing.rs` explicitly selects
`BuiltinAudioProcessorBuilder::new().config(config)` with the native APM
pre-amplifier enabled at gain 2.0. It runs an unprocessed reference and two
processed applications with unchanged input tones, pacing, Opus selection and
receive polling. Over equal virtual media durations and returned block counts,
received PCM energy must exceed twice the reference energy in both directions.
This is an installed-processing/output witness, not a config readback or an
exact amplitude assertion after lossy encoding and native playout.

The source processes an owned input copy through the installed APM before
passing it to the native source sinks and Opus send path. Managed factories use
the current caller-owned driver for the default worker role, so this ordinary
caller-side `worker_thread()->BlockingCall` takes the SDK's direct `IsCurrent`
branch. Push and teardown must not advance virtual time. This does not establish
all posted-wait or nested-callback variants of `BlockingCall`.

`PULSEBEAM_PCM_PROCESSING_TRACE_PATH` writes the same first/second trace format
as the plain fixture, plus the unprocessed run at extension `reference.json`.
Complete processed PCM, timing, events and clear RTP observations are replay
criteria; all raw wire bytes remain separately retained. Optional source echo,
noise and gain choices retain their existing native processing/admission rules.
No custom DSP, runtime mode-specific algorithm or new wait hook is installed.

These short cases are bounded, clean-delivery mono PCM cases. They do not
qualify every voice-engine APM configuration, multi-source mixing or arbitrary
NetEq impairment/reconfiguration. Those paths need their own native consumers.

### Sustained native RTCP and explicit RTP loss

`tests/managed_pcm_periodic.rs` extends the same application to 300 input blocks
per sender over three virtual seconds. One case delivers every packet. The other
uses an outer-caller policy to drop every tenth RTP packet at a fixed position,
while continuing to deliver STUN, DTLS and RTCP. The policy is retained per packet
and every dropped packet must match the actual negotiated Opus payload type.
No native loss model, codec or timer is replaced.

Each peer uses a caller-supplied event logger to retain the real native outgoing
and incoming plaintext RTCP bytes. These callbacks occur at the SDK's native
transport/logging boundaries, not by interpreting SRTP ciphertext. The fixture
requires SR/RR packet types in both directions after the source-grounded initial
2.5-second RTCP evaluation deadline, measured from the earlier logger creation.
It retains the event's unmodified timestamp separately from the callback time
queried from the logger's supplied SDK environment. Ordinary native events begin
with timestamp zero; assigning log-time timestamps belongs to the logger.

The before/after native RTP stats retain actual record IDs, timestamps, SSRCs,
packet counts, optional loss and exact jitter bits. Both Linux flavors produced
150 Opus packets per sender and 299 source-correlated returned blocks per
receiver. The clean case reported zero loss; the impaired case dropped 15 RTP
packets per direction and native inbound stats reported 15 lost packets each.
Both retained 244 RTCP callback records, including late SR/RR. Complete selected
observations replayed within and across fresh processes and across flavors,
including full returned PCM and unmodified RTCP bytes/timing. The earlier short
plain and pre-amplified observations remained unchanged.

`PULSEBEAM_PCM_PERIODIC_TRACE_PATH` and
`PULSEBEAM_PCM_PERIODIC_DROP_TRACE_PATH` write the same first/second JSON format.
Only the existing cryptographic identity/ciphertext exclusion applies. Logger
stop, sink/peer close and session shutdown must not advance virtual time.
Continuous monitoring covered 16 sustained native lifecycles, with zero physical
thread/helper-process creation or real socket syscalls. Both new core cases also
passed ASan with leak detection alongside the six earlier stock-media cases.

This qualifies the scoped native reporting and loss observations, not every
periodic RTT update, custom logger serialization, stock buffered-file logging,
long-running audio, arbitrary impairment or reconfiguration. A received SR/RR alone does not prove the separate periodic RTT
update ran. These reporting cases close from the ordinary application actor.

### Stock audio teardown from an active caller queue

`tests/managed_pcm_teardown.rs` runs the same six-block PCM/Opus application,
then moves its graph into `TaskQueue::post_local` on a queue created by the same
world. Sink/peer close, session shutdown and final peer/factory/Call destruction
all occur before that callback returns. The callback must remain on its own
queue before and after native teardown; the external caller must not remain on
that queue afterward. Completion takes one top-level pump with no time advance.
The ordinary cases now also check time after complete graph destruction.

This exposed a native debug sequence failure in `AudioTrack::RemoveSink`:
`Thread::IsCurrent()` accepted the driver thread but the current task queue was
the caller queue, not signaling. The managed thread now scopes the requested
SDK thread queue for an already-current inline `BlockingCall`, preserving the
native invoke-policy implementation and restoring the caller queue on return.
There is no SDK source patch, disabled check, cleanup worker or changed media
algorithm. Matching core/native and core ASan artifacts were re-exported through
the producer, not relabeled from old libraries.

`PULSEBEAM_PCM_TEARDOWN_TRACE_PATH` retains complete first/second observations,
including queue-entry/exit, pump count and teardown times. The selected native
media/timing observations equal the ordinary actor-close case; the added queue
observations replay in full within and across fresh processes and flavors.
Eight new continuous native lifecycles showed no thread/helper-process creation
or real socket syscalls. The native sequence checks and ASan/leak case passed
alongside the eight earlier stock-media cases. Earlier plain, processed and
sustained reporting/loss observations remained unchanged.

This covers a distinct active caller queue, not a stop invoked from inside the
stock encoder queue. That queue is not exposed by the inspected public stock
Rust surface; a source-level self-wait possibility is not a demonstrated public
runtime path. Arbitrary nested callbacks and full scheduling closure remain open.

### Advertised primary codec and plaintext replay

`tests/managed_pcm_codecs.rs` selects PCMU, PCMA and G722 through the actual
sender capabilities and codec preferences, reusing the same generic `Session<R>`
application, native factories and 300-block capture/report/loss harness. Each
format has clean delivery and externally selected every-tenth-RTP-packet loss
cases. Negotiation retains the actual RTP map; every observed sender frame must
use that negotiated payload type. This is real stock encoding and native
NetEq/playout, not prerecorded input or a codec-option readback.

All shared PCM cases now install an identity `FrameTransformerProvider` per
sender. It copies the SDK frame's actual plaintext compressed bytes, SSRC, RTP
timestamp and payload type for observation, then completes the same owning frame
without mutation. Completion statistics check delivery, no in-flight frames and
no abandonment, late completion, capacity drop, wrong-frame use or callback
panic. These full encoded observations participate in replay alongside every
returned PCM sample, native report bytes, stats, timing and delivery decisions.
This also closes the previous bounded Opus plaintext-byte replay gap, including
the pre-amplified and active-caller-queue teardown cases.

The sink preserves native callback formats. PCMU/PCMA produce 8 kHz and 48 kHz
blocks; G722 produces 16 kHz and 48 kHz blocks while its RTP clock remains 8 kHz.
The SDK receive path obtains audio from NetEq and applies its own requested-rate
resampler before notifying the sink. The fixture checks 10 ms block sizing and
correlates tones using the actual returned rate; it does not resample samples,
discard silence/concealment or require every callback to use the decoder's
nominal rate. Both directions produced 150 plaintext frames/packets and 299
source-correlated blocks per receiver in each clean sustained case.

Current Linux x86_64 core/native and core ASan/leak checks passed all 11 shared
PCM cases. Repeated fresh processes and both artifact flavors matched complete
observations, including compressed bytes and all returned PCM. Continuous
monitoring covered 92 complete acquisition-through-final-drop lifecycles with
zero physical thread/helper-process creation or real socket syscalls. Whole
wire bytes remain retained separately; cryptographic identities/ciphertext are
still excluded without rewriting them or adding clock/entropy shims.

`PULSEBEAM_PCM_{PCMU,PCMA,G722}_TRACE_PATH` and
`PULSEBEAM_PCM_{PCMU,PCMA,G722}_LOSS_TRACE_PATH` write first/second JSON traces,
using the same format as the Opus cases. These bounded primary-format cases do
not qualify peer/NetEq installation of non-advertised L16 or multi-channel
Opus, auxiliary CN/DTMF/RED paths, arbitrary APM settings, multi-source mixing or
all NetEq impairment/reconfiguration paths. The direct standalone codec results
below are distinct from installed-peer qualification. Remaining gaps are not
upstream unavailability or determinism opt-outs.

### Direct L16 and multi-channel Opus

`tests/managed_audio_codec_families.rs` uses one generic standalone application
on threaded and managed runtimes. It creates the environment through the common
runtime factory, then directly creates builtin encoder/decoder products for
`L16/48000/2` or `multiopus/48000/6`. The latter uses `num_streams=4`,
`coupled_streams=2` and `channel_mapping=0,4,1,2,3,5`. No peer capabilities,
default codec preferences, media algorithms or artifact sources are changed.

Each case submits twelve interleaved 10 ms blocks with distinct channel tones
and wrapping RTP timestamps. It retains every encoding result, including empty
buffering, complete primary/redundant encoded metadata, compressed bytes, packet
duration/FEC observations, parsed timestamps/priorities, frame metadata, speech
type and every decoded sample. Native negative duration sentinels remain intact.
Stereo L16 uses its native default 10 ms packets, producing twelve packets and
11,520 decoded interleaved samples. Six-channel Opus buffers 20 ms packets,
producing six packets and 34,560 samples. L16 must roundtrip bit-exactly; all six
Opus channels must retain their distinct source tones without removing startup
samples or normalizing output.

Configuring factories and the original environment are dropped before use.
After encoding and parsing, the original encoder/decoder handles are dropped
before the owning parsed frames decode. Passive observations survive complete
native retirement. Managed construction, use and final product retirement leave
virtual time frozen; these synchronous standalone cases do not require packet
transport or caller-driven media pacing.

Both current Linux x86_64 artifact flavors matched complete observations across
threaded/managed execution, repeated managed scopes and fresh processes. Core
ASan with leak and stack-use-after-return checks passed. Continuous monitoring
covered sixteen complete managed acquisition-through-final-drop lifecycles with
no physical thread/helper-process creation or real socket syscalls. The threaded
baseline runs outside those managed lifecycle markers.

`PULSEBEAM_L16_TRACE_PATH` and `PULSEBEAM_MULTIOPUS_TRACE_PATH` write the complete
JSON tuple `[threaded_observation, [managed_first, managed_second]]` after native
lifecycles end. Compare the entire objects, not only compressed bytes or channel
counts. These results qualify the stated standalone formats and ownership path,
not every rate/channel/mapping/packetization setting, peer advertisement,
installed transport, NetEq reconfiguration or custom codec providers.

## Installed stock encoder controls

Peer factories retain the selected stock encoder factory and wrap its actual
products with the owning encoder control guards. No codec, payload or metadata
is substituted. `audio_encoder_failure_observer` observes the first rejected
installed control or unsafe Query/Create input independently of factory and
product retirement. An unsupported/null native disposition is not a failure.

Installed Opus packet-loss smoothing requires retained nondecreasing clock
bounds whose microsecond width fits `INT64_MAX`. An enforced singleton range
also proves frozen time without an additional monotonic flag. Production default clocks gain
this evidence through an identity-checked retaining production lease; caller
clocks retain only their own enforced evidence. Neither clock identity nor
current values are rewritten, and admission does not sample the clock again.
Unsafe callbacks are refused before native smoothing and report `UnsafeClock`.
The existing ANA enablement limitation remains distinct from packet-loss clock
admission.
