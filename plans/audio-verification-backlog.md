# Audio implementation and final qualification backlog

Approved goal: `plans/parallel-slices/03-audio.md`.
Execution change requested by the user: implement remaining surfaces in consolidated
batches first. Defer test authoring/execution, native artifact rebuilds, provenance
refresh and compile/lint qualification until the implementation pass is complete.
This file is tracked and must be updated as implementation proceeds.

Latest execution direction: finish every feasible implementation without changing
libwebrtc. Record paths that cannot meet the contract through existing interfaces
and move on; do not wait for permission to patch the SDK or fabricate EncodedInfo.
The source-first pass now transitions to compilation, focused fixtures and matching
artifact qualification/repairs. Documented exceptions below are not claims of
native unsupported behavior or full installed parity.

The batch ledger below is historical. The final delivery checkpoint at the end
supersedes its source-only/unverified status statements for the specifically
executed Linux release-profile paths. It does not certify the original full
parity contract, other profiles, physical streaming or the explicit exceptions.

## Baseline and evidence boundaries

Last verified source baseline: `a9f2579`, after stock dump `50e0dfa`.
- Native candidate rebuild: verifier02c40dcd.
- Six caller and five stock dump tests + provenance/diff: verifierfce3aa0b.
- Library127 passed/1ignored,75 existing integrations and native-feature compile:
  verifier0d97d43b.
- These receipts do not qualify subsequent native, CXX or Rust changes.
- After bridge/native edits, the existing local archive and checked-in provenance
  pins are intentionally stale until the final matching rebuild. Do not relabel
  the old archive as evidence. Do not describe source-only batches as tested.

## Implementation ledger

| Area | Implementation state | Final qualification still required |
| --- | --- | --- |
| Stock APM config/runtime/statistics/linear output | Matching primary Linux regressions pass | Asserting-profile execution |
| Stock/caller AEC dumps | Primary Linux regressions pass | Protobuf-off FILE ownership execution; installed validity exceptions |
| Custom capture-post/render-pre processors and capture analyzer | Selected native PCM, failures and retained ownership pass | Wider render envelope; strict installed publication exception |
| Echo detector/control and neural estimator builder providers | Actual selected echo factory, detector metrics and stock-AEC3 neural callbacks pass | Full concurrent/shared-provider and all dependent-value matrix |
| Other applicable APM builder/config members | Typed AEC3 defaults/validation and selected builder paths pass primary tests | Broader DSP/configuration/profile matrix |
| Owning mixer/APM factory, peer and ProductionSession installation | Owning selection and scoped mixer/native source regressions pass | Strict installed custom publication and physical owning APM exceptions |
| Explicit Linux AudioDeviceModule/factory/operations | Native lifecycle/ownership and live Pulse metadata pass | Physical capture/playout and exhaustive hardware control outcomes; static Create is declaration-only |
| Remaining codec/NetEq/ANA requirements | Primary linked regressions and enabled ANA effects pass | Installed caller codec return-shape exceptions (#17) |
| Wider GC2 initial gain and remaining admission gaps | Supported boundary/history passes; wider range remains unqualified | Do not rename binding/qualification gaps native unsupported |

## Custom processing implementation checks

- Use the real BuiltinAudioProcessingBuilder and unique-owner callback inputs.
- Preserve Initialize, Process/Analyze, ToString and runtime-setting semantics.
  SDK CustomProcessing::SetRuntimeSetting's actual default is a no-op.
- ToString is native std::string; retain arbitrary bytes rather than forcing UTF8.
- Safe borrowed full-band planar FloatS16 views, exact graph rate/channel/frame
  metadata, no raw pointer escape or callback borrow retention. External float
  APM APIs use normalized units and must not be conflated with callback units.
- Send-only mutable providers need serialized access and same-provider recursive
  call refusal before taking their Mutex. Catch callback and provider Drop panic.
- Retain providers, observers, native and Rust dependencies through unique handoff,
  failed construction, outer retirement, module destruction and actor migration.
- Callback methods return void. Report wrapper failure separately; never fabricate
  native APM codes, successful buffering or unchanged/zeroed successful PCM.
  Preserve actual native state/drain progress even if publication is withheld.
- Mutating callbacks operate on a copied working frame; validate before committing
  back to the SDK buffer. Failure must leave/restore finite safe working data and
  publish no PCM. Record any gap in pre-callback native finiteness assumptions.
- No unconditional [-32768,32768] callback output whitelist. Capture-post finite
  extremes are clamped by traced downstream RMS/output conversion; finite post
  products can overflow but scaler/output conversions saturate. Float output
  clamps before output resampling (AudioBuffer::CopyTo float).
- Render-pre is graph-dependent: two-band raw float QMF subtraction can overflow
  for alternating +/-FLT_MAX; three-band filters multiply by +/-2 and accumulate
  opposing terms. Follow-up751a3830 proves reachable overflow operations, not a
  universal safe envelope or a specific end-to-end NaN. Any conservative render
  qualification guard must be named/documented as a gap, not native unsupported,
  and resolved/qualified at the final pass. Do not silently normalize caller data.

## Unverified batch 1: custom processing/analyzer

Source: src/audio_processor_custom.rs, Rust bridge/runtime helpers and
native/audio_processor.{h,cc}. Added owning typed processor/analyzer products,
serialized Send callbacks, safe FloatS16 row views, actual virtual Initialize and
byte-preserving ToString, SDK builder capture/render/analyzer injection and retained
failure generations. Working frames commit only after callback success/finiteness;
capture-post finite extremes are not range-whitelisted. Configuration callbacks
now participate in failure-generation checks.

A provisional render qualification boundary is active only when echo cancellation
or GC1 selects downstream render DSP. Output beyond conventional FloatS16 range
records UnqualifiedRenderRange and is not committed. This is not established full
native range parity; final qualification must derive/refine graph/stateful bounds
and include later echo/neural selections in policy. With neither render DSP graph
selected, finite extremes are not rejected by this boundary.

Explicit pending checks/repairs: compile/CXX signatures; initial-build callbacks,
config-triggered init failures and provider Drop; current rate metadata; no stale
failure poisoning; capture-output resampling saturation; render policy replacement;
pre-callback native frame finiteness; public callback-buffer projections vs
applicable native operations; native owner Send and installed concurrency isolation.
No tests were authored/run and no native artifact was rebuilt for this batch.

## Unverified batch 2: echo detector

Added a refcounted caller echo detector with exact shared native clone identity,
four-argument initialization, render/capture analysis, optional double metrics,
serialized Send provider callbacks, retained failure observation and actual SDK
builder injection. Echo detector selection participates in render qualification
policy and aggregate callback failure tracking. Native/CXX/Rust remain uncompiled.

Final checks: Native RefCountInterface construction/destruction, exact shared
identity through multiple builders, null/optional metric fidelity, arbitrary
native diagnostic double values, concurrent shared-provider failure isolation,
render queue behavior, and final observer/provider lifetime. Aggregate generation
tracking is currently only qualified for single-sequence standalone calls.

Installation investigation d0d589aa found no existing fail-stop channel for void
custom callbacks in the native voice-engine path. The caller-driven PCM source
already gates actual ProcessStream status before source publication. Implement
owning selection paths without inventing native statuses; final implementation
must add a real publication gate for custom failures or report the exact pinned
SDK incompatibility. A stock-only installation cannot certify custom installation.

## Unverified batch 3: Linux owning AudioDeviceModule

Source adds src/audio_device.rs and native/audio_device.{h,cc}. Explicitly consumed
caller WorkerThread and retained Environment own creation, all control calls and
final native release on that worker; no hidden worker creation or fallback.
Actual static Create and CreateAudioDeviceModule helper remain distinct and retain
nullable outcomes. Linux/native Rust opt-in only; core has no platform factory
reference. Typed lifecycle/device/volume/mute/stereo/effect/availability controls,
raw native current-index/underrun outcomes, byte-preserving names, optional Stats
and all platform-state optionals are mapped. Private dispatch opcodes are not a
public generic operation/config surface. No compilation or runtime checks yet.

Transport and Linux observer callback source added in batch 4 below. Pending
implementation: selected device module factory/session installation and full
lifetime transfer, direct native transport value operations where applicable. Pending final checks:
worker-affine construction/release and outer retirement; backend Init/Terminate
ordering and every control's actual native status; raw index/underrun defaults;
initialized name extents, optional unknown vs false diagnostics; null static/helper
outcomes; core link exclusion; explicit native backend selection; absent hardware
and real physical playout/capture evidence where available. Check thread/env
retention and reverse drop order including failure paths before runtime execution.

## Unverified batch 4: owned device transport/observer callbacks

Added serialized Send transport/observer providers, independently retained failure
observers, panic/drop-panic containment and same-provider reentry refusal. Native
transport implements both recording overloads, NeedMorePlayData and PullRenderData;
observer maps Linux devices-updated and speech events. Actual ADM nBytesPerSample
is interleaved bytes-per-frame (audio_device_buffer.cc:333,370), not channel bytes.
All out parameters and PCM storage initialize before callbacks; staging commits
only on valid callback outcomes. Capture timestamps preserve optionality. Native
callback status, valid returned frame count and timing outputs remain distinct
from wrapper failures. Accepted raw-pointer registrations stay owned through final
ADM release, including replacements and unregister failures. No checks run.

Final checks: dynamic mono/stereo/multibyte extents and zero spans, overflow/null
arguments, initialized full output/tails, exact mic/timing outputs and overload
identity, provider native errors vs wrapper rejection, malformed counts/panic/
reentry/Drop, concurrent record/playout, registration failure/replacement/unregister
and final teardown on worker. Void PullRenderData cannot abort its native caller:
its initialized zero failure fallback is observer-visible, not qualified successful
custom PCM. ADM AudioDeviceBuffer also logs nonzero transport status and may keep
running; do not claim hardware publication fail-stop from an error return alone.

## Unverified batch 5: owning mixer factory/session selection

Added consuming PeerConnectionFactoryBuilder::audio_mixer, actor-owned source
recipe ProductionSessionBuilder::audio_mixer_sources and independent retained
mixer failure observers through factory/session accessors. The installed SDK-facing
adapter delegates the exact selected SDK mixer; shared state retains all registered
Rust participants even after outer wrapper retirement. Scope/guard tracking also
runs when native AudioTransport invokes Mix directly. Source-only, no checks run.

Final checks: exact selected mixer effect vs default, SDK engine sources plus
caller sources, duplicate registration and shared product identity, dynamic rates,
wrapper/native scoped reentry, actual initialized/muted frames, callback failures,
worker/actor migration, queued work and final observer/provider destruction.
Installed Mix is void: on error this adapter records failure and mutes the output;
it does not establish an installed publication fail-stop channel. Resolve/report
that exact consumer limitation, rather than certifying fallback silence as custom
success. Caller implementation of the complete mixer interface remains pending.

Provider follow-up3bd8e84f inventories echo/neural headers but its claim that
implementation sources are absent applies only to the package: the parent has
.work/checkout/src/modules/audio_processing/audio_processing_impl.cc. Use that
checkout for callback-position/initialized split-band decisions; do not infer a
pinned-source blocker from the package-only investigation.

## Unverified batch 6: complete typed AEC3 configuration

Added 24 nested typed Rust config structures with all 136 scalar field paths from
api/audio/echo_canceller3_config.h, native default/multichannel default and actual
mutating Validate operations, lossless private flat CXX mapping, and builtin SDK
SetEchoCancellerConfig selection. Explicit absent multichannel config preserves
SDK mono reuse (audio_processing_impl.cc:1870-1881). Linear-output convenience
updates selected configs explicitly; direct selected export flags also participate
in the existing linear-output admission/availability state. No checks run.

Final checks: every public field/constructor/default, clone/assignment and exact
float/usize/int/bool round trips, native Validate return/fixups (not a complete DSP
safety validator), mono vs absent/explicit multi graph selection, actual AEC effects
and linear output under reinit/config changes. Arbitrary mutable values require
final source/numeric admission review BEFORE execution: Native Validate does not
cover every reached allocation/arithmetic/smoothing/indexing domain. Do not label
missing qualification native unsupported or silently normalize builder inputs.

## Unverified batch 7: owning neural residual echo estimator

Added unique neural estimator provider/product, real Estimate/AdjustConfig/Reset
virtual dispatch and builtin builder handoff, retained serialized Send provider
and operation-specific observers. Exact borrowed render Block band/channel/64
views and independent 64-sample/65-bin row projections preserve all span lengths;
16 kHz per-band metadata follows the public Block contract. Caller returns owned
fixed-size arrays so native output initialization is never inferred from scratch.
Real native standalone invocation covers every virtual operation, not Rust echo.
AdjustConfig maps the complete typed Suppressor. No checks run.

Failure staging does not read potentially uninitialized native R2 outputs; on a
void Estimate failure it supplies finite native continuation storage and reports
failure so owning APM publication is withheld. It is NOT successful estimated
power or proof of installed fail-stop. Count/nonfinite checks are implemented;
negative/extreme finite spectra and suppressor values still require actual
consumer admission/qualification BEFORE executing the integrated DSP path.

Final checks: all native shapes including empty/independent span lengths, exact
row/band layout, actual selected AEC estimation effects, reset/state retention,
Suppressor roundtrip/adjustment and constructor callback failures, malformed
spectra/panic/reentry, nonfinite and finite boundary arithmetic, non-null unique
handoff and all destruction paths. Matching CXX/template span signatures and
initialization contracts must be checked in the final compile/safety pass.

## Unverified batch 8: explicit native device creation in factory/session

Added typed Linux device creation recipe selecting exact layer and either static
AudioDeviceModule::Create (including bypass flag) or CreateAudioDeviceModule helper.
Peer and ProductionSession builders explicitly opt in, marshal creation/Init and
release to the retained factory worker, and preserve existing absent/default
headless behavior. Nullable SDK factory failure and actual nonzero Init status
remain distinct; no fallback is selected. No checks run.

Final checks: each SDK creation path/arguments, actual ActiveAudioLayer and device
selection, default helper compatibility, explicit disabled opt-in after recipe,
headless/core link and controlled-driver guards, worker-affine failure teardown,
production actor migration, direct topology vs platform-state snapshot fidelity, selected backend profile capabilities vs unavailable
sound service, plus hardware discovery/capture/playout where available. Native
nullable creation alone does not identify unsupported backend vs absent service;
do not infer a reason the SDK did not return. Owning standalone module transfer
and complete installed device operation/observer access still remain open.

## Unverified batch 9: installed device controls and actor operations

Added InstalledAudioDevice retaining the exact factory graph/module and worker,
with typed controls, availability/state/name/statistics projections and safe module
reference release on that worker. Standalone full lifecycle remains separate:
installed views cannot terminate or replace the engine-owned transport. Factory
and session expose actual selected device operations; actor results are owned
values, never local graph handles. Observer registration retains every accepted
raw-pointer dependency through actual factory ADM release, not a temporary view;
inherited SetObserver -1 remains native unsupported. No checks run.

Final checks: exact module identity/lifetime, source-device selection and live
native states, all control statuses, temporary view retirement without stopping
capture/playout or unregistering transport, accepted observer replacement/unregister
and final destruction, unavailable headless factory, session closed paths and
caller-thread migration. Native engine-owned capture/playout activation stays on
existing set_native_audio_enabled rather than conflicting direct ADM lifecycle.
Standalone owning module transfer and caller device factories remain open where
required by the applicable operation boundary.

## Remaining implementation constraints to reconcile

- Stock EchoControl/Factory products and selection are implemented in batch 11;
  caller EchoControl/Factory providers and caller mixer remain unimplemented. Exact pinned
  EchoControlFactory::Create overloads are non-null; audio_processing_impl.cc:1862
  consumes Create and DCHECKs the product. Rust Err/panic has no native factory
  failure result. Do not return null, fabricate a no-op product or substitute stock.
  Expose feasible stock/product operations and report the exact creation-channel
  incompatibility if no faithful no-SDK-patch path exists.
- Headless owning APM factory/session selection is implemented in batch 14.
  Final ownership qualification must cover the entire module + Rust Environment and
  callback/dump dependencies through final SDK release. Stock CustomAudioProcessing
  is the actual pinned selection API. Caller-driven PCM already has a publication
  gate; installed voice capture does not honor a new void-callback error channel.
  No invented native status or silent fallback can satisfy that obligation.
- Installed observer retention currently uses the factory's accepted-registration
  owner vector. Final source/lifetime review must prove callbacks quiesce before
  that owner retires or tie dependencies to actual final ADM release. Do not assume
  factory-wrapper destruction alone proves no retained SDK module references.

## Unverified batch 10: standalone custom processor/analyzer operations

Added safe borrowed planar FloatS16 constructors, actual CustomProcessing::Process,
CustomAudioAnalyzer::Analyze and direct CustomProcessing::SetRuntimeSetting dispatch.
Processing copies native work back only after callback success; analysis is read-only.
Initialization must establish the same native format, and native buffer backing/index
bounds are checked without limiting rates to APM's conventional rate set. This adds
the missing direct product operations rather than relying only on installed callbacks.
No checks run.

Final checks: 8/16/32/48 kHz plus a native nonstandard floor(rate/100) format, mono,
stereo and larger admitted channel counts, format change and failed initialization,
shape/overflow boundaries, exact full-band FloatS16 units, untouched caller audio on
Err/panic/nonfinite output, callback reentry, explicit default runtime setting and
every factory payload including inactive nonfinite values. Verify current SDK
RuntimeSetting factory bounds and AudioBuffer constructor/index arithmetic.

## Unverified batch 11: stock owning echo factory/controllers and selection

Added exact EchoCanceller3Factory default/single/optional-multichannel constructors,
both Create overloads (including nullable borrowed neural estimator), owning
EchoControl and all native public operations. Persistent native render/capture
AudioBuffers preserve split-filter history. Full-band FloatS16 input/output and
optional 16-kHz linear output commit only after operation-local success. The neural
loan and environment remain alive through controller destruction. Actual builtin
builder SetEchoControlFactory selection supersedes AEC config, without substituting
a stock factory for a caller selection. Caller factories/controllers still missing.
No checks run.

Final checks: constructor/default distinctions, exact environment identity and
four vs five argument null/non-null neural dispatch, stereo/multichannel layout,
all public methods/overloads, split/full-band ordering/history, all initialized
linear output samples, factory retirement before products, neural exclusive-loan
compile behavior, destruction/migration, actual selected factory effects. Qualify
native AEC3 config construction/Validate limits, field trials and channel index
arithmetic. Provisional conventional FloatS16 input and nonnegative internally
converted delay guards are explicitly UNQUALIFIED ranges, not native unsupported:
resolve wider valid inputs, signed multiply/add and subsequent delay-map arithmetic.
Selected original and SDK-adjusted AEC configs now have copy-only native Validate
and suppression-order preflights at enabled construction/ApplyConfig, in addition
to existing trial-parser checks. These are NOT a complete numeric safety proof;
inspect reached config sizes, arithmetic, validation NaN behavior and neural
AdjustConfig outputs before qualification. Delay metadata includes original and
adjusted fixed capture delays conservatively.
Linear export metadata currently requires both mono and optional multichannel
configs; refine against actual native selection if it excludes valid mono-only
operations. Verify optional linear-buffer format/availability before reading it.
The convenience linear_echo_output flag does not mutate an explicitly selected
factory's immutable config, matching SDK precedence.

## Unverified batch 12: direct mixer source frames and destructor containment

Added actual standalone Source::GetAudioFrameWithInfo dispatch with native normal,
muted and error dispositions. kError does not become a Rust callback failure.
Muted results keep the native implicit-zero interpretation without reading invalid
producer PCM. Full shape/initialized-prefix validation remains on the source
adapter before native frame ownership transfer.

Source providers now retire explicitly inside a panic boundary, and caught panic
payloads use the existing safe disposal routine rather than potentially panicking
again while crossing CXX. No checks run.

Final checks: standalone direct dispatch vs registered dispatch, complete metadata,
muted/error no-PCM-read behavior, initialized tails, callback error/panic/reentry,
ordinary and hostile provider Drop/panic-payload destructors after outer retirement
and final installed native release. Preserve Send/Sync source serialization across
multiple owners. This does not fill the still-missing caller mixer product binding.

## Unverified batch 13: retained owning APM handoff foundation

Added private move-only Rust-to-native handoff and an actual reference-counted
AudioProcessing facade forwarding the full pinned pure-virtual surface. Every SDK
reference owns the selected original module, exact Rust Environment and module's
retained dump/queue/custom dependencies. The native core is explicitly destroyed
before its environment. Original full config is retained for deliberate restoration
after media-engine default option application. No new module or environment is
substituted.

Next implementation steps: pass the owning handoff through PeerConnectionFactory
and a Send-only BuiltinAudioProcessorBuilder recipe in ProductionSession; select
it through the SDK's actual CustomAudioProcessing builder; retain its shared core
in caller-driven sources; apply native source option changes through admitted core
helpers and suppress PCM on wrapper/callback failure. Preserve full selected config
rather than rewriting it from the old echo/noise/gain convenience projection.

This is foundation, NOT completed factory installation: no public selector uses
the handoff yet. Raw virtual forwarding alone bypasses the core's wrapper admission,
queued-setting bookkeeping and operation-local publication checks. Integration
must use admitted core operations at headless PCM, retain exact full configs and
callback generation, and reconcile SDK-originated config/runtime changes. Do not
enable a physical callback path that ignores these failure outcomes. No checks run.

Final checks: exact virtual signatures and complete dispatch, all native ref
retirement orders, source/factory/SDK outer-owner retirement, module/dumper before
worker/environment destruction, identity preservation, migrated final release and
failure rollback/retention. Follow-up investigation f04ef8dc confirms native
capture only DCHECKs processing status and PullRenderData omits reverse processing;
neither is a working failure/publication gate.

## Unverified batch 14: owning headless APM factory/session and checked PCM

Added PeerConnectionFactoryBuilder.audio_processor(product) unique transfer and
audio_processor_builder(recipe); ProductionSession accepts the Send-only recipe,
built in its actual environment. Processing selectors are explicitly last-choice
wins. Actual CustomAudioProcessing selects the retained native facade, never a
new builtin substitute. Full original config is deliberately restored after voice
engine defaults, and caller PCM applies only source enable/disable choices while
retaining the selected algorithms/parameters and DSP history.

Selected source PCM uses admitted core config/processing helpers and real native
status, then gates sink publication on wrapper/callback failure. AudioFrameError
distinguishes exact native processing errors from typed ProcessingAdmission and
missing render/analog feedback. Native-originated config/runtime controls now use
admitted helpers and queue bookkeeping; errors from void controls are retained for
construction failure or the next caller-owned capture operation. Handoff rejects
unsafe queued runtime reinitialization before SDK construction.

Added same-core capture reentry refusal before owner locks, rejected source close
before setting Rust closed state, and failed/reentrant public processing snapshots
with explicit failure + unknown component values rather than cached success. A
private SDK GetConfig reentry snapshot is drain-only and marks the outer operation
failed. SDK-originated Initialize invalidates core format hints and records callback
failures. No checks run.

Still NOT complete: physical owning APM is rejected as AudioProcessingAdmission
with an explicit unqualified binding/failure-channel message, before device
activation; it is not reported as native unsupported. Headless PullRenderData does
not drive reverse APM, so echo and analog-feedback-dependent capture are refused
on the source path rather than advertised as effective. Existing simple native
processing remains separate and unchanged.

Final checks:
- Full Rust/CXX/native signatures and actor Send/non-Sync/static ownership.
- Exact selected product/environment after outer retirement, factory defaults,
  track changes, format changes and multiple sources sharing one module.
- Real custom/stock PCM effects through negotiated call and ProductionSession,
  untouched tails, errors before sinks, initialized/muted layouts and no second
  APM construction. Last-selector ordering and absent baseline behavior.
- Rejected/panicking Initialize/Process/runtime/dump callbacks, pending void-control
  errors, operation-local failure isolation, failed construction and retained dump
  queues through final SDK reference release, including thread migration.
- Source close, sibling-source push, snapshot and other reachable peer/track/sink
  retirement reentry. Close/snapshot/core gates are implemented, but broader SDK
  graph-lock reentry and cross-thread ordering still require actual-consumer review.
- SDK Initialize entry sites and every pre-handoff queued-runtime state: confirm no
  raw SDK reinitialization bypasses core admission after handoff. Direct physical
  DSP forwarding is NOT qualified merely by the shared lifetime facade.
- Full config restoration, no generic Linux analog/high-NS substitution on capture,
  dual GC1/GC2 choices and source enables, GetConfig/runtime bookkeeping and repeated
  identical frames retaining native history.
- Core/native builds, controlled/carrier exclusions, typed unavailable combinations,
  explicit physical opt-in remaining rejected for owning APM, no hidden device.

## Unverified batch 15: installed full configuration and explicit render/delay

Added actual selected owning APM config read/apply and explicit reverse-i16 input
and stream delay on PeerConnectionFactory and ProductionSession. All operations
marshal to the retained worker and use admitted core helpers. Application config
updates replace the desired full native config (including actual native validation/
reset outcome), so caller capture does not silently restore obsolete constructor
settings. Successful capture retains consumed native runtime parameter changes
without turning a source's temporary enable flags into shared requested defaults.

The headless caller path may now use echo only after explicit successful render
input and native accepted/clamped delay input on that same module. Headless pulls
still do not supply render automatically. A failed reverse call invalidates render
readiness. Analog feedback remains refused. No physical device is activated.
No owning selection is a distinct binding error, not native unsupported. The
fallible factory processing snapshot and session snapshot surface reentry failure
as an error; the retained compatibility DTO also carries its explicit failure.

Reverse PCM stages initialized input, commits no caller PCM on callback/admission
failure and preserves extra suffix storage. Native status/error-copy behavior is
returned exactly. Signed-prefix/downmix bounds match standalone admission.

Final checks: actual selected GetConfig vs constructor echoes, failed apply and
native reset/readback, source options/runtime setting persistence across repeated
frames and multiple tracks, explicit render processors and real echo effects on
capture, correct capture/render/delay sequencing and native warning statuses,
reverse failure recovery, direct and sibling reentry, prefix/tail bounds including
empty/native unsupported formats, nonnative rate status behavior, no-owner error,
actor migration and closed actor paths. Review readiness resets at native format/
config reinitialization against actual SDK AEC state, not setter receipts alone.
Do not treat explicit caller reverse support as physical callback qualification.

## Unverified batch 16: owning caller mixer and scoped native inputs

Added AudioMixer::from_provider with actual native AddSource/RemoveSource/Mix
dispatch, no stock mixer construction. ProductionSessionBuilder accepts the
Send-only provider recipe and constructs the exact mixer inside the actor; it
is last-choice-wins with audio_mixer_sources.

Opaque cloneable tokens identify native registration epochs, not SSRCs or guessed
pointer joins. A per-product registration table protects raw SDK source access
during callbacks. Registration info loans expose current SSRC/preferred-rate
queries; only the single-thread mix loan can drive real GetAudioFrameWithInfo.
Tokens alone cannot call a source, foreign/retired tokens fail, and removal always
retires access before SDK destruction even after Rust failure. Native source
normal/muted/error outcomes preserve initialized/mute state. Caller outputs are
validated against the actual native mix/frame contract before copy.

Add refusal remains a native false distinct from callback failure. The pinned
AudioState logs false non-fatally, then may still call RemoveSource on stop;
the adapter safely handles that absent removal. Mix is void in both transport
consumers and has no fail-stop result. The installed drain now establishes valid
muted metadata even when caller Mix produced no frame, preventing transport
resampler access to an untouched frame. That private failure drain is NOT certified
successful caller output or physical fail-stop; installed publication remains an
explicit qualification gap.

No checks run. Final checks: selected custom provider used standalone and through
actual SDK receiving-stream registration/mixing; false/Err/panic/current answers,
same/different-thread add/remove vs Mix and native receive-stream retirement;
token epochs after re-add/address reuse, outer retirement, destructor panics and
actor migration; real source frames, native errors/mute/metadata, tails and no
preinitialized output, malformed provider output rejection; callback reentry and
graph retirement during SDK source calls; actual installed publication failure
channel. The pinned AudioMixerImpl serializes source callbacks/removal under its
mutex; the caller adapter supplies its own full callback lock. Same-thread SDK
graph retirement during a callback needs review separately from concurrent removal.

## Unverified batch 17: existing owning ADM handoff

PeerConnectionFactoryBuilder::audio_device_module consumes the actual owning
AudioDeviceModule, adopts its original worker, retains its original environment
and transfers the exact SDK module into dependencies.adm. A separate supplied
worker is a typed AudioDeviceAdmission error, not backend unsupported. Native
construction also checks worker identity before selecting the module. Last-choice
native selectors retire an earlier owner instead of secretly keeping it selected.

Native device state and accepted transport/observer registrations survive through
final installed module release. Successful handoff changes standalone lifecycle
ownership to engine ownership; the wrapper then releases references/registrations
on its original worker without Stop/Terminate on an already engine-owned module.
Failed construction retains ordinary standalone teardown. Installed device views
retain the factory graph and continue to control that exact module.

The pin's AudioDeviceModuleImpl::Init is idempotent if initialized. However
adm_helpers::Init deliberately selects default device index 0, initializes
speaker/microphone, sets stereo to native availability; voice-engine ApplyOptions
sets hardware processing defaults, then RegisterAudioCallback replaces standalone
transport. Handoff documents those ACTUAL native defaults rather than promising
unmodified pre-install config or replaying guessed state. Installed control APIs
can apply/read native post-init choices. Caller observers/transports remain alive
through their raw-registration retirement, even when the engine replaces transport.

Standalone native device construction now reserves a production lease before
physical construction, rejecting controlled-world coexistence. Factory handoff
takes over under its already-acquired production lease without an unleased gap.
Actor uses its existing native_audio_device creation recipe in its own environment/
worker, never transports a non-Send standalone ADM across the actor boundary.

Pinned AudioDeviceBuffer::RegisterAudioCallback writes a non-atomic raw pointer
and only warns if media is active. Standalone replacement/unregistration and
owning factory handoff now reject active recording/playout before that write.
This is explicit binding admission (ActiveTransportRegistration or owning ADM
admission), not a fabricated native status or silent Stop. Failed owning build
still performs normal consumed-owner teardown. Include active-registration
refusal/recovery and no pointer write on refusal in final native qualification.

No checks run. Final qualification: Rust/CXX/native signatures, worker adoption
without a replacement thread, exact module/environment/callback identity and
successful/failed native init teardown, selector order and native device config
readback before/after native engine defaults, factory views after outer retirement,
last reference destruction on worker, active recording/playout transition and
transport replacement, production lease failure/continuity, controlled exclusion,
hardware-null vs Init error vs binding admission and unavailable core artifact.
A machine without native audio cannot certify real physical capture/playout.

## Unverified batch 18: owning caller EchoControl and initialized band/output loans

Added EchoControl::from_provider and native EchoControl virtual dispatch for both
analysis methods, both ProcessCapture overloads, metrics, signed delay, usage and
active-processing. It owns the exact serialized Send-only Rust provider, original
environment and failure observer; no stock factory/controller is constructed.
Source-only product use is independent of the still-missing caller factory.

Added scoped full/split-band snapshots, preserving native full vs split storage
and one-band alias semantics. Multiband capture analysis exposes no uninitialized
split storage; render/process views follow the real split phase. Mutations stage
separately and only validated successful callbacks commit. Native band merge and
filter history remain real; no synthesized full-band reconstruction in getters.

Linear output is WRITE-ONLY until each channel is explicitly supplied from
initialized caller storage. Native linear storage is never read or prefilled to
fake user output. Missing channels, nonfinite output, callback rejection/panic
withhold both direct capture and linear copy-back. Null output vs the no-linear
overload remain distinct. Metrics/active failures cannot return successful
default/false direct results. Caller signed delays bypass stock AEC3 arithmetic
bounds, since no stock delay calculation runs. Caller products admit native 8-kHz
single-band buffers as well; stock AEC3 still refuses 8 kHz before construction.

Also repaired batch 17 owning-ADM Rust cfg boundaries so core builds do not refer
to the native-only public module type. No checks run.

Final checks: Rust/CXX slice-loan lifetimes and aliasing, no callback view escape
or Send/Sync, actual split-filter effects and state across repeated calls, correct
full-vs-split visibility and single-band mutation alias, native 8/16/32/48-kHz
buffers, render/capture channel differences and linear allocation bounds, exact
native metrics/delay/usage and failure statuses, all initialized output channels
and unchanged caller PCM on partial/failed/nonfinite callbacks, destructor panic,
outer environment retirement, reentry and callback serialization. Qualify wider
float output at actual merge/downstream consumers rather than arbitrary ranges.
Pinned synthesis source inspection found float-only 2-/3-band merge (no sample-
dependent integer conversion/index), so finite-extreme overflow on this DIRECT
float-output path remains float Inf/NaN, not merge conversion UB. It does not
certify subsequent native PCM/NS consumers or guarantee finite merged output;
matching compiler/FP/runtime qualification remains deferred.

NOT installed yet: the factory non-null handoff remains unresolved. When this
controller enters an APM, failed linear callbacks must establish private defined
native drain data before noise suppression can read it, while EVERY public
PCM/linear/statistics/effective-state projection still reports failure. Do not
reuse the direct-product error gate as proof for a void SDK consumer. Shared
callback-counter generation snapshots also need operation-local concurrency/
nested-call isolation review before qualification.

## Unverified batch 19: standalone APM statistics failure projection

AudioProcessingStatistics now carries explicit binding failure. Failed native
snapshot callbacks produce no optional metric values, distinguishable from a
successful empty native snapshot by failure. Added try_statistics and the
deprecated remote-track-overload fallible projection. Callback fallback zeros
cannot become successful public metric values on this owning direct path.

No checks run. Final checks: native default optionals, stock actual snapshots/
aggregation side effects, exact deprecated argument dispatch, detector metrics
rejection/panic/recovery, no stale fields on failure and no second native pull for
the fallible API. Existing fixtures with struct literals need the new failure
field or Default update. Shared callback generation counters still need actual
concurrency/nested-call isolation review; an observer counter is not an operation
token. This change does not qualify SDK-owned PeerConnection stats collection or
the installed facade's raw getters.

## Unverified batch 20: operation-local callback failure and cached publications

Replaced native aggregate counter comparisons with synchronous per-thread,
nearest-matching-operation failure scopes across direct custom/detector/neural/
echo products, APM construction and owning operations. Shared observer totals are
diagnostic only. A failure in another thread, or a nested operation whose error
the provider handles, cannot by itself poison an outer successful operation.
Scope matching uses the actual retained status object, not guessed provider IDs.

Corrected batch 19's getter-only assumption: pinned APM collects controller/
detector metrics DURING capture and publishes to a capacity-one queue. The getter
only removes a queued snapshot or returns its cache. Added validity tags following
that exact queue/cache order, without copying metrics, pulling early, clearing
native reporting state or replacing a full queue's older snapshot with a newer
frame. Owning and SDK-facade capture producers now record tags; both standalone
and facade getters consume them. A failed queued/cached snapshot remains failed
until native consumption of a successfully queued successor. Standalone fallible
statistics return error rather than cached callback fallback numbers. Per-source
metric validity also persists when a native frame skips its metric callback
(for example muted echo-detector capture); a successful different provider or
successful capture alone cannot relabel those stale fallback fields as valid.

Linear publication remembers the last capture callback failure and refuses
copy-back until a subsequent successful capture. SDK facade getters serialize
against capture and record failure in the retained owner's channel. These
changes do NOT qualify raw physical capture PCM publication, which still forwards
native buffers/status and remains unenabled for owning APM selection. Public
peer stats also still need an operation-paired error projection; private SDK
empty-statistics drains are not genuine public absence.

No checks run. Final checks: cross-thread shared detector failures, nested handled
reentry/failure, refusal before provider Mutex, all scope lifetimes/destruction;
good-queued/bad-discarded and bad-queued/good-discarded order, repeated cached
reads, successful successor recovery, muted/skipped detector metric callbacks
with another successful metric provider, both native capture types and getter
overloads, no extra native pulls, native error paths before queue publication,
linear failure/recovery with unchanged destinations, SDK facade synchronization
and actor migration. The matching-runtime proof must confirm every producer and
getter is covered. Metadata tags are not evidence for physical PCM withholding.

Factory investigation also confirmed exact constructor Environment provenance
and that SDK final Submodules destruction destroys its estimator BEFORE its echo
controller (declaration order). Do not expose a Rust estimator reference retained
by controller Drop under an assumed reverse order. Direct owning loans remain
exclusive. Installed context design must make all applicable estimator access
safe and retain actual Rust dependencies without substituting another estimator.

Newly identified selected-factory audit: SDK NeedEchoController is config.enabled
OR factory presence. Stock selected factories therefore activate a controller
even with echo.enabled=false. Remaining admission, feedback readiness and
effective-state projections must account for actual factory presence, not only
the config bit. Do not call this source-only selection fully safe/qualified.

## Unverified batch 21: actual selected-factory activation and linear allocation

Owning APM admission now follows the pinned NeedEchoController predicate:
config.enabled OR the uniquely installed factory. Stock selected config/trial/
linear-export validation, render callback range admission, missing-render
initialization refusal and headless feedback readiness cover factory-only echo.
configure_effects also validates stored AEC3 configs before enabling their actual
consumer. Raw config/source option bits are preserved: disabling the echo config
bit does NOT detach the selected factory or silently substitute another module.

Added a forwarding factory observation receipt. Both Create overloads forward
the exact environment/rates/channels/nullable estimator and return the original
native product unchanged. Successful creations are observed without caching
provider answers. A module baselines any earlier standalone creations before
Build; it records allocation state after every native entry capable of creating
a controller, including capture/reverse implicit initialization and SDK facade
initialization. Linear allocation follows actual InitializeEchoController calls,
not export/config echoes. Export-only changes without native reconstruction
cannot manufacture availability or erase an extant native buffer. No controller
wrapper, replacement DSP, hidden progress or SDK patch was introduced.

Factory/session echo snapshots account for actual unique factory presence, using
the SDK's exact platform active/resolved fallback when merging effective state.
native_reported_echo_cancellation preserves the original config-derived SDK
diagnostic separately, including native optionals and platform observations.
No HPF/AGC activity is inferred from factory presence. Missing factory snapshots
start with unknown tri-states, not default-false activity. Owning pending failures
and reentry project to failed/unknown snapshots rather than fabricated state.

No checks run. Final checks: selected stock factory with config.enabled=false,
actual render/capture and source echo-off options, feedback refusal before sink
delivery, native config readback distinct from actual selected state, unchanged
high-pass/noise/gain reports, native/platform merge cases, callback render range
and selected config/trial guards in both config APIs, zero-render refusal.
Exercise linear buffer creation/recreation through Build, explicit/implicit
initialization, reverse and config-induced reinitialization; export-only toggles
with/without intervening getters, prior standalone products, original product/
environment/estimator identity and outer retirement. Confirm every native entry
is observed and qualifies the creation/allocation relationship in matching code.
Struct-literal fixtures for AudioProcessingState need the new native-report field.

This does not implement caller factory Create or qualify physical PCM/public peer
statistics failure transport. A future rejected caller creation needs persistent
invalid-product health in snapshot/publication gates; factory presence alone
must NOT certify a failure-drain controller as enabled successful processing.
Caller factories not consuming AEC3 must not inherit stock-only trial/config
bounds merely from factory presence.

## Caller echo factory creation and remaining installed constraints

Pinned api/audio/echo_control.h annotates BOTH factory Create overloads as
absl_nonnull. Only the five-argument estimator input is nullable. Its default
implementation forwards to four-argument Create. audio_processing_impl.cc
initialization calls the five-argument overload, DCHECKs the controller, then
dereferences it for analysis/processing/delay/metrics/output-usage operations.

A Rust factory Err/panic cannot be represented as a native null product. Do not
add nullable caller creation, fabricate a stock replacement, claim native
unsupported or treat an SDK DCHECK as recoverable failure. The bridge now transfers
real selected products on success and uses a private failed controller on rejection.
Per-product invalid health gates checked caller PCM, standalone statistics and
installed configuration snapshots. Linear failure initializes only private native
drain storage before downstream readers. Actual environment components, geometry
and used-estimator identity are checked, and unique handoff retains Rust/native
dependencies. Estimator loans are callback-scoped and unavailable at destruction.
Earlier standalone products cannot change a later installed module's observation
domain. All of this still needs matching compilation/runtime qualification.

Remaining installed constraints are not resolved by this bridge: pinned physical
capture continues to send after APM failure. Exact frame-buffer identity permits
pairing through AudioFrameProcessor, but that interface requires a reply to an
installed sink and has no rejection result. Peer RTCStatsReport collection also
has no per-report APM validity identity across asynchronous/coalesced/cache-hit
collection. Do not substitute owner-counter deltas or guessed joins. These remain
documented contract/consumer exceptions under the latest instruction to note
incompatibilities and move on. They are not native unsupported, do not establish
full installed parity, and do not block qualification of feasible implementations.

## Unverified stock owning ANA implementation

Source now installs delegates at the initial creation of the privately proven
builtin Opus leaf using its original SDK SDP config, payload/options and actual
retaining environment. Other factory selections are unchanged. There is no
selected-product retrofit, metadata-based cast, algorithm replacement, clock
replacement or SDK runtime patch. `native/audio_encoder_ana.inc` is fingerprinted
alongside the private read-only native state access header.

`enable_audio_network_adaptor` is source-wired for direct Rust and retained SDK
products. The original bytes create the real ControllerManagerImpl and controllers.
Per-controller bitrate offsets and all FEC time constants are retained separately.
Constructor admission no longer rejects duplicate runtime writers wholesale:
V2 can be inactive, and its native writer has no occupied-slot assertion at all.
Actual writer DCHECKs, overhead positivity/offset assertions, threshold ordering
and receiver-range ordering follow the matched asserting profile. Native V2
writes/overwrites and nonasserting behavior are preserved.

Legacy/V2 arithmetic admission inspects actual native cached metrics/frame state
and only reached transitions/candidates. Bitrate proxies retain only exact forwarded
sparse inputs and actually consumed frame history. Native sorting is delegated once
on ordinary valid paths without an extra clock sample. Unsafe comparator domains
use actual native cached-order/time/distance skip gates, never guessed history.

The actual native pipeline runs once in native selected order during Prepare.
Only a complete admitted native decision reaches the real adaptor's unchanged
stats/event-log writer via a single-use private DecisionHandoff. Failure does not
publish partial/fabricated success metadata. Output bitrate field-trial arithmetic
and actual FEC loss estimates are admitted before codec/event-log consumption,
including the logger's float-to-uint32 narrowing. FEC durations are not filtered
by predicted recurrence averages. Exact sparse setters mirror the pinned adaptor's
only last_metrics_ writers. No successful response is cached across operations.

Per-operation scopes clear only failure flags, not native smoothing/controller/
reorder history. Reset preserves ANA; disable/re-enable replaces controllers but
retains the bitrate smoother. Rust controls report failures; SDK void calls retain
typed failure observation. Standalone encode withholds output on adaptation failure;
SDK encoding retains genuine native payload/metadata and separately observes the
adaptation failure, never inventing successful zero output. RTT/overhead/frame-range
Rust methods now return Results.

Source review was not qualification. Its reported V2 missing-metric ordering bug
was not present in the inspected parent source; direct inspection instead confirmed
that V2 has no occupied-slot assertion, and the production guard was corrected
accordingly. No compiler, diagnostic, test, rebuild or new commit has run.

Deferred qualification must include real enabled/native decision effects, multiple
writer orders (especially inactive/active V2 and nonasserting overwrites), actual
slot DCHECKs, per-instance offsets/durations, comparator skip/cache paths, sparse
metric mirrors, one native clock/algorithm execution, no failed-decision stats/log
publication, FEC amplification and asynchronous event-log narrowing/FP context,
bitrate average endpoints, ignored/active extreme periods, byte-buffer prefixes,
partial packets, reset/replacement/disable lifecycles and retained dependency drop.
Update existing pending-adaptor and constructor-duplicate fixture expectations;
handle new Result-returning controls. Match protobuf-on/off and asserting/nonasserting
archives and refreshed provenance before using any of this as outcome evidence.

## Documented caller encoder SDK product incompatibility

Source audit 38616235 confirms that existing factory SDK dispatch and native
RetainedAudioEncoder forwarding do not supply a caller-provider AudioEncoder
SDK product. CallerAudioEncoder remains Rust-only fallible standalone dispatch.
Its real SDK getter/control virtuals and Encode dispatch are still unfinished,
separate from the already completed standalone decoder SDK receipts.

A real SDK Encode override must return EncodedInfo even on provider Err/panic.
Under the literal prohibition on fabricated EncodedInfo, no graceful return is
established. A standalone-only mandatory receipt could reject the whole public
operation and prevent all packetization/result publication, but would still need
an internal ABI drain return. The no-fabricated-EncodedInfo constraint is retained;
no such exception is assumed authorized. Under the latest direction, record this
as an unimplemented SDK-product path and move on without patching libwebrtc. It
also does not solve installed Query/ACM failure channels, physical capture refusal
or per-report APM validity.

These original outcome requirements remain unimplemented. Do not relabel missing
product APIs or exact installed consumer conflicts as native unsupported, completed
parity or qualification-only work. Qualify and deliver the feasible implementations
with these explicit exceptions; no new checks/rebuild/commit had run at reconciliation.

## Final verification matrix

### Native build / packaging identity

1. Freeze the completed implementation candidate.
2. Rebuild changed bridge/adapters and the required pinned SDK profile(s), without
   SDK/dependency runtime patches. Produce matching core and explicitly native
   device artifacts only then.
3. Refresh generated CXX/header/source, adapter and configuration identities in
   build_support/manifest.rs, native manifests and support fixtures.
4. Run provenance/inventory checks and Rust native-feature compile/language checks.
5. Keep each tested candidate unchanged until its verifier run completes; repair
   failures only after completion, then invalidate affected evidence and rerun.

### Actual processing and builder outcomes

- Stock/caller capture effects on actual native int16 and normalized float PCM;
  full-band FloatS16 input/output in each callback, mono/stereo and format changes.
- Analyzer timing before GC2/post processing, render preprocessing before queues
  and splitting, exact initialization metadata, native custom runtime payloads.
- Finite extreme capture output saturation, nonfinite output rejection, active
  render filter overflow cases and source-derived safe admission boundaries.
- Native negative/warning best-effort effects, initialized prefixes/tails and
  absent/muted frames. Failure scopes must not poison later successful calls.
- Selected echo/neural providers affect real native data/metrics and preserve
  optional/null creation outcomes. Test callable value operations where needed.
- ToString/runtime/default behavior and all applicable builder members are mapped;
  configuration echoes alone cannot qualify DSP or installation.

### Failures, ownership and installed paths

- Rejection, panic, malformed output, reentry, concurrent callback isolation and
  destructor failures at actually reached native consumers.
- Real factory/peer/ProductionSession selection uses owning selected products,
  never substitutes stock/default products or a hidden registry.
- Outer owner retirement, queued work/frames, codec removal/replacement, callback
  dependencies, thread migration and final native destruction.
- Native void callback limitations stay observable and never publish fake success.
- Controlled execution never creates hidden worker/device progress or falls back
  silently to production resources.

### Devices

- Explicit native opt-in only; headless core construction stays device-free.
- Native device enumeration/selection, initialized capture/playout callback spans,
  supported operations and genuine unsupported/unavailable/error distinctions.
- Do not use absent hardware as proof of successful physical capture/playout.
  Record tested backend/environment and any physical-hardware evidence gap.

### Adaptation and regressions

- Remaining stock encoder controls and ANA enabled-controller numeric, smoothing,
  ordering, reset/replacement/history and durable clock-domain boundaries.
- Preserve exact environment/dependency identity and selected caller codec frames.
- Library and focused audio/NetEq/codec/mixer/APM/session integration suites, then
  the applicable full integration gates; classify unrelated baseline failures.
- Existing dump tests: mono capture overload order and swapped int16 render args
  are pinned SDK behavior, not fixture expectations to normalize away.
- Protobuf-off native false and FILE ownership; asserting SDK profiles for guards
  currently supported by source evidence only.

## Completion rule

The implementation pass and the final qualification pass are separate. Missing
members, unqualified numeric ranges, unexecuted installation or hardware paths,
failing gates and historical receipts cannot certify the approved whole goal.
Update this file with exact final commands/results and remaining blockers; do not
mark the whole slice complete until applicable implementation and verification
obligations are satisfied.

## Consolidated qualification checkpoint

This checkpoint supersedes source-only wording in the historical batch ledger;
receipts below qualify their recorded candidate, not subsequent source repairs.

- Core adapter/SDK archive rebuild including the explicit `api/audio:aec3_factory`
  root passed: verifier `d0e0ccf4`. SDK source diff remained pristine.
- Linux native-only adapter compilation passed: verifier `36e67edf`.
- Native-feature Rust all-targets check passed: verifier `18715881`.
- Linked library plus every `audio*`, `neteq*` and `production_audio` target passed:
  verifier `fd0e5509`, **292 passed, 1 ignored**, 32 targets. This includes real
  ANA DTX/bitrate/V2/history effects, custom processor/analyzer failure publication,
  caller echo-factory selection, scoped caller-mixer inputs and GC2 boundary history.
- Full serial all-targets run `9bae7749` remains red in 12 non-audio targets,
  comprising 20 failed cases. All 20 reproduce from archived source `a9f2579`
  against the same freshly rebuilt SDK (`dcc3db1b`); no current-only failed case.
  The old PCMU redundant-duration fixture also fails at baseline and was corrected
  from the selected native override, not by changing provider answers.
- Core package/CXX provenance passed `d0c80625`. Python discovery `d7e97788`
  exposed a stale hardcoded iOS translation-unit count and missing local archive.
  The archive is now produced; the count fixture now checks the complete source
  inventory exactly once. Rust-only Git consumer validation additionally requires
  all newly added source/header files to be staged before its tracked-file copy.
- Full Linux native SDK compilation/export reached Rust link smoke in `dab5987b`.
  It exposed the SDK's declaration-only `AudioDeviceModule::Create`. Source repair
  now reports `UnlinkedStaticCreate` (and an explicit factory error), without a
  helper substitution or fake native null. The implemented free factory remains
  available, including its explicit bypass argument. Both archives and native
  runtime fixtures must be rerun after this last native-source repair.

Retained explicit limits: wider GC2 initial gains remain unqualified; strict
installed custom APM/mixer publication and caller-codec return-shape conflicts
remain as documented, without SDK patches or fabricated successful output.
Physical capture/playout, protobuf-off and asserting-profile execution are not
established by the primary Linux release-profile receipts or by an explicit SDK
Dummy backend. Record any final environment limitations rather than claiming it.

## Final no-patch delivery checkpoint

The feasible implementation batch is delivered with the exceptions above, not a
claim that every original parity or qualification obligation is complete.
No libwebrtc source/runtime patch was applied.

### Matching artifacts and consumers

- Corrected core SDK/adapter rebuild: `baa1c4e4`, including an SDK HEAD source diff
  check, passed. Full Linux native SDK/export, C++ and Rust link smoke:
  `d0d16a6e`, passed.
- Both packages report adapter SHA256
  `7ebcae831ad7d5ce1a74721cc6c20e42bbc6571a0702d95cdcff2a8b4d00c8ab`
  and CXX bridge-source SHA256
  `4f6237a7e5f2aca5c9c56677219c40f3f0bce4a940075cb0a7c6d48bb8370c42`.
  Checked-in core manifest and support fixtures were refreshed from the rebuilt
  candidate. Local archives are build outputs, not published release artifacts.
- Full Python discovery `d7502591`: **168 passed**, including provenance,
  iOS source inventory and Rust-only Git consumer build/runtime/negative identity
  checks. Commands: `python3 -m unittest discover -s tests -p 'test_*.py'`, after
  packaging the matching core archive at `dist/webrtc-core-linux-x86_64.tar.gz`.
- rust-analyzer diagnostics: zero across `src/audio_device.rs`,
  `src/audio_processor_custom.rs` and `src/audio_processor_neural.rs`.

### Runtime results

- `7f987662`: linked core library, every `tests/audio*.rs`, `tests/neteq*.rs` and
  `tests/production_audio.rs`, serial execution, **296 passed, 1 ignored**.
  Command: select `--lib --test production_audio` plus each audio/NetEq test target,
  then `cargo test --locked --offline ... -- --test-threads=1` with
  `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR=.work/package/webrtc-core-linux-x86_64`.
- `556a777e`: Linux native-feature linked device, mixer-provider, custom-processing,
  detector, neural-estimator and production-audio tests: **13 passed, 1 ignored**.
  Use the pinned SDK clang linker and Debian Bullseye sysroot, matching the
  `Justfile` `_rust-smoke` linker flags. The host lacks unversioned desktop-library
  development links; the initial host-link attempt was not a runtime failure.
- `a4570819`: explicit live Pulse backend creation, Init, actual active-layer
  query, enumeration and Terminate passed: **2 playout and 3 recording devices**.
  Run the ignored `live_pulse_metadata_uses_the_selected_backend_without_starting_audio`
  test explicitly with `--exact --ignored --nocapture --test-threads=1` and native
  artifact/linker settings. Recording and playout were deliberately not started.
- Selected shared detector tests preserve arbitrary optional metrics, exact shared
  lifetime through two builders, failed capture publication and the SDK's queued
  report error/cache behavior. Metrics execute during capture, not stats reads.
- Stock AEC3 really invokes the selected caller neural estimator on 40 paired
  reverse/capture frames; AdjustConfig, Reset, Estimate, construction rejection
  and final destruction are observed. Initial fixture status/timing mistakes were
  repaired before the passing receipts, without altering native behavior.

### Baseline failures and remaining limits

The serial full-suite failure comparison remains `9bae7749` versus rebuilt
`a9f2579` baseline `dcc3db1b`: **20 current failures across 12 non-audio targets,
all reproduced at baseline; zero current-only failures**. Retained logs:
`/tmp/pi-verifier-9aDPDT/output.log` and
`/tmp/pi-verifier-pSUJRX/output.log`. Parallel-only data-channel/session failures
vanish serially. Do not suppress these tests or describe the full suite as green.

The declaration-only static ADM Create is an explicit binding error, not native
null or an implicit helper fallback. The implemented free factory remains usable
with its bypass argument unchanged. Caller encoder SDK EncodedInfo/ACM return
shapes and strict installed custom APM/mixer/physical publication lack the required
existing-interface failure channel. They remain explicit implementation exceptions,
not fake successes or claims that the native feature itself is unsupported.
Wider GC2/render bounds, physical streaming, exhaustive device/installation and
concurrent/shared-provider outcomes, protobuf-off and asserting-profile execution
remain unqualified. These are not established by the primary-profile results.

## Rebase onto main 5d8dee6

The audio commits were replayed onto main, preserving its connectivity/media
APIs, native sender identity leases and deferred cleanup. PCM sinks join that
cleanup handoff. The original branch is retained at
`backup/audio-before-main-rebase-54d725a`. The duplicate fixture-fix commit was
already present on main and was skipped.

Main's per-track `AudioProcessor` / `NativeAudioProcessor` is distinct from the
standalone APM. The latter is now `AudioProcessingModule` /
`NativeAudioProcessingModule`, including builders, dependencies and generated
CXX declarations. Matching provenance pins, the support fixture and the checked-in
core artifact manifest were regenerated from the combined source, not substituted
from either parent.

Post-rebase receipts:

- Core SDK/bridge/export `311b6822` and native SDK/bridge/export `5c3569a2` pass
  against the unchanged pinned SDK source.
- Python/package/Rust-only consumers `26191801`: 182 tests pass. iOS bridge and
  Linux ASAN/H264 recipe checks `4927adca` also pass.
- Native all-targets check and selected device, mixer, custom/detector/neural,
  production audio, media-reference and production-cleanup runtimes `c65bdc1a`
  pass (16 passed, 1 ignored). No physical streaming qualification is implied.
- Initial full core suite `5efbf0e6`: 633 passed, 4 failed, 3 ignored. The NetEq
  conflict resolution had selected PCMU silence while retaining a nonzero-output
  assertion. Restoring the nonzero audio fixture passes all 8 NetEq tests in
  `d8f76e2b`, without changing production code. Final full core run `87d11793`
  has 634 passed, 3 failed and 3 ignored, with only `local_network_permission`
  failing. Its source hygiene, ancestry, SDK-pristine, formatting and both
  artifact-provenance checks pass.
- The other three failures are local-network permission STUN callback timeouts.
  Fresh main `5d8dee6` baseline `6a45fe0a` reproduces all three at the same
  `tests/local_network_permission.rs:152` assertion, after provenance validation
  and the C++ smoke check pass. Main's bridge/archive was rebuilt against the
  same pristine SDK. Its old local package was stale; the cross-worktree producer
  also required loading the actual SDK compiler helper from the audio worktree
  (byte-identical to main's helper), not pretending its path had changed. Main's
  adapter/CXX identities and real archive checksum remain independently validated.
  No main or SDK tracked source was modified. Baseline log:
  `/tmp/pi-verifier-vWivEq/output.log`; merged full-suite log:
  `/tmp/pi-verifier-cegRzh/output.log`. These three reproduced baseline failures
  remain explicit and unsuppressed; the full suite is not claimed green.
- Rust-analyzer reports no diagnostics for the merged library, standalone APM,
  peer factory and deferred cleanup sources.

The earlier 20-failure comparison above is historical evidence for the pre-rebase
branch and its old baseline, not the result of this merged branch.
