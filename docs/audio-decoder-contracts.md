# Standalone caller audio decoder and frame contracts

## Scope

`AudioDecoderProvider`, `AudioDecoderFrameProvider` and their owning products
supply fallible, serialized standalone modern decoder dispatch. They are not
SDK `AudioDecoder` implementations or peer/NetEq dependencies. Existing native
factory providers still return only existing owning `AudioDecoder` products.
No builtin fallback, fabricated packet/frame, synthesized PCM/PLC or patched SDK
is supplied. Full consumer admission is tracked separately as #39. Registration
`sdk_factory` now creates a persistent standalone CallerAudioDecoderFactory for
existing native products, not these caller DSP products or NetEq installation;
see `docs/audio-factory-sdk-contracts.md`.

The human-approved public surface excludes legacy Decode, DecodeRedundant,
HasDecodePlc and DecodePlc. The SDK still requires DecodeInternal and preserves
legacy compatibility internally, including its default ParsePayload path. Any
future native caller adapter must fulfill that actual compatibility obligation,
not silently select a different codec. The human now allows a deliberate typed,
negative-return/no-write refusal of excluded internal legacy paths for standalone
caller adapters. This is not genuine legacy DSP or installed internal-CNG support;
modern work still does not erase the ABI or internal-consumer admission gate.

The pinned NetEq audit distinguishes the actual paths: ordinary non-CNG packets
use the decoder's modern ParsePayload override and returned frame Decode, not the
SDK default OldStyleEncodedFrame. Codec-internal CNG still calls public native
Decode with null/zero encoded input; merge consults native HasDecodePlc/DecodePlc.
DecodeInternal remains abstract and receives no output-capacity argument. The
SDK's duration-based byte precheck is optional when duration is unknown; the
protected method itself promises no failed-decode/no-write behavior. Negative
length is consumed as failure, followed by ErrorCode and generic/specific integer
error status. Explicit typed unsupported/no-write refusal could supply a safe
failure boundary, but is not genuine DSP compatibility or installed CNG support.
Base HasDecodePlc false, DecodePlc zero and redundant-to-primary routing remain
actual SDK defaults, not permission to substitute codec output. Native caller
installation remains gated on actual consumer proof; standalone adapter policy
now permits explicit typed refusal. Public legacy exclusions and #39 remain.

## Owning dispatch and environment

`CallerAudioDecoder::from_provider` retains the supplied environment and mutable
Send provider. Clones share a mutex-protected state. Sample-rate/channel reads,
reset, error code, packet-duration/redundant-duration/FEC queries, parse and PLC
callbacks are independently named, uncached operations. Native metadata defaults
are exact: error code zero, duration and redundant duration -2, FEC false.
GeneratePlc is a required explicit provider method; parse is the modern owning
contract, not a legacy convenience fallback.

Metadata is observed without a codec/rate/channel allowlist. Sample rate and
channel count must remain constant for eventual SDK use. Arbitrary standalone
values, packet duration sentinels and unknown speech tags are not installation
permission. No native span or mutable native buffer escapes.

Callbacks must not synchronously wait for/reenter their owning actor. Same-thread
same-decoder callback reentry is rejected before locking. A callback panic
records the operation and disables subsequent decoder/frame callbacks, including
waiting entries. Provider Err is distinct from ordinary native disposition and
does not reset state or disable ordinary later standalone operations.

## Parse and frame lifetime

Parse consumes owned bytes and the exact RTP timestamp and returns zero or more
owning products. Each result retains its exact timestamp, nonnegative signed
priority and nullable frame. Negative priority is rejected, never clamped; null
frames remain observable rather than becoming fabricated decodable frames.
NetEq assumes non-null frames and needs separate refusal/admission.

`AudioDecoderFrameProduct` transfers a Send frame provider into the decoder's
frame registry. Its transfer observer survives binding and retirement and records
its destructor failure independently; a bound destructor failure is also recorded
on the shared decoder observer. Bound operation errors are observed through
`CallerDecodableAudioFrame::failure_observer`, which names the shared observer.

Each returned `CallerDecodableAudioFrame` retains the actual decoder/environment.
Clones share one lease and one decode attempt. All frame callbacks and bound
frame destruction use the decoder mutex. Dropping the last frame owner during a
same-decoder callback queues its private registry ID; destruction occurs after
the callback under that mutex, not recursively or concurrently. Retirement works
even when callbacks are disabled. Frames retire before decoder provider state,
and both retire before the retaining environment. Application-owned strong
reference cycles remain the application's responsibility.

## Decode and explicit capacity

Frame duration is per-channel, zero if unknown; DTX defaults false. The pinned
SDK expects ParsePayload frames between 10 and 120 ms, but raw standalone
observations do not imply a consuming-domain proof or stock capacity limit.

`decode(max_output_samples)` uses an explicit total interleaved-sample bound.
It checked-multiplies actual duration by actual channels and refuses insufficient
or unrepresentable capacity before starting Decode, allowing a correctly sized
retry. Once dispatch starts, success, ordinary None, callback Err, malformed
output and panic all consume the attempt. Successful owned output contains its
actual sample count (`samples.len()`) and speech tag. Count beyond the supplied
bound is rejected without truncation, silence or None-on-error substitution.
A second attempt is refused without reentering provider getters/Decode; a globally
disabled product reports Disabled without callback entry.

This owned-result contract does not prove a native mutable output span safe for
an arbitrary codec. Existing stock wrappers allocate a backend-proven bound;
that proof cannot be borrowed for arbitrary caller products.

## Standalone real SDK decoder virtuals and approved refusal

Caller decoder now also exposes named `_via_sdk` operations for SampleRateHz,
Channels, ErrorCode, Reset, both packet-duration queries, FEC, ParsePayload and
GeneratePlc. Each synchronous operation creates a genuine SDK AudioDecoder
subclass retaining the actual Rust caller decoder/environment and a fresh
receipt. Getters are not cached, raw metadata is not convenience-whitelisted,
and provider errors cannot become successful scalar/false/empty/void results.
These are temporary standalone SDK owners, not persistent native caller codec
products, factory selections or installed NetEq dependencies.

Modern ParsePayload returns actual SDK ParseResults containing exact timestamp,
priority and nullable EncodedAudioFrame ownership. Private constructor provenance
permits moving the original Rust lease back out of that exact SDK leaf, never a
format/type guess. All native frames retire before the temporary SDK decoder;
returned Rust frames independently retain their actual decoder/environment and
shared single-shot state. A CXX Rust Vec's Box fields remain valid/non-null during
native processing: a consumed transfer is swapped with a real absent transfer
before moving the C++-local box. Absence storage does not create a fake frame.

GeneratePlc uses a real owning SDK BufferT initialized with the actual prefix and
the current operation's explicit append bound. Existing Rust checks validate
requested counts/owned output; the native subclass separately verifies the prefix
and append length before changing its buffer. The caller's Vec is replaced only
after receipt success, with actual SDK bytes; errors remain distinct from ordinary
no-output. No stock bound, synthesized PLC, DSP fallback or metadata repair is
introduced.

The mandatory protected DecodeInternal now implements the human-approved explicit
refusal: record NativeCompatibility/UnsupportedCompatibility, return -1, and never
read encoded input or write unsized PCM/speech pointers. Native redundant routing
reaches that same refusal; HasDecodePlc/DecodePlc retain actual SDK false/zero
defaults. No public Rust legacy methods are added. A private unit witness calls
actual SDK current/redundant legacy wrappers and checks negative status, untouched
PCM/speech and typed observation. This is a refusal witness, not genuine internal
CNG DSP or installation parity; #39 remains open.

`tests/audio_decoder_product_sdk.rs` supplies real PCMU metadata/reset/packet
operations, exact nullable parse fields and migrated/retired leases, prefix/append
bounds, None/error/panic distinctions, scalar/void refusal and open raw metadata.
Actual PCMU empty input is a zero-duration frame via SplitBySamples, not an empty
vector; the existing native wrapper supplies non-null capacity without altering
payload bytes. A caller's ordinary zero-frame disposition is tested independently.
All fixtures and private witnesses remain unrun. Parent fixed the invalid
moved-from Box-in-RustVec hazard and the empty-PCMU-result assumption before
qualification; dependent narrow source/fixture review found no remaining delta
issue. Static signature/type and ownership review is not compiler, ABI, artifact
or behavior qualification.

## Standalone real SDK frame virtuals

Caller frame `duration_via_sdk`, `is_dtx_packet_via_sdk` and
`decode_into_via_sdk` now construct a genuine owning SDK EncodedAudioFrame leaf
for each synchronous operation. No full native caller AudioDecoder, stubbed
legacy virtual, factory selection or NetEq installation is constructed. The SDK
leaf owns a clone of the original caller lease and still uses its shared decoder
mutex and single-shot attempt state. The SDK object's individual Decode is also
one-shot; pre-dispatch capacity refusal leaves the shared Rust attempt retryable
through another driver invocation.

SDK Duration/bool getters and optional Decode cannot carry Rust errors. Each
operation owns an independent receipt. Getter error placeholders and Decode's
nullopt-on-error remain internal, never exposed as successful zero/false/None.
Ordinary callback Ok(None) still returns Ok(None). Past observer errors or nested
SDK operation receipts do not replace a later operation's genuine result.

Decode receives an exclusive caller-owned mutable PCM span, not an implicit
stock-size allocation. Exact duration*channels/representable capacity is checked
by the original Rust caller frame before DSP dispatch; the native leaf also
checks pointer/byte span limits. Status, returned sample count and speech enum
representability are validated before the first write. Failure/None preserves
the whole span; success copies only the actual returned prefix and leaves the
tail unchanged. No unstated positive/Duration-sized decoded-count floor, silence,
truncation or invented PCM is supplied. AudioDecoderFrameDecodeResult preserves
actual total interleaved count and speech value.

Rust AudioSpeechType remains an open i32 in standalone observations. The pinned
non-fixed C++ enum has named values 1 and 2, and a defined cast range [0,3]. The
SDK leaf preserves that exact range, including unnamed values, while refusing
out-of-range casts before copying PCM. It does not invent a named-tag-only
whitelist. Defined value transport is not evidence of meaningful speech/NetEq
consumer admission; #39 retains those downstream obligations.

Additional `tests/audio_decoder_frame_sdk.rs` fixtures delegate genuine PCMU and
cover SDK getters/retired ownership/migration, exact preflight retry and tail
preservation, Rust/SDK one-shot sharing, None/error/panic/malformed output,
getter-panic refusal, short/empty/unknown-duration results, native enum domain,
concurrent receipt isolation and same-frame SDK reentry. Metadata/fault probes
are standalone contracts, not fake wire codec or installed-media qualification.
All remain unrun; the scoped SDK leaf source review found no concrete defect.
An initial fixture review questioned the first PCMU block's empty output without
inspecting the codec. Actual pinned G711 defaults to 20-ms packets and buffers
two 10-ms blocks; source-backed dependent review confirmed that setup. The
identified reverse Rust-first/SDK-second one-shot case is now covered and narrowly
reviewed, not executed or accepted as broader fixture qualification.

## PLC append contract

`generate_plc(requested_samples_per_channel, prefix, max_appended_samples)`
passes the real existing prefix read-only and receives only newly appended,
owned interleaved samples. It checks representable append capacity and required
sample multiplication before dispatch. Empty output is legitimate no output.
Nonempty output must cover the requested per-channel count times channels,
contain complete interleaved channel groups and fit the caller-supplied bound.
Only validated output is reserved/appended. Any failure leaves the existing
prefix unchanged, without resetting backend state or manufacturing concealment.
No stock 120-ms maximum is assumed for this caller contract.

NetEq uses always-on checks for short nonempty PLC. #39 therefore includes actual
per-product output capacity, arithmetic, stable metadata, parse/nullability and
legacy compatibility proof before native SDK/NetEq admission.

## Failure and evidence limits

Typed errors identify operation and provider, shape/capacity/output, single-shot,
reentry, panic or disabled cause. First-failure observers can outlive retirement
and cross caller threads. Single destructor panics are caught and recorded;
providers must still not panic in destruction. Double panic, process abort and
OOM cannot be made recoverable by this boundary.

Fixtures in `tests/audio_decoder_provider.rs` create payload through the real SDK
PCMU encoder and decode it through the real SDK PCMU decoder. They cover migrated
retained use, exact metadata and nullable parse results, priority refusal, bounded
single-shot decode, None/Err/panic/output distinction, shared panic disabling,
prefix/no-output PLC, malformed append refusal, cloned concurrent attempts,
deferred destruction during parse and post-retirement destructor observers.
Scoped source and fixture reviews found no concrete defect. Fixtures remain
unrun; static review and source formatting are not compiler, runtime, ABI or
installed-consumer qualification. The new SDK frame-leaf and decoder virtual drivers are also unqualified; approved
legacy refusal does not remove native product/consumer/NetEq admission gates.
#17/#39 and whole-goal qualification remain open.
