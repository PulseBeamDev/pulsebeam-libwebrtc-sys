# Native media, video codecs and controlled replay

Status: APPROVED

## Motivation

Deliver one independently accepted area of the native WebRTC application surface
without waiting for another plan or repeating completed baseline foundations.

## Outcome

Applications exchange raw PCM/video and concurrent caller-encoded native media, select video products, transform/protect frames, replace individual sources and deterministically replay the supported workflows, with sender-admission and lifecycle safety included.

## Behavior

### Send and receive owning PCM audio

- Expose applicable source/sink/frame options, supported rates/layouts, timing and native processing handoff without imposing unrelated format policy.
- Honor native initialized-prefix and mute behavior; preserve real contents, media metadata and legitimate native rejection.
- Source/sink replacement, queued callbacks, streams/tracks and migration retain their dependent buffers and native resources.

### Send and receive owning raw video

- Expose applicable source/sink/frame/buffer metadata and supported formats with truthful geometry, timing and capture/content information.
- Reject genuinely unsafe framing/consumption before native access without silent repair or arbitrary valid-format restrictions.
- Use the baseline supported sender-configuration contract. This outcome does not require new general allocation/profile APIs; any safety gap reached by its own raw workflow belongs to this slice.
- Local and actor/session paths preserve source, buffer and sink ownership across replacement, callbacks and migration.

### Publish, replace and replay independently identified native media

- Expose applicable pinned owning sender RTP parameters/transceiver initialization, sync/async installation, full-value video factories/selectors/products and encoded-source operations through local and ProductionSession paths. Native errors, optional/default choices and effective state remain exact.
- Fresh sender transactions, actual allocator-group/default/trial interactions, multi-sender attachment/reconfiguration/rollback and publication are safe at reached native consumers. Reject invalid whole input batches before source-state mutation. For accepted native operations, report actual outcomes and effective state; do not invent all-or-nothing native rollback or atomic wire publication.
- Selected full-value video Query/Create/context/product/selector operations genuinely choose installed products. Native-null, unsupported, callback failure and current query results are not replaced with cached information or stock fallback.
- Generalize encoded audio/video to actual supported native formats, source counts and spatial/temporal modes. Preserve required H264 L1T1, L1T3/DD, three-rung L1T1, each distinct pairwise combination of those three profiles and two independent valid L1T3 temporal-fraction sources.
- Actual sender initialization recognizes its selected scalability/temporal support before input is required. Whole multi-encoding input admission occurs before source-state mutation; preserve native RID/transceiver routing and real renegotiation semantics.
- Protect source/producer/access-unit identity through fanout, codec queues, feedback, packetization, crypto, receive recreation, late packets and source retirement. Preserve caller payloads, framing, capture times, per-unit audio level/V-voicing/speech-CN metadata and meaningful video metadata. Absence is distinct from explicit zero/false.
- Required rate/feedback/keyframe callbacks reach the correct active encoder/source. Stock WebRTC owns DD/VLA generation and serialization; callback/rate observations do not fabricate receive support or passive keyframe feedback.
- Replace or retire one same-kind/cross-profile source without rebuilding the peer/factory or perturbing unrelated sources. Old queued output keeps its old identity; pending control results and retained handles stay bounded and consumable after source retirement.
- A caller owns the controlled world's thread, time, seed, queue order, packet decisions and finite progress budget. Ready/deadline/cancellation behavior has no hidden pumping, time advancement, threads, real waits or production fallback.
- Controlled execution exercises actual negotiation/gathered SDP, bidirectional prerecorded VP8/Opus and required mixed H264 sources with IPv4/IPv6 loss, reorder and duplication. Same- and fresh-process replay matches canonical meaningful decoded data, media metadata, logical times and packet decisions.
- Continuous controlled lifecycle evidence includes acquisition, actual native work, rejected setup and final queued-work destruction. Production actor migration and controlled sequence-bound ownership remain distinct.

### Transform live encoded frames

- Preserve existing stock frame ownership/cloning and required callback composition; existing standalone evidence is a foundation, not installed delivery proof.
- Honor producer/role provenance and actual packetizer/metadata/time/geometry consumption for supported mutations.
- Retirement makes late completions safe and observable. Unsupported frame construction or mutations must not become fabricated native success.
- Install through public endpoint/builder/session paths using baseline media, without requiring generalized sources.

### Protect frames with owning caller crypto

- Preserve actual input/output bounds, crypto results and native optional/error behavior through local and actor/session paths.
- Getter/alias identity refers to the installed provider after outer Rust owners or actor identifiers retire.
- Catch/observe callback failures without malformed copying or synthetic protected data. Retain resources through concurrent native calls and migrated teardown.
- SFrame exposes only the pinned implementation's real creation outcomes and enable side effects; an absent pipeline remains explicitly unsupported.

## Public contract scope

The SDK header roots below are relative to the pinned source checkout, not another
planning document. The operation boundaries define the complete public scope.

### Send and receive owning PCM audio

- Audio source/track/sink and stream interfaces in `api/media_stream_interface.h`; frame/borrowed views/layout/packet metadata in `api/audio/audio_frame.h`, `api/audio/audio_view.h`, `api/audio/channel_layout.h` and `api/rtp_packet_infos.h`.

All public operations/values of these audio source/track/sink/frame roots are covered for baseline supported PCM calls. Implementing new device/APM factories or generalized encoded producers is excluded.

### Send and receive owning raw video

- Video source/track/sink interfaces in `api/media_stream_interface.h`, `api/video/video_source_interface.h`, `api/video/video_sink_interface.h`; frame/buffer roots in `api/video/video_frame.h`, `api/video/video_frame_buffer.h` and `api/video/i420_buffer.h`.

Cover public raw-source/track/sink/frame/buffer and meaningful metadata operations for baseline-supported sender configurations. New capture implementations, general allocation APIs and compressed multi-profile producers are excluded.

### Publish, replace and replay independently identified native media

- Sender parameter-write and encoding initialization contracts in `api/rtp_sender_interface.h`, `api/rtp_parameters.h` and `api/rtp_transceiver_interface.h`.
- `VideoEncoderFactory`, `VideoDecoderFactory`, `VideoEncoder`, `VideoDecoder`, `EncoderSelectorInterface`, output callbacks and their values in `api/video_codecs/video_encoder_factory.h`, `api/video_codecs/video_decoder_factory.h`, `api/video_codecs/video_encoder.h`, `api/video_codecs/video_decoder.h`, `api/video_codecs/sdp_video_format.h`, `api/video_codecs/scalability_mode.h` and `api/video/encoded_image.h`.
- The repository's caller encoded-audio/video sources, source identities/control outcomes and controlled-world public contracts, using baseline native peer/queue/clock/packet primitives.

Video families include discovery/implementations, both QueryCodecSupport overloads, Create/context, factory and sender selectors; selector current-encoder/bitrate/resolution/broken callbacks; encoder initialization/encoding/rate/FEC/feedback/info/release/output callbacks; decoder configuration/decode overloads/info/name/release/output callbacks. Required full-value settings, codec/encoded/frame metadata and resources are included. Controlled scheduling and mixed-media behavior are defined above. Native media identity uses baseline transport protection; this plan also owns its caller frame-crypto contract below.

### Transform live encoded frames

- `FrameTransformerInterface`, transformed-frame/completion callback interfaces and supported acquisition/cloning roots in `api/frame_transformer_interface.h` and `api/frame_transformer_factory.h`.

All public transformable audio/video frame and transformer/completion members are in scope, plus actual endpoint installation. Constructors requiring unavailable native provenance and unrelated packetizer internals are not public APIs.

### Protect frames with owning caller crypto

- `FrameEncryptorInterface`, `FrameDecryptorInterface` and results in `api/crypto/frame_encryptor_interface.h`, `api/crypto/frame_decryptor_interface.h`; `api/sframe/sframe_encrypter_interface.h`, `api/sframe/sframe_decrypter_interface.h`.
- Corresponding install/get/enable operations on `RtpSenderInterface` and `RtpReceiverInterface`.

Full public crypto/SFrame interface operations and input/output/result values are included subject to explicit pinned SFrame limitations. Unrelated RTP controls are excluded.

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

- New caller audio codec factories/products, stock ANA expansion, custom peer/actor NetEq and physical capture/device implementations; available baseline stock audio/media remains usable.
- Producer-VLA overrides, guessed received VLA or simultaneous three-rung stock reception, passive keyframe substitutes, invented SFrame pipeline and cryptographic/cross-build byte-identical replay.
- Other-platform expansion, downstream migration/simulator integration, real-SFU interoperability, publication and patent/license clearance.
- Internal architecture, files, worktrees, fixtures and routine case enumeration are builder choices, not semantic requirements.

## Verification

Evidence must observe stable public behavior/state and reject plausible incorrect
implementations. Reuse existing faithful evidence where applicable; choose the
narrowest mechanism that proves the risk. A real native/continuous lifecycle
boundary is required only where isolated values or mocks cannot faithfully show it.

### Send and receive owning PCM audio

| Material risk | Sufficient observable evidence |
| --- | --- |
| Synthetic output, metadata loss or malformed frame handoff | Send distinguishable PCM on a real call and observe decoded contents plus supported layout/rate/timing through public sinks; observe malformed or unsupported input rejection. |
| Replacement/teardown corruption | Replace/reconfigure inputs and sinks with pending native work, retain aliases and migrate the session before final drop. |

### Send and receive owning raw video

| Material risk | Sufficient observable evidence |
| --- | --- |
| Only standalone frame ownership or synthetic media | Known frames traverse a real negotiated call and arrive with distinguishable contents, geometry and meaningful metadata. |
| Invalid consumption or stale frames | Observe rejected inputs, replacement, retained sink/buffer aliases and queued-work migrated teardown on the same candidate. |

### Publish, replace and replay independently identified native media

| Material risk | Sufficient observable evidence |
| --- | --- |
| Unsafe sender allocation or false installation | Observe successful and rejected multi-sender initialization, fresh parameter transactions, attachment/reconfiguration/rollback and native product selection on the real supported compiler/trial/runtime configuration. Admission must protect reached arithmetic and publication, not depend on statistics or last-request state as a barrier. |
| Public video product behavior missing despite selected-factory identity | Observe standalone public Query/Create overloads, exact formats/contexts/options and the encoder/decoder/selector families named in Public contract scope through real native dispatch. Actual encode/decode outputs and product callbacks must identify the selected product; factory identity alone cannot satisfy this risk. |
| Cross-source payload/metadata/feedback confusion | Use distinguishable concurrent sources and the required H264 combinations through actual native packetization, protection and reassembly. Observe initialization before input, native DD/VLA/rate/keyframe effects, meaningful received payload/metadata and source identity before and after individual replacement. |
| Unbounded or invalid retirement | Observe full/closed synchronous rejection, exactly-one accepted terminal outcomes, retained aliases, old queued output, replacement/recreation and migrated close with pending native work. |
| Hidden controlled progress or nondeterministic substitute peers | Observe real bidirectional media under caller-selected impairments and finite pump budgets, with same/fresh-process canonical replay and continuous no-thread/process/socket lifecycle evidence. Input/output distinguishability must reject synthetic media and substitute peer implementations. |

### Transform live encoded frames

| Material risk | Sufficient observable evidence |
| --- | --- |
| Standalone clones passing without live delivery | Observe actual transformed bytes/metadata delivered through sender and receiver native paths on a real baseline call. |
| Unsafe mutations or late callbacks | Exercise supported mutation boundaries, unregister/late completion, retained aliases, caller migration and final drop. |

### Protect frames with owning caller crypto

| Material risk | Sufficient observable evidence |
| --- | --- |
| Only constructed crypto values or synthetic protection | Exchange actual protected/transformed frame data through installed crypto and observe public getters and received results. |
| Malformed copying or invalid provider lifetime | Reject malformed callback output and exercise failure, negotiation removal, concurrency, aliases and migrated final drop. |

Applicable buffer/callback/destruction hazards require matching-runtime safety
evidence. Standalone values, setter echoes, unexecuted fixtures and launch
receipts cannot certify actual installation. Hardware absence is distinct from
binding absence and cannot certify physical capture/device execution.

## Risks

- The checkpoint does not certify group-level sender allocator admission. A faithful safe installation witness is required within this outcome; a future sibling implementation, invented reservation state or relabeled source-only proof cannot satisfy it.

## Deferred

Unrelated capabilities and final integrated composition. A later material change
to this contract requires renewed human approval.
