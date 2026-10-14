# Matching Linux artifacts, packaging and consumer tooling

Status: APPROVED

## Motivation

Deliver one independently accepted area of the native WebRTC application surface
without waiting for another plan or repeating completed baseline foundations.

## Outcome

Applications build and consume truthful core/native Linux native packages with actual advertised implementations, provenance/configuration validation, real consumer execution and useful native diagnostics.

## Behavior

### Consume matching Linux native artifacts

- Both core and native flavors have matching SDK/source/compiler/FP/GN/manifest identities and reject stale/malformed artifacts.
- Publicly constructible VP8/VP9/AV1, built-in audio, software APM, H264 and required event/RTP-extension helpers have real symbol closure and advertised behavior; non-device H264 is available in headless core.
- Protobuf on/off preserves actual capability behavior without assertions. Licenses/notices and compiler/configuration prerequisites remain available.
- Rust-only, SDK C++ and real Rust-linked companion consumption use actual runtime implementations; meaningful native diagnostics are not missing-include cascades.

## Public contract scope

The SDK header roots below are relative to the pinned source checkout, not another
planning document. The operation boundaries define the complete public scope.

### Consume matching Linux native artifacts

- Public constructible built-in audio factories in `api/audio_codecs/builtin_audio_encoder_factory.h`, `api/audio_codecs/builtin_audio_decoder_factory.h`; video factories/products in `api/video_codecs/builtin_video_encoder_factory.h`, `api/video_codecs/builtin_video_decoder_factory.h`.
- Software processing construction in `api/audio/builtin_audio_processing_builder.h`, required public event/RTP-extension helpers and the repository's SDK/bridge artifact manifest and consuming interfaces.

This outcome certifies actual advertised implementations/build artifacts and consuming boundaries, not all future public Rust feature extensions. Each matrix row is independently required for each applicable flavor/configuration; no sibling feature change is necessary.

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

- Certifying future Rust feature extensions or the final composition of other outcomes, publication, tag/history rewriting and patent/license clearance.
- Other-platform expansion or breakage of existing platform packaging contracts.
- Other-platform expansion, downstream migration/simulator integration, real-SFU interoperability, publication and patent/license clearance.
- Internal architecture, files, worktrees, fixtures and routine case enumeration are builder choices, not semantic requirements.

## Verification

Evidence must observe stable public behavior/state and reject plausible incorrect
implementations. Reuse existing faithful evidence where applicable; choose the
narrowest mechanism that proves the risk. A real native/continuous lifecycle
boundary is required only where isolated values or mocks cannot faithfully show it.

### Consume matching Linux native artifacts

| Material risk | Sufficient observable evidence |
| --- | --- |
| Relabeled stale archives, missing symbols or substituted codec execution | For each core/native configuration, observe provenance rejection and actual public compile/link/consumer behavior, including real H264 encode/decode and advertised software codec/processing functionality. |
| Flavor/configuration capabilities overstated | Record separate per-flavor outcomes for codecs, software processing, protobuf helpers and opt-in devices; absent hardware cannot waive software implementation proof. |
| Unsafe native/bridge consumption or useless tooling | Use applicable matching-runtime sanitizer evidence at real consumer boundaries and a meaningful pinned-source diagnostic sample. |

Evidence must cover every row separately in both core and native flavors; one
successful archive or H264 run cannot stand in for the rest.

| Required implementation | Headless core evidence | Native Linux evidence |
| --- | --- | --- |
| VP8 and VP9, separately | Public factory/product compile-link closure and real advertised encode/decode | Same implementation-level evidence |
| AV1 | Publicly constructible pinned implementation compile-link and advertised execution; explicit native unsupported configuration if genuinely absent | Same implementation/configuration-level evidence |
| Built-in audio codecs | Each advertised codec's discovery/create, compile-link and encode/decode | Same advertised codec-level evidence |
| Software APM/audio processing | Builder/product compile-link and actual supported processing, no device activation | Same software behavior evidence |
| Non-device H264 | Factory/product compile-link and real encode/decode, supported metadata/scalability behavior and required notices | Same non-device implementation evidence |
| Event-log/RTP-extension helpers | Public helper compile-link and execution, with protobuf enabled/disabled outcomes separately distinguished | Same helper/configuration-level evidence |
| Physical audio/camera/screen backends | Explicit core exclusion with no device side effects | Explicit opt-in, compile-link and actual available-device/provider outcomes; hardware absence remains a limitation |

The evidence identity includes SDK, source/bridge, compiler/FP, GN/protobuf,
manifest and archive checksum per flavor/configuration. Existing faithful evidence
may be reused for an unchanged matching candidate; unrelated future feature
installation and full-member API extension are not package prerequisites.

Applicable buffer/callback/destruction hazards require matching-runtime safety
evidence. Standalone values, setter echoes, unexecuted fixtures and launch
receipts cannot certify actual installation. Hardware absence is distinct from
binding absence and cannot certify physical capture/device execution.

## Deferred

Unrelated capabilities and final integrated composition. A later material change
to this contract requires renewed human approval.
