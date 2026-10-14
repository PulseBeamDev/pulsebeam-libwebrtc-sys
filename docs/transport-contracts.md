# Owning native transport access

The source-wired low-level path is RTP sender/receiver `dtls_transport()`, peer
`sctp_transport()`, SCTP `dtls_transport()`, and DTLS `ice_transport()`. An absent
native reference remains `None`. Handles retain their peer/factory context and
are sequence-bound, non-Send/non-Sync; this is not yet the production actor
transport interface. Compilation/runtime qualification is pending.

## Snapshots and registrations

DTLS and SCTP information are owned native copies, not refreshed getters or
setter echoes. Optional values preserve native absence versus zero. DTLS copies
retain the actual certificate chain and export copied certificate members; an
absent chain differs from a present empty chain. SCTP copies retain the snapshot's
actual DTLS reference and peer origin. A snapshot's DTLS association can therefore
differ from the current SCTP transport association after teardown.

Per-peer weak handle maps deduplicate aliases by retained underlying interface
identity. One live Rust handle state owns one native observer registration,
whether obtained through a sender, receiver, SCTP handle or SCTP snapshot.
Callbacks enqueue their actual by-value snapshots/errors. They do not execute
Rust user code, read application clocks, pump tasks or fabricate timestamps.
Registration requires positive capacity and rejects an already registered alias.
A new registration clears the old queue and overflow counter; unregistration
stops future callbacks but preserves already queued events. Full queues evict
the oldest event and report a saturating dropped count, reset on reading.

Native register/unregister operations run on the actual retained network thread.
Final root destruction unregisters before beginning observer-object destruction,
including its vtable teardown. Native handles and their registration state drop
before the retained Rust peer/factory context and native threads.

## ICE internal revocation

The pinned `api/ice_transport_interface.h` comment describes signaling-thread
use of its borrowed internal pointer. For the actual standard PeerConnection
path, `pc/ice_transport.{h,cc}` capture the construction thread and assert that
thread in `internal()` and `Clear()`. JSEP/DTLS construct, consume and clear these
objects on the network thread. This concrete path, not a guessed signaling
executor, governs these bindings.

Retaining `IceTransportInterface` retains only the public facade. Native
`Clear()` revokes its internal pointer before destroying the internal object.
`IceTransport::internal()` returns a retained access facade, not a durable native
internal handle. Every facade operation runs on the actual network thread,
reacquires `internal()`, uses it within that same task, and reports
`IceInternalUnavailable` after revocation. It never stores, exports or retains a
raw pointer across tasks. A successful `internal()` getter is not a promise that
a later operation remains available.

The currently source-wired internal observations are native state, JS-style
transport state, component, role, gathering state and optional RTT. This is not
complete ICE-internal parity. Configuration, candidate mutation/stats/pairs,
callbacks and inherited packet-transport operations remain ledger obligations.
Information default/state/full/deprecated constructors and native copy/move
assignment are now source-wired, along with native SCTP stringification. DTLS
`remote_ssl_certificates()` clones the actual native chain into the existing TLS
certificate/chain API; `remote_certificate_data()` is explicitly just an export
helper. Production actor transport installation remains incomplete.

## Peer ICE observations and byte candidate I/O

Local peers and ProductionSession now expose actual signaling-thread snapshots
of legacy ICE state, standardized ICE state and optional can-trickle metadata.
`None` is distinct from false, and native states are distinct from the overall
peer-connection state. Closed peer handles retain native Closed observations;
actor shutdown retains its separate closed-session admission rule.

Legacy/standardized ICE callbacks enqueue distinct callback-time values through
the existing bounded control queue. They are not reconstructed from a later
getter. They share the 128-control/256-KiB bound and overflow accounting, not an
expanded advisory snapshot budget. Native candidate arrival/removal callbacks
also enqueue exact byte copies and retain native constructor normalization of
m-line indices. Text events are checked projections; non-UTF-8 values use the
corresponding byte variants rather than fatal CXX String conversion.

`IceCandidateBytes` feeds the actual native parser through length-delimited
slices on async add, synchronous add and removal. Synchronous byte errors retain
native parser explanation bytes; legacy text projection can fail distinctly.
Native false remains a rejection, not an invented error. Async accepted
operations use the existing exactly-one terminal/admission-retirement path,
including byte errors and migrated actor forwarding. Native parsing may
normalize values and reject malformed input; no setter echo is promised.

Source regressions in tests/peer_ice.rs cover actual offer/remote trickle state,
closed state, checked projection, native byte-bearing parser errors and migrated
actor retirement. Private observer-unit fixtures cover bounded callback-time
states and raw candidate arrival/removal serialization; they are not wire
transport evidence. All are unrun. Complete owning JSEP/candidate value resources
and remaining ICE-internal/actor transport operations are still obligations.

## Native error values

DTLS error events own native RTCError copies rather than flattening kinds/details
into a peer error or storing an echoed DTO. `RtcError` supplies typed open enums,
actual native constructors/named factories, copy/move assignment, field mutation,
StringBuilder text/byte append and rvalue byte streaming. A lifetime-bound
exclusive builder facade delegates native integer/floating insertion, str byte
view, size, Clear and Release. The standalone owned `StringBuilder` adds actual
native default/string-view/copy construction and copy assignment; the error
facade can copy to that owned value or assign from it. RTCError's consuming
scalar/byte/boolean operations invoke the SDK rvalue streaming operator; native
long conversion failures return the unchanged owner rather than truncating it. Byte views cannot outlive or coexist with mutation
of their owner. The actual const-char* message getter terminates at NUL; native
stringification reads the full builder. Constructors/factories/setters accept
arbitrary bytes as well as text. Lossless byte getters preserve non-UTF-8 output;
text conveniences are explicitly UTF-8-lossy. Only native long representability
is checked at platform-dependent narrowing; supported artifact int/unsigned
widths are compile-time asserted rather than silently narrowed.
`api/rtc_error.cc` ToString directly indexes name arrays without checking enum
bounds. Unknown discriminants remain constructible/stored/observable, but their
unsafe type/detail name conversion returns None before entering that function.
Full error stringification checks only the type it actually consumes, not an
unrelated detail. No arbitrary validation is added to harmless constructors.
The variadic native AppendFormat method remains an explicit incomplete parity
obligation. Its pinned implementation computes predicted_length + 1 in signed
int and adds predicted lengths to size_t without adequate checks; a faithful safe
format/argument bridge is not established. The constrained HasAbslStringify
insertion template also needs explicit reachable-native-type inventory. No
unsafe opt-in, SDK patch, Rust printf emulation or success placeholder is added.
