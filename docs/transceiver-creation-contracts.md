# Native transceiver creation

The pinned SDK exposes four `PeerConnectionInterface::AddTransceiver`
overloads in `api/peer_connection_interface.h`. Rust maps their behavior to:

| Native overload | Rust operation | Installed consumer |
|---|---|---|
| `AddTransceiver(MediaType)` | `PeerConnection::add_transceiver(kind, &RtpTransceiverInit::default())` | Actual peer, without a sending track |
| `AddTransceiver(MediaType, const RtpTransceiverInit&)` | `PeerConnection::add_transceiver` | Actual peer, without a sending track |
| `AddTransceiver(scoped_refptr<MediaStreamTrackInterface>)` | Audio/video `add_*_transceiver_with_init` with default init | Actual peer and retained local track |
| `AddTransceiver(scoped_refptr<MediaStreamTrackInterface>, const RtpTransceiverInit&)` | `add_audio_transceiver_with_init`, `add_video_transceiver_with_init` | Actual peer and retained local track |

These operations are available in Linux x86_64 core and native artifacts.
The track-taking bindings currently accept local typed audio/video tracks;
there is no full-init overload for an arbitrary owning `MediaTrack` reference.
`ProductionSession::{add_transceiver,publish_audio_with_init,publish_video_with_init}`
provide the same operations with session-owned identities. A trackless
transceiver does not require a source, track or dummy media input. Its sender
has no track until one is explicitly attached.

## Values and native outcomes

`RtpTransceiverInit` owns the complete native direction, stream-ID byte
strings and initial encoding values. Installation borrows this value and does
not change it, even when native validation rejects it. Returned sender values
are actual native snapshots, not copies of the requested configuration.

Native normalization is preserved. For example, this revision limits audio
to one encoding and removes a single encoding's RID. Unsupported media types,
unsupported encoding fields and invalid RID combinations return the SDK's
own error. Full-init operations distinguish `RtpSenderOperationError::Native`
with an owning `RtcError` from local `Binding(PeerError)` failures.

The binding checks owner identity, closed resources and synchronous callback
reentry. It also prevents concrete unsafe native consumption: AddTransceiver
on Plan B aborts in the SDK, unknown direction values are not safe to install,
and video resolution/framerate values must not overflow native arithmetic.
These are local failures, not fabricated RTC errors. Arbitrary standalone
metadata remains representable in `RtpTransceiverInit`.

Direct encoded-source adapters retain their explicit consumer constraints.
Current controlled media admission remains a separate limitation of the
existing execution path; these creation operations alone do not qualify
unrestricted simulated software media.

## Ownership and observation

The returned transceiver retains its peer and factory. An installed local
track is retained in the sender's ownership graph after the caller releases
its track/source handles. Session operations never export captive local
handles; the same session IDs remain valid after real-owner migration.

Creation is not negotiation completion or media delivery. Use the peer's
native offer/answer operations and completion events, inspect negotiated
direction, and attach receiver sinks for received media. A receive-only
transceiver may negotiate without a local sending track. When answering a
remote offer, inspect the transceivers actually associated with that offer.
This SDK does not reuse unassociated transceivers created by AddTransceiver
for incoming media sections; a separately pre-created sender is not an
implicit match for an incoming offer.

Focused consumer contracts live in `tests/transceiver_creation.rs`,
`tests/rtp_transceiver_init.rs` and the trackless negotiation fixture in
`tests/production_session.rs`. They cover creation, native normalization and
errors, ownership, and real negotiated directions, not deterministic codec
execution or physical device behavior.
