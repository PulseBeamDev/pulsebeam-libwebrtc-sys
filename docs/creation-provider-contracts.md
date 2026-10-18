# Native creation-provider contracts

This records implemented ownership paths and the remaining pinned dependency
inventory. It is not a claim of complete creation-interface coverage.

Execution status: the final Linux x86_64 non-device core/native and ASan
core/native suites passed, along with source/package checks. Exact counts,
completed evidence, limitations and the unresolved live-capture scope are in
[the verification results](provider-verification-plan.md#final-results).
Historical source-delivery notes below saying fixtures were unrun are superseded
for the default non-device cases by those results, not for ignored live or
prerequisite-dependent gates. Unsupported/unwired interface limitations remain
unchanged; a passing suite does not implement those missing seams.

## Opt-in native capture source progress

`ProductionSession` now owns camera and desktop captures in its private source
map under opaque SessionSourceId values. Discovery, stopped camera creation,
explicit camera start/stop, desktop frame requests, capture status and control
operations do not export local handles. Existing publish_video and
replace_video_source accept these actual captured video sources. Source close
and session shutdown stop capture before releasing source/factory role owners;
ordinary actor migration keeps the same native objects and SDK role.

Native capture creation, mutable operations and final destruction now run on
the factory's existing signaling role. In particular, V4L2's api_checker sees
the same sequence after actor migration. No capture executor is introduced.
Control calls reject provider callback/forbidden role reentry before a blocking
hop. Physical capture still rejects controlled execution before platform entry.

Wayland desktop capture now pushes an isolated GLib thread-default context for
SDK creation/control and dispatches its ready GIO work on that same existing
signaling role. Weak delayed tasks provide bounded dispatch batches without
retaining the actor or adding a capture executor. It never pumps the application's
global GTK context on an SDK thread. Close first destroys the capturer, whose
SDK PortalGuard invalidates late portal access and whose stream stops its native
callbacks, then drains currently ready context work and retires the weak pump.
Repeated observer enablement is idempotent rather than reaching the SDK's
non-null/non-null Observe DCHECK.

**Pinned SDK limitation, no libwebrtc patch:** the portal API exposes no count or
completion fence for canceled asynchronous GIO operations. Draining ready work is
not proof that future I/O completions have arrived. SDK-only GTask/context/guard
resources may remain pending after early close; complete deterministic native
GIO reclamation is not promised or qualified. These late callbacks must not own
or invoke released Rust receivers or access the destroyed capturer. Do not hide
this limitation behind a guessed timeout, a private-layout refcount probe, or
pumping unrelated application GTK callbacks. Final qualification must distinguish
this native cleanup limitation from Rust ownership and native pointer safety.

CameraSettings preserves every VideoCaptureCapability field, including signed
width/height/FPS, scoped pixel-format bits and interlace. Complete capability
queries, orientation and native best-match index/settings are distinct from the
legacy filtered CameraFormat convenience view. Device product IDs and desktop
display IDs are preserved. CameraControlError retains signed native result codes.
Creating a stopped camera and then start_device forwards complete settings
without a bridge-selected best-match substitute; the SDK/backend still performs
its own native negotiation. CaptureSettings is the SDK output, not guessed actual
hardware geometry. stop_device does not terminally end its source; stop does.
Rotation, apply-rotation, current-device-name and capture-started operations call
the real SDK methods. Ordinary false results remain false. Status reports
explicit device stop separately from failure and unavailable control contexts.

Desktop controls forward source-list/select/focus/occlusion, excluded windows,
maximum frame rate, resolution, delegated-selection presence/visibility and
native selection-observer clear/restore. Visibility calls are requests, not
claims that UI appeared. Unsupported delegation is distinct from ordinary false.
With observation disabled, selection status retains its last notification.

DesktopCaptureOptions now owns a real SDK value, choosing the empty/default
constructor and either the actual selected Environment or SDK global clock.
Its clock getter owns a delegate with independently retained leases and checks
the native options clock's identity, not a configuration echo. Clock delegates
record the actual SDK identity separately from their safe forwarding adapter.
Captures retain SDK copies of the options/resources and use that options clock
when stamping desktop video frames. Flags and PipeWire dimensions/damage policy
are read and written through SDK accessors. X11 display and screencast stream
leases support actual create/get/share/clear and native identity comparison;
failed display connection is None, not a fallback. These local resources are
role-bound. Actor options use opaque SessionDesktopOptionsId values, never local
handles, and are cleared only after capture shutdown. The explicit screen/window/
generic creation path uses the selected SDK factory with no fallback and forwards
optional source/restore-token selection without source-list filtering. Existing
legacy open helpers keep their prior policy. SDK Wayland detection is exposed.

RawCameraSinkHandle installs the SDK raw callback overload, preserving mutable
borrowed bytes, complete settings, rotation and capture time; snapshots own their
bytes. The signed callback return passes through, but the pinned SDK ignores it.
Registering uses actual DeRegisterCaptureDataCallback then RegisterCaptureDataCallback:
this is deliberately non-atomic, and frames in the gap may be discarded. The SDK
api_lock_ fences old callbacks before retirement. None restores the binding-owned
video sink; raw mode bypasses video-source publication. Panics quarantine the
receiver and contain hostile payload/drop behavior. Reentry is rejected before
blocking. A focused source review found no concrete defect in this path. A
separate options/role/clock/actor review also found no concrete source defect.
These reviews are not build/runtime evidence.

CameraDeviceInfo is now a stable SDK product, not a fresh enumeration snapshot.
It retains its factory/signaling role across local use or private actor IDs and
preserves NumberOfDevices, name/optional-product buffers and their caller-selected
capacities, capability count/index lookup, orientation, best-match index/settings
and native errors. Close and destruction release the actual SDK product on its
role. SDK null creation remains None, distinct from a product with zero devices.
Device name bytes exclude only the C terminator; no UTF-8 repair or truncation is
introduced by conversion. The real Linux settings-dialog member returns native
-1 and displays no dialog; no platform window pointer is exported. Implementation
CreateCapabilityMap is not a public DeviceInfo root member.

The former #12 missing desktop progress/serialization owner now has the isolated
context implementation described above. Its native cancellation-drain limitation
is explicitly separate from implemented dispatch and application graph safety;
neither runtime nor leak qualification is claimed.

An additional pinned camera-session issue was found while reconciling the
remaining VideoCaptureOptions implementation. PipeWireSession::Cleanup holds
callback_lock_ while StopPipeWire calls pw_thread_loop_stop; that loop can be
inside Finish waiting for the same callback_lock_. Cleanup can therefore join a
thread blocked by its own lock. The public options/session API exposes no loop
quiescence or shutdown fence to repair that ordering in the binding. Backend
private-field access and libwebrtc patches are not permitted. The remaining
options work must distinguish this unsafe PipeWire session path from feasible
V4L2 options/Init/factory overloads, preserve configured flags, and never replace
a selected PipeWire camera with V4L2 or manufacture an SDK Init result.

CameraCaptureOptions now owns the real SDK value on its retained signaling
role. Empty construction and SDK copies preserve actual allow_v4l2/allow_pipewire
accessors. Descriptor setters own a duplicated input FD and clearing removes that
lease; the SDK has no descriptor getter, so none is invented. V4L2 initialization
uses the actual SDK Callback virtual and reports its observed notification:
UNAVAILABLE when disabled and SUCCESS when enabled are native initialization
answers, not proof of physical hardware. Optional user observers are serialized,
panic-quarantined and independently retainable. Selected PipeWire initialization
is rejected before SDK entry with a binding UnsupportedOperation for the native
cleanup hazard above, with no fake SDK callback/status and no fallback. Actor
options now use SessionCameraOptionsId values for create/copy/settings/descriptor/
Init/release. Both actual SDK options-taking Create and CreateDeviceInfo overloads
are wired, rejecting unsafe PipeWire selection before creating a source or
contacting a device. Returned camera/DeviceInfo products retain independent SDK
option copies and owned descriptor leases after the original options are closed.
Native null DeviceInfo remains None. Disabled backend flags are not substituted
with the SDK's no-options overload.

Pinned Linux limitations: FullScreenWindowDetector has no Linux capturer
consumer and defaults are created only for macOS/Windows, so custom detector
installation is not claimed. Other-platform-only options stay outside scope.
Raw screen/window factories are protected SDK implementation helpers, not public
root operations. No SDK patch or unsafe public adapter is introduced for them.

DesktopCaptureReceiver and DesktopSelectionObserver registrations now implement
the actual SDK virtual callbacks, including OnFrameCaptureStart, exact result
values and native frame presence, and selection/cancellation/error notifications.
The explicit callback-taking constructor installs registrations before SelectSource
and Start. Runtime replacement retains in-flight registrations outside slot locks;
receiver mode bypasses ordinary video publication, None restores it. Clearing a
selection observer calls SDK Observe(nullptr), retaining the last status rather
than inventing further notifications. Panic quarantines a receiver without
silently restoring another consumer. Actor entrypoints take Send registrations,
not local SDK handles.

CapturedDesktopFrame owns the actual native frame and exposes shared pixels
read-only; mutable SDK BasicDesktopFrame CPU copies are explicit. Frame wrappers
are local, while owning snapshots contain packed pixel rows and original layout/
geometry/timing metadata. Size, stride, FourCC, origin/rect/scale, DPI, optional
device scale, cursor indication, capture time, capturer ID, ICC profile, updated
regions, texture/CPU presence and shared-memory identity/size/presence come from
actual SDK getters. Native metadata and copy/move-info, both pixel-copy overloads,
scaled intersection, pixel lookup and black operations are forwarded with checked
native arithmetic/bounds. Native black operations assume packed aligned positive
rows; unsafe layouts are refused instead of calling their linear scan/memset.
Texture-only frames remain texture-only, not fabricated CPU images. Captured
metadata is mutable independently of shared pixels; move-info retains the SDK's
updated-region swap semantics.

GetMetadata now owns each non-null borrowed GObject with g_object_ref/unref;
copies retain native proxy/cancellable identity without raw pointer exposure.
Local owning metadata and private SessionDesktopMetadataId aliases outlive the
capture as independent SDK data. The pinned portal returns empty session details;
that actual empty value remains distinct from an artifact without GIO support.

DesktopMemory owns an actual SDK SharedMemory subclass, its Linux memfd/mapping
or explicitly selected CPU heap storage, and optional panic-contained Send Rust
dependency leases. Allocation/mapping failure preserves errno, never falls back.
Owned input files require monotonic F_SEAL_SHRINK before size inspection/mapping;
undersized files/products and out-of-bounds buffer operations reject before native
access. Native size/id/handle getters and owned FD duplication are exposed.
SharedMemoryDesktopFrame::Create consumes DesktopMemoryFactoryHandle through the
actual SDK virtual, including null rejection and panic/undersized quarantine.
The preferred constructor (also covering deprecated overload semantics) accepts
owned storage with checked padded or signed-stride row origins. Offset views
retain the full allocation and leases; native SharedMemory data/size describe the
rebased forward view. Captured frame FD conversion does not guess that every
platform-specific SDK integer handle is an FD: only known owning products are
converted. These operations allocate buffers, not physical capture devices.

Pinned Linux limitation: X11 and BaseCapturerPipeWire inherit the SDK's no-op
SetSharedMemoryFactory. The internal PipeWire stream has a separate setter, but
that private/backend input is not forwarded by the public capturer. A selected
factory is therefore UnsupportedOperation at capture construction/replacement,
not fabricated installation, ignored selection or a stock substitute. Rejection
at construction occurs before creating the video source/capturer. None retains
the native absent/default path. Owning source/facade and actor paths share this
behavior; standalone SDK shared-frame factory consumers genuinely use the source.
No libwebrtc patch is made to invent Linux capture support.

Capture source coverage is delivered with the recorded SDK limitations. Matching
native builds and non-device frame/shared-memory/options/controlled-rejection
fixtures passed the final normal and sanitizer suites. This does not qualify
physical capture, live callback delivery, portal/session behavior or canceled
GIO drain. Those live/prerequisite-dependent gates remain unqualified.

## Owning native network values

`Network` owns a stable SDK allocation on its caller sequence. Names and
descriptions are bytes. Constructors, address-list operations, best-IP
selection, type/VPN state, scope, ID, preference, active/ignored state, slice,
key and debug formatting call the actual native members. `share` retains native
identity; `Clone` calls the SDK deep clone and does not copy connected signals or
provider links. IP replacement preserves native membership-based change
reporting, order and duplicates. `IpNetworkMask` owns the SDK `NetworkMask`,
retaining the supplied address and signed prefix length without truncation or
normalization. The existing configuration-only `NetworkMask` remains unchanged.

Network cost reads the supplied owning field-trial view. Adapter inference,
cellular classification and inverse cost mapping also use the SDK. The reached
VPN-as-underlying-VPN cost DCHECK and negative non-EOF suffix bytes reaching
native `isdigit(char)` return typed errors; storing those values or safe
unrecognized interface names is not rejected.

`DefaultLocalAddressProviderHandle` owns a native provider adapter. Its trait
accepts native address-family values and returns an optional owning IP value;
Some(Unspecified) remains distinct from absence. Network's default-provider link
retains the allocation, and returned aliases can outlive the network. Native
Clone omits that link. Provider panic deactivates later calls, with failure
observable on the handle; adapter-owned final release contains unwinding.
The final non-device normal/ASan qualification includes these network values
and default-provider fixtures.

`Network::subscribe` registers tagged SDK adapter/preference/slice receivers.
Its sequence-bound owning subscription retains the source, permits self-cancel,
and unsubscribes on final drop. Callback and capture destruction contain
unwinding. Cancelled callbacks remain leased through the SDK's deferred receiver
cleanup, so closure destructors can reenter once the native Send has finished.
Same-channel recursive notification setters and receiver additions return
`NetworkChangeInProgress`; unchanged setters retain the SDK's early return.
Other event channels remain independently usable. `notify_change` explicitly
calls the native publication method with this actual network as sender, without
changing its fields.

These values are a binding foundation, not proof of native transport installation.
Full manager observation and packet socket products remain incomplete. Simulated endpoint providers retain
their existing behavior.

## General network manager products

`NetworkManagerFactory::from_provider` selects a thread-safe factory whose
sequence-bound `NetworkManager` products are created and initialized on the
native network role. Both local and ProductionSession builders accept this
selection. Endpoint recipes and general factories are mutually exclusive;
manager selection also excludes a separate stock-manager monitor selection.
`None` or creation/initialization panic fails construction, never stock fallback.

Creation receives owning environment and actual native trial views, plus
thread-safe changed/error signals. The implementation calls Initialize because
pinned ConnectionContext does not initialize injected managers. The provider
must emit changed/error on Start, as required by network.h. Delivery posts to the
existing role queue, remains active across multiple clients' balanced Start/Stop,
and invalidates stale queued work on last Stop/restart and retirement.

Enumeration returns owning Network handles. The adapter preserves returned order
and duplicates and retains every actual native allocation until manager
retirement. Repeat enumeration should use `share`, not deep Clone, to preserve
the native identity contract. Permission, any-address enumeration, default-local
address queries, dump and VPN masks dispatch to the selected product. Network
provider links must be installed explicitly, not presumed to be added by the
allocator. Product receivers use shared access for synchronous read reentry;
mutable implementation state remains sequence-local.

Operational panic disables that product and records aggregate registration
failure without disabling healthy sibling products. Stop is still attempted
after failure. SDK facade destruction marshals dependent product/network release
to the network role, including rejected initialization. Controlled use requires
the explicit controlled capability and existing caller-owned socket selection.
No manager-specific executor or implicit pumping is added; calls use the
existing network role. Production final-owner handoff is described below.

### Manager observation

`NetworkManagerContext::observations` returns a sequence-bound source for the
actual manager's SDK NetworksChanged and Error callback lists. Tagged owning
subscriptions may be registered during creation and bind to that facade before
Initialize. Failed construction retires them without synthesizing delivery.
The source exposes readiness/retirement separately; synchronous `notify` before
facade construction returns NotReady. Cross-thread posted notifications continue
through `NetworkManagerSignals` and its Start/Stop generation guard.

Observation publication calls the SDK Notify members, independent of updating
client count. Same-channel recursive Notify and receiver addition return
DeliveryInProgress before the SDK precondition checks. Other channels remain
usable. Self/sibling cancellation removes actual SDK receivers and retains Rust
captures until SDK Send and receiver cleanup return. Callback and capture Drop
panics latch failure on retained subscription metadata, without disabling the
manager product or sibling observers. Manager retirement cancels subscriptions,
releases their captures on the network role and clears the native facade pointer
before releasing that role. Source aliases become inert, not facade owners.
`NetworkManagerStats` exposes the SDK's standalone count value, not a live query.

### Scoped mDNS products

`NetworkManagerContext::mdns_responder` creates an owning sequence-local native
responder backed by `MdnsResponderProvider`. Creation/removal operations expose
native asynchronous callbacks, exact owning IP/name bytes and removal results.
The provider owns the native name/reference-count policy; the binding generates
no names and implements no substitute mDNS protocol. Owning completions are
Send/Sync, mutex-guarded single-submission handles. Completion with a different
IP is rejected without consuming the request, protecting Port's actual address
DCHECK. Cancellation does not manufacture a name or a removal success.

`Network::set_mdns_responder` installs the actual native provider link and retains
its facade; the getter checks the real native responder identity. Shared network
handles see that same link, while SDK deep Clone omits it. The manager product
also exposes the native GetMdnsResponder operation. A failed or retired selected
responder remains installed, preventing null-provider plaintext-IP fallback even
when its network link is retained by another live consumer.
Provider failure records a manager-local failure and emits the native error
signal when active, without disabling unrelated manager products. Public callback
and capture failures are observable separately from provider admission success.

Manager retirement first disables notifications, then cancels outstanding mDNS
callbacks, clears their role-dispatch pointers and retires the Rust provider on
its creation role. Active provider frames hold immutable registration leases
across reentrant local retirement. Retained responder/context aliases reject
new work after retirement; completion aliases cannot revive callbacks or retain
an SDK callback past scope close. Enrollment tracks actual weak owners and
prunes expired entries; completed/cancelled requests remove their owner tracking.
No hidden worker, wait, time advance or global lookup is introduced.

### In-progress retirement admission

Controlled dispatch/publication turns now temporarily retain actual registered
peer and factory owners through weak dependency edges on their existing driver.
The outer turn releases those leases after native code returns. Newly created
owners join the active turn; nested destruction appends to an outer retirement
batch without holding a RefCell loan through user code. Idle tracking stays weak.
No identity lookup, new executor, implicit dispatch or time advance is added.
These leases cover driver/cooperative-factory pumping, ticket execution and
cancellation, network notification operations, standalone mDNS requests,
controlled factory/peer construction and native owner destruction.

`PeerConnection::close` now marks admission before SDK Close and rejects native
provider/context callback reentry or forbidden worker/network blocking calls
with InvalidState. Posts through `TaskQueueContext` may execute on private SDK
queues whose deletion would wait on the running task. They retain native callback
admission restrictions; they are not equivalent to control work on caller-owned
queues. Ordinary caller-owned task roots retain their existing control behavior.
`PumpResult::blocked` distinguishes rejected provider/retirement reentry from
idle dispatch, without changing native deadlines or advancing time.

This is implementation-only, not a complete manager-root or runtime parity
claim. ProductionSession now reserves one single-use retirement worker after
successful native factory construction. Final Drop from callback scopes, SDK
roles or native queue contexts moves the entire exclusive graph to that worker,
without allocating or spawning during handoff. The worker performs final native
shutdown/destruction off graph-owned roles, then exits. It accepts no application
commands, does not proxy normal API calls and is never used by controlled graphs.
Ordinary caller Drop cancels/joins the unused worker only after graph teardown.
Failure to reserve the worker fails actor construction with ResourceExhausted.

Explicit session close/shutdown checks admission before observer unregistration
or other partial cleanup. Callback scopes also cover foreign-producer readiness
wakes, which may have no SDK queue TLS; waker invocation and final release use
hostile-payload containment. These scopes are admission markers, not resource
leases or proof of a specific peer/queue identity.

The final lifecycle fixtures exercised owned native roles, foreign completion,
internal event-log and actual codec queues, caller-owned queues, actor migration
and nested final Drop. Network manager and subscription fixtures also passed.
Conservative callback admission remains binding-side safety policy, not an
inherent SDK locality claim. A callback marker alone is not lease or queue
identity proof; the actual consumer/lifetime evidence is recorded separately
in `docs/provider-verification-plan.md`.

## Packet socket values and owning products

The public general factory and owning product adapters are installed through the
factory/session builders described below. Existing endpoint-bound simulated
products remain available; conflicting selections reject rather than silently
replace one another. Final non-device consumer/lifetime qualification is
recorded in `docs/provider-verification-plan.md`.

`AsyncSocketPacketOptions`, `PacketTimeUpdateParams`, `PacketInfo` and
`SentPacketInfo` retain the SDK members and defaults, optional network ID, signed
IDs/times, authentication bytes and batch flags. Packet/protocol tokens retain
native scoped enum values. Unscoped DSCP, option, state and decryption tokens
check representation before native conversion without normalizing representable
unknowns. `PacketSocketFlags` retains native int bits and named TLS/DTLS flags.
`ReceivedIpPacket` owns payload/address storage; native CopyAndSet retains its
other metadata. The legacy path calls the SDK constructor and rejects below -1
and INT64_MAX before its reached checks. Ordinary receive values retain missing,
negative finite and infinite timestamps; value-copy support is not permission to
deliver every such value into media consumers.

`PacketSocket` declares the sequence-local native address/send/close/state/
option/error operations. Native send payloads are owned before provider reentry.
GetOption either writes its successful value or preserves the exact nonzero
result without writing the native output. Operational panic deactivates only
that product; Close still attempts cleanup. Product/capture destruction contains
unwinding, and retained context metadata observes failure. The native helper for
socket information reads the live virtual address, not a creation snapshot, and
does not expose successful output after that provider query fails.

`PacketSocketContext` describes owning environment and synchronous SDK
notifications; its separate `PacketSocketEvents` handle queues owning values from
other threads. Submission success is not receiver/transport success. Target and
role loans are cleared at native product retirement, so retained aliases cannot
revive delivery. Direct publication enters controlled owner turns; same-channel
recursion is rejected and no submission mutex spans a provider/native callback.
These product-frame leases are not a whole-graph or SDK facade lifetime proof.
Receive delivery rejects decryption status 3 before the piggyback DCHECK while
value copies preserve it. Sent delivery preserves signed times and packet sizes
without preflighting SDK transport-feedback arithmetic. Arrival admission uses
the actual Call receiver: nonfinite RTP time is rejected before SDK delivery for
all RTP media types, while RTCP/control delivery is unchanged. PeerConnection's
call_admission_stats and the actor equivalent expose saturating binding rejection
counts, actual creation-clock bounds and Call absence/live state, not native
RTCStats. Finite timestamps remain unchanged. Internal timestamp history and
cumulative/shared sent accounting are libwebrtc-owned, not mirrored or constrained
by a binding time range or traffic quota. Final boundary/lifetime evidence is
recorded in the verification results, including the distinction between direct
Call admission and socket arrivals normalized by the native voice channel.
General factory/local/actor wiring is implemented below. Controlled standalone
packet ownership and its exclusive receive source are described below.

The private SDK factory scope now has a separate RAII owner: metadata aliases
cannot postpone its retirement. It disables all resource targets before releasing
any provider, with no locks held through user destruction. Packet entry frames
lease the actual provider allocation; retired SDK facades no longer dereference
a saved role. The installed general factory now owns this guard, including
partial construction; scope retirement precedes source/environment release.

Source-level ListenSocket/Context and AcceptedPacketSocket add native listener
state/address dispatch and move-only accepted product creation. Standalone queued
acceptance holds unclaimed products on the native role, returns the same product
on rejected submission, and exposes atomic status/cancellation without moving
products into Send ticket aliases. Synchronous publication uses the same actual
SDK listener list with a single explicit ownership-taking receiver. Retirement
cancels pending transfers; pending-product pruning rechecks every live target
after user destruction. Passive receivers do not gain a second raw owner, and
already claimed standalone children retain independent retirement ownership.
Controlled/production standalone ownership and asynchronous creators are wired.
The final non-device consumer/lifetime fixtures passed normal and ASan runs.

Pinned ICE TCP limitation: the listener factory receives only a bind address,
not TCPPort's actual Network membership. TCPPort stores an incoming socket and
later TCPConnection repeatedly calls its live GetLocalAddress and DCHECKs that
its IP belongs to Network()->GetIPs(). Bind-IP equality rejects legitimate
multi-IP inputs; getter snapshots cannot guard later changes/panics and would
not preserve the live getter contract. There is no public SDK admission hook to
check the actual owner network before that constructor. No SDK patch, address
synthesis, guessed network join or frozen getter is used. The private installed
ICE handoff is therefore NativeConsumerUnavailable: context creation refuses
before invoking a child creator, submission returns the unclaimed owned product,
and posted creators return an explicit unavailable ticket without running the
creator. The native dispatch path also refuses before NotifyNewConnection/raw
ownership transfer. This limitation is separate from provider failure and does
not disable supported standalone listener acceptance or outgoing TCP/UDP.

`PacketSocketContext::subscribe` now creates owning receivers on the actual
facade's close, ready-to-send, connect, sent and address-ready native lists.
Creation-time registrations bind when that facade is constructed; rejected
construction retires them without invented notifications. Native trampolines
hold weak sources and owning tag metadata. Self/sibling cancellation disables
delivery immediately, but native removal waits until that channel's Send returns,
avoiding the pinned SDK's sibling pending-tag DCHECK. Tag allocations stay alive
until actual native removal/facade destruction. Rust captures stay pinned until all enclosing native Sends
and SDK receiver-vector cleanup return. Same-list Add/Send is rejected with
`InProgress`, including capture cleanup; registrations are not silently queued.
The owning callback payload copies sent metadata and addresses, not a socket or
callback-scoped pointer. A receiver's panic/capture-Drop failure latch is local
and survives retirement. Cancellation and Drop establish controlled owner turns.

The explicit deprecated untagged connect overload is also called natively. Its
Rust capture can be cancelled, but SDK has no corresponding native removal:
an empty weak receiver shell remains in that facade until destruction. Repeated
registration should use the tagged connect operation instead. This is not a
claim that untagged SDK storage is bounded independently of socket lifetime.

Socket notification admission also checks the actual SDK queue when the socket
was created in a queue-bound network context. Same-thread cancellation from a
different cooperative queue disables delivery immediately, then queues tagged
removal/capture cleanup on the actual network role. It does not call the close
list's sequence checker from the wrong queue. Null-queue creation binds the SDK
checker to its native OS-thread domain. Factory/accepted-product creation checks
the selected native network Thread queue rather than matching a resource name.
These source guards do not establish whole-graph teardown safety. SDK-managed
synchronous acceptance is implemented above, with explicit readiness and same-list
recursion checks.

`PacketSocketFactory` now exposes controlled standalone UDP/client-TCP/client-UDP
creation, preserving explicit capability/driver/environment checks, provider None
and provider failure without stock substitution. `OwnedPacketSocket` owns an
actual native facade and a separate retirement scope. Its native core retains
the exact recipe through product contexts, the selected environment and actual
controlled driver, rather than a borrowed thread wrapper. Native operation and
notification frames lock that exact core; callback-time shell Drop retires its
publication/source captures but cannot destroy an active native facade. Lifecycle
cleanup on a different cooperative queue establishes the actual SDK Thread queue
on that same OS thread, without dispatching tasks, waiting or advancing time.
Environment retention is released before the driver's final ownership edge.

Typed address/state/send/send-to/close/option/error members now resolve the actual
native virtual facade through `PacketSocketContext`; the owning product exposes
those members through its context. Contexts remain non-owning, sequence-bound
capabilities: before native binding they return NotReady, after retirement they
return Retired, and queries cannot turn failed provider defaults into successful
output. Addresses are owned copies, signed results and nonzero GetOption codes
are preserved, and GetOption failure does not expose an invented output value.
Close cleanup and error inspection remain callable after operational failure.
SDK-managed parent-graph final-drop safety is still task #10, not established by
these context/member leases.

Only `OwnedPacketSocket::register_received` can claim the SDK's single receive
callback. Ordinary nonexclusive subscribe does not steal an SDK peer receiver.
A second registration returns ReceiverAlreadyRegistered, including when an old
logically cancelled receiver still awaits native deregistration on its role.
Self-cancellation/panic disables delivery immediately but defers destruction of
SDK AnyInvocable storage until its invocation returns. Native trampolines contain
weak sources and owning tag metadata; deep packet copies and the existing local
failure latch outlive the native loan. New registration during that receive/
cleanup frame is rejected, not implicitly deferred. Production standalone
ownership is source-wired below; remaining factory/transitive contracts stay open. No build, test, matching artifact or runtime
qualification was run for these batches.

### Controlled owning listeners and external accepted creators

`PacketSocketFactory::create_server_tcp_socket_controlled` creates an actual
owning `OwnedListenSocket` through the selected recipe, with exact environment
and controlled-driver retention and distinct None/panic behavior. Its context
queries native state/local-address virtuals. `register_accept` installs exactly
one ownership-taking receiver on the actual SDK list; `observe_accept` installs
non-owning observers, with SDK ordering and no substitute observer collection.
The standalone listener requires a live owner registration before publication,
not the private TCPPort first-turn fence. New registration and recursive inline
publication during Send/cleanup return InProgress, not deferred replacement.

Each standalone accepted product has an independent owning native core, scope,
environment and driver lease. Transfer is still bound to its actual listener's
scope. An actual Send frame escrows unique ownership until its owner receiver
moves it once into `OwnedPacketSocket`. Passive observers receive a non-owning
context for that exact native argument, never a second raw-pointer owner. If the
owner is cancelled before its turn, unclaimed escrow retires on-role. A delivered
socket survives listener Drop; unclaimed pending products do not. Active listener
and child frames pin actual facades through callback panic, owner Drop and SDK
cleanup. Callback/capture failure is local and observable on the subscription.
Cancellation defers native removal and capture destruction until Send returns,
including retirement and capture-Drop cancellation reentry.

`ListenSocketContext::events` returns a Send/Sync source whose `post_accept`
accepts a Send creator, not a non-Send product. The existing listener role runs
creation; a subsequent queued transfer preserves the SDK-managed receiver
installation boundary. The shared atomic ticket remains Pending until native
transfer, then reports Delivered/Cancelled/Retired/ProviderFailed. Cancellation
can race a running creator; it need not undo creator side effects, but prevents
an unclaimed transfer. Rejected, cancelled, retired and discarded creator work
terminalizes without a hidden worker or pumping. Actual products stay in
role-local storage. Capture Drop is panic-contained, with retained ticket failure
diagnostics that do not overwrite a Cancelled/Retired result. Sender/ticket
aliases cannot retain a live product or revive a retired role.

The final non-device accepted-transfer and graph-lifetime fixtures passed.
Pinned unsupported cases and admission constraints above remain limitations;
source presence or passive observation alone is not native-consumer proof.

### Production owning standalone role

`PacketSocketRuntime::new` consumes the actual owning `NetworkThread` and retains
its exact environment plus a production execution lease. Borrowed controlled
wrappers and non-qualified/cooperative environments reject without substitution.
`post` uses that selected SDK thread and supplies a non-Send `PacketSocketRole`;
no local native owner is returned through a Send continuation. Callback/capture
panic is contained and observable on the runtime. Queued work captures a weak
runtime edge, upgrades only while executing, and is discarded if no real owner
survives. There is no implicit runtime-to-queued-task ownership cycle.

The four production standalone factory creators accept this role instead of a
controlled driver. Packet/listener cores retain a strong runtime edge through
actual native operations, notification and capture cleanup. Listener contexts
hold only a weak production owner; creating accepted children upgrades it into
an independent owning core/scope, so adopted products survive parent retirement.
The same actual recipe dispatch and None/provider-failure behavior apply; no
stock transport, borrowed thread facade or fake role identity is introduced.

A single-use retirement worker is reserved at runtime construction. It accepts
only final resources, never application operations. Last ownership release from
an SDK queue/role/provider loan detaches that one handoff instead of joining on
an SDK caller stack. Other caller Drop synchronously destroys the native role
and cancels/joins the unused worker. Resource teardown explicitly destroys the
actual Thread first, closes the environment next and releases the production
lease last. Product core field order releases socket/events/scope before the
strong role owner. Metadata aliases retain no owning runtime edge.

Source review found no concrete scoped ownership/FFI defect or missing named
packet-factory/socket/listener operation. Matching artifacts and the final
non-device normal/ASan consumer, cancellation, lifecycle and teardown fixtures
passed. Standalone legacy DNS is described below. This does not erase pinned
unsupported cases or imply proof for all possible native races.

## General packet socket factory source installation

`PacketSocketFactory::from_provider` owns a Send + Sync
`PacketSocketFactoryImplementation` recipe. Both
`PeerConnectionFactoryBuilder::socket_factory` and
`ProductionSessionBuilder::socket_factory` select it, with corresponding
`socket_factory_provider` conveniences. The existing simulated endpoint
`packet_socket_factory` selection remains separate and unchanged; selecting both
is an error, not a precedence/fallback rule. Controlled capability must match
the selected peer driver, and a panicking capability query rejects construction.

The private SDK factory overrides UDP, server TCP, client TCP, client UDP and
legacy asynchronous DNS creation. Socket contexts own the exact incoming SDK
Environment and retain the actual recipe through active product frames and
provider destruction. The no-environment DNS member receives the installed
factory environment and reuses the owning DNS adapter/completion/result path.
Each new DNS product has its own failure latch; aggregate recipe diagnostics do
not poison another product or installation. Normal None socket rejection is
passed to the SDK as null, never replaced with a stock socket. Consumer-required
DNS construction instead returns the existing failed resolver after binding
failure, with an error result rather than invented resolution success.

`PacketSocketTcpOptions` owns native byte-valued ALPN/curve vectors and the
existing open `PacketSocketFlags`. Its optional `SocketTlsVerifier` retains the
actual verifier backing independently of socket, factory and peer retirement.
The SDK peer dependency proxy and every verification entry frame retain that
same backing. Thus synchronous creator verification, rejection/temporary product
cleanup and reentrant proxy retirement do not invalidate the verifier. Rust
callbacks run outside native locks. The internal TURN-CA verifier uses the same
proxy, serializes its native-only calls and supplies both leaf and chain
verification. The private SDK pointer conversion accepts only these binding
peer dependency origins; arbitrary C++ verifiers are not reinterpreted or
registered by guessed pointer/label identity.

Source review found no concrete factory/TLS/selection signature or lifetime
defect. Matching builds and non-device installed-socket, TLS and lifetime
fixtures passed final normal and ASan qualification. The pinned native admission
limitations remain explicit rather than patched or hidden by substitutes.

## Network monitor

`PeerConnectionFactoryBuilder::network_monitor_factory` and
`ProductionSessionBuilder::network_monitor_factory` register a thread-safe owned
`NetworkMonitorFactory`. Native factory dependencies own the adapter and retain
the Rust registration independently of its builder. The native network manager
owns each freshly returned monitor.

- Creation and operational calls on a monitor run on the native network thread.
  Final monitor and factory destruction can occur on another native role thread:
  the pinned default `ConnectionContext` releases its manager on signaling.
  Providers and shared application state therefore require `Send + Sync`.
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
  A factory panic selects the no-monitor fallback. Adapter-owned destruction
  follows the shared [panic containment policy](provider-panic-contracts.md).
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
then reads retained chain owners after peer retirement. These non-device cases
passed the final normal and ASan suites.

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
Send + Sync and must not synchronously wait for or reenter their owning actor.
Non-panicking destructors are recommended; adapter-owned release follows the
[provider panic containment policy](provider-panic-contracts.md).

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

The runtime fixture targets the pinned default peer's actual remote ICE
candidate permission checks: actor migration, cross-thread grant and one-shot
rejection, pending-query retirement, denial, query bypass, factory/query/request
panics, connectivity-check packet release or suppression, and retained addresses.
It does not certify permission-gated STUN-server gathering. The SDK default peer
factory does not pass the selected permission factory into `BasicPortAllocator`;
that allocator has a separate owning factory input. Binding and qualifying that
additional construction path remains incomplete, not a claim of native absence.

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
latching, retained environments, scheduler conflicts and actor migration. These
non-device cases passed final normal and ASan qualification. Focused static
reviews remain source evidence only, distinct from those completed executions.

`SimulatedClock` is distinct from the bounded controlled-world `ManualClock`.
It accepts finite signed time and signed advances, preserving atomic wrapping
except for transitions to unreadable timestamp sentinels. NTP seconds wrap by
native design, including the 2036 boundary. Finite timestamp conversion delegates
to the SDK without an additional negative-fraction admission policy.

`Clock::real_time()` owns a view of the SDK singleton and delegates finite
NTP conversions to that same singleton. Calibration, fractional rounding and era
interpretation remain SDK-owned; the binding adds no matched-anchor arithmetic
or modular extension. Infinity returns `TimeValueError::InfiniteValue` from
built-in clock queries and invalid NTP to native consumers. Custom providers
retain their explicit conversion contract.

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
  without preflighting SDK backend arithmetic. A retained snapshot cannot post off that queue's current
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
- `TaskQueue::post_delayed_native` also supports built-in runtimes. SDK stdlib
  queues receive native signed/infinite values unchanged and own deadline/wait
  arithmetic. The binding imposes no numeric scheduling horizon.
- Cooperative queues accept all finite signed delays. Negative values clamp to
  zero; the native scheduler discards deadlines beyond its manual-clock domain
  while holding its scheduler mutex. Infinity cannot pass its native finite
  microsecond conversion and is rejected. Posting success means dispatch to the
  native void posting API, not acknowledgement of execution or indefinite
  retention. No autonomous progress is introduced.
- `NetworkThread`, `WorkerThread` and `SignalingThread` expose the same native
  delay/precision operation. SDK threads receive native values unchanged and own
  their millisecond rounding and queue arithmetic. Borrowed controlled threads
  require finite delays under the cooperative scheduling contract. Current-queue
  snapshots apply those same rules without exporting a queue owner. Duration
  conveniences still require representability as a finite native TimeDelta.
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

### Owning standalone legacy DNS

`PacketSocketFactory::create_async_dns_resolver` accepts an explicit production
`PacketSocketRole`; its controlled counterpart validates the selected driver,
clock/queues and factory capability. Both create `OwnedDnsResolver` through the
actual selected recipe and retain its exact owning environment, factory and role.
Creation panic returns a typed error, not a successful failed substitute. The
installed SDK-required non-null failure path remains separate and unchanged.
Per-product failure is independent of aggregate recipe diagnostics.

`DnsResolverContext` is a sequence-local, non-owning operation capability. Both
Start overloads and result lookup invoke actual native virtuals. The owning
standalone facade enforces the SDK's single-name-query lifecycle with an explicit
AlreadyStarted result; this does not claim portable repeat-Start support. Native
operation frames pin the actual product/role through provider and capture Drop
reentry. Reentrant operation loans reject before the provider's mutable mutex.
Controlled entry establishes its actual SDK Thread queue while making native
calls, so completion posts to the existing driver without pumping or time change.

`DnsResolverQuery` owns the native callback registration. Cancel/Drop suppresses
only the pending notification, not provider-owned I/O; query status distinguishes
pending, delivered and callback/capture failure. Retire/cancel races preserve the
existing post-only completion contract and never invoke Rust callbacks under
submission locks. Callback and capture Drop panic is contained and observable.
Contexts/query metadata cannot own the native resolver or revive its retired role.

`OwnedDnsResolverResult` owns the actual returned SDK result object and its role
lease independently of the resolver. Queries remain live native virtual calls,
not completion-time snapshots. Missing family returns None, while signed resolver
errors remain exact; provider failure is a separate typed error rather than a
successful fabricated error/address value. Factory ownership survives through
native result destruction. Shared result wrappers preserve the actual allocation,
and result-operation admission prevents same-object recursive calls. The bridge
also tracks actual Rust provider loans before mutex acquisition, preventing
same-resource recursive lock deadlock across distinct native wrappers without
removing ordinary cross-thread serialization.

Focused source lifecycle and pinned public API reviews found no concrete scoped
DNS defect or remaining missing #9 operation. These source paths and all new
callback/result/role lifetime boundaries still require combined compilation,
actual-consumer and sanitizer qualification. No runtime/build pass is claimed.

## Video bitrate allocator factories and products

`PeerConnectionDependencies::video_bitrate_allocator_factory` retains a shared
`VideoBitrateAllocatorFactory`. The provider convenience builder constructs
the same registration. Omission leaves the native per-peer default untouched;
`builtin()` explicitly selects the SDK helper. `ProductionSessionBuilder` also
accepts a default video bitrate allocator factory/provider for subsequently
created peers. An explicit per-peer dependency takes precedence; absence of both
retains native stock selection. The actor retains this default across migration
and releases it after peer dependents but before its role-owner factory. This
source wiring has not been runtime qualified. A caller factory may independently
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

Source review and final executed coverage include both allocation overloads,
infinity and signed-value preservation, inactive codec slots, arbitrary format
parameters, non-Sync factory/products, serialized factory failure, overflow,
panic/destructor containment and actual native video encoder rate consumption.
Matching builds and the final non-device normal/ASan suites passed.

## Network-state predictor factories and products

Install predictors through an explicit `NetworkControllerFactory::goog_cc`
selection with `GoogCcFactoryConfig::network_state_predictor_factory`. That
owning adapter supplies the actual SDK factory's borrowed predictor pointer and
retains its owner through native factory destruction. Native products retain
independent provider state. An absent optional predictor preserves omission;
no stock predictor is silently substituted.

The older local/production `network_state_predictor_factory` builder slot is
rejected with `InvalidParameter`: the pinned SDK copies that dependency into
CallConfig but its default send-side GoogCC construction does not consume it.
An injected custom controller also does not consume it automatically. Accepting
that setter as installed functionality would be misleading. Explicit GoogCC
selection requires the injection trial and explicit RFC 8888 negotiation state,
not an inferred default or silently enabled field trial.

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

Executed coverage includes actual native adapter factory/product calls after
factory retirement, signed extreme timestamps, sentinel/unknown states,
non-Sync factory/products, nullable products and callback/destructor panics.
Installed congestion-control consumer fixtures and the consolidated non-device
normal/ASan suites passed; see the verification results for exact scope.

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
calculations return zero and failed mask selection returns false. Successful
encoder-target results pass through unchanged; native protection/budget arithmetic
is libwebrtc's responsibility. The SDK's video-send caller requires a non-null
controller, so factory panic yields a
non-null rejecting adapter, never a silently successful stock controller.
Provider destruction and native completion wakes contain Rust panics.

Registration and async dispatch add no native threads, timers, runtime or
caller executor. Controlled progress stays with the caller's existing worker.
Executed coverage includes native factory/product independence, controller
payloads, signed/unknown values, native callback outputs, sender-lock rejection,
invalid requests, retention/retirement, cancellation, concurrent panic failure,
and real video-path worker dispatch. Matching builds and final non-device
normal/ASan suites passed.

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

Returned updates preserve native fields and optional/default values. Direct unit
construction rejects malformed kind tags, negative finite one-sided magnitudes
and reserved sentinel collisions with typed InvalidUnits failure. Beyond those
representation requirements, pacing, probe/rate arithmetic, congestion-window
interpretation and target-rate calculations remain SDK-owned. The binding keeps
no probe history and imposes no burst/count budget, timestamp-span restriction or
field-trial-derived numeric admission. Zero pacing windows retain the native
one-second rate window; SDK pacing interprets padding and data rates. A nonfinite
process interval preserves native disabled periodic processing; finite intervals
must satisfy the scheduling boundary's nonnegative-delay requirement.

Panics in creation, interval/capability queries, product updates and provider
destruction are contained. Native consumers requiring a non-null controller
receive a rejecting product after creation failure, not a substituted stock
implementation. Rejected callbacks return a finite zero transfer-rate target;
already engine-owned packets retain the last valid pacing configuration.
Clearing pacing while those packets are enqueued would violate native packet
admission assertions.

Executed coverage includes callbacks and nested values, native output
consumption snapshots, factory/product independence, retained environments,
panic/invalid-value failure outcomes, explicit root trial gating, native
feedback/pacing helpers, and an injected-controller video send path requiring
actual encoder invocation and a native sent-packet callback. Matching builds
and final non-device normal/ASan suites passed.

## Actual Call RTP boundary

ObservedMediaFactory retains the selected native environment and matches its
clock object to the actual CallConfig environment before transferring durable
bounds. ObservedCall owns the selected SDK Call, intercepts its PacketReceiver
and preserves the actual packet and stream implementations. CallAdmissionStats
exposes bounds from that identity-checked creation context, live/absent state and
a saturating nonfinite-arrival rejection count, not RTCStats or effective BWE state.

RTP requires a finite arrival at the native delivery boundary. Finite timestamps,
including negative/default values, pass through unchanged regardless of creation
clock bounds. RTCP/control packets do not enter this check. Worker delays,
per-media receive statistics, BWE/TWCC history, receive-time correction and sent
accounting remain libwebrtc-owned. No binding timestamp envelope, clock/history
subtraction check, byte quota or receive-time field-trial rejection is imposed.
Per-peer trial overrides and inherited factory defaults retain native precedence.
Creation-clock bounds are diagnostic metadata, not an admission profile.

Matching-runtime qualification must observe native forwarding, selected clock
identity, ordinary trial-enabled creation, retained observations after close and
callback/owner retirement. It does not certify libwebrtc's internal arithmetic.

## Owning native GoogCC selection and network-state estimators

`NetworkControllerFactory::goog_cc(GoogCcFactoryConfig)` selects the actual SDK
GoogCcNetworkControllerFactory through the existing local/production controller
installation path. `GoogCcFactoryConfig::new(rfc_8888_feedback_negotiated)` requires
that native boolean explicitly. Its optional estimator and predictor factories
are independent owning registrations. The bridge forwards Create and the actual
SDK GetProcessInterval without implementing another congestion algorithm or
inferring negotiation from field trials. The native factory retains its borrowed
predictor-factory owner; returned controllers retain the Rust execution-domain
and registration owners until after actual SDK controller destruction. The SDK
still applies its own field-trial-controlled algorithm choices.

`NetworkStateEstimatorFactoryProvider::create` receives None for a null native
FieldTrialsView pointer, otherwise an owning copy of the actual supplied view.
GoogCC supplies its actual controller environment's trials. Each product retains
that view independently and implements GetCurrentEstimate, transport feedback,
received PacketResult (the SDK-default no-op is preserved), and route changes.
Factory and product callbacks are serialized, require Send rather than Sync,
and must not wait for reentrant actor work. Optional product/estimate absence
is preserved without creating a stock estimator or fabricated estimate.

Factory panics disable future creation on that registration. Product panic or
invalid output disables that product alone; observed_failure records the first
aggregate failure without disabling healthy siblings. Destructor panics are
contained. NetworkControllerFactory's own diagnostics are separate from child
estimator/predictor registration diagnostics.

Every estimate DataRate encoding is checked before native unit construction.
Beyond those representation requirements, estimates pass through unchanged.
Logging widths, probe-factor products, AIMD backoff and field-trial arithmetic
remain libwebrtc-owned. Absent estimates, native infinity sentinels and IEEE
diagnostic fields retain their native representations; the bridge adds no
capacity range, estimator-quality or ordering policy.

Source/ABI/ownership and focused estimator numeric reviews found no concrete
scoped defect. No compilation or runtime evidence is claimed. Final acceptance
must observe actual GoogCC feedback/prediction, native rates/probes/logging,
retained products and environment views, isolated failures and migrated teardown.

The pinned Call owns and registers its internal TargetTransferRateObserver before
ObservedCall exists. CallConfig has no send-transport factory/observer seam and
Call::Create constructs that transport internally. An application Call observer
cannot be installed at the current boundary without changing/replacing SDK Call
construction. No observer replacement, controller-output echo, SDK patch, or
application-observer support is claimed here. User direction confirms no
libwebrtc patches: this unavailable application Call hook is recorded as a pinned
SDK construction limitation, not an unfinished injection implementation. Call's
internal observer and actual native rate effects remain intact.

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
| Environment clock | System/shared manual clocks, owned custom Clock, signed/wrapping SimulatedClock, and native real-time conversion | None for this provider |
| Environment task queues | Default/cooperative built-ins, owned custom factory/scheduler products, native delay domains/precision and current-queue reference posting | None for this provider |
| Factory role threads | Selected production and controlled owned threads, including owning standalone network roles | None for these provider seams |
| Factory packet sockets / network manager | Endpoint-bound controlled and general caller-provided socket factories/products; owned network manager/providers | Pinned unsupported ICE TCP accepted-IP admission remains explicit |
| Factory network monitor | Owned custom factory and monitors | None for this provider |
| Factory event-log factory | Owned custom and selected built-in factory | None for this provider |
| Factory network-state predictor | Owned factory/products and signed Update payload | Final non-device normal/ASan qualification passed |
| Factory FEC | Owned factory, complete concurrent controller and expiring synchronous/asynchronous protection callbacks | Final non-device normal/ASan qualification passed |
| Factory network controller | Owned complete factory/products and network-control message/update graph with explicit pinned trial gate | Final non-device normal/ASan qualification passed; no public Call target-observer injection seam in this SDK |
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
| Peer local-network permissions | Owned factory/products, query overrides, asynchronous completion and actual remote ICE checks | Separate `BasicPortAllocator` factory input for STUN/relay permission checks is not wired by the default peer path |
| Peer certificate generator | Owned asynchronous provider, reused key/certificate resources and signaling-posted completion | None for this provider |
| Peer video bitrate allocator | Owned factories, callable products and full codec/allocation values | Final non-device normal/ASan qualification passed |

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
