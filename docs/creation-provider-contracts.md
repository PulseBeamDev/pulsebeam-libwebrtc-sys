# Native creation-provider contracts

This records implemented ownership paths and the remaining pinned dependency
inventory. It is not a claim of complete creation-interface coverage or of
qualification. The new provider tests have not been run.

## Network monitor

`PeerConnectionFactoryBuilder::network_monitor_factory` and
`ProductionSessionBuilder::network_monitor_factory` register a thread-safe owned
`NetworkMonitorFactory`. Native factory dependencies own the adapter and retain
the Rust registration independently of its builder. The native network manager
owns each freshly returned monitor.

- Creation and all operations on a monitor run on the native network thread.
  Factory destruction may occur on another native role thread. Providers and
  shared application state therefore require `Send + Sync`.
- Creation receives an independently owned field-trial copy and a notification
  handle. Returning `None` selects the native no-monitor fallback.
- Interface metadata preserves adapter type, VPN underlying type, preference,
  availability and network slice. Interface names are byte slices, not assumed
  UTF-8. Binding receives an IPv4/IPv6 address, the interface name, and a borrowed
  socket. The socket must not be closed or converted into an owned descriptor.
  Every native binding result is represented.
- Notification handles can be cloned and retained on application threads. They
  post asynchronously to the native network thread; they never invoke the
  manager inline. Notifications before Start, after Stop, and after retirement
  return false. Acceptance does not guarantee delivery before a subsequent Stop.
- Stop invalidates the active generation, canceling previously posted work. It
  must quiesce the provider's event source. Destruction retires the target before
  its owning network thread can be released. The no-monitor path also retires
  its target, including when a factory callback panics.
- An operation panic disables subsequent provider operations, returning
  unavailable metadata or a failed bind. Stop is still attempted for cleanup.
  A factory panic selects the no-monitor fallback. Destructors must not panic.
- Native dependencies prohibit combining a custom network manager with a monitor
  factory. Rust rejects that combination before constructing native resources;
  the C++ bridge also checks before entering native factory creation. Controlled
  networking supplies its own manager and therefore cannot select this monitor
  factory. This is a native incompatibility, not an actor-locality restriction.

## Per-peer field trials

`PeerConnectionDependencies::field_trials` registers an owned, immutable,
thread-safe provider for one peer. Both local
`create_peer_connection_with_dependencies` and actor-safe
`create_peer_with_dependencies` accept it together with an `RtcConfiguration`.
Existing convenience creation methods pass default dependencies.

The adapter reuses the environment's Rust `FieldTrialsView` ownership and copy
path. Native `PeerConnectionDependencies::trials` overrides that peer's owning
Environment, including the environment delivered to its event-log factory. It
does not merge with or mutate the factory view. A missing override retains native
factory defaults. Independent snapshots retain their own copies after peer and
factory retirement. Executable providers are not part of configuration equality,
GetConfiguration, or SetConfiguration.

## Custom TLS verification and certificate leaves

`PeerConnectionDependencies::tls_certificate_verifier` supplies an owned
`TlsCertificateVerifier` on either creation surface. Both native `Verify` and
`VerifyChain` dispatch to the Rust provider. Default chain verification rejects
an empty chain and otherwise invokes legacy verification of its leaf. Default
legacy verification rejects safely rather than entering native's not-reached
check. Verification is synchronous on the native network sequence and cannot
wait for or reenter the owning application actor. A panic disables both callback
entries on that native registration; subsequent verifications reject.

The provider receives owned `TlsCertificate` / `TlsCertificateChain` handles,
not borrowed pointers into the handshake frame. Clones may outlive peers and
cross application threads. The wrappers are compile-time restricted to the
qualified pinned BoringSSL backend, whose certificate owners retain immutable,
concurrently reference-counted CRYPTO_BUFFER bytes. No SSL session or backend
mutation is exported. Other backends cannot silently inherit Send/Sync.

Certificate values expose PEM parsing/export, DER, native Clone, signature digest
algorithm, digest computation including capacity failure, exact expiration
seconds and native nullable stats. Parsing uses owned terminated storage and
checks the native BIO length domain. Nonzero digest capacity avoids the native
DCHECK; oversized capacity is reduced to the native maximum digest size without
changing outcomes. Empty chains, checked member access, native chain Clone and
nullable chain stats are supported. Stats preserve every public field and the
optional issuer chain, with deep Copy.

The selected `turn_tls_ca_pem` verifier and custom verification cannot both be
registered: both occupy the same native unique dependency. Creation returns
InvalidParameter instead of silently replacing either implementation. Existing
configuration snapshots and update semantics remain separate from providers.

Tests cover leaf/chain values, copy/teardown, cross-thread retained reads, default
legacy behavior, provider conflicts and panic deactivation. A coturn-backed test
exercises the native custom chain callback for acceptance, rejection and panic,
then reads retained chain owners after peer retirement. They remain unrun.

## Local-network permissions and address leaves

`PeerConnectionDependencies::local_network_permission_factory` registers an
owned `LocalNetworkPermissionFactory` on both creation surfaces. Every Create
returns an owned permission product. Its default query matches the SDK's
private-or-loopback check, and callers can override the query independently of
the asynchronous request. An absent registration retains native's
no-permission-check behavior.
A factory panic disables future creation callbacks and produces rejecting
products instead of passing the SDK an invalid null product. A product-method
panic disables later callbacks on that product and attempts to deny an unanswered
request. An already accepted completion remains the first decision.

The permission receives a deep-owned `SocketAddress`, preserving IP family,
hostname bytes, resolution/literal state, port, and signed scope ID. Creation,
queries, requests and product destruction occur on the native network sequence.
Factory destruction can occur on another native role thread. Providers require
Send + Sync, must not synchronously wait for or reenter their owning actor, and
must not panic in destructors.

`LocalNetworkPermissionCompletion` is cloneable and Send + Sync. First submission
wins. Delivery is always posted to the originating native sequence, not invoked
inline from Rust. Returning true means accepted for delivery, not delivered.
Product destruction cancels queued delivery, releases the native callback on its
sequence, clears the dispatch target under the same mutex used for posting, and
rejects retained handles. Native callback invocation can destroy the product;
the adapter moves the callback out before invoking it and does not touch the
product afterward. Dropping an unanswered Rust completion does not fabricate a
decision; the native product retains its pending query until completion or
retirement.

Shared address leaves include all pinned `IPAddress`, `InterfaceAddress` and
`SocketAddress` value operations, byte-preserving formatting, comparisons and
native hashes, interface flag bits, and initialized target-native sockaddr
conversions with explicit written sizes. Empty or invalid-range socket parser
inputs are checked before native assertions, while supported native parser
quirks and failure-without-mutation outcomes remain observable. Sockaddr bytes
are ABI-specific values, not portable serialization or borrowed pointers.

Unrun tests cover actor migration, cross-thread grant and one-shot rejection,
unanswered query cancellation at teardown, denial, query bypass, factory/query/
request panics, native STUN release or suppression, and retained address reads.
A focused static review found no actionable defects in the permission bridge.

## Custom clocks and time leaves

`ClockProvider` implements both pinned virtual methods: a finite signed current
`Timestamp` and conversion of an exact `Timestamp` state to an owned `NtpTime`.
`Clock` provides owning, shareable registration and all native instance clock
queries/conversion entry points. `EnvironmentBuilder::clock_provider` and
`clock_handle` select it for local factory creation; the production actor builder
accepts the same owned registrations. Factory and actor `clock_timestamp` queries
read the selected native environment and return owned signed values.

Native environments own shared references to the adapter, independently of the
application registration handle. Copies retain the provider after that handle
is dropped. Reads and conversion may run concurrently on arbitrary engine roles;
providers require `Send + Sync`. No process-global time hook is installed. A
callback panic latches failure: later reads use the last successful finite time
(zero if none), and conversion returns invalid NTP zero. Already-entered callbacks
may finish. Destructors must not panic and callbacks must not synchronously wait
for or reenter their owning actor.

Manual and custom clock selectors replace one another. Custom clocks cannot be
combined with cooperative queues, whose clock must be the actual registered
manual clock. The Rust builder rejects that combination; native construction
checks it independently. Custom clocks also do not implicitly establish a mapping
to process `TimeMillis` for the encoded Opus capture-time convenience path.

`Timestamp` and `TimeDelta` preserve all signed microsecond representations,
including explicit infinities and the finite negative range actually accepted
by this pinned implementation. Integral constructors retain the native strict
pre-scaling checks; floating constructors retain float32/double multiplication,
truncation and overflow-to-infinity. Queries preserve native negative-half
rounding, checked nanosecond scaling, fallbacks, ordering and formatting.
Arithmetic checks the exact native intermediates before invoking the SDK. This
includes indeterminate infinity operations, asymmetric endpoint negation/Abs,
llround scalar semantics, and size_t unsigned promotion/wrap. Delta resolution
rounding preserves native truncating negative behavior, not mathematical floor
or ceiling. Timestamp's inherited rounding templates cannot be instantiated in
the pinned SDK and are not presented as callable native methods.

`NtpTime`, `Q32x32` and `Uq32x32` preserve raw bits, validity, ordering and the
pinned conversion/rounding behavior. NTP-to-UTC exposes an exact Timestamp,
including minus infinity for invalid zero and the pinned unsigned epoch
subtraction behavior. Fixed-point conversions reject unsafe intermediate ranges
rather than entering checked casts or signed overflow.

Tests cover signed/extreme values, float precision, infinity arithmetic,
constructor/getter boundaries, scalar and resolution quirks, callback panic
latching, retained environments, scheduler conflicts and actor migration. They
remain unrun. Focused static reviews of custom provider ownership/creation and
the time-value conversion/arithmetic guards found no concrete defect. This is
not executable qualification.

`SimulatedClock` is distinct from the bounded controlled-world `ManualClock`.
It accepts finite signed time and signed advances, preserving atomic wrapping
except for transitions to unreadable timestamp sentinels. NTP seconds wrap by
native design, including the 2036 boundary. Negative fractional seconds are
rejected before the SDK's undefined floating-to-unsigned conversion; negative
exact seconds remain available.

`Clock::real_time()` owns a view of the SDK singleton. A matched current-time/NTP
sample captures its process-static calibration modulo one NTP era. Fractional
NTP resolution is finer than a microsecond, so the original microsecond remainder
is recovered exactly. Wide integer arithmetic then preserves every native-defined
conversion bit, including fractional truncation and era wrapping, without reading
private state, patching the SDK or invoking arbitrary-input signed addition.
For finite inputs outside the SDK's defined nonnegative/signed-addition domain,
the wrapper defines the same mapping modulo one NTP era. It does not identify
those inputs as native-supported. Infinity returns `TimeValueError::InfiniteValue`
from Rust queries and invalid NTP to native consumers. Simulated and custom clock
conversion rules are unchanged.

Construction relies on the SDK's ordinary valid-system-clock assumptions and
returns `RealTimeClockUnavailable` while process testing time is installed.
The shared native adapter holds a production lease through all clones and
installed environments, preventing controlled-hook installation until the last
native user releases it. The singleton's existing calibration is preserved even
if it was initialized during earlier controlled execution; UTC is not resampled
to invent a replacement offset.

## Custom task-queue factories and schedulers

`TaskQueueFactory::from_provider` registers a `Send + Sync` factory and its
independently owned products. The same installed native factory backs public
queue products and engine-created queues in environments using it.
`ProductionSessionBuilder` accepts either the factory handle or a provider;
cooperative factories are rejected before entering the production actor graph.
The adapter does not drive a controlled world or replace process time hooks.

- Factory creation preserves all five native priorities and exact name bytes,
  including empty, NUL-containing and non-UTF-8 names. The existing string
  convenience constructor keeps its previous NUL validation.
- Queue products implement the native name default independently of requested
  factory names. Names copied across the ABI are owned byte vectors. Native
  `queue_name()` views are only valid while their queue owner lives, as with
  other member-backed SDK views. `TaskQueueContext` stores an owned current-name
  snapshot and address-equality token, not a borrowed pointer, retained engine
  owner or globally unique resource identifier. Its `post` method resolves the
  calling thread's live native `Current()` and compares addresses before posting
  asynchronously. `post_delayed` adds exact native delays and precision choices
  with backend-aware conversion checks. A retained snapshot cannot post off that queue's current
  execution scope. Address reuse retains address-equality semantics; a snapshot
  is not a persistent resource identity. Admission can return `NotCurrent` or
  `QueueUnavailable`; accepted work can still be canceled by native shutdown.
  Custom queues reject admission during capture retirement or provider failure.
- Move-only `TaskQueueTask` tickets can cross scheduler threads. Execution
  consumes a ticket; true means accepted delivery, not necessarily completed
  execution. Immediate successors remain gated until their predecessors are
  submitted or canceled. Concurrent submission/cancellation cannot overlap
  native execution or callable-capture destruction.
- A source posting callback must schedule asynchronously. Inline submission on
  that callback's stack is rejected before entering native work and latches
  failure. Cancellation that releases an accepted FIFO-gated successor can
  produce an additional asynchronous drain-notification ticket.
- Native tasks and cancellation establish the product's exact native
  `Current()` identity. Callable captures are cleared before that identity is
  restored. SDK Delete waits for active posting/name callbacks and running work,
  cancels pending captures, calls the source's deletion hook once, then retires
  the queue identity. Retained tickets cannot execute after retirement.
- Full native signed/infinite delay bits and both precision modes reach the
  custom scheduler without deadline arithmetic. The pinned standalone Location
  is empty; Chromium's extended source-location type is not in this artifact.
  Custom schedulers remain responsible for timing and progress.
- `TaskQueue::post_delayed_native` also supports built-in runtimes. Linux default
  stdlib queues accept both infinities, every finite negative value, and finite
  positives through `(i64::MAX - 999_999_999) / 1000` microseconds. Positive
  values beyond that bound can overflow the SDK's POSIX nanosecond wait
  arithmetic and return `UnsupportedNativeDelay`. This check requires no stale
  wrapper timestamp sample: native SystemTime stores an i64 nanosecond clock
  divided into microseconds, Timestamp addition uses microseconds, and the wait
  bound leaves room for the entire normalized `tv_nsec` range. Minus infinity
  is already due; plus infinity remains indefinitely pending until retirement.
- Cooperative queues accept all finite signed delays. Negative values clamp to
  zero; the native scheduler discards deadlines beyond its manual-clock domain
  while holding its scheduler mutex. Infinity cannot pass its native finite
  microsecond conversion and is rejected. Posting success means dispatch to the
  native void posting API, not acknowledgement of execution or indefinite
  retention. No autonomous progress is introduced.
- `NetworkThread`, `WorkerThread` and `SignalingThread` expose the same native
  delay/precision operation, with the SDK Thread backend's distinct domain:
  finite microseconds in `[-1998, i32::MAX * 1000]`. The small negative interval
  rounds to zero under the SDK's truncating millisecond conversion; more
  negative values violate its nonnegative TimeAfter contract. Larger positives
  overflow its signed int millisecond conversion. Both infinities are rejected.
  Borrowed controlled threads use the cooperative domain instead. Both
  precision modes retain actual backend behavior, including Thread's upward
  millisecond rounding. Current-queue snapshots apply the same restrictions
  without exporting a queue owner. Unsupported conversions have inspectable
  errors before posting; existing Duration conveniences reject unsafe finite
  conversions rather than entering native overflow.
- Callback panics are contained and latch factory/product diagnostics. A failed
  public product cannot be created successfully; SDK factory creation receives
  a non-null rejecting product. A rejecting runtime cannot promise SDK progress
  and may stall synchronous engine operations. Source deletion is still
  attempted after failures. Callbacks must not wait for or reenter their owning
  actor, and destructors must not panic.

## Encode and decode metronomes

`MetronomeProvider` implements the complete pinned `webrtc::Metronome`
interface. Register independent providers with `encode_metronome_provider` and
`decode_metronome_provider` on either `PeerConnectionFactoryBuilder` or
`ProductionSessionBuilder`. The corresponding handle setters accept a borrowed
`Metronome` and retain an owned registration. Clones share the Rust provider,
while each native factory slot owns a separate adapter and pending requests.

`tick_period` returns an exact signed/infinite `TimeDelta`. The interface does
not globally forbid zero, negative, or infinite periods. Providers must choose
values and progress suitable for their consumers, including safe timestamp
arithmetic. Requests are serialized on the factory worker and must not block on
or reenter the application actor. Shared registrations can migrate between
workers, so implementations are `Send` and callback access is synchronized.
Callback panics latch `Metronome::has_failed`; subsequent period queries return
plus infinity and requests are declined. Provider destructors must not panic.

Each request owns a move-only `MetronomeTick`. Retain it until the next tick,
then call `signal` from any Rust thread. Admission always posts delivery to the
requesting native worker, never invokes the SDK callback inline. Success means
accepted delivery, not acknowledgement of execution. Dropping an unsubmitted
ticket posts capture disposal to that same worker. Native factory destruction
retires every outstanding request on the worker before releasing it; retained
tickets then return `MetronomeTickError::Retired` and cannot post through the
old thread. This bridge retirement is not an engine cancellation interface:
the native API promises neither cancellation nor a request-completion result.
The trait's default request is a no-op, as in the SDK. Consumers needing ticks,
particularly synchronized decode, can stall if the provider supplies no
progress.

Controlled factories accept caller-driven metronomes. Providers must retain
and signal tickets according to the caller's virtual-time policy, without
helper timer threads or wall-clock progress. Signaling only queues work;
`ControlledWorld::pump` is still required to deliver it. The bridge creates no
threads, timers, sockets or autonomous work.

## Asynchronous certificate generators

`PeerConnectionDependencies::certificate_generator_provider` installs an owned
`CertificateGeneratorProvider`, or `certificate_generator` shares a registered
`CertificateGenerator` handle. Requests preserve the native `KeyParams` values
and optional unsigned expiration milliseconds. The bridge reuses
`CertificateKeyParams` and `Certificate`, with no duplicate identity resource
or synchronous method added to the native interface. `Certificate::generate`
remains the separate native static generator helper. RSA validation preserves
native unsigned parameters; signed unsupported ECDSA curve values remain
queryable and fail validation before conversion to the SDK enum.

The sole provider method enters on signaling. It receives a move-only
`CertificateCompletion` and may retain it or send it to Rust generation work.
`complete(Some(&certificate))` retains the existing native certificate before
posting; `complete(None)` posts native generation failure. Both always post
callback delivery on the requesting signaling sequence, never invoke inline,
and consume the public ticket. Success means accepted delivery, not execution.
A provider panic is caught and latched by `has_failed`, with a private guard
submitting failure if no result has already won. A retained ticket that loses
this panic-fallback race returns `AlreadyCompleted`, not `Retired`.

Stock generation work can outlive the generator itself, so destroying a peer
or generator does not prematurely retire its retained tickets. SDK peer
callbacks retain their native weak-peer checks. The owning factory retires
remaining tickets on signaling before releasing its role queue; subsequent
completion returns `Retired`. Dropping an unsubmitted ticket posts capture
disposal to signaling without invoking the native callback. Callback execution
and disposal occur outside the admission lock and can reenter native ownership.

The absent dependency retains native default-generator selection. Providers
are also accepted in controlled factories, without adding threads or timers:
caller-controlled completion only queues work, and the controlled driver must
still pump to deliver it. Providers must supply progress and avoid blocking or
reentering the calling peer/session on signaling.

## Owned asynchronous DNS interfaces

`PeerConnectionDependencies::async_dns_resolver_factory` shares an owned
`AsyncDnsResolverFactory`; `async_dns_resolver_provider` constructs one directly.
Both local and production actor creation forward the same dependency. An absent
registration preserves native default selection, including controlled DNS where
supplied by the selected socket factory. Executable dependencies are not
configuration snapshots or mutable SetConfiguration fields.

`AsyncDnsResolverFactoryProvider` implements unstarted Create and both immediate
CreateAndResolve overloads. The latter default to Create followed by the matching
Start; independent overrides are forwarded rather than replaced by that default.
Every product is an owned `AsyncDnsResolver`. Both Start overloads are callable;
the no-family default uses the input address's native family, as does the pinned
stock resolver. Unknown signed address-family values remain expressible.
Products are expected to honor the SDK's single-query lifecycle. If another
Start is nevertheless supplied, the adapter safely supersedes the older ticket
and disposes its captures on the caller sequence. This does not promise portable
repeatability of native implementations.

Product `result()` returns an owned `DnsResolverResult`, not a completion-time
snapshot. It registers a callable `AsyncDnsResolverResult` provider. Clones retain
the same resource; queries continue to observe its current error and family-
selected address. Native result() calls the Rust product accessor, retains the
returned view until product destruction and reuses the native wrapper when its
resource identity matches. Previously returned native references remain valid
for the product lifetime. Error is an exact signed resolver code. Providers must
return the source hostname and port with the selected resolved IP, or None for a
missing requested family. `SocketAddress::clone` and `set_resolved_ip` preserve
these fields without a second address representation.

Factory, product and result implementations need Send, not Sync; bridge mutexes
serialize calls on each registered resource. Native product entry, result
queries and destruction follow the requesting sequence. Factory destruction can
occur on another owning role sequence. Providers must not block or reenter their
calling peer/session. Source owners remain retained throughout native-to-Rust
calls, including capture disposal reentrancy. Panics in factory/product/result
callbacks latch fail-closed diagnostics. Later calls recheck failure after mutex
admission. Factory failure returns a nonnull failing product because SDK callers
dereference factory results; result failure yields -1 or no address. Opaque
provider destruction catches unwinds at the FFI boundary; destructors should
still not panic.

`DnsResolutionCompletion` is a move-only Send + Sync notification ticket. Update
the shared result before consuming it with `complete`. Native notification always
posts to the exact originating `TaskQueueBase`, not merely Thread::Current, and
never invokes inline. Acceptance is not a delivery guarantee. Product retirement
clears its queue and pending callback under the same mutex used for admission;
queued delivery cannot reach a retired product. Callback invocation can delete
the product and uses only retained request state afterward. Old or canceled
handles report Retired, Superseded or AlreadyCompleted as appropriate. The
bridge's native peer ownership destroys products before releasing their role
queues, matching the interface's sequence-affine destruction contract. An
unsupported caller sequence with no native dispatch queue returns
NoDispatchSequence rather than posting through a null pointer. Unsubmitted
capture disposal posts to the origin; if no queue was present, the product keeps
those captures until same-sequence retirement instead of releasing them on a
Rust completing thread.

Controlled registration adds no timers, DNS worker threads or real sockets.
Providers must supply caller-driven progress, and the driver must pump after
completion. The per-peer factory resolves remote hostname ICE candidates;
STUN/TURN server resolution uses the selected packet-socket factory instead.

## Video bitrate allocator factories and products

`PeerConnectionDependencies::video_bitrate_allocator_factory` retains a shared
`VideoBitrateAllocatorFactory`. The provider convenience builder constructs
the same registration. Omission leaves the native per-peer default untouched;
`builtin()` explicitly selects the SDK helper. A caller factory may independently
select `VideoBitrateAllocatorProduct::NativeBuiltin` for an individual product,
without replacing or narrowing the custom allocator contract.

`VideoBitrateAllocatorFactoryProvider::create` receives an owned environment
view and complete `VideoCodecConfiguration`. Codec values retain exact signed
and unsigned fields, kilobit-per-second bitrate units, frame-rate values, all
three simulcast slots and five spatial slots including inactive slots, optional
per-slot formats with arbitrary SDP parameters, scalability mode, and all
VP8/VP9/H264/AV1-specific settings. The environment copy retains the SDK's
resource owners and exposes exact timestamps and immutable field-trial values.
It remains queryable after peer, factory and product destruction.

Products implement `allocate`, `get_allocation`, and
`set_legacy_conference_mode`. Allocate receives the shared `DataRate` state
(including signed finite bits-per-second values and distinct infinities) and
double frame rate. GetAllocation independently preserves both unsigned 32-bit
arguments; its default calls Allocate just as the native interface does.
Legacy-mode changes remain callable after creation, not only metadata.

Results are a five-by-four matrix of optional unsigned layer bitrates and the
native bandwidth-limited flag. Unset and explicitly zero cells remain distinct.
The complete allocation sum must fit native unsigned 32 bits. Overflow rejects
the whole allocation, latches product failure and disables all subsequent
callbacks rather than returning a partial allocation or a native replacement.

Factory and product implementations need Send, not Sync. Bridge mutexes
serialize callbacks, and failure is rechecked after lock admission. Native
encoder setup calls these methods on its worker/encoder path. Providers must
not block or reenter the calling peer/session. Factory callback panics latch
shared registration failure; later creation returns nonnull rejecting products
because native encoder setup dereferences the result. Product panics latch
product failure and return empty allocations or ignore setters. Destruction
contains provider unwinds at the FFI boundary; destructors should not panic.

Controlled registration adds no threads, timers or autonomous progress.
Providers must do caller-controlled synchronous work. Local and production
actor peer creation forward the same owned dependency.

Source review and unrun coverage include both allocation overloads, infinity
and signed-value preservation, inactive codec slots, arbitrary format
parameters, non-Sync factory/products, serialized factory failure, overflow,
panic/destructor containment and actual native video encoder rate consumption.
Builds, runtime acceptance and sanitizer qualification remain deferred.

## Network-state predictor factories and products

Local and production factory builders accept
`network_state_predictor_factory` registrations or the provider convenience
builder. The native peer factory owns the adapter for the complete lifetime
of its borrowed CallConfig predictor-factory references. An absent registration
preserves native omission; no stock predictor is silently substituted.

`NetworkStatePredictorFactoryProvider::create_network_state_predictor` returns
a nullable owned `NetworkStatePredictor` product, exactly like the SDK. Products
remain callable after factory destruction and may hold their own dependencies.
The complete product method is `update(send_time_ms, arrival_time_ms, state)`.
Both timestamps preserve signed 64-bit millisecond values without conversion
to unsigned durations or multiplication into a narrower timestamp domain.
The shared `BandwidthUsage` value preserves the native normal, underusing,
overusing and Last sentinel states, plus unknown signed enum bits. The legacy
event-log name aliases this shared value; event construction keeps its existing
operational-state validation separately.

Factory and product calls are serialized on bridge mutexes, with latched
failure rechecked after admission. Implementations need Send, not Sync.
Factory panics return null future products and set `has_failed`; product
panics leave the incoming computed state unchanged on that and later calls.
No panic crosses the ABI, including provider destruction. Callbacks occur
during native congestion-control processing and must not block or reenter
the calling session. Controlled providers must perform only caller-controlled
synchronous work. Registration introduces no threads or timers.

The pinned header describes this native interface as work in progress and
mentions Initialize in commentary, but declares no Initialize method. The
bridge implements the actual factory and Update declarations only.

Unrun coverage includes the actual native adapter factory/product calls after
factory retirement, signed extreme timestamps, sentinel/unknown states,
non-Sync factory/products, nullable products and callback/destructor panics.
Real congestion-control runtime and consolidated qualification remain pending.

## FEC factories, controllers and protection callbacks

Local and production builders accept `fec_controller_factory` and its provider
convenience method. Absent registration preserves native default selection.
`FecControllerFactory::builtin()` explicitly selects the SDK default; a custom
factory returns an owned callable controller, not merely a policy selector.
Creation receives a retained environment. Factory callbacks are serialized and
need Send, not Sync. Controllers need Send + Sync: the SDK calls encoded-data
updates from encoder sequences as well as rate/protection work on its worker.
No bridge mutex spans controller work, avoiding lock inversion with native RTP.

The complete controller covers callback installation/clear, protection method,
encoding geometry/temporal layers/payload size, rate calculation with the
complete loss vector and signed frame-rate/RTT inputs, encoded-data updates and
loss-vector selection. Encoded frame types include native empty/key/delta and
unknown scoped values. Packet masks cover both declared unscoped native values;
retransmission mode retains signed and unknown integer policy bits.

Protection callbacks are retained expiring capabilities, not owning senders.
Synchronous requests require a live native-controller invocation borrow and
worker affinity, and preserve the native signed return plus all three output
rates. Retransmission mutation requires a live borrow and rejects rate-update
invocations, where the sender already holds its nonrecursive mutex. Async
variants post on the existing native worker, create a new borrow at execution,
and return move-only Send, non-Sync runtime-neutral operations with `try_take`
and Future readiness. Do not await them inside synchronous provider work.
Dropping an operation abandons observation, not an admitted native side effect.
Retirement and task cancellation have separate inspectable outcomes.

Native RTP modules are destroyed before the controller, so affinity alone is
not a safe borrow. Dynamic loans prevent retained synchronous access during
that gap. Peer/resource ownership retains FactoryInner and its worker owner or
controlled driver; pinned peer teardown destroys Call/send streams on worker
before those dependencies can be released. Controller destruction and async
posting share the retirement mutex. Once retired, no queue pointer is touched.
Queued work checks retirement before calling a sender. Thus stale handles never
keep senders alive or access freed workers. Source ownership evidence is in
`src/peer.rs` (FactoryInner/PeerInner), `src/production.rs` (shutdown), pinned
`pc/peer_connection.cc` (worker batch cleanup), `call/call.cc` (stream deletion),
`video/video_send_stream_impl.cc` (RTP sender destruction), and `rtc_base/thread.cc`
(Stop/join); cooperative queue deletion also defers while dispatch is active.
This is a source-lifetime proof, not a runtime or sanitizer qualification claim.

Invalid native packet-mask/rate requests return errors before sender access.
Factory/controller panics latch fail-closed state; an already-entered concurrent
controller callback rechecks failure before accepting its result. Failed rate
calculations return zero and failed mask selection returns false. The SDK's
video-send caller requires a non-null controller, so factory panic yields a
non-null rejecting adapter, never a silently successful stock controller.
Provider destruction and native completion wakes contain Rust panics.

Registration and async dispatch add no native threads, timers, runtime or
caller executor. Controlled progress stays with the caller's existing worker.
Unrun coverage includes native factory/product independence, every controller
payload, signed/unknown values, native callback outputs, sender-lock rejection,
invalid requests, retention/retirement, cancellation, concurrent panic failure,
and real video-path worker dispatch. Builds, runtime and sanitizer qualification
remain deferred.

## Owned network-controller factories and products

`NetworkControllerFactory::from_provider` and the local/production builder
registration paths install an owned native `NetworkControllerFactoryInterface`.
With no registration the SDK still selects stock GoogCC. The pinned
`PeerConnectionFactory::CreateCall_w` only uses an injected factory when the
**factory environment** enables `WebRTC-Bwe-InjectedCongestionController`.
Registration rejects a disabled gate with `InvalidParameter`; it neither
silently ignores the provider nor changes field trials. Per-peer trials do not
control this factory-root gate.

The provider implements Create and GetProcessInterval. Independently owned
products implement all twelve update callbacks and SupportsEcnAdaptation.
These mutable, `Send` providers need not be `Sync`; adapter mutexes serialize
factory and product callbacks independently. Callback work must return
synchronously and cannot wait for reentrant session/actor work. Registration
adds no thread, task queue, timer, runtime or caller executor.

Creation owns the native environment and the complete target constraints,
stream config and default pacing window. Retained environment views do not
retain a peer, controller or factory provider. Product lifetimes do not depend
on the factory provider: native source probes destroy the factory before
invoking the product. Factory failures invalidate its products; a product
failure only invalidates that product. The shared registration failure query
reports either failure without implicitly disabling creation of unrelated
products after a product-only failure.

The public graph includes every pinned network-control message, nested sent
packet/pacing/RTP metadata, ECN markings, feedback vectors, optional fields,
network-state estimates and all update fields. Owned snapshots preserve
native widths, optional zero/false versus absence, signed/infinite time units,
unit infinities, IEEE state estimates and unknown fixed-width ECN values.
Packet-result predicates, native receive ordering, feedback helpers, pacing
rate/window calculations, probe comparison and update presence are exposed.
Checked pacing arithmetic returns typed errors instead of overflowing native
microbits.

Returned updates are validated before the SDK consumes them. Finite unit
sentinels, invalid one-sided magnitudes, negative/minus-infinite congestion
windows, unsafe pacing arithmetic, zero pacing rates (unsafe with admitted RTP),
probe arithmetic/count budgets (including recurring packet overshoot), rounded
zero-byte probe bursts, unsafe queued probe timestamp differences, invalid target
rates/loss/congestion reduction
ratios, and unrepresentable RTT/BWE-period conversions latch typed failure.
Zero pacing windows use the native one-second rate window; padding above data
rate remains valid and is capped by native pacing. Infinite congestion windows
remain valid. A positive-infinite process interval preserves native disabled
periodic processing. Probe byte budgets use the standard PeerConnection root's
1500-byte RTP/FEC/padding ceiling and its accepted transport overhead below 500
bytes. This proof does not cover an independently constructed raw Call or future
caller-exposed MTU setter. Timestamp validation conservatively retains the last
six issued probe times, matching the pinned queue's maximum possible occupancy;
completed probes can make this history larger than the actual remaining queue.

Panics in creation, interval/capability queries, product updates and provider
destruction are contained. Native consumers requiring a non-null controller
receive a rejecting product after creation failure, not a substituted stock
implementation. Rejected callbacks return a finite zero transfer-rate target;
already engine-owned packets retain the last valid pacing configuration.
Clearing pacing while those packets are enqueued would violate native packet
admission assertions.

Unrun coverage includes every callback and nested value, native output
consumption snapshots, factory/product independence, retained environments,
panic/invalid-value failure outcomes, explicit root trial gating, native
feedback/pacing helpers, and an injected-controller video send path. Builds,
runtime and sanitizer qualification remain deferred.

## Video factory declaration, optionality and panic boundaries

`VideoEncoderFactory::implementations` is independent of `supported_formats`,
with the SDK-equivalent default delegating to it. Native GetImplementations on
both the plain provider factory and simulcast wrapper forwards the actual
implementation list. Full-value discovery returns owning SdpVideoFormat values;
it does not infer implementation tags from signaling declarations.

Encoder support-query mode presence survives both Rust-to-native and native-
to-provider conversion. `None`, `Some("")` and a nonempty mode are distinct.
Both native support booleans are preserved independently. The existing legacy
resolution/text DTO limitations are not full-value Query/Create support.

With panic=unwind, video factory/provider callbacks contain panics. Factory
callback panic disables later callbacks on that registration; ordinary Create
rejection does not poison unrelated creation. Product panic disables later
provider operations, including GetInfo, which returns the existing safe failed-
product information rather than calling the partially failed provider again.
Provider state is not claimed to have rolled back atomically.

Factory and product destruction contain destructor panics before they can cross
CXX. Release retires a product exactly once even after an operation panic. If
product destruction itself panics, release returns the binding's failure code
instead of reporting success. Final drop contains the same destructor boundary.
Known standard string panic payloads are freed safely; unknown user payloads are
intentionally forgotten because their destructors or field cleanup can panic.
That safety limitation is not a guarantee of arbitrary user-payload cleanup.
Panic=abort and process-aborting panic hooks cannot be converted into outcomes.

### Owning video factory dispatch

`VideoEncoderFactoryProvider` and `VideoDecoderFactoryProvider` expose complete
owning SDP formats, byte-aware support queries and `VideoCodecCreationContext`.
Their handle constructors are `from_provider`. The encoder's modern default
implementation list delegates its full supported-format declarations. The older
`VideoEncoderFactory`/`VideoDecoderFactory` convenience traits remain separate;
legacy implementation-list overrides retain their existing behavior.

`query_support_value` returns native support and power-efficiency independently.
Absent mode, present-empty mode and arbitrary mode bytes are distinct, as are
absent resolution and signed zero/negative resolution values. The selected stock
query paths do not consume resolution geometry. Encoder fuzzy matching guards
only its reached signed parameter-count and diagnostic mode-naming operations,
then delegates to the actual selected SDK query once. Caller queries do not use
stock-format or positive-resolution whitelists.

Legacy provider conversion failures return `VideoFactoryQueryError`, not a
native unsupported answer. Projection rejects only unrepresentable bytes,
optional format fields, unnamed scalability metadata or signed legacy geometry.
Ordinary projection rejection does not poison later queries. Native-driven
query callbacks have no error return channel; an independent
`VideoFactoryFailureObserver` records their first binding failure. Public owned
queries retain the individual operation's typed error independently of history.

Each peer installation clones the actual selected Rust provider registration's
Arc state and captures that installation's Environment. Create receives an owned
copy of the SDK callback Environment with identity-checked clock/queue retention
and the explicit installation's Rust-only randomness lease. No global lookup,
replacement environment or inferred source association selects these leases.
A codec retains its creation retention through release and final destruction.
The simulcast forwarding encoder additionally owns a stable native Environment
and the exact private plain factory, outliving deferred Create during InitEncode
and all nested encoder teardown. Every pinned virtual method is forwarded;
no forwarding lock wraps SDK calls. Shared provider state is destroyed once.

Generated SDP completion and description observations now cross CXX as bytes.
Non-UTF-8 successful offers produce one `OperationCompleteBytes` terminal on the
peer and production actor rather than a fatal String conversion or invented
native failure. `descriptions_bytes` retains exact snapshots; text convenience
projection reports `DescriptionNotUtf8`. Legacy video capability projection
likewise reports `UnrepresentableCodecCapability` after owned-byte conversion;
complete native RTP capabilities remain available. Byte local/remote SDP setters now call the actual native parser and retain the
same operation admission and controlled-media gate as text input. Native peer
operation/ICE error messages also cross as bytes; non-UTF-8 errors retain their
kind and content in OperationFailedBytes/IceCandidateErrorBytes, forwarded by the
production actor without a duplicate terminal. Text input remains a convenience
projection. Migrated native malformed-byte input/close/terminal fixtures are now written in
tests/production_sdp_bytes.rs but unrun. That invalid first line returns an ASCII
native parser explanation; it is not a fixture for a byte error-message callback.
Remaining native consumers, deterministic byte-error fixtures and complete JSEP
resource access are not claimed finished.

The source regression files `tests/video_factory_values.rs`,
`src/video_factory_values_tests.rs` and `tests/sdp_bytes.rs` are written but unrun.
Static review found no definite defect in the factory ownership/dispatch slice;
it is not compilation, runtime or artifact evidence. Public standalone video
codec products, installed caller selectors and remaining consumer proofs are
still obligations.

### Explicit actor codec-factory selection

`ProductionSessionBuilder` now accepts shared audio encoder/decoder and video
encoder/decoder factory handles. `None` preserves the existing encoded-Opus
carrier and selected Opus decoder defaults; an explicit builtin/caller factory
is passed to the normal native factory builder without substitution. Modern
video-provider conveniences are available on local and production builders.
Provider constructors retain their typed construction failure before build.
Factory registrations are thread-safe shared selections, not imported local
codec/peer products. The actor remains move-only Send and non-Sync.

Compressed-input conveniences still require their actual selected carrier. An
explicit stock audio encoder therefore makes `create_opus_source` reject rather
than silently changing factories. A configured private encoded-video input and
an explicit encoder factory are conflicting producer registrations and reject
before construction; an explicit decoder remains an independent selection.
Generalized per-source encoded-media routing remains a separate unfinished task.
Caller audio providers, device/mixer/processing/frame processors, role-thread
inputs and the remaining actor transport graph are not claimed complete.
tests/production_codec_factories.rs contains unrun installation/migration/drop
fixtures for these choices.

### Owning decoder values for standalone products

`OwnedVideoDecoderSettings` and `OwnedVideoDecoderInfo` hold independent actual
SDK value copies, not executable codec products. Copy construction and
`clone_from` call native copy/assignment. Settings observations retain signed
render dimensions, optional signed pool size, cores and the native codec enum.
The setter rejects only an out-of-range non-fixed enum cast or the reached
non-positive-core DCHECK in the matched artifact, atomically before mutation.
Unnamed enum values 6/7 remain independent metadata; Configure admission for a
selected product is separate and is not provided by these value checks.

Info names and actual SDK `ToString` output use bytes. Native equality includes
both name and hardware flag. These values may leave their owner or actor and
cross threads, since they retain no executable engine graph. The older
`VideoDecoderSettings` and `VideoDecoderInfo` provider convenience DTOs remain
separate narrower projections. tests/video_decoder_values.rs is source-only
coverage, unrun. `VideoDecoderFactoryHandle::create_decoder` now calls the actual selected SDK
Create. `OwnedVideoDecoder` retains the exact Environment/Rust-only leases,
private factory adapter and selected factory through native teardown. It is
caller-sequence non-Send/non-Sync, not actor-importable. GetDecoderInfo returns
an independent full byte-preserving SDK value; the separate deprecated
ImplementationName method preserves its actual SDK default. Release forwards
native statuses without caching them. Native null and the reached unsupported
format's unsafe diagnostic naming are separate creation errors, with no stock
fallback. tests/video_decoder_product.rs writes unrun ownership/context/native
VP8/null/drop fixtures. Configure/Decode, complete callbacks and the owning
encoder product graph remain open under #8.

### Standalone caller selectors

`OwnedVideoEncoderSelector` is a move-only Send, non-Sync zero-refcount SDK
unique product. `into_shared` consumes that ownership and establishes the first
SDK reference. `VideoEncoderSelector` aliases share one stateful Send provider
with serialized native callbacks; aliases can be used from different threads.
Both owner forms call all four actual SDK virtual methods and preserve complete
owning format/render inputs and selected formats. Standalone calls execute
synchronously on the caller, not on a fabricated encoder queue.

An independent first-failure observer distinguishes legitimate native no-switch
from callback panic/reentry/destructor failure. Same-registration recursion is
rejected before locking; failure publication precedes releasing serialization.
Original panic payloads are destructed inside a second containment boundary.
Only an arbitrary secondary non-string panic payload is forgotten to guarantee
termination; arbitrary malicious payload cleanup is not promised. Finite rate
sentinels and compiled one-sided negative-rate assertions are distinct input
errors; native infinity states and release-artifact signed finite rates remain
intact. Panic=abort and process-aborting hooks are outside unwind containment.

`tests/video_encoder_selector.rs` contains unrun owning/callback/domain/drop/
reentry regression sources. These standalone resources are not real sender,
factory or actor selector installation.

### Caller audio factory consumer prerequisite

Owning stock audio discovery/query/products already exist. Installing a caller
factory additionally exposes its discovery/query outputs to codec collection,
PT assignment, SDP and send configuration. In particular the pinned voice
engine re-queries a selected send format, DCHECKs presence and dereferences
optional info. Callback rejection/panic/stability needs a sound installed-path
contract before arbitrary providers can be admitted; caller factories are not
claimed installed. Typed absl::Format formatting is not C-varargs printf, and
byte metadata does not by itself justify a UTF-8 or size_t-format whitelist.

`OwnedAudioEncoderOptions` now owns actual SDK Options with default/copy/
assignment, unrestricted signed payload fields and exact optional pair-ID
copy/set/clear. This prepares owning callback options but is not an installed
caller factory. `AudioEncoderFactory::create_with_options` now passes this
actual SDK value into the selected factory's real Create, reusing stock
operation-specific admission without projecting through Rust fields. Fixtures
cover G711 signed payloads, actual encoded metadata, source-option drop, migrated
product use, native unsupported results and distinct Opus/carrier admission.
They remain unrun.

`AudioCodecInfo::new` / `with_bitrate_range` now delegate the distinct SDK
constructors with matched-artifact DCHECK guards, rather than unconditional Rust
metadata restrictions. Native constructor defaults and raw release metadata
remain observable independently of consumer safety. Profile/extreme constructor
fixtures remain unrun.

`AudioCodecPairId::copy_value` now makes an independent actual SDK identity copy,
without consuming another generated ID, for exact owned creation-option values.
`AudioCodecInfo::has_fixed_bitrate` now calls SDK HasFixedBitrate, guarding only
its reached bitrate DCHECKs in the matched artifact. Release artifacts preserve
the direct comparison; standalone rate/channel metadata is not narrowed.
Regression sources are unrun. These prerequisites do not complete caller audio
factories/products, peer/actor provider installation or qualification.

### Standalone caller audio dispatch and transfer preparation

`AudioEncoderFactoryRegistration` and `AudioDecoderFactoryRegistration` now own
mutable `Send` providers. Discovery, query/support and named Create entries are
serialized across caller threads. Same-registration same-thread reentry returns
a typed per-call error before acquiring the callback mutex. Ordinary provider
rejection does not disable unrelated operations; a panic disables subsequent
callbacks and retains the original first failure. Destruction occurs only after
the last registration/callback owner, contains provider-destructor panic, and
reports through an independently retained failure observer. Contained panic
payloads are deliberately not dropped, since their user-defined destructors may
panic again. Providers may not wait for or reenter their owning actor.

Creation inputs own the supplied Environment, byte-preserving format and actual
SDK encoder Options/pair-ID copies, including Rust-only environment leases.
Create does not implicitly query, prefilter, cache support or impose a format/
rate/channel policy. Mutable standalone queries can return unsupported after an
earlier success. Decoder overload defaults preserve both native delegation
directions: modern-to-compatibility with absent ID and compatibility-to-modern
ignoring ID. Leaving both defaults unimplemented reports construction failure
instead of entering their native recursive default path.

These registrations return existing owning native products. Both encoder and
decoder dispatch now also have the per-operation real SDK factory drivers
described below. **Neither registration is a persistent SDK
factory handle, caller-implemented audio product or installable peer/actor
dependency.** That graph remains open, and installed query handling still needs
#37. Fixtures for byte/extreme metadata, real G711 creation/encoding,
context retention/migration, overloads, serialization, reentry, rejection and
panic/drop behavior are written, not executed.

Internal consuming transfer preparation uses Arc::try_unwrap, refusing aliased
products without replacing their backend. Decoder preparation also gates native
shared-State/parsed-frame owners. Successful preparation keeps the complete
NativeAudioEncoder/Decoder wrapper (mutex, backend, environment and provenance)
and Rust Environment lease together, with backend destruction preceding lease
destruction. It does not use fresh-decoder detachment. Native readiness is only
handoff preflight, not an SDK adapter or a proof of consumer admission. Native
forwarding must preserve actual backend guards and never infer an Opus concrete
type from a caller format name. Ownership/packet-state/native-frame fixtures
remain unrun.

Native decoder product forwarding is now source-wired: consuming transfer owners
produce an actual SDK-derived AudioDecoder. Every decoder virtual, including
SDK-internal compatibility entries, forwards to the original backend under its
original mutex. Parsed frames retain their actual SDK frame and transfer owner,
with decode-once enforcement and destruction before backend/lease release. Nested
forwarding layers share an actual live-frame counter; readiness and fresh
extraction reject those owners as well as shared State owners. Byte payload,
timestamp, priority, duration, DTX and optional decode results are not rewritten.
Safe Rust still uses owning frames and the original stock-origin buffer bounds;
no public raw Decode/DecodeRedundant/DecodePlc method is added. Raw SDK consumers
retain their native caller-buffer contracts, which are not proved for arbitrary
installation by this adapter alone.

Fixtures compare real forwarded G711/L16/G722 decoders to real original products,
exercise nested frame retention after outer-owner drop, and isolate an ignored
process-global randomness-lease case. That ignored case must be explicitly run
alone in consolidated qualification. Static review found no concrete signature,
frame-counter or lease defect, with a bounded reentry caveat for virtual/destructor
calls under the non-recursive source mutex. No SDK callback reentry was established
for the selected stock path. No runtime result or complete native factory graph
is claimed. Persistent native factory integration, caller product implementations
and peer/actor consumer admission remain open. Encoder product forwarding and
per-operation factory dispatch are described below.

`AudioDecoderFactoryRegistration` now also exposes `supported_decoders_via_sdk`,
`is_supported_decoder_via_sdk`, `create_via_sdk` and the distinct
`create_with_codec_pair_id_via_sdk`. Each operation owns a real refcounted SDK
AudioDecoderFactory adapter and an explicit Rust registration/environment/error
receipt, rather than a registry or cached discovery/query. Its virtual methods
invoke the serialized provider. SDK Create receives the actual supplied native
Environment and optional pair identity and returns the actual SDK-derived
consuming envelope. The envelope retains the original returned backend, mutex,
provenance and Rust lease, even when a provider uses another environment or
returns a product unlike its request/discovery metadata. No original backend is
detached, cloned or selected from format names. Shared products/frames return
ProductNotUnique; an envelope invariant failure is ProductTransferFailed.

Callback failures are recorded separately from SDK's required empty/false/null
projection and surfaced as typed per-operation errors to Rust. Ordinary null
remains Ok(None). This is not permission to install those error projections in
unproved SDK consumers. Native Create never prefilters via discovery/support or
falls back to stock. Decoder overload defaults remain the caller trait's exact
named delegation, including absent identity. The temporary SDK factory is retired
after dispatch; returned products/frames own their resources independently.

New SDK factory fixtures cover full byte/extreme metadata, actual product-origin
retention, context/migrated frame lifetime, exact overload/callback order and
optional identity, ordinary null versus rejection/panic/disabled errors, and
shared-product refusal with original-product survival. Scoped static review found
no concrete defect in this path; fixtures remain unrun. Persistent factory/builder
wiring, caller-defined products and installed consumer admission are still open.

### Encoder SDK product forwarding

Consuming encoder transfer owners now produce an actual SDK AudioEncoder subclass.
Encoding passes the existing caller Buffer and original EncodedInfo through under
the original mutex. Getters/ranges/stats and admitted controls use that same
backend. Forwarded owning state resolves to its directly retained original, so
Opus multiplier/clock/overhead guards and internal concrete-type admission never
mistake a proxy or format label for the actual Opus implementation. Nested
forwarding preserves the exact owner chain and Rust environment leases.

A durable atomic AudioEncoderSdkFailureObserver reports the first rejected SDK
void control without retaining the backend. Failed bitrate/bandwidth/allocation/
loss admission does not mutate native state or clamp input. Direct Rust controls
retain their own Result behavior. Recoverable-loss default is not invoked;
Opus ANA enablement records UnsafeAdaptor and returns false until #11/#12 are
implemented, rather than claiming native lack of capability. ReclaimContainedEncoders
is explicitly not called/overridden. These gates, encoder factory dispatch,
caller-defined products and installed consumers remain open scope.

Fixtures for exact buffer prefixes/native packet metadata, buffered state across
nested transfers, original Opus safety predicates and observers after destruction
are written, not run. Bounded static review found no actionable forwarding defect;
that does not qualify every backend control, ABI or installed media behavior.

### Encoder per-operation SDK factory dispatch

`AudioEncoderFactoryRegistration` now adds `supported_encoders_via_sdk`,
`query_encoder_via_sdk` and `create_via_sdk`, using a real refcounted SDK factory
per operation. Full byte/scalar/map metadata round trips without voice-engine
admission or stock parser restrictions. Mutable Query preserves current Some/None;
callback failure has a typed receipt, not unsupported substitution or cached info.
Create does not Query, normalize options or infer product type. Actual SDK Options
are copied into owning callback storage, preserving signed payload and exact
optional pair ID. The actual SDK Environment and Rust leases are retained.

Unique returned native encoders become privately constructed RetainedAudioEncoder
envelopes. Factory Create moves only that envelope; original backend/mutex/Opus
provenance/clock/overhead and environment remain directly owned. The subsequent
owning wrapper resolves guarded state to that exact origin. ProductNotUnique and
ProductTransferFailed are explicit creation failures, not native null success.
Ordinary provider None remains null. No stock fallback or guessed association is
introduced. Typed SDK-control observers remain independent of factory receipts.

Fixtures cover raw metadata and mutable query success-to-unsupported, exact
options/pair/environment after retirement and migration, no implicit prequery,
actual PCM returned for unlike Opus-named requests, full signed payload output,
ordinary null/rejection/panic/disabled outcomes and refusal of shared encoders
without losing original packet accumulation. Scoped static review found no
concrete defect; all fixtures remain unrun. These are not persistent factory
handles or peer/actor-installed dependencies. #37, caller-defined products, ANA
admission and whole-goal qualification remain unfinished.

## Standalone caller encoder products

`AudioEncoderProvider` and `CallerAudioEncoder` now supply owning serialized
fallible caller product dispatch, full named getter/control methods, exact 10-ms
shape and owned byte/redundancy validation, typed per-operation failure, callback
panic disabling and a retaining environment/destructor observer. Defaults retain
native target/allocation bandwidth delegation through actual SDK unit getters,
with artifact-local assertion guards, not unconditional infinity rejection.
The asserting native recoverable-loss default is represented as typed failure.

This product is intentionally distinct from the existing owning native
AudioEncoder. Factory providers still return only native owning products; no
caller Encode adapter or safe peer installation is claimed. SDK Encode has no
recoverable failure return, and ACM distinguishes buffering using genuine
metadata. #38 records the explicit Encode failure-channel/refusal obligation,
separate from #37 mutable Query admission and ANA #11/#12. Full ownership and
failure details are in `docs/audio-encoder-contracts.md`; fixtures are unrun.

## Standalone caller decoder/frame products

`AudioDecoderProvider`, `AudioDecoderFrameProvider` and owning
`CallerAudioDecoder`/`CallerDecodableAudioFrame` now supply complete named modern
standalone dispatch. Parse consumes owned bytes and preserves exact timestamp,
nonnegative priority and nullable frame ownership. Frames retain the decoder and
actual environment; one decoder mutex serializes decoder/frame callbacks and
retirement. Last-frame drop inside a same-decoder callback is deferred under that
mutex. Clones share a single Decode attempt, including None, Err and malformed
output; pre-dispatch capacity refusal is retryable. PCM and PLC are owned real
output, checked against explicit caller bounds, never stock-size guesses or
silence/None-on-error substitutes. PLC preserves the prefix and native no-output.
Callback panic disables the shared product; typed first-failure and transfer
observers survive retirement and binding.

The public legacy exclusions remain in force. SDK-internal DecodeInternal and
legacy compatibility still require a genuine implementation, not a stub.
Existing stock parse/frame/PLC admission bounds cannot certify arbitrary caller
DSP. #39 tracks those compatibility, capacity, arithmetic and NetEq-consuming
obligations. These standalone products are intentionally not SDK AudioDecoder
handles and cannot be returned by the native factory Create contract. Detailed
contracts are in `docs/audio-decoder-contracts.md`; real PCMU-delegating fixtures
and all new source remain unqualified. Scoped source/fixture and observer-delta
reviews found no concrete defect; they are not test/build/ABI results.

The caller frame now additionally exposes real standalone SDK Duration,
IsDtxPacket and Decode virtual drivers. Each genuine SDK EncodedAudioFrame leaf
retains the caller lease, uses the original serialized/single-shot state and an
independent error receipt, and copies validated owned output into an explicit
caller PCM span. Before-copy guards retain count/tail and exact native speech-enum
cast domain [0,3], not a named-tag-only whitelist or stock output bound. Getter
placeholders and nullopt-on-error are refused as public success by the receipt.
This leaf does not instantiate a native caller decoder or stub legacy virtuals;
#39 installed decoder/NetEq admission remains open. Fixtures are unrun.

CallerAudioDecoder now also has named standalone real SDK decoder virtual
drivers for metadata, reset, packet duration/redundancy/FEC, ParsePayload and
bounded GeneratePlc. Temporary SDK AudioDecoder owners retain the actual caller
state; exact private frame provenance moves original Rust leases through native
ParseResults before every native leaf retires ahead of its native decoder. Fresh
receipts prevent errors appearing as scalar/empty/void success. Prefix and explicit
append bounds are checked before mutation. With explicit human approval, the
mandatory internal legacy method deliberately returns negative, writes nothing
and records typed UnsupportedCompatibility; actual SDK redundant/default legacy
PLC behavior remains. This is not internal CNG DSP or installed parity and adds
no public Rust legacy entry. Source/fixtures are unqualified; #39 remains open.

## Persistent standalone caller SDK factories

Both audio factory registrations now expose `sdk_factory` returning distinct
`CallerAudioEncoderFactory`/`CallerAudioDecoderFactory` owners. Each retains one
actual refcounted SDK adapter and its registration; handle clones share one
instance, separate constructors share only the mutable registration. Construction
runs no callbacks. Full discovery/query/Create operations use fresh scoped call
contexts and receipts, actual environment/options/pair identities and existing
unique retained native products. There is no Query cache, prefilter or fallback.

A per-registration SDK scope rejects same-/cross-handle reentry before the handle
mutex; waiting calls recheck panic disabling. Native CallScope borrows/restores the
operation request synchronously, never retaining last-call environment/receipt.
The SDK owner and registration retire independently of returned codec/frame
owners. Raw C++ interop callers must preserve owner serialization and matching
request lifetime; safe Rust owners enforce them.

These are not selected AudioEncoderFactory/AudioDecoderFactory dependencies;
builders and peer/NetEq graphs still cannot install them. #37/#38/#39 remain open,
and native caller DSP products are not returned through the Create contracts.
Scoped source and fixture reviews found no concrete defect; exact encoder pair
identity and distinct-handle concurrency coverage were added after review.
Fixtures remain unrun. Details and evidence limits are consolidated in
`docs/audio-factory-sdk-contracts.md`.

## Remaining inventory

The pinned declarations are `api/environment/environment_factory.h` and
`api/peer_connection_interface.h`. Selected built-in implementations are not
substitutes for caller-supplied interface implementations.

| Creation leaf | Current bridge | Remaining caller-provider work |
| --- | --- | --- |
| Environment field trials | Owned values and custom `FieldTrialsView` | None for this provider |
| Environment event log | Owned custom `RtcEventLogger` | None for this provider |
| Environment clock | System/shared manual clocks, owned custom Clock, signed/wrapping SimulatedClock, and matched-anchor real-time conversion | None for this provider |
| Environment task queues | Default/cooperative built-ins, owned custom factory/scheduler products, native delay domains/precision and current-queue reference posting | None for this provider |
| Factory role threads | Selected production and controlled owned threads | Review public socket-factory injection and thread dependency seams |
| Factory packet sockets / network manager | Endpoint-bound controlled providers | General caller-provided interfaces |
| Factory network monitor | Owned custom factory and monitors | None for this provider |
| Factory event-log factory | Owned custom and selected built-in factory | None for this provider |
| Factory network-state predictor | Owned factory/products and signed Update payload | Consolidated runtime qualification pending |
| Factory FEC | Owned factory, complete concurrent controller and expiring synchronous/asynchronous protection callbacks | Consolidated runtime qualification pending |
| Factory network controller | Owned complete factory/products and network-control message/update graph with explicit pinned trial gate | Consolidated runtime qualification pending |
| Factory NetEq | Full guarded stock and serialized caller products; standalone owned factory/context creation with typed failure and exact SDK input retention; complete value/frame graph | Peer registration and complete SDK consumer arithmetic proof outstanding; qualification deferred |
| Factory SCTP fake injection | Pinned test-only opaque factory/internal-transport seam; native connection context selects stock SCTP | Excluded as test-only; public SCTP transport info/observers remain application-surface work |
| Factory encode/decode metronomes | Owned optional providers, exact TimeDelta periods, worker-posted one-shot tick tickets and retirement | None for this provider |
| Factory audio/media providers | Selected ADM/audio codecs/processing and custom video factories; stock audio discovery/Query/Create/products and complete values; standalone serialized caller audio factory dispatch, per-operation real SDK encoder/decoder factory Query/Create, owned contexts and consuming product forwarding; standalone fallible owning caller encoder and modern decoder/frame products; persistent standalone caller SDK factories; real standalone caller SDK frame/decoder virtual drivers with explicit output bounds and approved typed legacy refusal | Native caller audio products and safe factory/consumer installation; audio devices/processing, builders, mixers and frame processors; qualification pending |
| Factory media enablement | Native `EnableMedia` | Keep implementation-detail MediaFactory distinct from public provider interfaces |
| Peer observer | Existing native event adapter | Continue observer coverage separately |
| Peer field trials | Owned custom override | None for this provider |
| Peer TLS certificate verifier | Selected TURN CA verifier and owned custom Verify / VerifyChain | None for this provider |
| Peer async DNS | Owned factory/product overloads, live callable result resources and origin-queue completion | None for this provider |
| Peer allocator, ICE/DTLS/RTP transport | Not generally exposed | Full owned factory/product interfaces |
| Peer local-network permissions | Owned factory/products, query overrides and asynchronous completion | None for this provider |
| Peer certificate generator | Owned asynchronous provider, reused key/certificate resources and signaling-posted completion | None for this provider |
| Peer video bitrate allocator | Owned factories, callable products and full codec/allocation values | Consolidated runtime qualification pending |

The NetEq source inventory and legacy decoder capacity/lifetime constraints are
consolidated in `plans/completion.md`. Selected builtin decoder factories
now expose metadata, capability queries, both distinct Create overloads and
native-generated owning `AudioCodecPairId` values. Decoder products expose
ParsePayload, decode-once owning frames, GeneratePlc, Reset, ErrorCode and packet
queries; frames retain both their decoder and environment. Stateful decoder and
frame calls share one native mutex. Frame buffers use the pinned stock backends'
proven 120-ms maximum per channel, not a generic custom-decoder guarantee.
Legacy public Decode/DecodeRedundant and HasDecodePlc/DecodePlc are deliberately
omitted by the recorded human scope decision; SDK-internal compatibility paths
remain unchanged. Standalone caller decoder dispatch is now exposed; native
native caller product adapters and safe factory/consumer installation remain
open; per-operation and persistent standalone real SDK factory dispatch plus fallible
modern caller decoder/frame products are source-wired above.
Encoder factory discovery now calls the actual prioritized GetSupportedEncoders
list, including the Opus carrier's single canonical RTP identity and mono codec
metadata. Owning encoder values now cover Create options/native pair identity,
packet and redundant-leaf metadata, newly appended bytes, all optional ANA
statistics and native frame-length/bitrate ranges. These are prerequisites, not
complete caller encoder factory/product implementation. Stock Query/Create and
product controls and standalone fallible caller encoder products are source-wired,
while native caller factory/product adapters and safe installation remain open.
#37, #38 and #39 are not resolved by standalone dispatch.
New tests are written but not executed, with builds/runtime qualification deferred.
Decoder support queries return `Result<bool, CodecError>`: native unsupported
results remain `Ok(false)`, while an error prevents the stock parser's aborting
channel conversions or overflowing multiopus stream-count sum. Ordinary Opus resolves before irrelevant multiopus
parameters, and the Opus-only factory's non-Opus name short circuit is preserved.
These parser guards are specific to the currently selected SDK built-ins, not a
policy for future caller-provided factories.

The shared NetEq graph now includes complete owning `RtpHeader`/extensions,
video timing/playout/HDR/ColorSpace values, `RtpReceivedPacketInfo` and cheap-copy
`RtpReceivedPacketInfos`. The new owning native `AudioFrame` is distinct from PCM
convenience types. It exposes all metadata and native reset/update/copy/mute,
layout/rate/capture and sample-access operations, with operation-local guards
and initialized full backing storage. It is Send but not Sync. Native shape
and enum bounds use this SDK pin; writable public-field shapes are not silently
repaired into successful audio. The full guarded owning stock NetEq product is
now exported. It retains the actual environment and decoder factory, preserves
lazy registration/native normalization, and refuses unsafe construction,
decoder handoffs, prospective RFC3389 speech extraction and queued-packet
RemoveAll without a hidden flush or reset. Durable ClockTimeBounds and the
shared widened receive-time proof guard both Rust overloads and native virtual
insertion without extra clock reads. Regression and boundary tests are written
but unexecuted; independent read-only review is not qualification.

Standalone caller NetEq products and factory creation are now wired in source.
`NetEqFactoryProvider::create` receives an owned `NetEqCreationContext` with the
actual copied SDK environment/config and optional exact refcounted decoder
factory. `NetEqFactory::create` returns a complete non-stock caller product or a
typed `NetEqFactoryError`. Calls are serialized, so mutable providers require
Send, not Sync. Context clones outlive callbacks and original owners. Product
owners independently retain the environment's native dependencies and Rust-only
process-randomness lease. Factory and product failure observers remain usable
independently after destruction. Ordinary rejection does not disable unrelated
creation; a factory panic disables future callbacks on that registration, and
factory/product destructor panics are contained.

Peer registration remains gated. The pinned receive-channel path assumes
installed Create returns a non-null product. Native factory failure dispatch
constructs an explicit non-stock failed product: packet/audio operations fail
without touching outputs, formats/timestamps/NACK are absent and statistics and
delays are zero. Its SDK-specific constructor provides a mixer-safe last output
rate; standalone NetEq::failed retains its separate raw getter domain. Factory
absence, rather than construction failure, is the only stock-selection path.
No peer builder currently installs the new factory. The native adapter implements
Create, but public standalone creation uses shared input/creation dispatch to
retain standalone semantics and expose typed construction failures.
GetAudio frame adaptation now has a lifetime-bound AudioFrameMut view and an
internal borrowed native wrapper, without pre-call initialization or copy-back.
SDK buffers are not fully initialized: reads and mutable slices require an
explicit written-prefix proof, or the relevant native mute-clearing behavior.
The view cannot own/free/escape the frame or cross threads. Standalone callback
entry and internal SDK frame/rate/decoder guards are wired; full SDK admission is
not proved. Exact decoder-factory provenance gates capacity-sensitive operations
on retained contexts. Source review found no enforced video-delay bound that
would make an audio-only delay interval prove StreamSynchronization's signed
arithmetic safe. The human explicitly excluded custom NetEq installation into peers and
`ProductionSession` rather than authorize SDK patches or an unsafe opt-in. This
is an approved parity exclusion, not a proof that the combined native consumer
is safe. Standalone factories/products and stock peer selection remain required. Detailed synchronization formulas and
pinned source anchors are consolidated in `plans/completion.md`. Shared graph, frame/view,
stock/caller product and factory tests remain unexecuted.

Native `RtcConfiguration` value coverage is separate from this inventory.
Remaining provider work must preserve owned dependencies, native callback and
retirement sequences, native-supported state distinctions, and actor-safe
thread contracts without silently substituting a selected implementation.
