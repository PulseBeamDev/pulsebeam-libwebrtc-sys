# Audio codecs, adaptation, standalone NetEq and processing

Status: APPROVED

## Motivation

Deliver one independently accepted area of the native WebRTC application surface
without waiting for another plan or repeating completed baseline foundations.

## Outcome

Applications use and install owning stock/caller audio codec products, safely enable supported stock adaptation, drive standalone NetEq and select actual audio processing or explicitly opted-in Linux audio devices.

## Behavior

### Use and install owning caller audio codec products

- Expose the applicable pinned factory/product/frame contracts, exact formats/contexts/options/pair identity, Query/Create overloads and native optional/null/error results. Retain existing standalone products and selected passing evidence rather than recreate them.
- Standalone factory/product/frame outcomes remain independent; mutable Query observations, unique product handoff, decoder metadata/reset/packet dispatch, retained parse frames, single-shot decode state and exact initialized buffer spans remain truthful.
- Installed voice-engine discovery/PT/RED/bitrate and repeated Query consumption safely handle current answers, rejection, panic and reentry. Do not replace caller answers with cached success, fabricated supported info, arbitrary whitelists or stock providers.
- An installed caller encoder consumes supported native input and returns real encoded payload/metadata. Nonfallible Encode growth/sample checks, ACM buffering and packetization cannot turn callback Err/panic/malformed output into zero-output successful encoding.
- An installed modern caller decoder preserves stable sample/channel semantics, parse-null/priority/frame ownership, duration/capacity arithmetic, PLC and native speech classifications. Actual generic receive consumers and expected internal CNG/legacy compatibility are safe.
- Private legacy compatibility refusal may return a negative result without touching PCM/speech buffers and with observable failure. It is not genuine legacy DSP/CNG support; do not expose unsafe legacy decoder APIs or patch the SDK to claim compatibility.
- Factory, product, frame, environment and Rust-side dependency identities survive outer-owner retirement, codec removal with queued frames, reentry/concurrency, actor migration and final native destruction.
- Rejection and callback failures remain observable before unsafe native consumption, and distinguish failure from native null/unsupported or legitimate buffering. A standalone wrapper is not a successful installed path.

### Use safely admitted stock encoder adaptation

- Controls and admitted ANA configurations affect actual stock encoder/controller behavior, including configuration replacement/reset/history and native metric ordering.
- Admission covers reached construction, arithmetic, writer, smoothing and clock/FP behavior for every enabled controller path.
- Preserve actual environment identity and durable clock bounds; keep ANA millisecond and packet-loss microsecond semantics distinct.
- Protobuf-disabled or inherited unsupported controls report the real limitation without entering native assertion paths. No blanket fake preflight or synthesized adaptation is success.

### Drive an owning standalone NetEq

- Preserve exact configuration/environment/optional decoder-factory choices, lazy registration and independent factory/product outcomes.
- Expose applicable standalone packet/time/output/frame/statistics/delay/NACK operations and codec removal semantics.
- Respect native initialized-prefix/mute state and bounded decode capacities; no output preinitialization/copy-back to invent validity.
- Queued decoded frames and dependent codec/factory resources survive outer retirement; serialized Send-only caller callbacks have safe failure/reentry behavior.
- Optional decoder-factory selection uses available baseline factory/product contracts. Any admission gap reached by standalone NetEq's decoder consumers belongs to this plan, together with its peer caller-codec contract.

### Use audio processing and opt-in audio devices

- Expose applicable native software processing/provider and audio-device factory/product/operation contracts with truthful creation inputs.
- Software processing remains usable in headless core without activating physical devices. Device backends require explicit native opt-in.
- Installed components affect real audio with correct initialized/muted frames, layout and lifetime; actor/session selection is effective.
- Hardware unavailable, unsupported backend and caller rejection remain distinct; controlled mode never silently activates a production device.

## Public contract scope

The SDK header roots below are relative to the pinned source checkout, not another
planning document. The operation boundaries define the complete public scope.

### Use and install owning caller audio codec products

- `AudioEncoderFactory`, `AudioDecoderFactory`, `AudioEncoder`, `AudioDecoder` and `AudioDecoder::EncodedAudioFrame`, in `api/audio_codecs/audio_encoder_factory.h`, `api/audio_codecs/audio_decoder_factory.h`, `api/audio_codecs/audio_encoder.h`, `api/audio_codecs/audio_decoder.h`; options/pair/format/info/frame/buffer values include `api/audio_codecs/audio_codec_pair_id.h` and `api/audio/audio_frame.h`.

All public factory overloads and encoder/frame/modern decoder operations are accounted for, with the explicitly excluded unsafe legacy decoder family below. Stock encoder controls and the supported ANA enablement contract below are included in this plan. Generic installed decoder/CNG consumers are safety proof boundaries, not a custom peer NetEq API.

### Use safely admitted stock encoder adaptation

- Stock `AudioEncoder` controls, ANA enable/disable/statistics and dependent public values in `api/audio_codecs/audio_encoder.h`, using the pinned stock Opus implementation.

Controls include FEC/DTX/application/playback/target bitrate, reset, packet-loss/bitrate/bandwidth/allocation/RTT/overhead notifications and frame/bitrate ranges as supported by the pin. Internal ANA controllers are consumption proof boundaries, not new public controller APIs. The inherited aborting recoverable-loss operation stays genuinely unsupported.

### Drive an owning standalone NetEq

- `NetEq`/configuration/statistics/packet/output APIs in `api/neteq/neteq.h`, owning `NetEqFactory` creation in `api/neteq/neteq_factory.h`, selected `AudioDecoderFactory` and initialized `AudioFrame` values.

All public standalone NetEq factory/product operations and required packet/configuration/decoder handoff/metadata values are included. Controller algorithms and private NetEq internals are not extra public obligations; peer/actor registration remains excluded.

### Use audio processing and opt-in audio devices

- `AudioProcessing` and public builders/configuration/callback processors in `modules/audio_processing/include/audio_processing.h`, `api/audio/builtin_audio_processing_builder.h`; `AudioMixer` in `api/audio/audio_mixer.h`.
- `AudioDeviceModule` and supported creation contracts in `modules/audio_device/include/audio_device.h`, `api/audio/create_audio_device_module.h`; required initialized audio frame values.

Applicable public software processing/device/mixer/processor creation, configuration, operation and callback members are covered. Device activation remains explicitly native opt-in; private DSP algorithms and other platforms are excluded.

Every public member in these operation boundaries and its transitive owning
inputs/outputs/callbacks is accounted for, including applicable experimental or
deprecated overloads. Each is available from the baseline or delivered here with
native outcome evidence or an explicit limitation permitted by this contract.
Missing members and configuration echoes cannot pass completeness. This does not
require a separate test for each member or expose private unrelated SDK internals.

## Invariants

- Owning typed public APIs preserve valid native-supported input, legitimate defaults, optional values, open protocol tokens and native errors. CXX/raw pointers/unsafe adapters stay private; generic configuration bags are not parity.
- No fabricated success, payload/metadata synthesis, silent normalization, guessed identity join, hidden registry or stock substitution. Native packetization, transport, feedback, reassembly and codec behavior remain engine-owned.
- Protect actually reached native abort/UB and Rust memory hazards. Do not expand speculative invalid numeric domains into unrelated obligations. Unsupported is acceptable only for a genuinely unsupported pinned native operation, not missing binding work.
- Retain every relevant native and Rust dependency through asynchronous callbacks and final destruction. Applicable public builder/endpoint and ProductionSession paths are part of this outcome, not a later sibling integration task.
- ProductionSession remains move-only Send, non-Sync, single mutable-owner and usable after caller-thread migration. Controlled execution, when relevant, remains explicitly caller-owned and sequence-bound without hidden progress or production fallback.

## Constraints and baseline

Source checkpoint: `52b4d80d9f5ebe18064515c952ec8e6382d789a0`. Plan-only
commit `042579b` adds no feature implementation. Native SDK pin:
`ba469aa2093ba950066258ca0a59a6fbd1295582`. Existing public values, owning wrappers,
provider scaffolding, media APIs and controlled-world infrastructure are inputs,
not work to restart. Existing qualification covers only its named candidates and
behaviors; source presence is not installed-runtime proof.

Coverage includes the named application-facing public members and transitive
contracts needed to construct, implement, use and observe them, including applicable
experimental/deprecated APIs. Private/test-only internals and unrelated native
algorithms are excluded.

Scope is Linux x86_64: headless core and explicitly opted-in native device
behavior where relevant. Preserve existing packaging contracts. No new SDK or
dependency runtime patch, substitute engine or public unsafe escape hatch.
Public APIs may change when necessary for a simple, owning, typed contract.

This Markdown contains the complete slice contract. No other slice or shared
planning document is required. All required changes and safety/installation proof
belong to this outcome or use available frozen-baseline capabilities. Completion
and verification cannot require a sibling's changes. Final cross-slice composition
is separate and does not waive this slice's own actual-consumer evidence.

A changed native/bridge/configuration candidate requires a matching artifact with
known SDK/compiler/FP/GN/protobuf/source-manifest identity. Do not reuse or relabel
a historical archive as proof of changed native code.

## Non-goals

- Custom NetEq installation into peers/ProductionSession, public unsafe legacy Decode/DecodeRedundant/DecodePlc and PLC-query families, fake legacy DSP/CNG or synthetic output.
- Generalized compressed-video/source profiles and new execution/network/security providers; available baseline execution and peer paths remain usable.
- Other-platform expansion, downstream migration/simulator integration, real-SFU interoperability, publication and patent/license clearance.
- Internal architecture, files, worktrees, fixtures and routine case enumeration are builder choices, not semantic requirements.

## Verification

Evidence must observe stable public behavior/state and reject plausible incorrect
implementations. Reuse existing faithful evidence where applicable; choose the
narrowest mechanism that proves the risk. A real native/continuous lifecycle
boundary is required only where isolated values or mocks cannot faithfully show it.

### Use and install owning caller audio codec products

| Material risk | Sufficient observable evidence |
| --- | --- |
| Standalone-only parity or substituted factory selection | Observe public standalone dispatch and an actual negotiated call whose selected caller factory supplies the installed encoder/decoder, with exact contexts, metadata and buffer effects. |
| Unchecked installed Query or nonfallible Encode failures | Exercise repeated current Query outcomes and real installed callback rejection/panic/malformed output at native consumers; observation must distinguish true failure from supported discovery and legitimate buffering without unsafe consumption. |
| Unsafe generic decoding or invented CNG compatibility | Observe real modern decoded frames with exact initialized prefixes/tails and native speech classifications, plus malformed/capacity/failure/queued-removal and actual internal compatibility behavior. |
| Frame/resource receipt corruption | Observe retained frames/products after outer retirement, unique/shared handoff, reentry/concurrency isolation and migrated final release. |

### Use safely admitted stock encoder adaptation

| Material risk | Sufficient observable evidence |
| --- | --- |
| Arithmetic/admission errors or configuration echo | Observe actual adaptation under supported native inputs with operation-local boundary/configuration/replacement cases using matching compiler/FP/protobuf configurations. |
| Clock/ownership drift | Exercise actual clock-domain/time transitions and retained encoder/controller dependencies across replacement and teardown. |

### Drive an owning standalone NetEq

| Material risk | Sufficient observable evidence |
| --- | --- |
| Synthetic output or unsafe decoder/frame consumption | Actual packets generate real standalone audio/metadata through public APIs; malformed/unsupported input and callback failures cannot write outside admitted spans. |
| Incorrect handoff/removal/lifetimes | Remove codecs with queued frames, retain products after factory retirement and observe exact failure, concurrency and destruction behavior. |

### Use audio processing and opt-in audio devices

| Material risk | Sufficient observable evidence |
| --- | --- |
| Selected processing ignored or fake device capability | Observe deterministic actual audio-path processing and configuration effects; inspect native device discovery/selection or truthful unavailable outcomes. |
| Borrowed frame and lifecycle errors | Exercise supported format boundaries, callback failures, retained products and actor migration/retirement. |

Applicable buffer/callback/destruction hazards require matching-runtime safety
evidence. Standalone values, setter echoes, unexecuted fixtures and launch
receipts cannot certify actual installation. Hardware absence is distinct from
binding absence and cannot certify physical capture/device execution.

## Risks

- Pinned nonfallible Query/Encode and internal decoder/CNG consumers may lack a faithful error path. If the required caller behavior cannot be safely admitted under the pin and no-SDK-patch constraint, report that exact incompatibility rather than narrow the supported behavior or declare this slice complete.

## Deferred

Unrelated capabilities and final integrated composition. A later material change
to this contract requires renewed human approval.
