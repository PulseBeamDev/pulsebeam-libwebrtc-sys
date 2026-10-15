# Standalone audio encoder contracts

## Scope and ownership

`AudioEncoderFactory` selects either the native builtin factory or the existing
Opus transformer factory. Query and creation distinguish unsupported formats
from invalid inputs that would reach native assertions or unsafe arithmetic.

Each successful creation owns its native encoder and a retaining copy of its
environment. A per-product native mutex serializes encoder methods, including
metadata reads. Native spans and buffer storage do not escape into Rust:
encoding returns owned bytes, primary metadata, and ordered redundancy metadata.
The byte lengths are checked against the native encoded buffer.

`AudioEncoderFactory::create_with_options` consumes a const view of an
`OwnedAudioEncoderOptions` value through the real SDK `Create` method. It shares
selected-backend admission and product ownership with `create`, but does not
project the owned SDK value through Rust fields. Defaults, the full signed
payload field and optional pair identity remain unchanged. Opus's reached
constructor payload assertion remains backend-specific, not an options-value
restriction or a reason to reject an ordinary unsupported format.

`AudioCodecInfo::new` and `with_bitrate_range` call the distinct three- and
five-argument SDK constructors. They guard constructor DCHECKs only when enabled
in the matched native artifact, preserving standalone raw metadata otherwise.
`HasFixedBitrate` has its separate artifact-local bitrate guard; constructing,
copying or describing metadata does not authorize installing it into a consumer.
The export builds adapters with the SDK's exported definitions; qualification
must confirm those definitions and the linked constructor artifact match.

Standalone factory creation does not authorize installing the product into an
SDK peer. Provider registration and downstream packetization need separate
admission contracts. The standard voice engine re-queries selected send formats
and dereferences optional info after only a DCHECK. The bounded query audit found
no graceful rejection/interception path in the current integration. Full caller
query-disposition/panic support on that path remains unresolved, tracked as #37;
no cached or fabricated info and no custom-factory fallback is installed.

## Caller dispatch and one-shot handoff preparation

The new `AudioEncoderFactoryRegistration` invokes mutable `Send` factory
providers under serialized standalone dispatch. Owned creation inputs retain
actual supplied Environment and SDK Options copies. Metadata queries are not
cached, repaired or used to prefilter Create. An ordinary unsupported result is
None, not failure; callback/reentry/construction/destructor errors are typed and
retained independently by a first-failure observer. Same-thread same-registration
reentry is rejected before the mutex. Callback panic disables later dispatch;
already waiting callers recheck that state before entering the provider.

This registration is not itself a native SDK factory handle and cannot be
selected by builders. Its `sdk_factory` now returns a distinct persistent
standalone `CallerAudioEncoderFactory`, also excluded from builders. Lifetime,
scoped-call serialization and receipts are in `docs/audio-factory-sdk-contracts.md`. `supported_encoders_via_sdk`, `query_encoder_via_sdk` and
`create_via_sdk` now drive real refcounted SDK factory virtuals per operation.
Mutable Query returns its actual Some/None on each call, without replay/caching;
callback failures use an explicit per-operation receipt rather than ordinary
unsupported. Create takes actual SDK Options and the exact SDK environment, does
not prequery support, and consumes only a unique returned native product into the
retaining SDK envelope. Original backend provenance comes from that privately
constructed envelope, not discovery/request names. Full signed options fields
and optional actual pair-ID copies survive callback and product retirement.

It can return existing independently owned native encoders. Standalone fallible
caller products are now source-wired below; native caller product adapters and
safe persistent factory/builder installation remains open. No stock
fallback, failed-query synthetic info or installation permission is implied.
SDK factory metadata/context/error/unique-owner fixtures are unrun. Scoped static
review found no concrete defect, not compiler/runtime qualification.

Internal handoff preparation consumes only a uniquely owned AudioEncoder Arc,
keeping the exact native product/mutex/provenance and Rust environment lease
rather than cloning or detaching its mutable backend. A failed ownership attempt
returns the original product with packet state unchanged. Encoder SDK product
forwarding is now source-wired: the actual SDK subclass forwards encoding under
the original mutex, passing the caller Buffer through and returning the original
EncodedInfo unchanged, including redundant leaves. SDK Encode's sample-count
and appended-byte invariants are always-on checks. The private regression witness
preflights sample shape and preserves/observes an existing buffer prefix.

Forwarded NativeAudioEncoder state resolves through its directly owned original
product, including nested forwarding. Rust operations and guarded controls use
the original backend, multipliers, clock evidence and overhead, not the proxy's
format name or guessed concrete type. A proxy is never cast to
AudioEncoderOpusImpl. All required SDK getters/controls are supplied, with the
explicit pending gates below; ReclaimContainedEncoders is not called or overridden.

`AudioEncoderSdkFailureObserver` retains only a shared atomic first SDK-control
failure record and can outlive product retirement or cross caller threads.
Unsafe void controls are rejected before touching the backend and record typed
operation/error, rather than silently proceeding, clamping inputs or synthesizing
successful encoding. Valid later operations still use actual retained codec state;
direct Rust controls continue returning their own Result errors. The native
recoverable-loss default is not invoked and is observed as UnsupportedControl.
Opus EnableAudioNetworkAdaptor records UnsafeAdaptor and returns false pending
#11/#12 consuming admission. That is not a claim of native capability absence or
completed ANA parity. Other selected implementations preserve native unsupported
returns. Persistent standalone SDK factory ownership is now source-wired;
safe installation and native caller product adapters remain unfinished.

New prefix/packet accumulation, nested handoff, actual Opus guard, post-retirement
observer, unsafe-clock and pending-adaptor fixtures are unrun. Scoped static
review found no actionable forwarding defect, not runtime or ABI qualification.

## Standalone fallible caller products and Encode admission

`AudioEncoderProvider` supplies all named pinned product getters and controls,
excluding the expressly forbidden ReclaimContainedEncoders. `CallerAudioEncoder`
owns a mutable Send provider and a retaining environment copy. Clones share one
serialized product state. Getter reads are not cached; controls and allocation
updates pass their full inputs to overrides. The SDK default target-bitrate and
allocation methods delegate to bandwidth; allocation argument conversion calls
actual SDK DataRate/TimeDelta getters and retains native narrowing and release
infinity behavior. Only compiled constructor/getter DCHECK domains are guarded.
Recoverable-loss default returns typed UnsupportedControl rather than invoking
the SDK's asserting default.

Standalone Encode validates the exact SDK unsigned 10-ms sample-size expression,
then receives owned newly appended bytes and genuine metadata from the provider.
It rejects primary-byte mismatch, redundant byte-accounting overflow/mismatch,
callback Err and panic without repairing metadata or substituting empty success.
Ordinary zero-byte output remains valid buffering. No codec/rate/channel/payload
allowlist is imposed on standalone metadata. This is not downstream admission.

Errors identify the individual operation and cause. A first-failure observer may
outlive the product. Same-thread same-product reentry is refused before locking;
panic disables future callbacks, including waiting entries. Ordinary validation
and provider errors do not reset codec state or disable later standalone calls.
Provider destruction precedes release of the retaining environment; a single
destructor panic is caught and recorded. Providers must not synchronously wait
for/reenter their owner or panic in destruction. Double panics/abort/OOM cannot
be made recoverable by this boundary. Application-owned strong reference cycles
remain the application's lifetime responsibility.

The pinned SDK Encode returns EncodedInfo, not Result; it fatally checks shape
and appended byte counts. ACM treats zero bytes as legitimate buffering and can
packetize empty output when send_even_if_empty is true. Factory error receipts
are not an Encode failure channel. #38 therefore tracks a separate fallible
native product boundary and actual SDK/ACM refusal proof before installation.
`CallerAudioEncoder` is intentionally not an AudioEncoder SDK handle and cannot
be returned by the current native factory provider Create contract. No fake
encoded metadata, success-on-error buffering, fallback codec or patched SDK is
introduced. #37 Query instability and #11/#12 ANA admission remain separate.

Fixtures delegate to real PCM and cover retained environment/migrated use,
packet buffering/provenance, exact sample refusal, malformed output rejection,
native default controls/allocation behavior, panic disabling, post-retirement
destructor observation, concurrency and same-product reentry. All are unrun.
Static review found an unconditional infinity rejection in allocation defaults;
source now uses actual SDK getters and artifact-local guards. Dependent narrow
static re-review found no remaining allocation-forwarding defect. Neither that
review nor the new product source is compiler/runtime/ABI/consumer qualification.

## Packet loss and clocks

For ordinary builtin Opus, packet loss must be finite and in `[0, 1]`. Its packet
loss smoother also requires retained, nondecreasing clock evidence and a complete
microsecond range whose width is strictly less than `INT64_MAX`. At exact
equality the signed subtraction fits, but the pinned finite
`TimeDelta::Micros` constructor rejects the plus-infinity sentinel. Larger spans
also cannot prove safe signed subtraction. A current clock sample is not a substitute for that lifetime
guarantee. Rewindable clocks keep
their existing behavior and do not acquire monotonic evidence implicitly.

Other selected products retain their native packet-loss behavior without an
Opus-only input or clock restriction. Recoverable packet-loss notification is
reported as `UnsupportedControl`: no selected pinned product overrides the
asserting native default.

## Explicitly prohibited native method

`ReclaimContainedEncoders` is absent from the Rust API and bridge. The pinned
`api/audio_codecs/audio_encoder.h` explicitly says not to call or override it.
Products returning an empty span do not authorize that call.

## Audio network adaptor construction

ANA enablement is not exposed. Idempotent native adaptor teardown is available
as `disable_audio_network_adaptor`.

Internal construction preflight parses bytes with the pinned generated protobuf
parser without creating or replacing controllers. It checks supported oneofs,
required fields, constructor assertions, unique runtime-config writers, scoring
field presence, and the actual native encoder's channel and frame state. It
retains controller-specific arithmetic obligations separately. Construction
preflight is not a runtime admission decision.

Unknown fields and unused values are not rewritten or unnecessarily restricted.
An empty controller list is accepted. The removed RPLR controller and unset
oneofs are rejected. Legacy and V2 frame controllers cannot coexist because they
write the same asserted runtime-config slot. V2's absent minimum payload bitrate
and slow-adaptation flag retain their native zero/false defaults. Legacy frame
transition entries are retained independently, including older entries that the
protobuf comments describe as replaced but the native implementation still
inserts.

FEC curve checks use the SDK's float domain and `ThresholdCurve` ordering,
including rounded bandwidth endpoints and merged points. Scoring points are
retained separately, including NaNs that construction accepts. The internal
hypothetical-metrics preflight checks strict weak ordering using the pinned
normalization and rounded squared distances. A NaN distance is equivalent under
`<` to every scored controller: it breaks equivalence transitivity only when
there are also two distinct non-NaN distance classes. Unscored controllers form
a separate, strictly later class. A blanket scoring-NaN prohibition would reject
native-valid comparator domains.

No scoring arithmetic is evaluated when scoring is absent or either supplied
metric is absent. Reordering thresholds, including NaN and infinite thresholds,
are not rejected or used to bypass the comparator-domain check. This helper does
not mirror native retained metrics, order changes, or time/distance gates; it is
not yet a consuming-operation guard. Its overflow projection assumes the pinned
RN-even IEEE float narrowing semantics, which remain part of artifact
qualification.

The builtin Opus factory returns `AudioEncoderOpusImpl` directly. Its native
getters provide supported frame lengths, next frame length, and current coded
channels. A protobuf-disabled artifact or a non-Opus product reports unavailable
construction capability without invoking the asserting disabled controller
manager branch.

## Native runtime injection seams under investigation

The public `AudioEncoderOpus::MakeAudioEncoder` factory has no injection hook.
The pinned implementation's `CreateForTesting` accepts an adaptor creator and
a `SmoothingFilter`; stock SDP parsing remains available through
`AudioEncoderOpus::SdpToConfig`. The default adaptor creator's mutable initial
values have implementation-level native getters, including coded channels,
next frame length and FEC state. These seams are not yet installed in owning
creation.

A delegating `ControllerManager` can reuse `ControllerManagerImpl::Create`
without duplicating the controller factory. Its `GetSortedControllers` receives
the adaptor's actual aggregate metric optionals and values, while delegating
controller proxies can inspect metric updates and runtime-config slots before
native arithmetic. A guarded bitrate smoother can likewise forward to the
native `SmoothingFilterImpl` rather than predict its evolving average.
The internal bounded bitrate smoother and bitrate-controller proxy delegate to
these native components. They are not installed in owning creation. Exact reorder
gates, floating-point artifact qualification, operation failure reporting and
manager/proxy lifetimes still require a complete runtime contract.

`AudioEncoderOpusImpl::Reset()` recreates codec state but does not reset the
adaptor or its smoothing/scoring history. Disable destroys the current adaptor;
re-enable constructs a new one. Runtime evidence must follow these actual
lifecycles rather than treating codec reset as adaptor reset.

## Retained clock evidence

Each encoder derives immutable clock evidence from its actual retaining
environment, without sampling or replacing the clock. Known, finite, ordered,
monotonic microsecond bounds are converted with native `Timestamp::ms()` to
millisecond endpoints and a lifetime span. Internal smoother preflight combines
that evidence with the rounded initialization witness for the native 5000 ms
bitrate smoother and any configured FEC time constant. It also checks
initialization endpoint addition and timestamp differences against signed
`int64_t` limits.

ANA smoothers subtract integral millisecond timestamps. The existing Opus
packet-loss smoother instead subtracts native microsecond timestamps before
conversion, so its stronger lifetime-span requirement remains separate. A
monotonic clock covering every finite microsecond timestamp has safe ANA
millisecond subtraction but does not authorize the packet-loss callback's
microsecond subtraction. Frozen clocks and negative finite epochs are not
rejected solely for those values. This evidence alone does not authorize ANA
or establish runtime controller/scoring safety.

## Bitrate smoothing input boundary

The internal input predicate checks two distinct native period operations:

1. Signed `period_ms * 4` must fit `int64_t`.
2. The resulting value is narrowed to native `int` with the pinned ABI's
   conversion semantics. Its native rounded alpha, computed as zero for a zero
   duration and otherwise `exp(-1.0f / duration)`, must be finite and in `[0, 1]`.

This is not an arbitrary `int32_t` limit on the original period. Wide and even
negative periods may narrow to a valid time constant. Large negative native
durations can round alpha to exactly one, preserving a frozen filter. A missing period is not
consumed. These checks must not restrict operations where ANA is disabled and
the native implementation ignores the period.

On Linux x86_64, smoothing predicates require nearest-even rounding in both
x87 and SSE, all floating exceptions masked, SSE FTZ/DAZ disabled, and bridge
binary32 evaluation without `__FAST_MATH__`. Other targets are not admitted by
this internal seam. The Linux x86_64 recipe now compiles both the SDK and
bridge with final `-fno-fast-math -ffp-contract=off` options. GN uses a hashed
compiler wrapper, including for `common_audio/smoothing_filter.cc`; a changed
wrapper changes compile commands and an old configuration rejects compilation.
Export checks the configured wrapper identity, incorporates its hash and final
flags into the native-configuration digest, and records them in `build.txt`.
The wrapper bytes also participate in the Python/Rust adapter identity.
Written compiler/provenance regression fixtures have not run. Actual artifact
evaluation semantics and clock callbacks still need their own qualification
and admission contracts.

The sample check uses the exact binary32 interval
`[-2^31, 2^31 - 128]`, with upper endpoint `0x1.fffffep30f`.
`float(INT32_MAX)` instead rounds to `2^31`, outside the native float-to-int
conversion domain. Under round-to-nearest ties-to-even, the largest admitted
integer sample is `INT32_MAX - 64`.

### Rounded recurrence argument

Assume IEEE binary32 round-to-nearest ties-to-even and the SDK expression
`m * state + (1 - m) * sample`, without fast-math reassociation. Let
`B = 2^31 - 128`, `0 <= m <= 1`, and `c = RN(1 - m)`.

For every binary32 `a` in `[0, 1]`, `RN(B*a) <= B*a` in exact arithmetic.
`2^31*a` is exactly representable. For a non-power-of-two significand,
subtracting `128*a` places the exact product between half and one ULP below
that scaled value, so rounding selects its predecessor. At a power of two,
the lower-binade spacing makes the product exact. The same argument applies
to subnormal `a` after the exact scaling normalizes it.

Sterbenz's lemma makes `1-m` exact for `m >= 1/2`; otherwise its rounding error
is at most `2^-25`. Thus `c+m <= 1+2^-25`. At the upper endpoint, separately
rounded products are no greater than their exact products, giving a pre-final
value at most `B + B*2^-25 = B + 64 - 2^-18`. This is strictly below the midpoint
`B+64` between `B` and `2^31`. Final rounding cannot exceed `B`.

At the lower endpoint `-2^31`, the power-of-two-scaled products are exact.
Their sum can extend at most 64 below that endpoint, less than the outside
binade's midpoint distance of 128. Final rounding cannot pass `-2^31`.
Monotonicity extends the endpoint bounds to arbitrary state and sample in the
interval. Either single-FMA contraction also preserves the bound because the
uncontracted product at the upper endpoint still rounds downward and the
lower-endpoint products remain exact.

This proves the recurrence interval only. It does not prove that native clock
deltas, initialization calculations, or period changes actually produce finite
`m` in `[0, 1]`, or that every artifact uses the assumed floating-point semantics.
The pinned SDK's bitrate initialization duration is a fixed 5,000 ms, not a
field-trial-controlled duration.

### Initialization witness

A separate internal predicate evaluates the SDK's rounded initialization factor
and divisor, requiring a finite positive divisor. It checks the actual `powf`
outputs for every possible initial-phase integer exponent under a supplied
lifetime millisecond span. This checks monotonicity of the library's evaluated
powers rather than assuming that the exact-real power function proves it.
The resulting native power difference is nonnegative, so its exponential
multiplier remains in `[0, 1]` under the stated library range semantics.

Zero and one millisecond durations use distinct native branches. For negative
durations, the initialization phase ends before the first sample. A rounded unit
alpha, or a durable zero millisecond clock span, prevents an amplifying consumed
multiplier. Unused NaN initialization bookkeeping alone is not rejected in that
branch. Large positive durations with a rounded zero divisor are rejected.

The supplied span is not clock evidence. A successful witness must be retained
with the actual clock identity and lifetime bounds, with signed time arithmetic
admitted separately. It is computed for retention, not repeated on each update.

### Consuming average guard

The internal delegating smoother admits finite amplifying alpha after native
initialization, even when the retained clock can advance. The pinned native
extrapolation performs only floating-point operations; it has no numerical
assertion or integer conversion. With floating exceptions masked, overflow or
NaN in its `powf`/recurrence can therefore reach the actual `GetAverage` boundary
without an integer safety hazard. The wrapper rejects any returned nonfinite
average or value outside `[-2^31, 2^31)` before a downstream float-to-int
conversion, and records the failure as sticky. It does not predict or repair
native state. Signed time arithmetic remains independently guarded.

Written fixtures distinguish a safe advancing amplification from an overflowing
native result, ignored initialization periods and zero-delta endpoint behavior.
These fixtures have not yet run. Native artifact compiler provenance and all
other ANA consuming paths remain gates before public enablement.

## Controller integer arithmetic preflight

A separate internal predicate checks controller arithmetic using caller-supplied
metric presence and uplink bounds. Those inputs are not evidence of native state
and do not authorize execution. It checks all candidate frame states, so it can
be more conservative than a proven reachable-state guard.

Bitrate-controller overhead updates assert positivity even before a target is
known. Its offset assertions use the SDK's `SafeGe`/`SafeLe` mathematical integer
comparisons, not C++ unsigned promotion. With no legacy frame controller,
`last_fl_change_increase` stays false and only the decrease offset is consumed.

Legacy frame checks select the first configured transition in each native search
direction. Increase offsets require representable negation and bounded magnitude;
decrease offsets do not have that native assertion. The predicate preserves
defined `size_t` wrap and native narrowing before checking signed threshold sums
and target subtraction.

V2 narrows overhead to native `int` before its signed `*8*1000` calculation.
When its metrics are incomplete, the native early return does not consume that
arithmetic. Slow adaptation consumes the smoothed uplink interval; other V2
configuration consumes the target bitrate.

## Internal consuming-operation components

The bounded bitrate smoother retains immutable clock evidence and the native
5000 ms initialization witness. It forwards each admitted call to
`SmoothingFilterImpl`, without querying the clock or predicting an average.
Timestamp bounds and monotonicity are checked before native time arithmetic.
Only phase timestamps are retained outside the native filter.

Unlike the universal input predicate, this component admits rounded samples
through `2^31`. Under the same RN-even expression assumptions, the enlarged
interval `[-2^31, 2^31]` stays finite: multiplication of either power-of-two
endpoint by each binary32 coefficient is exact, and the coefficient sum can
overshoot one by at most `2^-25`. The possible 64-unit endpoint overshoot remains
below the outside binade's 128-unit rounding midpoint. The actual native average
is then checked against `[-2^31, 2^31)` before it can reach the SDK's int cast.
A rounded-high sample is not rejected merely because a future average could be
unsafe. For example, a frozen initialized filter can retain its earlier safe
average after receiving `INT32_MAX`.

Native period setters ignored during initialization are forwarded unchanged.
After initialization, the bounded component admits rounded alpha in `[0, 1]`,
and also alpha greater than one at the retained upper millisecond endpoint,
where every future admitted delta is zero. Amplifying periods with possible
further advancement remain outside this component's proof. This is not a
complete native-value admission policy and must not become a public enablement
path with that gap.

The bitrate-controller proxy retains the exact sparse target/overhead updates
and only frame changes actually consumed by native `MakeDecision`. It checks a
positive overhead update even before a target is known, the occupied bitrate
slot, the selected change-direction offset, the consumed frame divisor, and the
signed target subtraction. It preserves native `size_t` wrap and pinned int
narrowing. An inactive `INT_MIN` offset or a frame slot ignored before both
metrics are known does not cause rejection. Native code still computes and
writes the bitrate; the proxy does not synthesize that result.

Both components use sticky per-operation failure evidence. Their owner must
serialize calls, translate failures, and retain the original native objects and
matching construction arguments. Isolated internal probes exercise delegation
without enabling ANA or mutating an encoder/adaptor. They do not establish
complete controller-manager, codec, clock-callback or artifact admission.

## Requirements before ANA enablement

Every consuming operation must preserve retained obligations before unsafe
native consumption, including encode-driven adaptor updates, overhead changes,
bandwidth and allocation notifications, target changes, reset and recreation,
and receiver frame-range changes. Required guards include:

- Clock monotonicity and non-overflowing native time arithmetic.
- Defined native initialization and smoothing arithmetic. Coefficients in
  `[0, 1]` provide a sufficient bounded recurrence proof, not the entire
  native-valid domain.
- Controller-specific signed overhead products, offset negation, threshold sums,
  target-minus-overhead subtraction, and bitrate field-trial transformations.
- Safe scoring distances and ordering, including deferred NaN obligations.
- The SDK prohibition on receiver frame-range changes while ANA is active.
- Retained smoother state across adaptor teardown and replacement.

Artifacts using protobuf must ship the pinned generated ANA headers and their
pinned protobuf header dependencies with matching feature definitions. Header
availability alone does not authorize the native capability or runtime path.
