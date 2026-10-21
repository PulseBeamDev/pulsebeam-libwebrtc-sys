# RTP value graph, in progress

Source inventory: pinned `api/rtp_parameters.h`, `api/rtp_parameters.cc`,
`api/video/resolution.h`, `api/media_types.h`, `api/media_types.cc`,
`api/priority.{h,cc}`, `rtc_base/strong_alias.h`, and
`api/rtp_sender_interface.h`, `api/rtp_receiver_interface.h` and
`api/rtp_transceiver_direction.h`. This is source implementation work, not completed
parity or behavioral qualification. Builds and tests remain deferred.

## SDP video formats and render resolution

`SdpVideoFormat` owns the actual SDK value rather than the older three-field
`VideoCodecFormat` DTO. Name and parameter-map strings preserve arbitrary bytes;
raw uint8 scalability modes retain duplicates/order and unknown metadata values.
The native `packetization` and `tx_mode` optionals preserve absence separately
from present-empty values. All four native constructor overloads, copy/move
construction/assignment, field replacement, exact equality and the well-known
format factories are available. Static format factories describe metadata, not
whether a codec implementation is linked or usable.

Native exact equality compares all five fields. `is_same_codec` instead delegates
to the SDK's case-insensitive name and codec-specific parameter predicates,
ignoring mode lists and the two optional fields. `is_codec_in_list` uses actual
native matching. `fuzzy_match` preserves first-wins ties, including zero matching
parameters, and returns the entire selected supported value rather than the
request or a normalized substitute. Native fuzzy matching's signed-int matched
parameter counter is guarded before overflow.

Native format text is returned as bytes. Only native mode-name consumption
rejects unnamed scalability modes; storage, copying, equality and codec identity
remain unrestricted. Fuzzy matching's actual diagnostic branches can invoke
ToString on the request and/or selected value. In logging-enabled artifacts,
the guard covers these branches even if current severity suppresses them,
because process-global severity can change concurrently. Exact-equal matches do
not reach that formatting. Logging-disabled artifacts do not apply this
logging-consumption guard. Typed owning/borrowed StringBuilder insertion and
consuming RTCError insertion likewise guard native naming before mutation,
leaving the builder/error unchanged on rejection.

`VideoEncoderFactoryHandle::supported_format_values`, `implementation_values`
and `VideoDecoderFactoryHandle::supported_format_values` query the actual
selected SDK factory and return independent complete values. GetImplementations
is not replaced with GetSupportedFormats; the selected factory's native default
may itself delegate. These metadata snapshots retain no factory/peer origin.
The older text DTO API remains a separate convenience surface. Encoder support
queries preserve absent versus present-empty scalability-mode strings through
both native call directions, while the legacy resolution/text DTO limitations
remain. Full-value factory Query/Create and caller-selector installation are
separate contracts.

`RenderResolution` owns the SDK's two signed integers. Its actual default is
zero/zero, native Valid is exactly positive width and height, and construction,
copy/assignment, dimensions and equality preserve the full signed-int domain.
No PixelCount multiplication, encoder admission or geometry normalization is
invented by this metadata API.

## Sender observations and stream association

`RtpSender::ssrc` preserves native zero when the primary SSRC is not determined.
`media_type` returns the actual SDK enum. `stream_id_bytes` copies the complete
native stream-ID vector in one signaling-proxy query, preserving byte strings,
empty values and ordering. The existing `stream_ids` getter is a UTF-8
convenience view for the UTF-8 installation graph. `set_streams` invokes native
SetStreams on the supplied complete string batch; native first-occurrence
de-duplication, including empty IDs, is observed through a fresh getter. The
void native setter is not an SDP or renegotiation acknowledgement.

`ProductionSession::sender_ssrc`, `sender_native_id`, `sender_media_type`,
`sender_stream_ids`, `set_sender_streams` and `sender_initial_encodings` use actual retained sender
IDs and SDK proxy calls. Returned strings and encoding values are independent
owned copies, not captive engine handles. These operations retain production's
single mutable-owner migration contract. Initial encodings are not an effective
current-allocator ledger and do not start a GetParameters transaction.

`RtpSender::generate_keyframe` and
`ProductionSession::generate_sender_keyframe` invoke actual native
GenerateKeyFrame with RID byte strings and complete owning RTCError on failure.
Audio returns the SDK's UnsupportedOperation. A stopped or unconfigured video
sender returns the SDK's successful no-op before checking RIDs; an active sender
validates empty/unconfigured RIDs natively and posts its real worker-queue keyframe
request. Success is not evidence that the encoder produced a frame. The existing
`request_keyframe` convenience method retains its separate active-video policy.

Arbitrary-byte stream installation is not yet exposed. It must first preserve
bytes through the entire consuming SDP/observer graph: constructing a CXX
`rust::String` from invalid UTF-8 terminates, so a raw native setter alone would
make later safe SDP operations abort. Metadata string storage is not proof of a
safe live installation path.

## Receiver parameters, delay and source observations

Receiver GetParameters is now source-wired to a complete independently owned
SDK RtpParameters copy through the stock worker-thread proxy. It is not a sender
transaction, does not invoke a second getter, and does not use the legacy sender
overlay cache. The legacy receiver SetParameters call preserves the pinned
stock proxy's inherited unsupported false result, even for unchanged inputs.
Its parameters are not validated as sender inputs or installed on a sender.

Receiver SetJitterBufferMinimumDelay preserves None versus explicit zero and
forwards seconds unchanged through the stock worker proxy. The SDK owns the
saturated integer conversion and 0..=10000-ms clamp. Negative values, infinities
and large finite values are therefore not normalized or rejected by the binding.
Only present NaN is rejected before caching: pinned
pc/jitter_buffer_delay.cc:GetMs consumes the cached delay with
saturated_cast<int>, whose NaN path in
rtc_base/numerics/safe_conversions.h terminates with RTC_CHECK. This guard applies
before stream start as well as after a media channel is attached.

GetSources copies actual SDK SSRC/CSRC observations through the stock worker
proxy. RtpSource owns an actual SDK value with its constructor, copy/assignment,
timestamp, source ID/type, RTP timestamp, audio-level setter, and all Extensions
fields. Absolute capture time retains UQ32.32 bits and optional signed Q32.32
remote/capture offset. The distinct optional local/capture offset retains native
TimeDelta bits, including infinities. Audio level preserves the full uint8
metadata domain. Native equality intentionally ignores the local/capture offset;
the separate extension snapshot equality does not. SDK formatting and typed
StringBuilder/RTCError insertion are wired, including the SDK's SSRC formatting
fallback for unknown scoped source-type metadata. Values retain no peer, origin
sequence, borrowed state or callback and support owning Send/Sync.

All four receiver operations are also available on ProductionSession through
receiver IDs, preserving owner migration and typed lifecycle/delay errors.
They do not claim received VLA, invent source/track joins, or replace native
observation with a stats lookup. Fixtures in tests/rtp_sources.rs, tests/audio.rs
and tests/production_session.rs are written but unrun. Receiver media-stream
objects, transformation and SFrame contracts remain separate gaps. Legacy frame
encryptor/decryptor interfaces, retained caller providers and actor installation
are now source-wired separately in [frame crypto contracts](frame-crypto-contracts.md),
with native encrypted-audio fixture sources written but unrun.

## Delivered source slice

`Resolution` owns an actual native `webrtc::Resolution`, not a Rust substitute.
Its signed width/height getters and setters, SDK copy construction/assignment,
SDK equality, `ToPair`, and `PixelCount` are source-wired. `PixelCount` first
checks the product in a wider integer because the SDK multiplies signed ints
without overflow protection. Zero/negative dimensions remain storable,
comparable, and callable when the product is representable. Positivity belongs
to the separate SDK encoding-installation contract, not the standalone value.

The concrete SDK aggregate contains only two owned integers. It has no native
origin/thread affinity, lazy state, callbacks, or external references. Shared
methods only read/copy; mutation requires an exclusive Rust loan. This supports
the source-only Send/Sync argument. Test sources exercise native copying,
assignment, signed metadata and consumption-point boundaries; they are unrun.

`RtcpFeedback` and `RtpCodec` now also own actual SDK values: constructors,
SDK copying/assignment/comparison, every field and collection, native MIME and
stringification/classification helpers are source-wired. Feedback enum and
optional-enum AbslStringify calls go to the SDK, including its safe empty output
for unknown values. MediaType preserves unknown metadata and both native
conversion paths reject only unsafe consumption. Codec strings and map entries
preserve arbitrary bytes and NULs.

FMTP write/parse helpers operate on the owned codec's actual native map field
and the owned StringBuilder. Parse returns the full owning SDK RTCError.
Matching test sources are unrun.

`RtpSender::init_send_encodings()` now returns owned SDK encoding values copied
through the sender's signaling-thread proxy. `RtpEncodingParameters` preserves
all optionals, signed/unsigned scalar domains, raw string bytes, doubles,
collections, native constructors/copy/assignment and equality. Its codec and
resolution field getters return independently owned SDK copies. Equality omits
both `scalability_mode` and `request_key_frame`, as the SDK does; it is not a
derived field-snapshot equality. NaN metadata prevents an Eq claim.

Encoding AbslStringify checks only a present codec's unsafe MIME kind.
Signed dimensions and non-finite doubles are safe formatting metadata and
remain accepted. These values retain no engine origin or references: the
encoding owns primitives, strings, vectors and pure copied codec/resolution
values. Shared access reads/copies; mutation is exclusive, supporting the
source-only Send/Sync argument.

`PriorityValue` owns the SDK strong alias, exposing raw uint16 and guarded
Priority constructors, SDK copy/assignment, six comparisons, lifetime-bound
native value/dereference/arrow/reference-conversion loans, value-category
consuming value/dereference helpers, and native AbslHashValue via absl::Hash.
The enum constructor's fatal default is guarded at that conversion only; the
raw uint16 domain remains unrestricted. There is no derived default constructor
or SDK AbslStringify, so neither is invented. The stored scalar and exclusive
mutable loans support standalone Send/Sync.

Owned StringBuilder, borrowed RTCError message builders and consuming RTCError
streams now invoke the SDK typed template insertion for RtpCodec,
RtpEncodingParameters, MediaType, RtcpFeedbackType and its optional form.
Unsafe MIME/MediaType formatting leaves the output or returned error owner
unchanged; safe unknown feedback types emit the SDK's empty output rather than
an invented rejection. Errors retain type/detail/cause fields through native
rvalue streaming. New fixtures compare the SDK formatting paths and unmodified
rejected owners; these fixtures remain unrun.

`RtpCodecParameters` now owns the SDK derived codec object, preserving the
signed payload type and all inherited fields through an explicitly independent
SDK base-codec copy and complete base-subobject assignment. SDK comparison,
MIME/classification/formatting and typed builder/error insertion are wired.
`RtcpParameters` owns the SDK value, preserving optional SSRC (including zero),
CNAME bytes, reduced-size/mux flags, native copy/assignment/equality and native
formatting/typed insertion. Both are standalone pure metadata Send/Sync values.
Signed payloads and malformed strings remain valid metadata; only unsafe MIME
formatting is rejected. Matching fixtures are source only and unrun.

`RtpHeaderExtensionId` now owns the SDK signed-int strong alias, preserving the
full i32 metadata domain. SDK default/NotSet and native constants, copy/
assignment, int conversion, Valid/IsSet, native formatting/hash/comparison,
borrowed strong-alias access and consuming value categories are source-wired.
The constructor's commented-out checks are not resurrected as range gates.
The borrowed underlying conversion explicitly calls the inherited reference
conversion, rather than risking a temporary from the derived int conversion.
Fixtures include pointer identity and invalid-but-constructible IDs, and remain
unrun. RtpExtension now additionally owns the SDK value and source-wires
both typed and legacy constructor overloads, default/copy/assignment/equality,
all fields, ToString and AbslStringify, support/encryption predicates, all URI
and numeric constants, and native vector Find/Deduplicate helpers. Lookup
results retain actual SDK element pointers under immutable parent-list loans;
owned copies and dedup results are independent SDK values/vectors. The unscoped
Filter conversion guard accepts its full legal 0..3 domain, including unnamed
3, and rejects wider raw values only before a potentially undefined C++ enum
conversion. These binding range errors are not SDK RTCError completions.
Header-ID and extension typed streaming use actual SDK template insertion.
New constructor, metadata, constants, lookup, dedup, thread-loan and typed
stream fixtures plus the loan invalidation compile-fail fixture remain unrun.

`RtpParameters` now owns the entire actual SDK value: transaction ID/MID bytes,
codec/header-extension/encoding vectors, RTCP, and optional degradation
preference. Child getters copy SDK values independently; setters assign the
complete native values back. Default/copy construction, generated assignment,
SDK equality/inequality, IsMixedCodec and AbslStringify are source-wired.
Equality intentionally excludes transaction_id and retains the encoding
comparison's omissions and NaN behavior. IsMixedCodec ignores inactive
encodings and distinguishes absent versus present codecs.

DegradationPreference preserves arbitrary raw metadata. DISABLED is a true
alias of MaintainFramerateAndResolution, not a distinct value. Its native
string helper guards the SDK fatal default only at consumption. Full parameter
formatting and typed owned/borrowed builder/consuming error insertion likewise
guard invalid degradation preferences and nested MIME kinds before appending;
rejection preserves the original output/error owner and stored metadata.
Standalone parameters own only copied metadata, with no invented sender or
factory origin. Shared methods read/copy, mutations require exclusive access,
and destructors retain no external resource, supporting Send/Sync. New
`tests/rtp_parameters.rs` fixtures are source only and unrun.

`RtpSender::get_parameters()` now calls the stock signaling-thread proxy once
and owns the complete returned SDK snapshot. It does not consult or modify the
legacy overlay DTO cache. Every SDK get starts a new transaction and invalidates
earlier snapshots for a subsequent set. Retained sender/peer/factory origin
leases cover the synchronous blocking call; returned metadata has no origin
affinity. Snapshot/transaction regression sources in tests/audio.rs are unrun.

`RtpCodecCapability`, `RtpHeaderExtensionCapabilityValue` and
`RtpCapabilities` now additionally own complete native SDK capability values.
The codec capability preserves every inherited codec field, optional signed
preferred payload type, and ordered scalability-mode metadata; the header
capability preserves URI bytes, optional signed strong-alias ID, encryption and
raw signed direction. All SDK header-capability constructor overload semantics,
including deprecated integer conveniences, are source-wired. Children are
independent SDK copies, not live engine views. Native copying, assignment and
equality preserve all SDK fields and collection order/duplicates. These are pure
metadata values with exclusively borrowed mutation and no origin affinity,
supporting source-only Send/Sync. The older similarly named header-extension
negotiation DTO remains a separate, incomplete actor convenience surface.

Capability AbslStringify uses the actual derived SDK formatter: audio gets its
clock/channel fields; every other kind prints video. Unknown kinds/modes are
safe for that formatter, so no base-codec MIME guard is added to typed capability
builder/error insertion. The MIME helper separately guards its actual fatal
conversion. ScalabilityMode retains the full uint8 metadata domain, exposes SDK
known alternatives/all modes, and guards only the fatal SDK name conversion;
the optional overload delegates to the SDK's "nullopt" spelling. FecMechanism
preserves every signed enum value and numeric identity even when a known value
is constructed through Other. RtpTransceiverDirectionValue additionally
preserves the complete five-state signed SDK enum and unknown metadata,
including numeric equality, SDK AbslStringify and typed builder/error insertion.
Unknown stringification emits the SDK's safe empty result. Header capability
constructors/accessors use that complete typed enum; the legacy four-state actor
convenience path is not widened. Helpers in private pc/rtp_media_utils are not
claimed as public api/rtp_transceiver_direction.h contracts.

Factory `get_rtp_sender_capabilities` and `get_rtp_receiver_capabilities`
perform the actual synchronous SDK query on the retained signaling thread,
without constructing a peer or projecting through old DTOs. Their result owns
all codecs, header extensions and FEC metadata. Non-audio/video kinds return the
concrete factory's actual empty SDK result, not a binding-created replacement.
ProductionSession exposes these same queries against its actual selected
factory, including after caller migration. Its get_sender_parameters likewise
returns the full SDK value and retains fresh-transaction behavior. Capability
fixtures in tests/rtp_capabilities.rs, factory fixtures in tests/audio.rs and
actor migration fixtures in tests/production_session.rs are source only and unrun.

## Live transceiver capabilities

RtpTransceiver and ProductionSession now expose rtp_codec_preferences,
header_extension_capabilities_to_negotiate and
negotiated_header_extension_capabilities as complete independently owned SDK
values. Empty codec preferences mean an empty override, not synthesized factory
defaults. Header snapshots preserve optional strong-alias IDs, encryption, URI
bytes and full signed direction metadata; pre-negotiation unmatched entries are
marked Stopped by the SDK rather than hidden.

set_rtp_codec_preferences copies all supplied capability values into the SDK
span operation, without matching/replacing them by factory name/parameters.
The SDK owns validation, deduplication, retention and direction-specific cache
views. It compares kind/optional rates/channels and codec parameters with
supported codecs; unknown media preferences produce native RTCError. Preferred
payload type, feedback and scalability metadata are retained in preferences,
but the inspected deferred SDP matching selects actual supported native Codec
objects rather than constructing them from those arbitrary fields. No fatal
MIME/scalability string conversion is reached by this consuming path, so their
separate formatting guards are not generalized into installation restrictions.

set_header_extension_capabilities_to_negotiate passes the complete ordered
values. The SDK validates vector size/order and mandatory MID direction, then
mutates only direction. Caller IDs/encryption are not applied. Unknown
non-mandatory directions are safe fixed-underlying enum metadata on the inspected
SDK store/comparison paths; no broad closed-enum gate is added. Missing ID
fallback and present NotSet are not conflated by getters or reconstructed through
the legacy DTO. Standalone header IDs remain unrestricted.

RtpCapabilityInstallError::Native retains the complete owning SDK RTCError;
Binding denotes pre-dispatch actor/peer/ID failures, not invented SDK results.
The SDK transceiver proxy marshals the getter/setter calls to signaling; input
span backing storage survives the blocking call and the SDK retains copies.
Rust transceiver handles retain their peer/factory lease, while returned metadata
has no origin/thread affinity. direction_value/current_direction_value preserve
the full signed SDK enum. Current-direction absence uses a separate presence
flag, not a narrowed -1 sentinel that conflates raw metadata; legacy closed
direction setters are not widened. ProductionSession exposes the same operations
against captive real transceivers after caller-thread migration. Legacy
format/header DTO methods remain explicitly separate conveniences.

tests/rtp_transceiver_capabilities.rs adds full metadata/SDK deduplication,
native versus binding error, ignored header ID/encryption, unknown direction,
actual SDP operation, migration and post-teardown owned-value fixtures. These
are source only and unrun. A focused review of the four owning capability
operations found no definite signature, lifetime, proxy, unsafe metadata sink
or source-fixture defect. This is not compiler/runtime evidence and does not
qualify whole binding parity.

## Transceiver init metadata and remaining state getters

RtpTransceiverInit owns the actual SDK value with all three fields: full signed
direction, ordered arbitrary-byte stream IDs, and complete send encodings.
Default construction, native copy and implicit copy assignment retain SDK
semantics. Child getters return independent values; duplicates/order and
standalone unsafe-to-consume numeric metadata are not normalized. The object
has no origin/handles/thread-affine destruction and supports pure-metadata
Send/Sync. No equality or native stringify contract is invented for this SDK
struct. Rust Debug is explicitly metadata diagnostics. Installation uses
`PeerConnection::add_transceiver` for trackless receiving and the typed or
owning-track `add_*_transceiver_with_init` methods for sending. The common
`Session<R>` exposes these operations without a runtime-specific procedure.
Owning `MediaTrack` input includes actual receiver tracks from the same factory,
not just newly created local sources. Accepted local inputs retain their typed
source leases; receiver inputs retain the native track and factory without an
ownership edge back to the originating Rust peer. Full native errors are moved
into `RtcError`, separately from pre-dispatch binding failures. Unknown unsafe
directions, non-Unified-Plan configuration and unsafe video numeric conversion
inputs are guarded at consumption, not normalized in stored metadata. The
allocator-group safety qualification discussed below remains separate from
this installed input path.

The live transceiver and ProductionSession also expose actual SDK media_type,
stopping, receptive, fired_direction and optional SframeEnabled snapshots.
Fired direction preserves full raw signed values separately from absence;
SFrame absence is distinct from false. These calls use the actual SDK proxy
and retained transceiver lease, not cached DTO fields. The Chromium-oriented
fired_direction contract is public; StopInternal remains explicitly internal
and is not generalized into an application method.

tests/rtp_transceiver_init.rs adds default/copy/assignment/arbitrary-byte/full
encoding/unknown direction/NaN-bit/migrated shared-read/independent-child source
fixtures. The live fixture adds state getters before migration and after close.
`tests/transceiver_creation.rs` exercises owning local audio/video input and
retained successful products. `tests/media_references.rs` installs an actual
negotiated receiver track and reads it after the original peers close and drop.
`tests/session_runtime.rs` uses the same application procedure to install
native audio/video receiver tracks and observe exact SDK initialization errors
with threaded and caller-driven dependencies. These operation/lifetime checks
do not establish a forwarding-media replay guarantee or resolve the numerical
allocator issue below.

Preferred-direction installation now also has a full signed value entry point
through the actual synchronous SetDirectionWithError proxy, plus public
StopStandard with owning native errors and idempotent SDK behavior. The shared
RtpTransceiverOperationError separates native RTCError from pre-dispatch binding
failures and UnsafeDirection; RtpCapabilityInstallError is its capability alias.
All five known preferred directions reach SDK policy (including native rejection
of Stopped). Unknown preferred directions alone are guarded at consumption:
SetDirectionWithError stores them, then CreateOffer passes that value to
GetMediaDescriptionOptionsForTransceiver/CreateOfferOrError and the
direction-based codec selection at pc/codec_vendor.cc:1332-1348 ends in an
unconditional RTC_CHECK_NOTREACHED. This is actual reachable deferred failure,
not a closed-enum metadata constraint. It does not apply to the header-extension
setter, whose unknown directions use safe storage/comparisons instead. No
StopInternal application method is invented. Production exposes these operations
and the stopped getter. Source fixtures cover unknown rejection without changing
preferred direction, actual Stopped native rejection, stopped-state native
error/idempotent stop and owning teardown. Unrun.

## Encoding graph remaining

The older `RtpSenderEncoding` DTO and overlay Get/SetParameters API remain
incomplete. Full owned RtpParameters Set/async paths, remaining helper graphs,
remaining legacy actor reconciliation, typed native template streaming paths
and actor installation are still required. Initial encoding
snapshots and standalone metadata mutation do not constitute installation.

The full setter's downstream safety audit found invalid video degradation
preferences reach later VideoStreamAdapter fatal adaptation switches. Unknown
network priority instead leaves the SDK's initialized DSCP default untouched;
no rejection is justified for it on that evidence. NaN scale-by on a multi-layer
branch reaches a float-to-int cast, positive large scale-to dimensions can
multiply with signed overflow, and non-finite/out-of-range frame rates reach
integer encoder configuration. Positive infinite scale-by is adjusted to a
finite factor on the inspected path, so a blanket finite-only scale policy
would not follow that evidence. Bitrate-priority assertions and allocation
arithmetic also has confirmed cross-track cancellation UB with finite normal
priorities: a sum of 1e308 and 1.0 rounds to 1e308, subtracting the saturated
large priority leaves zero. If saturation also exhausts the remaining bitrate,
the residual division produces infinity and multiplication by zero produces
NaN at the final double-to-int compound-assignment sink. Individual finiteness checks do not suffice. SDK gets
change transaction IDs, so they are not neutral allocator-group inspection.
Audio field-trial overrides can also change effective priorities. The stock
group is each native Call's BitrateAllocator; its participants are audio/video
send streams, not sender identities or separate FlexFEC participants. Remote
negotiation can create a sender, but it is null-track/recvonly until an observed
local attachment. Public init_send_encodings is a one-shot init vector, cleared
when SetSsrc applies it, not a neutral current-priority snapshot; arbitrary
reused senders cannot be seeded from it. Negotiation rollback can restore init
encodings internally. Audio's exact rate_prio optional override is reproducible
using the actual SDK AudioAllocationConfig parser (no helper installed yet),
then falls back to AudioSendStream::Config::bitrate_priority; the stream's
parsed allocation settings are const. Audio priority reconfiguration updates
the observer before completion. Video is different: absent last-frame
information, successful configuration completion precedes its first later
frame-driven encoder configuration and allocator update. SetParameters
completion alone therefore cannot discharge old video priority reservations.
The exposed sender API supplies no explicit effective-priority query or
encoder-configuration quiescence barrier. With known frame information the
sync setter waits for configuration callback and its completion is ordered;
that does not identify the branch taken by an arbitrary call. Public
sender-selected GetStats does not acknowledge the requested configuration:
reports can be cached/shared, SSRC selection uses cached parameters, and
frames_encoded counts lack configuration identity. It cannot by itself
discharge a deferred reservation. A faithful ledger must cover creation,
every attachment/input, rollback and deferred old/new states, not simply record
last successful SetParameters values. These are feasibility findings, not an
implemented safety proof or installed full setter.

A later source study found a possible private instrumentation seam: wrap the
actual MediaFactory selected by EnableMedia, delegate CreateMediaEngine unchanged,
and decorate its actual returned Call and video send streams. MediaFactory is
explicitly an implementation detail, not a new caller API. Forwarding must retain
the selected factory and unwrap stream pointers for the underlying Call's destroy
methods. Per-factory serialized peer creation needs an ordered handoff, not a
configuration-equality or guessed SSRC/source join. The stock null-default video
stream factory must remain null; replacing it with a test factory is not a
transparent observation mechanism.

For that stock path, a monotonic-prefix witness is feasible in principle: track
actual reconfiguration requests and SDK callback completions, then post a
read-only witness to the same worker queue from the selected encoder's actual
Encode invocation, after the SDK has posted its allocator update. Capture only
the already-completed prefix and retain every newer candidate, including updates
that overtake the witness. Callback completion alone is not an installation
witness. Null-callback requests need private forwarding completion observation;
failed creation/init or absent Encode cannot be assumed to complete. This is a
source-pinned protocol candidate, not an installed decorator, completed lifecycle
proof, or exact-configuration identity claim.

The numerical studies likewise do not supply a complete admission rule. A
singleton positive normal priority does not require an absolute scalar ceiling.
For groups, sum/residual error bounds alone do not certify the integer sinks;
the allocator's capacities, remaining budgets, rounded sorting and saturation
branches matter. A checked observed execution can reject nonpositive/nonfinite
residuals, unsigned capacity-subtraction underflow and unrepresentable final
base-plus-share values, but there is no proved complete set of future executions
for a pre-dispatch guard yet. The relative-allocation sink is signed int, requiring
a final value strictly below INT_MAX+1, not merely below the uint32 bound.
Task #4 still tracks this consumer strategy, also relevant to allocation-trial
priorities on otherwise default sender creation. The legacy DTO setter additionally protects
max-framerate conversion using the precise exclusive int-max-plus-one bound,
and positive scale-to signed PixelCount overflow; fixtures are source only. All these domains remain available in standalone metadata; a future
safe installer must distinguish bridge safety rejections from actual SDK
RTCError completions, without inventing general convenience restrictions.

## First-packet observer slots

RtpSender::packet_observer and RtpReceiver::packet_observer return owning,
sequence-bound aliases of one SDK observer slot, keyed by the actual retained
SDK interface pointer within the peer, not by string IDs or SSRC guesses. Weak
caches do not own observers. Each owning observer retains its SDK proxy and
peer/factory lease and explicitly clears SetObserver(nullptr) through the
signaling proxy before the native observer/vtable is destroyed.

register_observer starts a positive-capacity queue epoch, rejecting zero
capacity/already-registered as binding queue-lifecycle errors, not SDK RTCError
results. SDK SetObserver itself is void. Late registration retains the stock
SDK's synchronous first-packet replay. SDK callbacks only enqueue fixed-size
records under a native mutex. No Rust callback, application executor, hidden
pump or observer reentry runs there. Poll explicitly with take_event and
take_dropped_events; these queues do not themselves wake ProductionSession::ready.
Overflow evicts oldest and saturates the loss counter; reading resets the counter.
Unregister preserves queued records and serializes against SDK callback delivery.

Records preserve SDK OnFirstPacketSent, OnFirstPacketReceived and
OnFirstPacketReceivedAfterReceptiveChange, including signed raw MediaType.
OnFirstPacketReceived is an SDK slot notification, not receiver-SSRC delivery
proof: the pinned SDK may fan it out to every receiver of the same media type.
The native observer interfaces do not expose arbitrary RTP packet payloads.

ProductionSession retains the same observers inside its captive graph, with
register/unregister/take-event/take-dropped operations using opaque sender and
receiver IDs. Actor close explicitly retires these slots before peer close;
queued events remain pollable afterward. Peer/receiver Stop or Close alone is
not SDK observer unregistration. The focused source review found no definite
ownership/proxy/teardown defect and identified the documented receiver fanout
caveat. Alias/lifecycle, actual Opus packet and synchronous replay fixtures, and
actor migration fixtures are source only and unrun. Overflow, receiver
receptivity-change, teardown races, controlled behavior and consolidated
core/native qualification remain obligations, not proven outcomes.

## Async setter remaining

`SetParametersAsync` needs actual completion RTCError snapshots. The base
interface default does not invoke its callback, whereas the concrete stock
sender override validates/completes and invalidates its transaction ID. Accepted
inputs are copied by value into the worker task/callback wrapper before return.
Early validation failure can invoke immediately without resetting the ID;
success resets it immediately before completion. Safe-task cancellation after
channel teardown can discard completion uncalled. Native callback retirement
must be separately observable and lifetime/origin leases retained, never
reported as a fabricated SDK completion. No synthetic success or assumed
default callback may replace that native path.
