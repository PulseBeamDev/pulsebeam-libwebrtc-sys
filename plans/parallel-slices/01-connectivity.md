# Peer connectivity, data channels and RTP transports

Status: APPROVED

## Motivation

Deliver one independently accepted area of the native WebRTC application surface
without waiting for another plan or repeating completed baseline foundations.

## Outcome

Applications create and negotiate owning peers, exchange native data-channel messages, observe/control supported RTP endpoints and retain/use actual transports safely through actor migration and teardown.

## Behavior

### Negotiate and retire an owning peer

- Cover pinned peer creation/configuration/capability, offer/answer, descriptions, rollback/restart, candidate operations, native state, observations and statistics reports, including the transitive public values needed to use them.
- Preserve default versus explicit configuration, optional values, byte-bearing descriptions/errors and native operation results.
- An accepted asynchronous operation has exactly one terminal result, retained until consumption. Closed/full rejection is synchronous and does not fabricate an accepted operation; pending and unconsumed results stay bounded.
- Close/drop, resource aliases and caller-thread migration retain correct native roles and dependent resources.

### Exchange and close native data channels

- Expose applicable configuration, readiness/state, buffering, message/event and transport properties through local and actor/session paths.
- Preserve native ordered/unordered and reliability settings, binary/text distinctions, actual bytes and native send/close failures.
- Retained handles, pending notifications, close and overflow have bounded, observable behavior; caller-thread migration does not invalidate them.

### Observe and control native RTP endpoints

- Cover applicable pinned endpoint/transceiver observations, codec/header preferences, stream identities, native media types, directions/stop state and DTMF operations.
- Observations represent actual native endpoints and events; optional/receptive values and native errors are preserved.
- Aliases of one native endpoint do not accidentally duplicate observer effects. Replay/overflow are bounded and observer retirement precedes invalid native callback access.

### Inspect and operate retained native transports

- Expose the applicable pinned transport information, state, certificate chains, configuration/candidates/statistics and supported internal ICE operations.
- Optional information, certificate ownership, native errors and transport identity remain truthful.
- Revocable ICE internals are not treated as retained capabilities after native teardown. Network-affine operations and observer retirement occur on the appropriate native role.

## Public contract scope

The SDK header roots below are relative to the pinned source checkout, not another
planning document. The operation boundaries define the complete public scope.

### Negotiate and retire an owning peer

- `PeerConnectionInterface`, peer observers/description observers, `PeerConnectionFactoryInterface` configuration/creation/capability methods in `api/peer_connection_interface.h`; descriptions/candidates in `api/jsep.h` and errors in `api/rtc_error.h`.
- Peer statistics request/retained report/callback/value contracts in `api/stats/rtc_stats_report.h`, `api/stats/rtc_stats_collector_callback.h` and `api/stats/rtcstats_objects.h`.

The operation families are peer creation/configuration/capability snapshots, negotiation/description/candidate operations, state/observation/statistics and close. Media-source/device/product implementations and provider-specific operations are excluded; baseline selections remain usable.

### Exchange and close native data channels

- `DataChannelInterface`, `DataChannelObserver`, `DataBuffer`, initialization/state/configuration and transport-access contracts in `api/data_channel_interface.h`.

All public channel operations and their required message/configuration/event values are in scope; peer negotiation is a baseline input, not another feature prerequisite.

### Observe and control native RTP endpoints

- `RtpSenderInterface`, `RtpReceiverInterface`, `RtpTransceiverInterface` and observer/value types in `api/rtp_sender_interface.h`, `api/rtp_receiver_interface.h`, `api/rtp_transceiver_interface.h`, with capabilities/snapshot values in `api/rtp_parameters.h`.
- `DtmfSenderInterface` and observer/value types in `api/dtmf_sender_interface.h`, plus endpoint track/stream references in `api/media_stream_interface.h`.

In scope: endpoint/track/stream identities and observations, source/capability/parameter reads, DTMF, codec/header preference operations, transceiver direction/stop/receptive state and first-packet observers. Sender parameter writes/allocation, selector installation, crypto and transformer installation are explicitly outside this outcome.

### Inspect and operate retained native transports

- `DtlsTransportInterface`, `SctpTransportInterface`, ICE transport views/observers in `api/dtls_transport_interface.h`, `api/sctp_transport_interface.h`, `api/ice_transport_interface.h`.
- Supported app-facing configuration/candidate/stats/packet operations of `IceTransportInternal` in `p2p/base/ice_transport_internal.h`, with certificates/addresses in `rtc_base/ssl_certificate.h` and `rtc_base/socket_address.h`.

Transport values, observers and supported native operations are in scope. Private ICE algorithms, internal pointer access and unrelated P2P helpers are not API obligations.

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

- New raw/encoded source implementations, general sender-allocation/profile writes, selected codec products, caller frame crypto/transformers and provider-specific implementations. Available baseline inputs remain usable.
- Real-SFU interoperability, SCTP consumption-credit/reassembly bridges and guessed received-VLA/SSRC joins.
- Other-platform expansion, downstream migration/simulator integration, real-SFU interoperability, publication and patent/license clearance.
- Internal architecture, files, worktrees, fixtures and routine case enumeration are builder choices, not semantic requirements.

## Verification

Evidence must observe stable public behavior/state and reject plausible incorrect
implementations. Reuse existing faithful evidence where applicable; choose the
narrowest mechanism that proves the risk. A real native/continuous lifecycle
boundary is required only where isolated values or mocks cannot faithfully show it.

### Negotiate and retire an owning peer

| Material risk | Sufficient observable evidence |
| --- | --- |
| Lost or fabricated operation outcomes | Observe successful negotiation and rejected native descriptions/candidates through the public operation/result boundary, then close with accepted work and unconsumed results. Verify exactly-one results and bounded retention. |
| Configuration or lifetime drift | Observe effective native configuration and state, and move a ProductionSession between caller threads before pending work and final teardown. |

### Exchange and close native data channels

| Material risk | Sufficient observable evidence |
| --- | --- |
| Incorrect transport semantics or substituted success | Exchange distinguishable real messages between negotiated peers under supported reliability settings and observe readiness, buffering and failed sends through public boundaries. |
| Unbounded retention or use-after-close | Exercise pending notifications, aliases, backpressure/overflow and migrated close/drop; distinguish rejected work from accepted work and final native state. |

### Observe and control native RTP endpoints

| Material risk | Sufficient observable evidence |
| --- | --- |
| Only setter echoes or synthetic observations | Use baseline supported media to trigger actual source/first-packet/DTMF activity and observe endpoint state and supported control effects. |
| Observer identity/lifetime errors | Retain aliases across stop/close and actor migration; observe native identity, event multiplicity, bounded retention and safe final retirement. |

### Inspect and operate retained native transports

| Material risk | Sufficient observable evidence |
| --- | --- |
| Snapshots or guessed operations replacing live transport behavior | Negotiate baseline peers and observe actual connected transport values plus successful and rejected supported operations. |
| Stale internal pointers or alias/observer lifetime failures | Retain transport/certificate aliases across peer close and caller-thread migration, invoke post-close operations and retire observers safely. |

Applicable buffer/callback/destruction hazards require matching-runtime safety
evidence. Standalone values, setter echoes, unexecuted fixtures and launch
receipts cannot certify actual installation. Hardware absence is distinct from
binding absence and cannot certify physical capture/device execution.

## Deferred

Unrelated capabilities and final integrated composition. A later material change
to this contract requires renewed human approval.
