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
times, packet metadata and clear Opus RTP headers/sizes. The separate `wire`
array retains unmodified packet bytes. Cryptographic identities/ciphertext are
not replay criteria and are not rewritten or made deterministic by the fixture.

This is a bounded, clean-delivery mono PCM baseline. It does not qualify optional
source processing, every voice-engine APM configuration, multi-source mixing,
NetEq impairment/reconfiguration, plaintext encoded Opus byte replay or
long-running audio. Those paths require their own native consumer observations.

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
