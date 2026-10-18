# Provider slice deferred verification

## Workflow and scope

Approved contract: `plans/parallel-slices/04-providers.md`.

The user requested implementation batching on this session: finish the remaining
implementations first, record verification requirements here, then finish missing
test coverage and qualify the combined candidate at the end. Do not perform
intermediate artifact exports, test runs or qualification builds. Keep this file
updated as new operations and lifecycle boundaries are implemented.

Internal libwebrtc arithmetic is upstream responsibility. Bindings preserve native
values without private-history mirrors, numeric traffic/time quotas or arithmetic
field-trial rejection. Retain direct FFI/unit representation requirements and
Rust/CXX ownership, callback, buffer-validity and teardown protections. Numeric
qualification observes faithful forwarding at supported boundaries, not a new
proof of SDK pacing/BWE/FEC algorithms.

Source presence and implementation notes are not qualification evidence. Do not
mark an outcome complete when its implementation is partial or required gates
are unresolved. Do not treat hardware absence as proof of physical capture.

## Reconciled state

- `0a7b66c` implements public native event subtype codecs and owning parsed
  records/statuses. Matching core/native/ASan exports and scoped regressions
  passed verifier `dd0c1ecd-5e83-46bf-af9f-a3f7cf7ce429` before batching began.
- Earlier owning environment/factory/logger/clock/queue/randomness changes retain
  their historical named evidence. Final native changes still require matching
  artifacts for the final combined candidate.
- Uncommitted network-value work adds sequence-bound SDK `Network` owners,
  shared native identity versus SDK deep Clone, IP list/best-IP behavior,
  selected-trial costs, native key/name helpers, and owning `IpNetworkMask`.
- Network-value verifier `54d24f73-dfe8-4bd6-954f-fa712c3c8c3c` stopped at export's
  Rust smoke build because `NetworkMask` collided with the existing configuration
  value. The owning wrapper was renamed `IpNetworkMask`; the legacy public
  configuration type remains unchanged. That fix has not been runtime verified.
  No native or ASan passes from that failed run are claimed.
- Existing `tests/network_values.rs` was written before batching was requested.
  Retain it for final qualification. Its runtime assertions are still unrun.

## Requested main rebase

- Rebased all 15 provider commits onto local `main` at `09c3f86` (0.7.0).
  The rebased tip is `4057d7e`. Historical commit IDs above name the original
  candidates only, not fresh qualification of the rebased worktree.
- Preserved the pre-rebase tip as
  `backup/providers-before-main-20261016-083350` at `0a7b66c`, and retained
  stash `03ac547718dedb242255e75e06ab1189b21da021` containing the unfinished
  tracked implementation. Untracked additions remain in the working tree.
- Resolved restored-work conflicts while retaining main's audio/media additions
  and current artifact manifest schema. Native hashes and fixture provenance
  remain main's historical identity until final regeneration; they do not
  qualify this changed candidate.
- Merged main's exhaustive ProductionSession resource handoff with the selected
  single-use retirement worker. The handoff includes capture option/metadata
  maps, the actor allocator default and the retirement token, along with main's
  PCM/raw-video and media resource graph. No persistent global cleanup executor
  is started during controlled-selection rejection.
- Requalify the rebased combination, not only the newly added provider files.
  Include all new main audio/media resources in callback-triggered final-owner
  handoff and migrated teardown coverage. No tests, builds, artifact exports or
  sanitizer qualification were run as part of this rebase.
- Narrow peer creation/teardown ABI review `33fa27f7` found matching parameter
  order/types, corresponding manager/socket declarations and retained dependency
  pointers. Production handoff review `e72da5aa` found the new fields and Source
  cases carried through. Its mutable-factory concern was reconciled directly:
  the capture factory methods take `&self`, so immutable `CaptiveFactory::Deref`
  suffices. These are source observations, not runtime qualification.

## Source-batch reconciliation

General network manager and socket/listener source, congestion/protection source,
and opt-in Linux capture source are implemented, with their explicit pinned SDK
limitations recorded below. Native-arithmetic policy reconciliation removes
private algorithm preflights from Call, sent events, controllers, estimators, FEC,
clock conversion and built-in task delays. The shared manual-clock adapter also
delegates finite NTP conversion without a negative-fraction filter. Direct unit,
callback, ownership, buffer and role protections remain in place.

Remaining work is final test-source coverage, public-root reconciliation against
those tests, then matching artifacts and combined qualification. Existing source
notes below describe implementation chronology, not unresolved batch order or
runtime proof. No outcome is certified by source presence or checklist counts.

## Implementation-only additions after batching request

- Added an owning default-local-address provider handle and trait, with actual
  SDK provider calls, signed/open family inputs, nullable address results, atomic
  panic deactivation and contained adapter-owned destruction.
- Network provider-link setter/getter now retains the actual native provider
  allocation. Shared networks see the same link; native deep Clone intentionally
  omits it. Returned provider aliases own their dependency independently.
- Added general NetworkManagerFactory/NetworkManager product dispatch and local
  plus ProductionSession selection. Products are constructed/initialized on the
  existing network role; rejected creation never selects a stock manager. Native
  facade teardown dispatches product/cached-network retirement to that role even
  when SDK ConnectionContext releases the facade on signaling.
- Every returned native network allocation is retained by actual pointer identity,
  without changing returned order or duplicates. Change/error delivery uses the
  existing generation-guarded notifier and tracks multiple updating clients.
- Product receivers allow shared reentry and sequence-local interior state.
  Product failure is local; the registration exposes aggregate failure without
  disabling healthy sibling products. Stop/final release contain unwinding.
- Controlled selection requires the explicit controlled factory capability and
  existing caller-owned packet providers. No new worker/clock progress is added.
- Added tagged owning native Network subscriptions for adapter, preference and
  slice changes. Cancel/drop removes the real SDK receiver. Closures survive
  self-cancel and SDK deferred receiver cleanup; closure and payload unwinding
  are contained. Same-channel recursive mutation and registration return a
  typed in-progress error instead of entering CallbackList's fatal checks.
- Added manager-context mDNS responders, owning network provider links and the
  manager's native GetMdnsResponder dispatch. Source code preserves binary name
  bytes, exact address/removal callback values and asynchronous completion on
  the existing role. Address mismatch is rejected before the reached Port DCHECK.
  A failed or retired selected responder stays installed instead of permitting
  a plaintext-IP fallback, including a link retained by another live consumer. No name synthesis or mDNS protocol replacement is added.
- Manager-scoped retirement cancels pending native callbacks, clears dispatch
  pointers before role release, and retires non-Send Rust providers on that role.
  Completion aliases are mutex-guarded Send/Sync single-submission handles.
  Context/responder aliases remain inspectable after retirement but reject work.
  Resource enrollment is weak and pruned; active provider frames lease the actual
  Rust provider across reentrant scope retirement. Provider/callback/capture
  panics are contained and observed separately from callback admission success.
- Added explicit Network::notify_change publication through the native SDK
  Notify methods, using the actual owning network as sender. It preserves the
  native receiver identity and rejects same-channel recursive Send. Extend the
  unrun subscription fixture with unchanged-field publications on all channels.
- Full packet socket products still remain to implement. Manager observation
  and production retirement additions are recorded below; neither source presence
  nor mDNS-local leases prove whole-graph callback-triggered retirement safety.
- New unrun fixtures in tests/network_manager.rs cover selected context/init,
  actor enumeration and migrated shutdown, cross-thread signal retirement,
  rejected construction and contained initialization/product-drop panics.
  tests/network_subscriptions.rs covers identity/clone separation, self-cancel,
  native receiver-cleanup Drop reentry and hostile callback/capture payloads.
  These are implementation fixtures, not passes or installed-consumer evidence.
- Earlier source review found NetworkStateEstimator/Factory and application
  TargetTransferRateObserver coverage gaps. Estimator source and the Call observer
  construction limitation are reconciled in the later #6 source batch below.
- Added provider-callback admission scopes shared by the existing panic helper.
  Tasks on caller-owned Rust queues retain panic containment without being
  mistaken for provider loans. Current()-snapshot posts may target private SDK
  queues and keep callback admission restrictions to prevent self-delete waits. Metronome callbacks now use the same helper; metronome provider
  final destruction is contained, including hostile payloads.
- Controlled drivers now track actual weak peer/factory ownership edges, not
  configuration identities or label-based lookups. Enclosing dispatch turns
  upgrade them to local strong leases, including owners created during a turn.
  Leases drain only after the SDK turn returns, outside RefCell loans; nested
  destruction appends to the next retirement batch. Idle edges remain weak.
- Driver/factory pumping, native task ticket execution/cancellation, Network
  notification setters/publication, standalone mDNS requests, controlled
  factory/peer creation and native peer/factory Drop now enter owner turns.
  Driver ownership outlives retained factory provider fields.
- Explicit peer close is idempotently admitted before native Close, rejects
  active native provider callback reentry and forbidden worker/network-to-
  signaling blocking calls, and restores the Rust flag on native refusal.
  Native close marks its latch before entering SDK Close as well.
- PumpResult.blocked distinguishes denied provider/retirement reentry from an
  idle queue. No tasks or time are advanced on that path; deadline snapshots
  remain actual. run_ready reports false for a blocked driver dispatch.
- Added manager observation sources/subscriptions bound to the actual SDK
  facade's changed/error lists before Initialize, including creation-time pending
  registration, synchronous Notify and distinct NotReady/Retired/in-progress
  errors. Self/sibling cancel retains captures through SDK receiver cleanup;
  callback and capture Drop failure latches survive cancellation/retirement.
  Scope teardown removes receivers/releases captures on the network role and
  clears the facade pointer. SDK Stats is a standalone value, not a live query.
- Read-only production trace found no already-wired executor independent of the
  owned SDK roles. Reposting final destruction to signaling/worker/network or an
  owned media queue does not establish a safe whole-graph retirement fence.
- Added src/production/retirement.rs: one pre-admitted, single-use production
  retirement worker, created only after successful native factory construction.
  Native-context/callback final Drop moves the whole exclusive actor graph to its
  one-slot state, detaches without joining its SDK caller, and permits existing
  native Close/queue deletion/destruction waits to run off owned roles. No spawn,
  allocation or channel-failure path is introduced at handoff. It accepts no
  application commands or normal operation proxying. Ordinary caller Drop
  destroys the graph synchronously then cancels/joins the unused worker. Resource
  admission failure rejects construction; controlled rejection starts no worker.
- Session peer/channel/source close and shutdown now preflight before
  unregistering observers or doing partial cleanup. Native factory construction
  also rejects forbidden caller-to-selected-role invocation edges before
  manager initialization or other SDK BlockingCall operations. Readiness wakes and final waker Drop use the common callback
  scope and hostile-payload containment, including foreign producer threads with
  no SDK queue TLS. Callback admission is not an identity or ownership proof.
- This remains partial lifecycle hardening, not a whole-graph safety claim. All
  producer fences, local-owner paths and remaining blocking control entries
  require applicable ownership/affinity analysis and matching qualification.
- Added the owning packet metadata foundation in src/packet_socket_values.rs
  and native/packet_socket.{h,cc}: send/authentication parameters, optional network
  IDs, packet/protocol tokens, sent metadata, and deep-owned receive spans and
  source addresses. CopyAndSet and both legacy payload/default-address choices
  use actual native receive operations. The legacy path rejects times below -1
  and INT64_MAX before the reached constructor DCHECKs; ordinary receive-copy
  metadata retains optional, negative finite and infinite native timestamps.
  Open scoped enum tokens and ECN bytes retain their values. Unscoped DSCP,
  Socket::Option, AsyncPacketSocket::State and decryption tokens check only the pinned enum representation
  before conversion, retaining representable unrecognized values. Added the new
  native unit to source hashing and both existing export compiler paths, without
  generating artifacts or updating candidate provenance pins. Socket provider
  creation/installation, listener transfer, event dispatch and TLS option leases
  are still missing; these values are not native transport outcome evidence.
- Added a source-level PacketSocket trait and private native AsyncPacketSocket
  adapter, with all abstract address/send/close/state/option/error operations.
  GetOption returns either an initialized value or the exact nonzero native
  result, leaving the SDK output untouched on error. Sends deep-copy the SDK
  payload before provider reentry. Product-local panic/deactivation and final
  Drop containment retain separate aggregate diagnostics; Close still attempts
  cleanup after operational failure. Address-query failure does not turn the
  SDK CopySocketInformationToPacketInfo helper into successful metadata output.
- Added sequence-bound PacketSocketContext and cross-thread PacketSocketEvents:
  synchronous native close/ready/connect/receive/sent/address notifications,
  queued owning submissions, same-channel admission, and retirement-synchronized
  role loans. Provider calls lease the actual Rust allocation; native callbacks
  run without the submission mutex. Direct notifications enter controlled owner
  turns. Posted messages own values only, not Rust products or consumer closures.
  SDK product teardown clears the event target/role before Rust product release.
  The adapter's general factory/local/actor source wiring is now implemented
  below. Complete listener/standalone subscription contracts remain unfinished.
  Product-frame leases alone do not establish a
  facade/whole-graph lifetime fence. No installed-consumer or safety pass is
  claimed, including callback-triggered sender destruction. A focused read-only
  receive-metadata audit found the DTLS/STUN piggyback status DCHECK: delivery
  now rejects decryption status 3 without changing the value-copy domain. Unknown
  ECN bytes remain unmodified; no reached ECN guard was indicated. The RTP media
  path additionally requires present, finite arrival time, whereas non-media ICE
  legitimately accepts absence; finite negative times pass the traced check.
  The actual Call receiver seam added below now rejects nonfinite RTP arrival
  without restricting control/RTCP packets or inventing a timestamp. The later
  exact-clock metadata below observes the actual selected Call context without
  adding a clock/history arithmetic profile. Sent-consumer review found unconditional
  Timestamp::Millis scaling and allocation-only DataSize conversion. Delivery
  preserves original signed send times and allocation sizes rather than
  preflighting SDK scaling or accumulation. Unknown packet/protocol tokens and
  absent network ID remain unchanged. ProcessSentPacket's untracked sizes, send
  timestamps and cumulative/shared accounting remain libwebrtc-owned. Required
  boundary and lifetime qualification is distinct from SDK arithmetic.
- Added a native socket scope owner distinct from metadata aliases. Its RAII
  destructor retires on the captured network role even when contexts remain
  alive, including rejected construction. Enrollment is weak/pruned and retains
  actual resource edges, not logical IDs. Retirement disables every enrolled
  target/role before releasing any product, outside all submission locks. Packet
  provider allocations now live in those states and entry frames lease them;
  late SDK facade destruction after scope retirement uses no stale role pointer.
  The general SDK factory now owns/wires this guard before role release.
- Added source-level ListenSocket/Context, private native AsyncListenSocket,
  move-only AcceptedPacketSocket, recoverable rejected submissions and atomic
  cancellation/status tickets. Accepted products inherit the actual listener
  environment/scope; cross-scope transfer rejects by actual owner identity.
  Unbound construction and rejected creator capture Drop contain panics and
  retire the new event node. Pending native products remain owned on the role;
  queued tasks/completion aliases own no products. Cancel/retire/drop destroys
  unclaimed products on-role; standalone delivered products transfer exactly
  once through the SDK callback. Private ICE acceptance is now explicitly
  unavailable due to the missing actual-network admission hook below; no raw
  product reaches TCPPort's unchecked incoming constructor. The later source batches
  add SDK-managed synchronous publication and controlled owning standalone
  acceptance/observation plus asynchronous external creators, as recorded below.
  Production standalone ownership is source-wired in the later batch below.
  Factory source wiring is implemented below.
  Pinned TCPPort exposes no pre-constructor network-membership admission seam;
  selected private ICE accepted-socket creation/publication is explicitly
  NativeConsumerUnavailable, not provider failure or fabricated address success.
- Read-only listener review found a concrete stale-target hazard when Prune
  drops a cancelled product and its destructor retires the listener. Run now
  resolves/rechecks scope, failure and facade only after pruning; aborted delivery
  retains local unique ownership and records Retired/ProviderFailed. Submission
  likewise rechecks all admission edges after pruning. This repair is source-only.
- ObservedCall now delegates PacketReceiver through the actual Call owner and
  rejects nonfinite RTP arrival before Call::DeliverRtpPacket (all media types,
  including BWE-only RTP). RTCP delegates unchanged; non-RTP/control packets do
  not enter this checkpoint. Finite negative arrival remains unchanged. The void
  SDK receiver has no rejection result; binding CallAdmissionStats reports a
  saturating rejection counter and live/absent Call state through PeerConnection
  and ProductionSession, including after close. This is not RTCStats or packet
  synthesis and uses the existing owning Call observation, not a profile/ID join.
  Identity-checked creation-clock bounds are diagnostic metadata only; finite
  arrivals pass through unchanged, without a first/latest history mirror or
  clock-subtraction admission policy. SDK internal arithmetic is upstream-owned.
  Actual boundary/graph-retirement qualification remains required.
- Added src/packet_socket_factory.rs and actual private SDK GeneralSocketFactory:
  all five native creators, exact incoming SDK Environment, existing open flag
  values and byte-valued TLS options. Wired PeerConnectionFactoryBuilder and
  ProductionSessionBuilder socket_factory/provider selections without replacing
  simulated endpoints. Conflicting selectors and controlled-capability mismatch
  reject before role startup; capability panic disables/rejects the recipe.
  Product contexts retain the recipe, constructor guards retire rejected event
  nodes, and factory scope retirement precedes source/environment release.
- TLS dependency proxies now share their actual backing. SocketTlsVerifier owns
  that backing rather than borrowing the SDK pointer; proxy entry frames retain
  it across reentrant destruction and synchronous creation/rejection. All native
  non-null peer verifier assignments use the binding proxy, including TURN-CA;
  its native-only calls serialize, while Rust verifier callbacks hold no native
  mutex. Closed origin was source reviewed, not inferred from a pointer registry.
  Internal CA also overrides leaf Verify, avoiding its inherited fatal default.
- Legacy packet-factory DNS reuses the existing native adapter with exact factory
  environment and retained recipe. New socket-factory DNS failures have separate
  product latches and aggregate diagnostics, so a failed socket/other installation
  cannot silently change a resolver's result. Existing DNS-factory behavior is
  unchanged. Failed required DNS products report the existing binding error, not
  stock resolution or fabricated addresses.
- Source review ed56ffb0 found no concrete factory/TLS/selection ownership or SDK
  signature defect. This is not a compiler, runtime or sanitizer result. Source
  factory installation is delivered; public socket contract completion (#9) and
  reached admission/whole-graph safety (#10) remain required before qualification.
- These additions have not been compiled or runtime qualified. Bridge/provenance
  pins are intentionally deferred until the full implementation batch is done.

- Added controlled standalone OwnedListenSocket source through the selected
  server-TCP recipe, with actual native getters, one ownership-taking accept
  receiver, native passive observers and local callback/capture failure metadata.
  Standalone publication requires an explicit receiver instead of TCPPort's
  first-turn readiness proof. Each accepted child owns an independent native
  scope/driver/environment. Per-Send escrow transfers exactly once at the actual
  native owner callback; cancelled ownership leaves no raw-pointer second owner.
  Parent retirement cancels unclaimed products without retiring adopted sockets.
- Added Send/Sync ListenSocketEvents asynchronous creator submission on the
  existing listener role. Only Send creator data crosses threads; actual products
  remain sequence-local, with subsequent queued transfer and atomic status.
  Creator envelope destruction terminalizes unhanded work; scope retirement also
  terminalizes pending creator tickets. Cancellation races preserve terminality.
  Creator-capture Drop containment has separate retained failure diagnostics.
- Inspected the pinned SDK CallbackList implementation: unsubscription of a later
  sibling during Send can encounter its pending-tag DCHECK. Packet/listener
  cancellation now deactivates immediately and defers native removal until Send
  returns; callback capture cleanup stays admitted through reentrant destruction.
  SDK closures retain actual tag metadata and weak sources. Retirement clears
  Rust captures on-role, leaving only inert metadata in late native shells.
  Read-only focused reviews found no concrete remaining delivery/FFI/lifetime
  defect in these source paths. Reviews are not compilation/runtime evidence;
  subsequent production standalone source is recorded below; remaining
  public/transitive contracts stay open.
- Added PacketSocketRuntime consuming an actual owning NetworkThread and exact
  Environment/production lease. Its non-Send role token enables production UDP,
  client TCP/client UDP and server TCP standalone factory creation. Core/frame
  owners hold the actual runtime, while listener contexts retain a weak edge and
  adopted accepted children acquire independent strong role/scoped ownership.
  Queued runtime work captures only a weak owner, so it cannot implicitly retain
  the Thread that owns that queue. Callback/capture Drop panic is observable.
- Runtime final destruction reserves a one-shot retirement worker at construction,
  with no ordinary application dispatch through it. SDK/provider final Drop hands
  off actual Thread/environment/production lease without a spawn or self-join;
  caller Drop is synchronous and cancels/joins the unused worker. Explicit resource
  destruction order is Thread, environment, production lease. Focused read-only
  source review found no concrete scoped defect, not build/runtime qualification.
  Legacy standalone DNS is source-wired in the next batch; final
  factory/transitive reconciliation is recorded after the DNS source batch.
- Added OwnedDnsResolver production-role and controlled factory creation, both
  actual native Start overloads, non-owning contexts, owning callback/query
  registration and independent callable native result owners. Actual role,
  environment and recipe survive through result/active-frame destruction.
  Standalone creator panic rejects instead of returning a fake successful failed
  product; installed SDK-required non-null fallback remains unchanged.
- Added explicit single-query/reentrant operation admission and per-product
  failure checks before exposing native result output. Callback cancellation and
  capture cleanup stay on the native role, outside submission locks. Completion
  remains post-only; retained query/context aliases cannot revive the resolver.
  Rust provider loans reject recursive acquisition of the same actual provider
  allocation before its mutex, while other threads retain ordinary serialization.
  All of this is source-only, not compile/runtime/sanitizer evidence.
- Focused DNS lifecycle and pinned packet-factory/socket/listener API reviews
  found no concrete remaining #9 source defect or missing named operation. #9
  is complete for source delivery only. #6 is active for remaining congestion/
  protection implementation. Reached admission and whole-graph teardown remain
  #10; final fixtures/actual-consumer/lifecycle/sanitizer evidence remain #11/#3.
  No intermediate builds/tests/artifacts were run, and source stays uncommitted.

- #6 source audit confirms existing controller callbacks/factory and predictor/
  FEC/video-bitrate operations/adapters are present, not proof of runtime effects.
  The initial gaps were NetworkStateEstimator/factory and application observer
  coverage. Native Call's existing internal target observer must not be replaced
  with an application observer or controller-output echo.
- Added ProductionSessionBuilder default video bitrate allocator factory/provider
  selection, applied only when a peer has no explicit allocator dependency.
  Default registration survives actor migration and retires after native peer
  dependents but before the session's factory role owners. Existing per-peer
  native installation remains the consumption path. This is unqualified source;
  estimator/observer installation and applicable arithmetic/loan work remain #6.
- Added owning NetworkStateEstimator/Factory provider contracts and adapters for
  every pinned virtual. Optional products/estimates and nullable actual trial
  input remain distinct; copied actual trial views survive factory destruction.
- Added explicit NetworkControllerFactory::goog_cc(GoogCcFactoryConfig), with
  mandatory explicit RFC 8888 state and owning optional estimator/predictor
  registrations. Native delegates actual SDK Create/GetProcessInterval and retains
  the borrowed predictor owner and product execution-domain/registration owners.
  Existing injected-controller environment trial rejection remains in force.
- The pinned default SDK send path does not consume the old peer-factory
  predictor slot, despite copying it into CallConfig. Local/production build now
  reject that misleading standalone registration; actual installation uses the
  explicit native GoogCC config. Qualify that error and the real prediction path.
- Estimator conversion checks native unit representations. Finite capacities,
  IEEE diagnostics and native infinities otherwise pass through unchanged;
  logging widths, probe factors and AIMD trial parsing remain SDK-owned.
  Source/ABI/ownership review is not compile/runtime qualification.
- Pinned CallConfig has no send-transport factory seam, and SDK Call::Create
  constructs/registers its internal observer before ObservedCall is created.
  An application Call observer cannot be installed at that boundary without
  changing/replacing SDK Call construction. No SDK patch or fake callback echo
  has been added. User direction confirms finishing without libwebrtc patches:
  record this unavailable application Call hook and move on. Preserve Call's real
  observer and actual native rate effects; do not claim application hook support.

- Controller/protection source preserves the explicit no-libwebrtc-patches
  observer limitation. FEC results pass through without a payload-budget
  quarantine. No new test/build/artifact/runtime result is claimed.
- Added private actor Camera/Desktop source ownership and native capture methods
  in src/production/capture.rs, publication/replacement via actual video tracks,
  and capture shutdown before source/factory role release. Local/native capture
  creation/control/destruction now uses the retained SDK signaling role, including
  V4L2's API sequence after actor migration. Callback/foreign-role admission is
  rejected before blocking; controlled capture rejects before platform calls.
- Added complete signed CameraSettings and native result errors, stopped module
  creation plus complete StartCapture/StopCapture separate from terminal close,
  settings/name/running/rotation/apply operations; complete capability/orientation/
  best-match queries and product IDs; desktop display IDs and actual SDK desktop
  select/focus/occlusion/exclusion/frame-rate/resolution/delegated controls.
  Finish desktop options/frame/resource/callback/creation coverage before #7
  source closure, then qualify these installed physical or faithful native
  consumer boundaries, migration/replacement/stale frames and final release.

- #7 source wave adds src/desktop_capture.rs and src/camera_sink.rs. Desktop
  options are actual SDK values with retained role, resources and independently
  owning exact clock delegates; native clock identity is separate from forwarding
  adapter identity. Verify empty/default + selected/global clock, flags, resource
  absent/create/share/clear/native identity, original-owner drop, actual generic/
  screen/window installation and restore-token selection, migration and shutdown.
  Desktop source timestamps now read the installed options clock.
- Raw capture uses existing SDK unregister/register with a documented non-atomic
  gap and restores the binding-owned video slot. Verify mutable bytes, native
  signed settings/rotation/capture-time, ignored return codes, panic quarantine,
  overlap with replacement/stop, snapshot independence and final callback Drop.
  Focused raw-sink and separate options/actor/clock source reviews found no
  concrete defect; neither is test/build/runtime evidence. FullScreenWindowDetector
  has no Linux consumer; protected raw desktop factory helpers are not public scope.
- Remaining #7 frame work must preserve CPU-data absence versus texture/shared
  memory, all native metadata and explicit copy/share/helper contracts. Inspect
  actual helper arithmetic: Basic/SharedMemory constructors multiply signed
  dimensions by four, rect adds origin and size, intersection helpers subtract/
  translate coordinates and cast rounded scale adjustments, data-at-position
  multiplies stride/coordinates, and black helpers assume CPU storage/layout.
  Guard reached hazards, do not copy padding past a cropped frame's final row,
  or claim a GPU texture is copied CPU pixels. Shared/SDK-owned capture pixels
  must not gain uncoordinated mutable Rust aliases. GetMetadata SessionDetails
  contains unowned GObjects; an owning output must retain them, not just copy raw
  fields. The pinned ScreenCastPortal getter returns empty session details;
  preserve that absence rather than inventing session identity.

- #12 is an actual source blocker, not a verification-only concern: ordinary
  webrtc signaling Thread does not pump the thread-default GLib context used by
  portal async callbacks/subscriptions. Supply progress and serialize native
  lifecycle/delegated operations with callback delivery; account for pending GIO
  cancellation and final context/resource release without guessed timeouts or
  private GLib layouts. Native GIO/GLib linker dependencies already exist. The
  follow-up Explorer failed from resource limits before proposing a repair;
  continue bounded source work locally rather than claim unavailable research.
- Stable CameraDeviceInfo products now expose actual device count, name-buffer
  capacities/product absence, capability count/index, orientation, best-match
  index/settings and signed native errors, with private actor IDs and SDK-role
  close/drop. Qualify stable product caching, invalid index/too-small buffer and
  empty device set versus null factory product, native Linux dialog -1, migration,
  original-owner drop and retained role release. CreateCapabilityMap is an impl
  method, not missing public root scope. VideoCaptureFactory's explicit
  VideoCaptureOptions overloads and that options Init callback/session/fd lease
  remain to implement; do not mistake legacy null-options creation for them.
- Desktop ARGB publication no longer has an arbitrary 4096-pixel dimension cap;
  admission guards signed image/row arithmetic, including negative stride, while
  retaining actual geometry. Qualify valid large images and cropped/padded row
  bounds with the forthcoming owned frame fixtures. This is source only.

## Verification requirements by risk

### Owning network values

Unrun fixtures: `tests/network_values.rs`, `tests/network_subscriptions.rs`.
Manager fixture: `tests/network_manager.rs`.

- Binary interface names/descriptions, exact prefix/family/key and native defaults.
- Shared handles retain the same SDK allocation; SDK Clone is a distinct value
  without copied subscriptions/provider links. Retained values survive original
  handle destruction.
- SetIPs preserves SDK membership-based change detection, duplicate/order quirks,
  flags and replacement despite a false change result. IPv6 best-IP selection
  follows actual deprecated/temporary/link-local/ULA rules.
- VPN/type resets, IDs/scope/preference/active/ignored/slice fields are native.
- Native costs consume the supplied retained field-trial view after environment
  close, including differentiated cellular/VPN/slice behavior and inverse mapping.
- Only the reached VPN-underlying-VPN cost DCHECK is rejected, not storing it.
- Interface-name inference protects reached negative non-EOF `isdigit(char)`
  inputs while retaining safe unrecognized binary names and EOF-byte behavior.
- IpNetworkMask retains raw address/signed length, native equality and safe
  cross-thread immutable reads. Preserve the existing configuration NetworkMask.
- Default-local-address provider: query the actual SDK interface, preserve None
  versus Some(Unspecified), unsupported/open family values and true native results.
  Observe retained aliases after original handle/network drop, replacement/clear,
  native Clone dropping the link, concurrent reads, reentry, panic deactivation
  and final provider destruction. Manager-origin native links must gain explicit
  manager-backed owners during integration, not silently become None.
- Compile-fail proof that Network is not Send; no physical network side effects
  are introduced by value construction or controlled use.

### Manager/provider and socket installation

Reuse faithful fixtures in `tests/network.rs`, `tests/network_monitor.rs`,
`tests/socket_address_values.rs`, `tests/async_dns.rs`,
`tests/local_network_permission.rs`, `tests/certificate_generator.rs`,
`tests/certificate_resource.rs`, `tests/tls_values.rs`, and controlled/production
peer tests. Add focused missing cases only after implementation batching.

- Observe selected arbitrary manager enumeration and actual candidate/transport
  creation on both local and ProductionSession paths, not registration echoes.
- Stable native network identity across repeated enumeration; retain every
  returned network until the manager's contract permits retirement.
- Actual Initialize/Start/Stop, permission, any-address/default-address behavior,
  VPN masks, changes/errors and network field/subscription behavior.
- For native Network subscriptions cover event identity and shared aliases,
  native deep-clone isolation, self-cancel/drop and sibling cancellation,
  reentrant registration/mutation and same-value mutation, closure Drop reentry,
  and hostile panic payloads. Assert no SDK recursive-Send/AddReceiver checks
  are reached and closures retire only after native receiver cleanup.
- Check explicit Initialize before first Start: pinned ConnectionContext does
  not initialize injected managers. Observe shared read reentry, product-local
  failure versus healthy sibling products, Stop cleanup after failure, and
  actual native trial/environment context identities.
- With two peers sharing a manager, closing one must not deactivate the other's
  notifications or cancel its queued work; last Stop/restart must cancel stale
  generations. Failed queries before Start must still deliver an error once
  notifications become active.
- Factory rejection and custom-product absence must not select stock products
  unless that exact native optional contract permits absence.
- All sequence-bound product operations, notifications, Stop and final drop must
  run on their native role. Pinned SDK ConnectionContext releases its manager on
  signaling, despite network.h's single-thread contract. Native bridge can reject
  after manager creation. Cover successful teardown and each material rejected
  creation boundary with actual product drop observations on the network role.
- Resolve callback-triggered owning graph retirement before qualification:
  product shared receivers permit read reentry, but do not by themselves retain
  the SDK facade, peer or factory graph if a controlled callback closes/drops
  an owning peer/factory. Native CallbackList prohibits destruction during Send.
  Also protect production role callbacks from reentering blocking peer/session
  operations on disallowed native role threads. Product docs are not a safety
  proof or a substitute for the required owner-frame lease/admission guard.
  Read-only ownership review confirms PeerInner owns its native peer and an
  Rc<FactoryInner>; final Rc release starts native retirement. Factory fields
  drop the native factory before its role owners. Controlled roles borrow the
  same creator thread, so BlockingCall can destroy the graph inline inside a
  callback. Historical native close's closed flag was set after Close; current
  source moves the latch before Close and adds admission separately. No
  preexisting general deferred-owner utility was found (PeerCallCreationScope is only a creation association frame).
  Any repair must cover explicit close and last Drop, retain role owners, and
  extend through the enclosing SDK operation/task, not just the Rust provider
  function's return. Do not rely on mDNS-local leases or CallbackList tombstones
  to keep the whole SDK caller graph alive.
- Qualify the controlled owner-turn changes with actual SDK pending work:
  drop final peer/factory handles in provider/network callbacks; prove no native
  destruction occurs until the enclosing SDK dispatch/publication returns.
  Cover owners created and dropped during a turn, sibling peers/factories,
  nested retirement/capture Drop, panic unwinding, zero-budget pumps and weak
  edge pruning. No idle strong cycle, extra pump, wait or time advance is allowed.
  Check direct cooperative TaskQueueFactory pumping and task-ticket execution,
  not only ControlledPeerDriver::pump. Check dropped-ticket capture cancellation.
- Verify explicit close: normal caller and caller-posted tasks still work;
  native provider/context-task reentry and forbidden role calls return InvalidState without
  blocking or changing peer state. Repeat close remains idempotent. Close
  flags must already be set during SDK Close callback reentry. Metronome
  period/request/provider Drop must contain hostile payloads and restore scope.
  Newly written provider_panic scope and metronome hostile-payload unit fixtures
  are unrun and do not replace actual SDK lifecycle/queue qualification.
  Before parity qualification, audit conservative Current()-snapshot callback
  admission against disjoint native queues/actors. Do not present missing queue
  ownership proof or a binding-side guard as an inherent SDK locality restriction.
- Retire queued notifications before role-thread release. Exercise retained
  notifier aliases, Stop/restart generations, cancelled delivery, panic/reentry,
  migrated close and final actor destruction. No identity registry or second
  application command executor is introduced. The new single-use production
  retirement worker must never receive ordinary work or controlled owners.
- Qualify production last Drop from actual network/worker/signaling callbacks,
  SDK-internal codec/event queues and foreign producer/readiness wakes. Observe
  current callback returning before dependent native teardown, every selected
  product's final release on its required role, role-thread release last, and
  eventual exit of the single-use worker. Include closed actors, no peers,
  pending callbacks, migration, caller-owned native queues and nested actor Drop.
  No permanent strong cycle, self-join, sender failure, retained callback use or
  release of the production/randomness lease before dependent teardown is allowed.
  Check failed worker admission and normal caller destruction/error paths.
  Source trace has not demonstrated a same-caller local-production final-peer
  Drop callback from encoded input (that source publishes on signaling), but raw
  source downstream SDK scheduling is not yet fully established. Do not extend
  Send to local handles or claim all local-owner fences from actor handoff.
  Post-rebase callback-scope source reconciliation now covers inherited video
  factory/codec, selector, encoded/decoded/FEC output, NetEq, frame crypto and
  transformer, audio device/custom processing/mixer/dump, and actual audio
  factory/decoder/frame SDK entries. Ordinary Rust caller-audio product and
  registration invocations remain caller roots, not native callback loans;
  provider destruction is scoped independently. Legacy double-catch payload
  cleanup delegates to the existing safe panic_payload helper instead of
  running arbitrary payload destructors. New private standalone-dispatcher
  admission tests and caller-audio root-versus-SDK-entry checks are unrun and
  do not prove installed media behavior or SDK codec-queue origins. The selector
  panic fixture now expects unknown payload erasure, not destructor execution.
  Continue actual foreign completion and disjoint-actor/queue admission audit;
  std catch_unwind alone is not admission or hostile-payload containment, and
  queue TLS alone does not identify a loan.
- Verify manager observations on actual selected facades, not an independent
  callback dispatcher: creation-time registration, Notify before/after binding,
  changed/error ordering, same-channel reentry rejection, other-channel use,
  self/sibling cancellation, post-Send capture Drop reentry, hostile callback/
  capture payloads, rejected creation, Stop/restart, migrated shutdown and inert
  retained aliases. Retired source metadata must not retain SDK callbacks/roles.
- Packet values must preserve every SDK field/default and exact enum value,
  optional network ID/arrival time, zero/large packet sizes, signed IDs/times,
  authentication key bytes including NUL/high bits, and batch flags. Receive
  copies must retain payload/address after the original SDK loan ends, preserve
  ECN and timestamp infinities, and use native CopyAndSet without changing other
  fields. Legacy -1 omits time; zero and finite positive times remain present;
  below -1 and INT64_MAX must reject before SDK DCHECKs. Check unspecified
  default source versus explicitly supplied binary hostname/address. Token
  representation checks must not clamp or reject representable unknowns. These
  value fixtures cannot prove socket factory installation or actual transport.
- Qualify packet-socket observations on the actual selected SDK lists: all five
  event kinds, before-facade registration/binding, rejected construction, exact
  sent/address/error copies, SDK receivers beside Rust receivers, self/sibling
  cancellation and panic-local cancellation without skipping unrelated receivers.
  Same-kind registration/recursive Notify must return InProgress before native
  Add/Send, not become an implicitly deferred receiver. Other-kind Add/Notify
  must remain possible. Keep capture owners through SDK tombstone cleanup and
  all nested different-kind Sends; capture Drop can cancel/register/publish,
  retire sibling resources, retain diagnostics or panic with hostile payloads.
  Check creation on the actual network queue versus its OS-thread-only context,
  wrong-cooperative-queue admission, immediate logical cancellation followed by
  queued tagged removal, queued cancellation versus retirement and late metadata
  destruction. SDK close-list sequence checks must not see the wrong queue.
  Qualify deprecated untagged connect separately: actual legacy overload,
  disabled delivery/capture release on cancel and native weak receiver shell
  lifetime until facade Drop; do not claim physical native unsubscription or
  an independently bounded legacy receiver list.
- Qualify controlled standalone UDP/client-TCP/client-UDP construction through
  the registered recipe: exact environment/options, None versus panic, capability
  and driver/clock mismatch, creation-time contexts and rejected capture cleanup.
  Prove the actual facade dispatches address/state/send/send-to/close/options/
  error operations, with exact signed codes, nonzero GetOption/no output, provider
  failure, continued Close/error inspection and inert retained contexts. Before
  binding use NotReady, after retirement use Retired, not successful defaults.
  Retain the actual driver/environment/recipe after caller handles drop. Native
  operation/Notify frames must survive callback-time owner Drop through SDK
  cleanup; same-thread different-queue lifecycle cleanup must establish the
  actual SDK Thread identity without pumping, waiting or advancing time. No
  metadata alias may own the native retirement guard or keep live Rust captures
  after retirement. Production standalone and controlled standalone listeners
  are now source-delivered, not complete factory/transitive qualification.
- Qualify production PacketSocketRuntime with the actual consumed NetworkThread:
  reject borrowed controlled roles and cooperative/unqualified environments;
  observe exact selected environment, native SDK queue identity and all four
  creators without stock substitution. Callback/capture failures must be contained
  and observable. Dropping the last caller handle before queued work must discard
  weak work rather than retain a hidden queue/role ownership cycle. If a task
  already upgraded, its role/product owners must retain the actual thread.
  Drop final runtime handles in source/receive/accept/capture-Drop callbacks,
  ordinary callers and foreign readiness callbacks; observe native callback return
  before SDK Thread join, all role-local products released before role owner,
  environment and production lease last, and one-shot worker eventual exit.
  Accepted children must retain runtime independently after listener/handle Drop.
  No Thread wrapper around a borrowed pointer, extra application executor or
  synchronous caller wait is allowed. Constructor worker/lease admission failures
  must clean up the consumed owning Thread and never publish a partial runtime.
- Exclusive receive cases on an actual owning standalone facade: second live
  registration, old logical cancellation with native removal still queued,
  prequeued replacement ahead of removal, self/sibling cancellation, callback
  panic, nested other-channel Send and capture Drop reentry. No second Register
  may reach the SDK CHECK while an inactive-but-bound receiver occupies storage.
  SDK AnyInvocable storage must survive its own invocation; only then deregister,
  prune and release Rust captures. No passive receiver API on SDK-managed peers,
  no loss of packet bytes/source/arrival/ECN/decryption fields, no borrowed payload
  escape, no hidden re-registration/deferred replacement. Include scope retirement
  during receive and late subscription/context/sender aliases.
- Exercise real UDP, TCP connect/listen/accept, readiness, packet receive/sent,
  address-ready, close, error/options, client UDP and batch flags where supported.
- Qualify both synchronous notification and cross-thread queued admission, with
  original sender identity and payload lifetime throughout native callbacks.
  Retired aliases and already queued messages must never access released roles.
  Same-channel reentry must reject before CallbackList checks, including cleanup;
  other-channel use must remain possible. Exercise send/read/control reentry,
  exact signed operation and option results, no output write on GetOption error,
  panic-local failure and continued Close cleanup, and contained final Drop.
  CopySocketInformationToPacketInfo must use the actual live virtual address
  operation and reject its failure without partially updating public output.
  Product-local diagnostic aliases must survive retirement without SDK ownership.
  Finish reached receive-metadata admission analysis before actual SDK dispatch.
  Separate value-copy range from delivery constraints: retain decryption 3 in
  values but reject delivery; preserve unknown ECN and finite negative arrival
  metadata. Qualify the actual Call receiver finite-arrival checkpoint for A/V
  and BWE-only RTP, without blanket rejection of control-packet absence. Observe
  exact per-peer binding rejection diagnostics for absent/plus/minus-infinite
  RTP time; valid finite negative time and unchanged RTCP/control delivery;
  absent Call, close, concurrent activity and actor migration. No guessed time,
  native error, sink retry or RTCStats count may be invented. For sent events retain signed -1/default
  metadata, packet ID sentinel, unknown enums and absent network ID; check exact
  strict native millisecond boundaries and conditional DataSize conversion.
  Resolve cumulative untracked size addition and send-time subtraction hazards
  across actual shared consumers before permitting installed event dispatch.
- Qualify scope retirement with retained contexts and Send event aliases:
  all siblings must be disabled before a provider destructor can reenter, Rust
  products release on-role, active entry leases release only after their frame,
  rejected construction breaks ownership cycles, weak enrollment prunes, and
  late inert facade/alias destruction never touches the released role. The
  factory scope owner must retire before selected role owners on every path.
- Listener acceptance cases: same-scope environment identity, rejection returning
  the same owned product, cancelled/retired/failed transfers, cross-thread ticket
  cancellation races, receiver installation before queued dispatch, accepted
  product Drop panic and creator capture Drop on early rejection. Include a
  cancelled-product destructor that retires the listener during pruning of a
  second transfer: no stale target, ownership leak or premature release. Observe
  pending versus delivered product ownership and terminal status separately.
  For synchronous standalone notify_accept, prove callback completion before
  return, recursive InProgress rejection, and no second raw owner after claim.
  Exercise receiver reentry, retirement/provider failure after pruning, early
  NotReady, listener destruction and queued metadata cleanup. Installed private
  ICE contexts must return NativeConsumerUnavailable before invoking a child
  creator, before queuing a transfer and before native raw handoff. Posted creator
  tickets distinguish this status from cancellation/retirement/provider failure;
  capture Drop remains contained and owned products are returned or retired on
  their role. Do not compare bind IP to a guessed network, freeze live getters,
  or claim physical incoming TCP success through this SDK limitation.
- Controlled owning standalone listener cases: actual selected server-TCP recipe,
  owning native state/address queries, exact environment/driver retention, None
  versus panic and creator-capture Drop rejection. Require an explicit accept
  owner, reject a second live owner, and reject publication without that receiver.
  Exercise passive observers in actual SDK ordering, owner cancellation before
  its turn, unclaimed escrow retirement, owner callback panic/Drop, listener Drop
  during Send, child Drop during Send and accepted products surviving parent Drop.
  No duplicate raw ownership or metadata alias may keep native access alive.
  Prove callback cleanup/capture-Drop reentry cannot mutate SDK lists in flight;
  sibling cancellation must not hit CallbackList's pending-tag DCHECK. Tag storage
  survives until actual removal/facade destruction; cancelled captures do not.
- External creator cases through ListenSocketEvents::post_accept: foreign producer,
  actual-role creation and subsequent queued SDK transfer (including posting in
  factory construction before native binding), Pending versus Delivered receipt,
  cancel before creation/during creation/before transfer, parent retirement in
  creator, rejected transfer and actual-product role-local destruction. Discarded
  queued creator closures must terminalize receipts. Late sender/ticket metadata
  stays inert and Send; creator capture Drop panic remains observable without
  changing a Cancelled/Retired terminal result. No hidden role, pumping or borrowed
  non-Send product may enter the producer capture. These are unrun obligations.
- Qualify all five general factory creators through native consumers, including
  actual selected default/per-peer Environment identity, all local/remote/port
  arguments, explicit client UDP, TLS flags and binary ALPN/curve fields. Prove
  None rejection/no stock substitution, endpoint-selector conflicts, controlled
  mismatch/capability panic, actor migration and constructor-rejection cleanup.
  Legacy DNS must see the installed environment and preserve actual callable
  results, cancellation and failure independently of other socket products.
- Qualify shared TLS backing through actual TURN TLS/DTLS handshake paths and
  public leaf/chain verification: Rust and internal CA selections, absent option,
  rejection, synchronous creator call, panic latch, concurrent calls, retained
  aliases after peer/socket/factory teardown and reentrant final owner release.
  Check every dependency assignment remains a binding proxy, native CA calls
  serialize without locking around Rust callbacks, no inherited fatal Verify,
  and last actual backing destruction waits for active verification frames.
- Standalone accepted SDK socket ownership transfers through the actual native
  listener callbacks; prove no double ownership, premature drop or leaks on claim,
  unclaimed teardown, callback cancellation/reentry and failure. Private ICE
  accepted sockets must never reach TCPPort's incoming collection under this pin.
- For mDNS exercise original IP/family and binary names, native network link
  identity, shared versus deep-clone behavior, manager GetMdnsResponder and
  actual obfuscated candidate creation. Assert no name/IP synthesis on failure.
  Cover create/remove native callbacks, reference-count policy from the selected
  provider, single-submission/invalid-address retry, cancellation versus posted
  delivery, scope rejection/init failure, retained aliases through migrated close,
  early responder drop, cross-manager retained links with no host-IP fallback,
  registration pruning, provider/callback/capture Drop
  reentry and panic, and wrong-thread completion racing final role retirement.
  No mDNS runtime fixtures have been executed; add focused ones after the
  implementation batch. Observe early-drop pruning and completed-request owner
  removal so historic operations cannot accumulate native callback ownership.
  A provider panic must not deactivate healthy sibling managers or expose the
  host IP by returning a null selected mDNS responder.
- Preserve TLS/DTLS option exclusivity, ALPN, curves and verifier lifetimes.
  Protect reached server-TCP OPT_STUN CHECK and native TLS/DTLS DCHECKs without
  inventing broad unsupported domains or implementing protocol substitutes.
- Qualify standalone legacy DNS on both actual production and controlled roles:
  exact factory/environment/driver retention, capability/clock/queue rejection,
  creator panic versus installed non-null failure products, independent sibling
  diagnostics, both native Start overloads including open signed families and
  actual existing queue delivery without implicit pumping. Second standalone
  Start must reject before native entry under the single-query contract.
  Resolve success/missing family/exact signed errors through live native result
  owners before and after completion, result changes and resolver destruction.
  Result owners retain the actual role/recipe, not the old resolver/provider.
  Distinguish legitimate -1/None from typed provider panic/failure.
- DNS lifetime cases: completion versus query cancel/Drop/owner retirement,
  callback deleting its resolver, capture Drop retiring or reentering it,
  callback/capture panic with hostile payload, late query/context/completion
  metadata, and result/provider Drop panic. Queries must expose pending/delivered/
  local failure without pretending notification delivery is resolution success.
  Native callback shells must retain no role-local captures after cancellation.
  Same-object native recursive result calls reject InProgress; distinct native
  views sharing the same Rust result must not recursively deadlock its mutex.
  Ordinary cross-thread Rust result calls still serialize. No pending request
  cycle, stale queue pointer or role release before final native frame is allowed.
- Preserve DNS/permission/certificate/TLS completion, pending cancellation and
  owning values through peer retirement, retained aliases and migrated teardown.
- Controlled mode remains caller-pumped, rejects incompatible providers before
  side effects and never creates a hidden production thread/socket fallback.

### Congestion and protection

- FEC provider output passes through unchanged, without comparing the encoder
  target to the input payload budget. Native protection subtraction is SDK-owned.
  Qualify forwarding, concurrent provider-failure recheck and healthy sibling
  independence. Allocator output retains native SetBitrate results and direct
  complete u32-sum representation checks, not an input-target budget policy.

- Observe actual GoogCC consumption of selected estimator and predictor through
  native RTP feedback, not just factory invocation. Exercise explicit negotiated
  RFC 8888 choices, injection gate rejection, actual per-controller trial view,
  SDK process interval and rejection of the unused legacy predictor builder slot.
  Retire the creating factory before controller/product use; retain trial views,
  inject independent sibling failures and destructor panics, and migrate/drop
  actors with queued feedback. Qualify effects through actual rates/probes/logs.
- Exercise estimator None/product rejection, direct unit conversion failures,
  native infinite bounds and IEEE diagnostic forwarding. Observe actual native
  probe-trial selection with ordinary supported values. Do not invoke invalid
  private-algorithm arithmetic merely to recreate removed binding rejection tests.
  Provider failure must isolate the failing product and return absent estimate
  rather than fabricated data.

- Qualify actor default allocator factory versus no default/explicit per-peer
  custom/explicit builtin override through actual sender allocator creation and
  native bitrate allocations. Cover migrated actor, multiple peers, independent
  failures and provider/resource teardown before factory role-thread release.

- Observe actual native packet/feedback/rate activity reaching the selected
  controller, predictor, FEC and bitrate products, not only isolated callbacks.
- Preserve feedback order, optionals/defaults, native metric values and errors.
- Creation environment/clock identity and all dependent resources remain valid
  through aliases, callback loans, concurrency/reentry, migration and final drop.
- Reject consumer-required null products safely. Cover direct representation and
  scheduling requirements and faithful probe/rate/interval/FEC forwarding;
  internal native numeric behavior is upstream responsibility.

### Native Linux capture

- Compile/runtime selection keeps core headless and physical capture opt-in.
  Controlled factories reject before device access, threads or hidden progress.
- Exercise camera/desktop public controls, options/source lists/delegated
  selection, geometry/timing, frames and real error/availability observations.
- Exercise replacement, stale frames, pending callbacks, aliases and migrated
  session/actor close. Do not infer device-loss causes from a stalled timeout.
- Distinguish binding absence, supported provider boundaries, absent hardware,
  portal cancellation/denial and real physical execution. Report unqualified
  hardware paths explicitly instead of manufacturing a success fixture.
- Exercise isolated GLib thread-default capture contexts on the existing SDK
  signaling role, progress without an application GTK loop, ordinary actor
  migration, and close with pending portal callbacks. Confirm no global GTK
  sources are dispatched on SDK threads, weak pumps stop without retaining the
  actor, and SDK callbacks cannot reach retired Rust receivers/capturers.
- Preserve the native limitation: PortalGuard invalidation is not a completion
  fence for every canceled GIO task. Ready-context draining is not an all-I/O
  drain. SDK-only pending GTask/context/guard reclamation after early close is
  unqualified and cannot be relabeled leak-free without an observable SDK fence.
  Do not patch libwebrtc, inspect private GLib refcounts, guess grace periods, or
  certify this unsupported drain operation using synthetic success fixtures.
- New CameraCaptureOptions value/copy/flag/owned-FD and synchronous SDK V4L2
  Init/observer source is unqualified. Exercise actual SUCCESS/UNAVAILABLE
  notifications, panic/quarantine/drop, reentry, retained diagnostics, descriptor
  clone/clear ownership and migration. Actor IDs and both actual SDK consuming
  overloads now have source: exercise original options release, dependent product
  retention, explicit disabled flags/native null and rejection before device access.
  Native desktop frame/memory sources and headers are now in the explicit
  adapter inventory; regenerate bridge/provenance with the final candidate.
  The pinned
  PipeWireSession has an additional native cleanup deadlock: Cleanup holds
  callback_lock_ while joining a loop that can be blocked in Finish on that lock.
  The public API provides no quiescence fence or loop access. Without SDK patches
  or private-layout access, do not enter this unsafe PipeWire camera session path.
  Return a distinct binding limitation before native Init/consumption, preserve
  configuration, and never fall back to V4L2 or fabricate native status. Record
  this separately from desktop PipeWire support and absent physical hardware.

- New native/desktop_frame.cc/.h implements actual SDK owning frames and guarded
  pixel/info operations. Qualify geometry near integer limits, signed stride/offset
  validation, all four-byte formats, region/ICC/DPI/optional metadata, SDK copy/move
  semantics, scaled intersection, zero-size behavior and packed-only black guards.
  Captured SDK pixels remain read-only; explicit SDK CPU copies are writable.
- Exercise actual DesktopCapturer callback consumers, not direct Rust leaf calls:
  construction-time installation before selection/Start, start/result/frame order,
  error/null/texture/CPU cases, original geometry/timing, receiver replacement,
  in-flight retention, quarantine with no fallback, SDK Observe clear/restore,
  observer reentry, migrated close/drop and provider body destruction.
- GetMetadata keeps GObject refs, not just pointer-presence snapshots. Verify
  native empty versus unsupported, SDK object identity and actual ref release
  after capture/actor/metadata aliases disappear. Exercise opaque actor metadata
  and camera-options IDs across migration/shutdown and wrong-session rejection.

- SharedMemory source now includes actual SDK subclasses and virtual factory
  consumers through SharedMemoryDesktopFrame::Create. Exercise native requested
  byte count, exact size/id/handle/data, null rejection versus panic/undersized
  quarantine, original factory destruction before retained frame destruction,
  independent FD aliases and Send dependency-token release on migrated threads.
  Cover heap invalid-handle versus mapped memfd, descriptor zero, close-on-exec,
  owned FD failure closure, unsealed/short/read-only files, zero-size and padded/
  bottom-up offset views, copy/black layout guards and metadata/CPU-copy behavior.
  Prove native frame deletion releases product leases even after creator aliases
  disappear. Include hostile panic payload/destructor and constructor-failure
  cleanup under the applicable matching sanitizer runtime.
- Pinned Linux capturers inherit a no-op SharedMemoryFactory setter. Selected
  capture factories must reject before source creation and at runtime replacement
  without callbacks, stock substitution or fake installation. None forwards the
  native absent path. Physical capture cannot certify a factory the SDK ignores;
  use actual SDK shared-frame factory consumers as the supported boundary.
  Do not reinterpret arbitrary backend-specific SDK handle integers as FDs.

### Call, sent-event and controller forwarding

- Qualify finite RTP forwarding unchanged through the actual Call receiver,
  including ordinary negative/default times, BWE-only RTP and queued worker
  delivery. Nonfinite arrival is a direct native precondition and increments
  binding diagnostics; RTCP/control delivery remains unchanged.
- Observe exact creation-clock bounds, mismatched/unknown bounds and retained
  observations after close. Bounds are metadata, not a numeric admission profile.
- Exercise ordinary enabled/absent/disabled WebRTC-Bwe-ReceiveTimeFix creation and
  inherited/per-peer trial precedence. No bridge-level rejection or trial change
  may substitute for native behavior.
- BWE/TWCC history, receive statistics and TransportFeedbackAdapter accumulation
  are SDK-owned. No permanent timestamp envelope, mirrored history/reset, traffic
  quota or cumulative arithmetic proof is required of the binding.
- Sent events retain signed milliseconds, size, allocation/feedback flags and
  identifiers without numeric preflight. Distinguish direct native metadata
  preservation from supported end-to-end packet delivery.
- Replace obsolete InvalidUpdate/probe-history/estimator-budget test expectations
  with forwarding assertions at the adapter boundary. Do not drive deliberately
  invalid numeric values into the SDK's private pacer/feedback algorithms.
  Retain InvalidUnits, provider panic, selection-gate and lifetime tests.

### #10 TLS and owner-retirement review, source evidence only

An independent read-only review found no concrete defect in the inspected TLS
backing, socket-scope retirement, rejected construction, peer close or migrated
ProductionSession teardown paths. This is not runtime/sanitizer qualification:

- native/tls.cc's peer verifier proxy owns shared NativeTlsVerifier backing and
  takes a local shared lease before callbacks. RetainPeerTlsVerifier returns that
  backing, not a peer-owned borrowed facade pointer.
- native/packet_socket.cc retains the verifier before entering Rust socket-factory
  conversion. src/tls.rs's cloneable SocketTlsVerifier owns that shared backing,
  so retained socket options do not depend on the original peer proxy lifetime.
- GeneralSocketFactory owns its resource scope. Role retirement disables enrolled
  event targets before releasing products. Rejected-construction guards retire
  event nodes; SDK close and actor teardown retain dependency-before-role order.

Final actual-consumer tests must still exercise retained TLS options after peer
close, verifier callbacks/reentry, rejected construction, callback-triggered owner
drop, migrated shutdown and final resource release under the matching runtime.
Internal SDK numeric preflight is not a source-completion requirement. Matching
runtime ownership, callback and buffer qualification remains required.

## Final test-source additions, unrun

The following fixtures were added after the requested rebase. None has been
compiled or executed against a matching artifact yet:

- `tests/packet_socket_lifecycle.rs`: native standalone socket members and event
  lists, received metadata, tagged/self-cancel receivers, accepted child linear
  ownership and survival after listener Drop, rejection without fallback, and
  canceled-product destruction retiring a listener during run/submission pruning.
- `tests/owned_dns_resolver.rs`: actual SDK legacy Start overload dispatch,
  cross-thread completion posted to the origin, one-query admission, live callable
  result owners after resolver destruction, queued completion cancellation and
  retirement, late completion status, and hostile callback/capture destruction
  with query-local diagnostics and a healthy reusable recipe.
- `tests/desktop_frames.rs` (native Linux only): actual SDK CPU/frame copy and
  metadata operations, direct pixel/layout bounds, signed/padded shared rows,
  independent CPU copies, dependency release on constructor failure and final
  frame Drop, the actual SharedMemoryDesktopFrame factory consumer, ordinary
  allocation rejection versus panic/undersized-product quarantine, and owned FD
  aliasing through frame destruction/migration. These do not certify hardware.
- `tests/installed_packet_socket.rs`: two controlled peers select the general
  network-manager/socket factories, gather native host candidates on the selected
  interfaces, and route actual SDK ICE/DTLS/SCTP bytes through caller-owned queued
  packet events. It asserts bidirectional data-channel payloads, native role/time
  selection, product/manager release, and retained event retirement. No endpoint
  packet factory, physical I/O or fabricated protocol response is used.
  Its media case now uses a seeded ControlledWorld, derives its selected trials
  from an actual SDK creation Environment without guessing private clock/queue
  identities, negotiates Opus/TWCC with RFC 8888 explicitly disabled, changes only
  first-delivery metadata for three real encrypted RTP packets to nonfinite time,
  and checks the actual receiving Call's unchanged rejection counter plus later
  non-silent decoded PCM. Pinned WebRtcVoiceReceiveChannel::OnPacketReceived
  replaces nonfinite arrivals with environment time before Call delivery, so
  socket-layer injection does not exercise the direct Call rejection branch.
  Direct malformed Call-receiver rejection remains source-only evidence here,
  not a runtime pass. Selected native GoogCC must create the estimator with its real
  trials and deliver nonempty transport feedback; final product Drop is counted.
  There is no public per-RTP-arrival getter: decoded media proves finite delivery,
  not observation of its precise timestamp. Unchanged finite-value forwarding
  remains source evidence at the Call seam. Narrow source review `21fd9a58`
  found no concrete setup/API defect; runtime progress remains unconfirmed.
- `tests/mdns_lifecycle.rs`: native responder virtual dispatch created through
  the selected manager, exact IP and binary names, invalid-address retry,
  single-submission cross-thread completion/removal, queued cancellation versus
  manager retirement, reentrant native requests, retained aliases and hostile
  callback/capture destruction without quarantining a healthy provider. This
  does not certify actual obfuscated ICE candidates or an mDNS wire protocol.
- `tests/installed_packet_socket/video_consumers.rs`: actual VP8 sender with
  selected callable FEC and per-peer bitrate allocator products, observed exact
  creation environments, live SDK protection callback outputs, allocator target
  cells reaching encoder SetRates, decoded VP8, actual primary RTP matched to
  outbound sender stats, and real transport feedback reaching the estimator.
  Retained protection capabilities must retire with the sender; creation trial
  views remain independently callable. Review `2caa6122` found no scoped source
  defect. The parent also checked pinned RtpVideoSender protection and feedback
  paths. Neither review executes the test.
- `tests/installed_packet_socket/mdns_candidates.rs`: the selected responder is
  attached to the actual network returned to the SDK allocator. The fixture
  holds real SDK port name requests pending, then checks actual obfuscated ICE
  events and local SDP after completion. A second actual port's pending name
  completions survive scope retirement only as inactive handles. Review
  `5c3f81c8` found no scoped source defect. This is not an mDNS wire implementation
  or a connectivity claim for remotely resolved `.local` candidates.
- `tests/installed_packet_socket/tls_verifier.rs`: actual SDK TURN TCP creation
  supplies selected internal CA or Rust socket-verifier options. The capture
  factory explicitly rejects TCP construction. After originating peer/factory
  retirement, retained aliases authenticate real caller-driven rustls sessions
  and encrypted application bytes through native leaf/full-chain verification.
  Rust provider panic must reject another genuine handshake and poison all
  aliases without provider reentry. This does **not** claim that this rejecting
  factory transports TURN or performs an SDK TLS socket handshake. Existing
  `controlled_turn_tls` and `turn` native handshake suites remain distinct
  applicable gates. The unchanged openssl credential recipe is shared through
  `tests/support/tls_certificate.rs`; its old local helper was moved there.
  Review `470f23b9` found no scoped source defect; it did not run handshakes.
- `tests/capture_options.rs` (native Linux only): actual SDK option copies and
  selected-clock reads/retention, cross-factory mismatch, selected unsupported
  SharedMemoryFactory rejection without calling its allocator, explicit PipeWire
  camera initialization/consumption rejection without notification or V4L2
  substitution, and actor option-ID migration, cross-session rejection and
  shutdown. No camera/desktop backend is opened by these tests. The delegated
  capture API investigation was blocked by tool file access; the parent inspected
  the relevant option, actor and factory declarations directly instead.
- `tests/controlled_capture.rs` now also covers new camera option/DeviceInfo/
  unstarted capture constructors, desktop option constructors and camera-control
  queries, retaining the no-time-advance/no-packet assertions.
- `tests/production_sdk_wake_drop.rs` now populates PCM/raw-video sources, tracks,
  source aliases, source/track sinks, a stream and writable I420 storage before
  actual SDK-callback final-owner Drop. Native Linux additionally retains camera
  and desktop option copies. Existing environment weak-release and exclusive
  controlled-world reacquisition assertions now cover that expanded owning graph,
  not just the older peer/channel graph. The modified test remains unrun.
- `tests/production_sdk_wake_drop.rs` also uses an actual native async logger
  stop-completion callback invoked by a foreign Rust producer thread, with no
  current SDK queue. Its registered waker drops peerless and already-shutdown
  actor owners. This is a real completion path, not synthetic readiness notify.
- `tests/production_sdk_wake_drop/event_queue.rs`: a real builtin SDK logger
  writes its encoded start event on `rtc_event_log`. The sink drops the final
  actor owner and holds its actual callback open while the caller checks sink
  retention and production-lease exclusion. Release must occur on that same SDK
  queue after the callback, followed by environment/role/lease reclamation.
- `tests/production_sdk_wake_drop/codec_queue.rs`: negotiates real native video
  peers with the builtin encoder and VP8 decoder, migrates the actor, installs
  an owning sender selector and publishes its first raw frame. The SDK's actual
  `OnCurrentEncoder` callback on `VideoEncoderQueue` receives and drops the final
  actor owner through a test-only publication channel. Pinned frame-cadence
  dispatch posts asynchronously, so the initiating push returns before owner
  publication without an actor mutex or retained-owner cycle. The callback is
  held open to check selected-provider/environment and production-lease retention,
  then released to check eventual role/lease reclamation. Selector destruction
  has no invented queue-affinity requirement. This is encoder creation/queue
  retirement evidence if it passes, not successful encoded-media output.
  Read-only review `6f0d4e8e` found no scoped source/API defect. All execution
  remains pending.
- `src/production/retirement_tests.rs`: dispatches onto the exact graph-owned
  network, worker and signaling threads through a cfg(test)-only Rust helper,
  rather than creating lookalike threads. Final-owner handoff must return while
  that real native task remains blocked; its production lease must remain held
  until the task returns. Cases include no peers and pending actual SDP work.
  A caller-local cfg(test) admission fault checks ResourceExhausted cleanup of
  the already-built factory; this is injected reservation failure, not actual OS
  thread exhaustion. Ordinary caller/error/already-shutdown Drop asserts the
  unused worker is cancelled and joined synchronously. Owned-role/admission
  source reviews `de534748` and `9c2f55cc` found no scoped defect; they did not
  compile or run these cases. Parent inspection subsequently corrected a missing
  `ice_restart` argument. Additional source cases retain a disjoint caller-owned
  native queue selected by a migrated actor and prove it still accepts work after
  actor teardown, and drop a nested actor from the selected field-trial provider's
  actual final destruction after migrating its parent. These latest cases are
  also unrun.
- `tests/desktop_callbacks.rs`: explicit ignored live callback gate launches a
  **fresh private Xvfb** using `-displayfd`, empty SDK options and an explicitly
  opened display lease with PipeWire disabled. It never opens ambient DISPLAY
  or a user portal. Actual SDK screen frames, native frame copies, receiver
  replacement/panic quarantine, lease retention and final close are asserted.
  Review `d6278da8` found no scoped source defect. The current host has no Xvfb
  on PATH; this fixture has not been run. An ignored/default-suite pass is not
  qualification of it. Native/X11 artifacts and provisioned Xvfb are required.
- `tests/camera_callbacks.rs`: explicit ignored physical V4L2 gate requires
  `PULSEBEAM_TEST_CAMERA_ID` naming an authorized device. It never auto-selects
  an available camera. It disables PipeWire, initializes actual options, consumes
  the selected device's reported capability, and checks real raw callbacks,
  replacement/quarantine/deregistration fences, consuming option/DeviceInfo
  retention, and final stop. Pixels are not retained, printed, hashed or saved.
  Review `8b4f4bdf` found no scoped source defect. It remains unrun and requires
  explicit operator authorization plus a usable device, not mere enumeration.

Include these binaries in the matching core/native and applicable ASan commands,
with single-threaded execution for controlled process-global suites. Source
review/design evidence cannot certify that these end-to-end fixtures run. Existing
manager/network-value/subscription fixtures also remain unrun. Actual selected
FEC/allocator, mDNS candidate and post-origin-retirement TLS-capability consumers
now have source fixtures, not qualification. Source audit `99d4cc19` identified
missing retirement origins and reservation/error assertions. Network/worker/
signaling, foreign completion, internal event queue and injected admission/normal
Drop source cases have now been added. SDK codec-queue, caller-owned queue and
nested/migrated final-drop assertions now also have source fixtures. Codec
source review found no scoped defect. Final inventory review `f0d7d69f` overlooked
existing private retirement cases; parent inspection located both in
`src/production/retirement_tests.rs`. Focused follow-up review `b680dc84` confirmed
that both named obligations are represented and found no scoped source defect.
Fixture-source reconciliation is complete; final compilation and execution
qualification remains unfinished. New standalone native dispatcher/admission units do not substitute for
those actual queue/consumer gates. Physical camera/portal consent and private-Xvfb
execution remain separate
applicable qualification/prerequisite decisions, not silently passed ignores.
Keep unfinished obligations unfinished rather than treating source presence,
source review, or checklist counts as executed proof.

## Final qualification sequence

After all implementation batches are finished:

1. Finish focused tests listed above, inspect final diffs, run configured Rust/C++
   diagnostics/code actions, and check source formatting/whitespace.
2. Regenerate CXX output from final `src/lib.rs` with pinned 1.0.200 tooling.
   Synchronize explicit Rust native-adapter input inventory, generated bridge
   hashes, manifest fixture and native adapter/build provenance. Do not relabel
   historical archives or reuse them as evidence of changed native code.
3. Export fresh Linux x86_64 core/native and ASan artifacts from the actual final
   candidate, preserving pinned SDK/compiler/FP/GN/protobuf/source identity.
4. Run final all-target/all-feature source checks, appropriate runtime suites,
   doctests/compile-fail ownership checks, core no-device/controlled checks and
   matching-runtime ASan/leak checks. Add narrow actual-consumer fixtures where
   existing regressions do not observe the newly selected behavior.
5. Recheck packaged CXX/native provenance, compatibility fixtures and public
   contract limitations. Record command receipts, completed results and candidate
   identity here. A launch receipt is not evidence.
6. Repair any failures, then rerun affected gates against a frozen repaired
   candidate. Commit coherent implementation only with truthful qualification
   status. Complete the whole slice only when all applicable requirements pass.

### Local command prerequisites retained from prior runs

- Workspace: `/home/lukas/wt/pulsebeam-libwebrtc-sys/providers`.
- Normal SDK/artifact work root: `.work/rebuild`; ASan work root: `.work/asan`.
- Pinned generator: `.work/rebuild/cxxbridge-tools/bin/cxxbridge`.
- Export recipe: `just _export core linux-x86_64` and
  `just _export native linux-x86_64`, with separate WEBRTC_DIST directories.
- Normal Cargo cache: `.work/rebuild/cargo-home`; target dirs:
  `.work/core-runtime` and `.work/native-runtime`.
- Normal runtime RUSTFLAGS:
  `-L native=$PWD/.work/native-link -L native=/usr/lib64`.
- ASan uses `PULSEBEAM_WEBRTC_SANITIZER=address`,
  `ASAN_OPTIONS=detect_leaks=1:halt_on_error=1`, Cargo cache
  `.work/asan/cargo-home`, targets `.work/asan/provider-runtime` (core) and
  `.work/asan/provider-runtime-native` (native), SDK clang++ linker
  `.work/asan/checkout/src/third_party/llvm-build/Release+Asserts/bin/clang++`, and
  `RUSTFLAGS=-C link-arg=-fsanitize=address -C link-arg=-fuse-ld=lld` plus the same
  native library search paths as normal runtime.
- Shared prerequisites used successfully: `TMPDIR=$PWD/.work/tmp`,
  `SCCACHE_SERVER_PORT=43424`, offline locked Cargo dependencies, `-j 2` and
  `--test-threads=1` for process-global fixture suites.
- Set PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR to each freshly exported matching package.
- Provenance tool: `python3 tools/cxx_provenance.py <package-directory>`.
- Send every eventual build/test/lint/acceptance command through `verify`.

## Final results

Implementation, fixture-source reconciliation, and the Linux x86_64 non-device
runtime qualification are complete. All final runtime runs below used the same
code/test candidate `4057d7e:5e78a44f` and fresh matching artifacts.

| Final runtime | Passed | Ignored | Completed verifier |
| --- | ---: | ---: | --- |
| Normal core | 768 | 3 | `be674f4f` |
| Normal native, excluding `device_smoke` | 780 | 6 | `be674f4f` |
| ASan core, leak detection enabled | 768 | 3 | `808602dd` |
| ASan native, excluding `device_smoke`, leak detection enabled | 780 | 6 | `a32d2f2a` |

No sanitizer reports occurred in these final ASan runs. Ignored cases are not
passes. The isolated decoder-frame randomness-retention case also passed its
separate all-four-configuration run `86b9b269`. Core doctests passed `a7276a63`;
native doctests and ASan C++/Rust-staticlib consumer smokes passed `a478a39d`.
Normal C++/Rust-staticlib smokes passed `42f96756`; cold Git Rust-only consumers
for both flavors passed `6838e7fa`. All 182 Python tests, all-target/all-feature
Rust source, formatting/CXX import/whitespace and all four package provenance
gates passed `6a439a79`. Closing all-target/all-feature source, formatting,
CXX import/whitespace, all 182 Python tests and all four package provenance
checks passed `4136b9d6` on unchanged code/test/native inputs.

This is not physical-camera, live-desktop, portal/session, optional external-VLA
probe, or ambient-IPv6 qualification. These prerequisite-dependent gates remain
unqualified, not silently passed. No camera, desktop or portal capture ran.
Whole-goal completion still requires resolution of the live-capture scope noted
below. No SDK source patches were made.

### Diagnostic history and repairs

The first all-target/all-feature source check
found duplicate packet-tag exports and a stale socket context field; repaired
while preserving existing ICE enum names and exporting the new open-valued tags
as SocketPacketType/SocketPacketInfoProtocol. The next source check found an
unhandled optional DeviceInfo in the ignored camera fixture; repaired. Neither
failed run is successful compilation or runtime evidence. Seven focused Rust LSP
files reported no diagnostics, which did not substitute for the compiler gate.
Bridge/source pins were refreshed with cxxbridge 1.0.200. Final Rust source,
formatting, CXX import and whitespace gates passed d950a3d3. Fresh normal core
build/export passed d78d7685 and native build/export passed 2861728f, with
unchanged final native/CXX source identity. Subsequent ASan build/runtime
results are recorded above.

The first complete core runtime run (67709e81) failed six targets, not qualification:
- Actor Drop incorrectly used an overbroad native-execution predicate in addition
  to the precise deferred-teardown predicate, which already distinguishes ordinary
  auto-wrapped callers from SDK role/internal queues. Removed the redundant test;
  normal caller Drop must remain synchronous, while callbacks/SDK queues retire.
- The clock test retained an obsolete negative-subsecond conversion restriction;
  finite conversion is SDK-owned. Infinite SharedClock queries remain rejected.
- Synthetic artifact-support archives inherited the actual export archive digest
  while packaging sentinel bytes. Generate only the synthetic payload digest in
  memory, preserving the tracked actual export and all source/configuration pins.
- The field-trial test incorrectly expected one lookup after querying a second,
  previously uncached key. Preserve all copy/live-owner and final-release checks,
  and assert both successful and empty groups are cached once per distinct key.
- Network monitor product destruction is dispatched to its network creation
  sequence, while the registration factory survives native manager teardown on
  signaling. Assert each native-owned lifetime independently, not thread equality.
- The codec stream formatting expectation omitted SDK empty-parameter formatting.
- Socket-layer nonfinite arrivals are normalized by the SDK voice channel before
  Call delivery, so they cannot prove the direct Call guard rejected three RTP
  packets. Correct the test and qualification description instead of changing
  production metadata forwarding or the Call finite-unit guard.
The next frozen-candidate core d12ce189 and native cfdc3d8e runs passed the
retirement/clock/field-trial/monitor/codec regressions, but still failed overall.
Synthetic notices also need matching in-memory license inventory and digest;
initial connected routes are intentionally not notified to GoogCC, so remove
only that callback-count assumption while retaining actual nonempty feedback;
the mapped-Opus media fixture must boundedly drain sink queues rather than wait
for an unrelated actor-readiness wake. Source check/fmt/CXX/whitespace passed
within 2debad8a, whose Python discovery run failed three Git consumer snapshots
because new implementation files were not yet in git ls-files. Stage the owned
new files before rerunning packaging snapshots. Doctests passed in a7276a63;
its consumer smoke command failed because bridge-smoke was not built in that
package directory, so rerun with the required _rust-smoke prerequisite.
Fresh ASan core/native build/export passed 23737fe5 on candidate bd0e220e,
with no SDK/native/CXX source changes since normal exports. Focused runtime,
source/fmt/CXX/whitespace, all 182 Python tests and all four package provenance
checks passed 6a439a79 on candidate 6a56e475. Complete normal core/native
(no-device whitelist) runtime passed a351a15c; normal C++ and Rust-staticlib
consumer smokes passed 42f96756; cold Git Rust-only candidate consumers for
both flavors passed 6838e7fa, including forbidden-compiler sentinels, verified
download, offline reuse and corrupt/mismatched/unavailable artifact cases.

The first complete ASan core runtime 15d1ad53 failed only video_encoder_selector
with a 32-byte leak from its intentionally abandoned heap-backed unknown panic
payload; native runtime did not start after that failure. Preserve the established
containment policy: unknown payload destructors cannot safely run because their
field cleanup can double-panic and abort. Use the existing zero-sized hostile
payload fixture to prove containment without deliberately retaining its counter
allocation. This does NOT claim arbitrary user panic payloads are leak-free:
unknown payloads are intentionally forgotten and can retain heap allocations
and dependencies. All ordinary provider/resource lifetime checks still run with
detect_leaks=1, no suppression or disabled leak checks. The final sanitizer runs
above passed the corrected containment fixture.

Native doctests in 30ff84d8 compiled 72 cases but one executable-link example
lacked host native library search paths; pass those through RUSTDOCFLAGS as well
as RUSTFLAGS. Those doctests and ASan C++/Rust-staticlib smokes subsequently
passed a478a39d. The overall failed runs above remain diagnostic only.

The ordinary-drop regression's provider-thread assertion was also too strict:
SDK environment copies may release their final thread-safe field-trial owner
on a native role during synchronous teardown. The test now uses a cfg(test)-only
handoff-used flag, held independently of worker state, to assert no retirement
handoff occurred, all environment resources were already released, and the unused
worker exited and joined before Drop returned. It does not weaken these lifetime
checks or change production behavior. Review e27440c1 found no counterexample;
normal core/native retirement tests passed 784507ff. The isolated decoder-frame
randomness-retention fixture passed all four normal/ASan core/native configurations
in 86b9b269; its default ignored result is not counted as that separate pass.

Complete ASan native runtime passed 722ee633 (780 default cases, six ignored),
with leak detection enabled and no sanitizer reports. ASan core 8268f14b passed
the corrected retirement and selector cases but failed an existing native-video
consumer's single-frame/tight-progress wait. It now feeds frames at 33 ms cadence,
progresses peers with a bounded wall-clock wait, and still requires both native
encoding and the installed controller's sent_packet callback. Failure reports
both operands. Review 48e911f2 supports this scheduling repair without asserting
that the earlier log proves the precise timing cause. Final normal core/native
`be674f4f`, ASan core `808602dd` and ASan native `a32d2f2a` all passed the repaired
fixture and their complete non-device suites.

Structured human answers requested excluding live desktop and physical-camera
qualification. No live access is authorized or will be performed. The session
scope-control tool rejected recording that exclusion without a direct current
human instruction, so those live gates remain explicitly unqualified and must
not silently count as passed or justify whole-goal completion. Other source,
build and non-device qualification continues.
