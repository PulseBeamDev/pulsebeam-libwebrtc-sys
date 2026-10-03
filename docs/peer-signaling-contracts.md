# Peer signaling contracts

Native authority: WebRTC revision
`ba469aa2093ba950066258ca0a59a6fbd1295582`,
`api/peer_connection_interface.h`, `api/jsep.h`, and the public description
completion interfaces. This inventory describes signaling and candidate
operations, not the complete peer, factory, RTP or transport interface.

## Operations and observation

| Native contract | Rust access | Meaningful behavior |
| --- | --- | --- |
| `CreateOffer`, `CreateAnswer` | `create_offer_with_options`, `create_answer_with_options`; default convenience methods | Native generation and validation complete through an owned operation outcome. |
| `RTCOfferAnswerOptions` | `OfferAnswerOptions` | Every native field is forwarded; see the value mapping below. |
| `RestartIce` | `restart_ice` | Marks subsequent offers for restart without creating an SDP operation. |
| `SetLocalDescription(description, callback)` | `set_local_description` | Consumes owned SDP, retaining native completion and error meaning. |
| `SetLocalDescription(callback)` | `set_local_description_auto` | Native signaling state selects, generates and applies the description. A successful outcome contains no SDP; read `descriptions()` afterward. |
| `SetRemoteDescription` | `set_remote_description` | Native application and validation complete asynchronously. |
| Callback-taking `AddIceCandidate` | `add_ice_candidate` | Preserves the native asynchronous operations-chain choice. |
| Synchronous `AddIceCandidate` | `add_ice_candidate_sync` | Does not wait behind the native SDP operations chain or create a completion ID. |
| `RemoveIceCandidate` | `remove_ice_candidate` | Preserves synchronous native acknowledgment/rejection. |
| Current/pending description getters | `descriptions()` | Copies all four slots in one signaling-thread snapshot, without exposing native borrowed pointers. |
| Effective `local_description`, `remote_description` | `PeerDescriptions::local`, `remote` | Prefer the pending slot over the current slot. |
| `ShouldFireNegotiationNeededEvent` | `should_fire_negotiation_needed` | Revalidates a queued negotiation ID against native state and outstanding operations. |
| `OnSignalingChange`, ICE gathering/connection callbacks | `PeerConnectionEvent` | Owned state observations, independent of operation completion. |
| `OnNegotiationNeededEvent` | `PeerConnectionEvent::NegotiationNeeded` | Contains the native ID for later validation. |
| ICE candidate, candidate removal, candidate error callbacks | `PeerConnectionEvent` | Own candidate/error values; removal is not a remote acknowledgment. |

Each operation also has a `ProductionSession` counterpart taking
`SessionPeerId` and owned values. Operation IDs and queued outcomes remain
associated with that peer across actor migration. The local peer and factory
handles remain sequence-bound and non-`Send`.

## Offer and answer values

| Native field | `OfferAnswerOptions` field | Default |
| --- | --- | --- |
| `offer_to_receive_video` | `receive_video: Option<u32>` | `None`, native unspecified |
| `offer_to_receive_audio` | `receive_audio: Option<u32>` | `None`, native unspecified |
| `voice_activity_detection` | `voice_activity_detection` | `true` |
| `ice_restart` | `ice_restart` | `false` |
| `use_rtp_mux` | `use_rtp_mux` | `true` |
| `raw_packetization_for_video` | `raw_packetization_for_video` | `false` |
| `num_simulcast_layers` | `simulcast_layers` | `1` |
| `use_obsolete_sctp_sdp` | `use_obsolete_sctp_sdp` | `false` |

`use_rtp_mux` is the native name for offering BUNDLE, not RTP/RTCP
multiplexing. A native answer preserves offered BUNDLE group lines even when
no MID is included in the groups. Plan B options do not replace Unified Plan
transceiver configuration. The engine determines applicability and validates
counts. Values outside native integer range fail before admission rather than
wrapping or being silently clamped.

`SessionDescription` preserves offer, provisional answer, answer and rollback
kinds and owned SDP text. An empty rollback restores stable negotiation state;
nonempty rollback SDP is rejected. `IceCandidate` owns the MID, m-line index and
candidate SDP. Parse errors carry `PeerErrorKind::Syntax`. Synchronous native
candidate rejection has no error detail and is represented by
`IceCandidateDisposition::Rejected`, not an invented reason. Acceptance does
not prove connectivity, selected-pair status, or that a removed candidate
previously existed.

## Admission, completion and execution

SDP operation admission reserves outcome ownership before native initiation.
A completion may occur before initiation returns and still remains observable.
At most one SDP operation may await its native callback per peer. All accepted
operations and unconsumed terminal outcomes share a 64-slot admission budget.
Synchronous admission exhaustion creates no queued outcome. Native completion
errors retain their error kind and message; shutdown can yield a native error
before binding cancellation. Consumed outcomes are not replayed.

Automatic local-description generation shares the same admission and shutdown
rules as explicit setters. Controlled peers use the caller-owned thread and
world. Non-media controlled factories cannot use automatic generation to
bypass the existing media-dependency restriction after tracks or transceivers
have been added. Admitted cooperative media factories retain native generation.

Public consumer examples of effective options, typed failure recovery,
automatic offer/answer application, candidate outcomes and direct ICE restart
are in `tests/peer_signaling.rs`. Actor-owned use and migration are exercised
in `tests/production_session.rs`; caller-pumped operation and media-dependency
restriction are exercised in `tests/controlled_driver.rs`.
