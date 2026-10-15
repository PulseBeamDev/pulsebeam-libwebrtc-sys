# Audio processing and echo components

`AudioProcessingModule` owns a standalone WebRTC AudioProcessing graph. It is
separate from `AudioProcessor`, the per-track SDK AudioProcessorInterface
reference exposed by the media-track APIs. The native wrappers are likewise
distinct; neither product is a substitute for the other.

## Owning processors and echo controllers

Custom processors expose explicit Process, analyzer Analyze and direct
runtime-setting delivery, with borrowed initialized
full-band planar FloatS16 views. These direct calls require successful matching
Initialize. Mutating callback results commit only on success.

EchoControlFactory exposes the pinned stock default/single/optional-multichannel
constructors and both native Create overloads. EchoControl owns persistent
render/capture split filters and its exact environment. An optional estimator is
exclusively borrowed until final controller destruction. Analysis, both capture
processing overloads, metrics, delay, output usage and activity dispatch to the
real native controller. Optional linear output is 16 kHz with capture channels.
BuiltinAudioProcessorBuilder can select the exact owning stock or caller
factory, which supersedes the builder's AEC config as specified by the SDK.
Stock configuration inputs are not substituted for caller factory decisions.
CustomAudioProcessorBuilder consumes an existing processor and invokes native
CustomAudioProcessing/Build. Like the pin's builder, it is one-shot: it returns
the exact original module, keeps that module's original environment/dependencies,
and ignores the extra build environment rather than constructing a replacement.
Echo sample/delay envelopes remain conservative binding admission limits, not
claims that the native controller lacks wider ranges.

EchoControlFactory::from_provider serializes both native Create overloads.
EchoControlCreationContext exposes the actual SDK environment, sample rate and
render/capture channels. Its consuming construction methods either create through
an owning factory with these inputs or transfer an owning caller controller.
The default five-argument callback preserves the native default: it invokes the
four-argument callback and ignores the estimator. Overriding it permits scoped
access to the supplied estimator. Controller callbacks receive fresh
EchoControlDependencies with the same creation inputs and callback-scoped
estimator access. No estimator loan or raw native reference escapes a callback,
and controller destruction cannot call a retired estimator.

Products transfer uniquely and retain their native/Rust environment dependencies.
Rate, channel, native environment-component and used-estimator identities are
validated before SDK handoff. Earlier standalone products retain independent
health when their factory is later consumed by an owning APM.

EchoControl::from_provider implements the owning controller
interface with current full/split-band callback views, both processing overloads,
metrics/delay/usage/active callbacks and a retained failure observer. Full-band and
split-band snapshots preserve native storage distinctions; multiband capture
analysis cannot read not-yet-split storage. Native merge/filter history remains
native. Linear output requires explicit initialized writes for every channel,
never a mutable view of uninitialized native storage. Direct failures commit no
caller PCM, linear output, default metrics or false active-processing result.
Rejected or panicking factory creation cannot return null to the SDK. A private
failed controller permits native unwinding, with persistent failed-product health
that suppresses checked caller PCM, standalone APM statistics and installed
configuration snapshots. It is not a
stock replacement or successful output. Incomplete or failed linear production
initializes only the SDK's private drain buffer before in-call consumers can read
it; no partial linear output is published. Invalid-product health persists until
native reconstruction selects a valid replacement.
Direct APM statistics snapshots now carry an explicit failure marker; fallible
try_statistics variants return errors instead of default metric values after
callback failure. SDK-owned peer RTCStatsReport collection does not carry per-report APM failure
validity. The facade's owner-level error channel is not a validity guarantee for
an asynchronously collected or cached peer report. Capture publication validity follows the pinned
statistics queue/cache order, including discarded newer snapshots. Callback
failure scopes are operation-local, not shared observer counter differences.
Linear getter copy-back is withheld after a failed capture callback until a
successful capture replaces that result. Physical owning APM PCM forwarding is
not enabled.

A selected owning echo factory activates the native controller independently of
Config.echo_canceller.enabled. Setting that bit or a source echo option false
does not remove the factory. Admission and explicit render/delay feedback checks
follow that native selection. Factory/session echo state accounts for this path;
native_reported_echo_cancellation separately retains the SDK's original
config-derived diagnostic. Platform observations and non-echo component reports
are unchanged. These are selection diagnostics, not evidence of echo convergence
or per-frame signal modification.

For selected stock factories, a forwarding receipt observes actual Create calls
and returns the exact native product. Linear-buffer availability follows native
reconstruction and its export setting at that time, not later config echoes.
Successful processing uses no replacement controller, buffer prefill or hidden
render pump. The SDK's initialized pre-capture linear buffer remains a legitimate
native result. Failure-only drain initialization is never exposed as successful
caller output.

PeerConnectionFactory/ProductionSession select either an exact owning
AudioProcessingModule or a Send-only BuiltinAudioProcessorBuilder recipe through native
CustomAudioProcessing. The facade retains the entire original
module/environment/dump dependency graph through final SDK reference release.
Caller-driven headless PCM uses admitted core operations and withholds sink delivery
on callback/admission failure, with typed errors distinct from native status. Full
selected configuration is preserved rather than collapsed to echo/noise/gain flags.
Native physical owning-APM selection is rejected: the pinned transport treats
APM failure status as a DCHECK and continues to send the frame. Its public
post-processing callback requires a reply to an installed sink, not a rejection
result. Observer notification alone cannot stop native capture publication. Headless
render pulls do not drive reverse APM or provide analog microphone feedback.

Factory/session controls can read/apply the selected owning module's full native
config and explicitly process reverse i16 audio and set stream delay. The caller
is responsible for the actual render stream and timing; no hidden pump fills this
in. Echo capture is admitted only after successful explicit reverse and delay
operations on that module. Reverse callback/admission failures leave caller PCM
unchanged. Native statuses and delay clamping warnings remain native results.
Analog-feedback-dependent caller capture is still refused. Session use requires
PCM codec selection, such as pcm_audio(), rather than its absent carrier default.


`AudioProcessingModule::builtin` owns a real builtin WebRTC AudioProcessing module and
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
configuration if reconstruction is desired.

## Stock AEC dump files and retained workers

create_and_attach_aec_dump invokes the real filename overload with byte-preserved
Linux paths. Embedded NUL is refused before the SDK's fatal filename check.
create_and_attach_aec_dump_file owns a std::fs::File and invokes the actual FILE
overload after fdopen, preserving access mode and current descriptor position.
Both operations consume the explicitly supplied TaskQueue on every outcome;
there is no hidden worker creation. A successful attach retains its exact native
queue plus Rust factory/dependencies in an opaque owning leaf of the native APM
state. Native replacement destroys and flushes the old dumper before releasing
its old worker; detach and module destruction do the same. Failed creation keeps
the existing attachment and owner unchanged. detach_aec_dump always calls native
Detach, including its legitimate no-op case.

The stock destructor posts a completion task and waits forever. Only the known
threaded queue implementation is currently proven to make progress across that
wait. Cooperative and provider-backed queues return UnqualifiedDumpWorker;
closed queues are InvalidDumpWorker. This is a consuming safety/qualification
boundary, not an assertion that the SDK lacks those queue types. Arbitrary
provider progress, rejection and cancellation cannot be inferred from a prior
successful post. Same-worker destruction must not block its own queue.

Return bool is the native creation/attach result, not a write acknowledgment.
Zero budgets may create an empty dump successfully; all negative size values
retain the native unlimited behavior. An open failure returns native false;
fdopen conversion failures are separate wrapper errors. In asserting builds,
readonly FILE access with a nonzero budget is currently unqualified because the
native writer DCHECKs failed fwrite. Zero budget proves no writes and remains
available. Filesystem/resource failures during later writes are not made
recoverable or represented as successful recording by the creation boolean.

The enabled factory consumes and closes FILE on every result. The protobuf-off
stub returns false without consuming FILE; this owning adapter closes its input
in that case rather than leaking it. It still calls the real overload and
preserves false, without substituting a dumper. The Rust file is consumed on all
wrapper results as well. Filename creation without protobuf retains native false.

## Caller-owned AEC dump callbacks

AudioProcessingDump is a non-cloneable owning implementation of the pinned
AecDump interface. attach_aec_dump transfers it into the actual stock APM; it
does not select another engine or create a file/thread. The Send + Sync provider
receives typed, borrowed events for every callback overload, including mono,
planar frame-view, planar pointer-array and interleaved sample layouts. Sample
units and native callback dimensions are preserved. The pin emits float capture
as ordered mono callbacks per channel, rather than one aggregated planar frame.
In particular the pin swaps
the channel/frame arguments of its int16 render callback; the binding reports
those actual values, not inferred or repaired metadata. Initialization separately
reports the four actual StreamConfigs and the native UTC timestamp. Processing
state preserves absent applied input volume. Config strings are bytes, including
NUL/non-UTF8; config default/equality use the actual native value implementation.
Runtime snapshots use the SDK's typed union getters, including float storage for
its integer compression-gain factory.

The independent failure observer survives unique handoff and native destruction.
It retains the first failure and a count of failures. Callback rejection, panic,
and provider Drop panic remain separate from native APM status. The single-owner
standalone operation uses a native operation-local callback failure scope.
A rejected callback returns CallbackFailed, not an invented native status.
Nested operations handle their own failures; unrelated observer counter changes
do not change the current operation's result.
Processing uses caller-initialized scratch output, preserving native negative
status/best-effort effects when no callback fails. On callback failure no PCM
is published and caller prefixes/tails remain unchanged. Because callbacks return
void, native DSP can continue and update state or deliver later dump events;
the wrapper does not pretend the engine aborted or rolled back. Capture-queue
accounting still follows the actual native processing result.

Attach/replacement errors do not undo a transfer already performed by the SDK.
Detach returns Result so a failing caller destructor cannot appear as an
unqualified successful operation. Native replacement/detach completes old dump
destruction before releasing a retained stock worker. Module/unattached owner
Drop contains provider panics and records them for retained observers. A panic
payload destructor is also contained; only a second panic payload is deliberately
forgotten if dropping the first payload panics. No normal callback data is leaked.
The provider must not retain event borrows after return.

## Custom processors, analyzers and neural estimation

Owning custom processor/analyzer products provide borrowed
full-band FloatS16 row views and an owning BuiltinAudioProcessorBuilder selecting
capture-post, render-pre and capture-analyzer modules through the real SDK builder.
Providers are Send and serialized, with same-provider recursive entry refusal and
retained failure observers. Native Initialize/ToString and runtime callbacks are
bound; ToString preserves bytes and the default runtime callback is a no-op.

Mutations use a copied working frame committed only after callback success and
finiteness validation. Capture-post accepts finite extremes. Render output uses
a provisional graph-dependent qualification boundary when echo/GC1 render DSP
is selected; this is a conservative binding admission limit, not native
unsupported behavior.

Typed AEC3 mono/optional multichannel configuration preserves native defaults and
explicit Validate behavior. Owning neural providers expose Estimate, AdjustConfig
and Reset. Neural callbacks preserve exact Block band/channel and fixed row
shapes; caller outputs are owned initialized arrays. Native estimate consumption
requires shape-correct finite spectra without an arbitrary amplitude cap.
AEC3 consumption of a returned suppressor validates a copy without changing the
provider result, requires strict mask ordering, and preserves the original
nearend-detector mode used at construction. Rejection records AdjustConfig failure
and uses the original configuration only to drain failed native construction.
An unhandled construction failure remains invalid for that owning module until
actual successful reconstruction; a later successful Estimate alone cannot
certify the rejected controller. Handled child creation attempts do not poison
a different product returned by the caller factory.
Explicit standalone and scoped-loan AdjustConfig calls return raw provider values;
they do not impose the separate AEC3 consumer's constraints.
