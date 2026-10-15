# Standalone stock audio processing

`AudioProcessor::builtin` owns a real builtin WebRTC AudioProcessing module and
retains the exact supplied environment, including its Rust dependencies, until
native module destruction. The module is single-sequence and neither Send nor
Sync. It does not create a peer, activate a physical device or supply a hidden
pump. This standalone API is separate from existing factory/actor capture
processing selection.

## Processing and output

Capture and reverse operations drive the actual i16 and normalized planar-float
SDK overloads. AnalyzeReverseStream consumes render data without returning PCM.
Frame counts use native floor(sample_rate / 100). Caller slices must cover the
entire requested channel-major or interleaved prefix. Extra suffix storage is
never exposed to the SDK. Float channel pointers remain private and synchronous;
no borrowed output or uninitialized storage escapes.

Integer operations use equal input/output layouts with copied source storage
and caller-owned in-place destination. This avoids overlapping memcpy in native
unsupported-format copy branches. Float operations accept independently supplied
native input/output layouts and initialized caller output. No wrapper zeroing or
copy-back invents successful output. Status preserves the exact native code,
including negative warnings. Native error paths may write actual best-effort
silence, copied input or broadcast first-channel data; an error does not promise
an untouched output prefix. Shape/admission failures occur before native entry
and leave destination storage unchanged.

Non-enumerated rates in the native supported range are resampled by the SDK,
not rejected by an artificial native-rate whitelist. Unsupported rates and
channel layouts retain native errors and their associated output behavior.
Negative rates and dimension overflow are refused before native size arithmetic.
Channel counts fit the reached signed native domains. Integer channel counts
also guard stock render mono-downmix accumulation and narrowed divisor hazards.
Normalized float inputs must be finite and in [-1, 1]. Resource exhaustion,
native aborts unrelated to admitted operation inputs, and process-global runtime
mutation are not recoverable allocation or callback errors.

## Settings and safety

`configure_effects` changes echo/noise/gain choices on the existing module using
GetConfig/ApplyConfig; it does not rebuild or substitute a product. Other native
configuration fields remain unchanged except the analog-manager enable flag:
digital choices disable that manager so the pin actually selects the requested
legacy digital mode, rather than merely storing a Mode value it ignores. Initialization, current processing rates
and channel counts, output-muted intent, stream delay/key state and analog-level
recommendation invoke the native operations. Delay clamping and warning codes
are SDK behavior; out-of-range analog levels are refused before native gain
assertions.

APM does not read the environment clock on these stock paths, so no additional
clock-range or monotonicity rule is imposed. Trial admission is operation-local:
GC1's reached scanf integer must be representable; AEC numeric parameters must
be safely parsed/narrowed, and the actual adjusted SDK configuration must satisfy
its native validity and suppression mask ordering. The SDK token dispatcher and
private adjustment preflight preserve defaults, ignored tokens, duplicate keys
and percent handling. Preflight never edits trial values or passes a repaired
configuration to the real builtin builder. Unsafe trials return UnsafeFieldTrial
before construction or live ApplyConfig; unrelated effect selections remain
available using the original environment.

## Complete configuration values and initialization

AudioProcessorConfig covers all 50 scalar/enum fields of the pinned SDK Config,
with typed nested groups, native defaults, GetConfig readback and native ToString.
Raw inactive fields are retained independently of consuming admission. Rust
PartialEq compares all represented fields; native_equals on the groups with SDK
operators preserves their exact semantics, including GC1 equality's omission of
clipping_predictor.enabled.

builtin_with_config passes the complete configuration to the actual builder.
apply_config drives native ApplyConfig on the same module. Constructor safety is
distinct from ApplyConfig's recoverable GC2 validation/reset, which remains native
behavior and is visible in subsequent readback. Admission covers active legacy
GC1 setter domains, analog/clipping construction and arithmetic, peak-predictor
float-to-int narrowing, reached pre/post/microphone scaling, compiled pipeline
and GC2 DCHECKs, and actual GC2 exponentiation. Event-predictor float comparisons
are not rejected as though they were peak-predictor integer conversions.

GC2's max_gain_db is not artificially capped: the actual noise limiter bounds
steady target gain. Initial gain above that steady envelope currently returns
UnqualifiedGainRange. This is an incomplete consuming proof, not an invalid raw
value, native capability absence or a silently repaired configuration. Preceding
DSP has no verified universal PCM-range output bound; a finite exponentiation
alone cannot certify arbitrary extreme initial gain. Infinite exponentiation
is independently refused as InvalidConfiguration. Wider initial-gain admission
remains outstanding.

SDK configuration readback is not proof that every submodule was reconstructed.
The pin retains an existing same-channel GC1 analog manager during ApplyConfig,
and its equality omits the predictor enable flag. These native limitations are
not hidden by rebuilding or replacing the selected product.

AudioProcessingFormats preserves all four SDK StreamConfig descriptions and
native empty ProcessingConfig defaults. initialize_with_formats calls the actual
explicit Initialize overload after protecting buffer/converter dimensions. That
overload does not return validation errors, so unsafe layouts are wrapper
InvalidDimensions, not fabricated native error codes. Wholly absent render is
supported with echo off; enabling echo requires real reverse channels. Capture
layout must be mono output or equal-channel; reverse conversion permits equal
channels or a mono side. Native supported rates are not reduced to a whitelist
of 8/16/32/48 kHz.

## Native observations

statistics and statistics_for_remote_tracks pull the native reporter, preserving
all nine optional values and its cached/one-entry reporting queue semantics. The
deprecated bool overload is dispatched with the supplied argument; in this pin
its implementation ignores the argument and delegates to the normal getter.
Unavailable metrics remain None rather than being replaced by zeros.

linear_aec_output retrieves native normalized 16-kHz rows of 160 samples. Only
the required channel prefix is written; extra caller rows remain untouched.
Native absence returns false in builds that permit that query. In asserting
builds, queries without a retained positive storage proof return
UnqualifiedLinearOutputQuery before entry. BufferTooShort is independent of
capability and leaves the caller buffer untouched. Conversion staging is uninitialized and copied only after native
availability confirms the SDK wrote every sample, not to invent initialized
output. The SDK's own ChannelBuffer allocation is initialized, so its available
pre-capture zero output is a legitimate native result.

builtin_with_linear_echo explicitly drives SetEchoCancellerConfig with the
native default mono and multichannel configs, enabling their filter's linear
output. The caller's separate AudioProcessorConfig remains unchanged. The pin's
top-level export flag only allocates storage, not AEC3's framer; enabling both
echo and export on a product without the explicit filter configuration returns
LinearEchoNotConfigured before the otherwise unsafe capture path. Export-only
ApplyConfig changes do not reconstruct storage; initialize explicitly when
changing that flag on an already enabled echo controller. An asserting build's
positive-proof tracking can conservatively refuse a valid read after a config
transition; the release getter retains the actual native available/absent result.

## Typed runtime settings and queue safety

AudioProcessingRuntimeSetting preserves all nine SDK variants, including the
unspecified default and typed playout device defaults. Linear pre/post gains and
custom-render payloads are floats; compression gain is an integer; playout
volume and device integers retain their open native domains. Compression and
fixed-post-gain factories retain their declared 0..90 dB domains. Unspecified
is a real native no-op with true Post result in nonasserting builds; asserting
builds refuse its reached DCHECK before entry. Inactive pre/post NaN and infinity
payloads are representable and passed verbatim when no scaler consumes them.
Active linear factors must be finite, and enabled preamp/adjustment products
must stay finite before the native float-to-int path.

set_runtime_setting calls the actual void SDK setter. Ok is admission and call
completion, never enqueue acknowledgment. post_runtime_setting returns the real
native boolean. Playout-volume posting attempts both capture and render queues:
false can coexist with a successful enqueue on one side and is not rollback.
No wrapper acknowledgment cache substitutes for these calls.

Prospective full-config and selector operations check owned evidence of queued
pre/post payloads. Acknowledged Post retains the latest same-kind target, since
the native FIFO handler overwrites earlier targets before processing samples.
Void Set retains a bounded range of possible targets because any enqueue can
have failed. A later acknowledged same-kind Post supersedes both kinds of prior
evidence. UnsafeQueuedRuntimeSetting identifies an unsafe acknowledged envelope;
UnqualifiedQueuedRuntimeSetting identifies unsafe possible-pending evidence,
not a native capability failure. Neither initialization, ApplyConfig nor render
processing proves a capture-queue drain. Successful capture through either
sample overload clears the evidence, including when capture output is unused.
A negative capture result can precede or follow the drain, so acknowledged
evidence becomes possible rather than being discarded. Caller admission errors
do not enter the SDK and do not change that evidence.

Runtime fixed post gain through 90 dB remains supported by the live GC2 setter.
The pin stores that value in Config, but GC2 reconstruction DCHECKs a stricter
fixed-gain limit below 50 dB. In asserting builds, affected explicit Initialize
and format-triggered processing return UnqualifiedRuntimeReinitialization
before entry; same-format proven-live processing remains available. Unknown
format state after a native failure is conservatively refused at that boundary.
Nonasserting builds retain actual native reconstruction behavior. initialize
therefore returns Result, just like format initialization and processing, rather
than fabricating a native status or silently resetting the gain. The wrapper does
not repair this SDK mismatch. A caller may explicitly apply a valid replacement
configuration if reconstruction is desired. This asserting-profile guard has
source evidence; matching asserting-artifact runtime qualification remains open.

AEC dump, processing providers and builders, and installation of standalone
selected products remain outside this
API surface. Existing peer processing selection is not standalone-product
installation or proof of complete pinned AudioProcessing parity.
