# Caller-driven runtime services and media

`ControlledWorld::acquire(seed, initial_time)` acquires the process-global
WebRTC clock and non-cryptographic random generator together. Conflicting
acquisition leaves active hooks untouched. A world is sequence-bound, as are
its peers, networks and queues. Production peers and parallel worlds must not
coexist with it. Crypto keys, certificates and ciphertext are not seeded.

The common application uses `Session::with_runtime(runtime, config)` for both
threaded and caller-driven execution. The outer harness acquires the world,
uses `world.create_network()` and registers an endpoint for each peer, then
supplies `world.runtime()?.with_endpoint(&endpoint)?` to that application.
The runtime supplies the matching environment and native role execution.
Standalone consumers use `runtime.environment_factory()` followed by
`runtime.create_environment(&factory)` after configuring SDK selections.
The lower-level `world.peer_factory_builder()` remains available for direct
native factory use, without a separate media profile.

Retain the **endpoint owner** for the intended link lifetime: provider recipes
retain state, but dropping or closing the endpoint explicitly shuts down its
link. Mixing providers from different endpoints or networks, or using a closed
endpoint, fails before native peer construction.

Queues created with `world.create_queue` retain the driver, just as peer
factories and controlled networks do. Dropping the world handle alone does not
invalidate those resources. The final retained driver tears down on the calling
thread and restores the global hooks. SDK environment copies retain native
scope tickets rather than an affine Rust driver. Release those aliases and
their dependent products before closing the final caller-owned runtime scope;
creator teardown drains their native-ready retirement work without adding an
executor or advancing time.

## Time

Time zero maps to Unix epoch zero, not the host's wall clock. WebRTC NTP time
adds the standard 2,208,988,800-second offset to the same timeline. Public
`Duration` values are measured in microseconds internally; fractional
microseconds of advances and initial timestamps are truncated. The supported
range is zero through `MAX_CONTROLLED_TIME`, inclusive, ending immediately
before the NTP era-zero seconds wrap in February 2036. It also fits the signed
nanosecond process clock. Values outside the supported range are rejected
before clock mutation; there is no saturation or real-time fallback.

Clock advancement only changes the clock. It does not execute tasks, deliver
packets or decode media. The external driver supplies media, delivers network
input and chooses each invocation and advance.

## Pump and deadlines

`world.pump(budget)` dispatches at most `budget` top-level tasks. Zero dispatches
nothing, even when tasks are ready. `PumpResult` reports the number dispatched,
whether work remains ready at unchanged time, and the earliest pending task
deadline. `world.next_deadline()` uses the same microsecond timeline and returns
`None` only when the native peer and media task queues have no queued work.
Outgoing packets awaiting caller decisions are separate caller-owned work.

Across native thread roles and cooperative queues sharing this clock, tasks
are selected by deadline, then by a clock-wide posting sequence number.
Equal-deadline posting order is preserved, including across queues; queue
priority does not override this ordering. Delayed posts use microsecond
precision, without the production thread queue's millisecond rounding. The
policy has no OS-race-dependent selection in controlled execution.

Synchronous native lifecycle waits may yield to other ready queues. A yield
never moves time and excludes every running/suspended queue, including the
current native thread role. Yields dispatch only engine-owned queues, never
caller-created queues; caller work remains subject to the external pump budget.
This prevents unrelated self-reposting caller tasks from trapping a lifecycle
wait after its dependency has signaled. Internal yields are not top-level pump
dispatches. Supported lifecycle waits require finite ready dependency work that
signals the event before the upstream wait continues. A yield hook does not
turn arbitrary upstream event waits into nonblocking operations.

The lower-level `TaskQueueFactory::pump(budget)` handles one factory and rejects
calls away from its cooperative creator thread before dispatch.
`ControlledPeerDriver::pump(budget)` handles every registered factory sharing
its clock. Compatibility `run_ready` dispatches at most 1,024 tasks and may
leave work ready. Cooperative factory `run_ready` returns zero off-thread;
explicit factory pump returns `BuildEnvironmentError::WrongThread`.

All live cooperative factories sharing a `ManualClock` must be created on
one OS thread, including through the lower-level APIs. Creating a foreign-thread
factory returns `BuildEnvironmentError::NativeConstructionFailed` before
registration; acquiring a driver with a pre-existing foreign factory also fails
before changing global hooks. Factories on independent clocks are unaffected.
This prevents inaccessible queues from stalling the clock-wide pump.

`TaskQueue::post_local` accepts sequence-bound closures only for cooperative
queues, enabling self-reposting caller work. Threaded posting retains its
`Send` bound. Dispatch budgets bound task count, not the execution time of
arbitrary caller-provided closures.

### SDK thread invocation from another queue

An ordinary cooperative callback has its own native task-queue identity while
running on the driver thread. SDK `Thread::IsCurrent()` checks thread identity,
not that task-queue identity. The managed thread's inline `BlockingCall` therefore
scopes the requested SDK thread queue around the native operation, then restores
the caller queue. Native invoke-policy checks and off-thread posting remain in
the SDK implementation. The threaded runtime is unchanged; no sequence check is
disabled and no extra thread or executor is introduced.

`tests/managed_pcm_teardown.rs` closes and drops an actual stock audio graph from
an active same-world caller queue. It checks current-queue identity before and
after native teardown, restoration on return and no virtual-time movement.
The [audio contracts](audio-runtime-contracts.md) describe its replay and scope.
This does not authorize arbitrary reentry from native media/provider callbacks
or prove teardown from an active encoder queue.

## Common media configuration

Media selection is independent of runtime selection. Headless defaults use
stock builtin audio/video factories and raw PCM/video inputs; native processing,
trackless receivers and stock logging do not require a simulated source flag.
The obsolete `controlled_media` constructor method has been removed, not
replaced with a second whitelist. Ordinary builder options select codecs,
processing, providers and explicit native devices. The `core` artifact does not
expose physical-device APIs. Explicit native-only services preserve their actual
behavior and are outside caller-controlled deterministic execution when they
bypass the selected runtime dependencies.

Construction support does not yet qualify every built-in software backend for
deterministic caller-only execution. Codec-owned workers, nested dependencies
and full raw-media replay remain open qualification work. See the
[capability matrix](capability-matrix.md) for bounded evidence rather than a
blanket deterministic-media claim.

### Private VP8 worker-seam foundation

SDK exports separately pin the DEPS libvpx revision and its runtime worker patch;
this identity is not attributed to the parent WebRTC resource patch. Producer
validation rejects extra tracked, staged or untracked child inputs, including
ignored files. Rust/Python consumers require that child identity and bind it to
the hashed native configuration.

The private optional seam routes VP8's unchanged persistent encoder/filter entry
points through the existing native worker interface only when explicitly
selected. With no selection, the original pthread path remains. Optional
semaphore/spin callbacks may decline to preserve native primitives. This seam is
**not installed by the managed runtime yet** and does not qualify caller-only
multiworker execution.

`consumer/vp8_runtime_workers.cc` and `tools/prove_vpx_runtime_seam.py` exercise
matching Linux x86_64 archives, eight actual encoded/decoded frames per variant,
original versus selected worker dispatch, native semaphore fallback and partial
worker-creation cleanup. They use raw libvpx configuration, not the complete
WebRTC builtin factory/media driver. Their byte/pixel digests remain raw
observations, not replay assertions: these raw realtime codec defaults adapt
speed to measured native elapsed time. A native-only debugger timing perturbation
changed those observations without installing a runtime interface. No codec
defaults or worker counts are changed to hide this timing dependency. Native
forwarding and cleanup evidence does not establish managed scheduling,
codec-clock coverage or arbitrary native stack/TLS safety.

## Explicit encoded input

Prerecorded input remains an optional native adapter, not a simulation mode.
Select `AudioEncoderFactory::with_opus_frames()` for Opus carriers, or use
`SessionBuilder::encoded_audio()` on the common session builder. Select an
`EncodedVideoInput` and its encoder factory for supported encoded video.
The corresponding receive factory determines whether decoding or encoded-only
receipt is supported. These adapters retain their actual format/metadata
constraints in both runtimes, without constraining ordinary stock raw sources.
Create `EncodedAudioSource` and tracks through that configured factory, and
encoded video sources/tracks through the selected `EncodedVideoInput`.
Capture timestamps are explicit: `push_opus_at(frame, Duration)` accepts a
microsecond world timestamp and a separate 48 kHz RTP timestamp;
`push_opus(frame)` uses the current controlled-world time. Opus capture time
plus packet duration must fit the supported range. VP8 access units carry
`timestamp_us` on this same timeline, with strictly increasing timestamps per
source and format-consistent dimensions/metadata. No adapter compresses input.

Opus front-end validation returns `InvalidPacket`, `InvalidDuration` or
`InvalidTimestamp` before reservation. Its 24 outstanding 10 ms carrier slots
are bounded; exhausted reservations return `Backpressure`. Video input validates
VP8 framing/header structure, dimensions, metadata and time before submission.
The shared encoded-video broker bounds pending input to 16 units and 8 MiB;
oldest-unit eviction increments the affected source's `dropped_frames()`.
`pending_frames()` exposes retention. Entropy-coded corruption accepted by the
front end is left to the real decoder; `decoder_statistics()` counts decoder
errors, not fabricated success. Source close discards its retained input.

Use `AudioSink::try_next_received_frame()` and
`VideoSink::try_next_received_frame()` for owned outputs. They include the
controlled recipient `peer_id`, receiver or track identity, world observation
time, capture timing, PCM format/samples or packed CPU I420 pixels. Peer IDs are
world-local logical IDs, not process-global object identities. Video receiver
track identity stays stable across sender replacement until renegotiation;
input `stream_id()` identifies the local producer, not a wire receiver ID.
Owned output remains valid after source/peer/world destruction and is passive,
so it can be read elsewhere without driving native work.

`RtpSender::set_track` replaces or detaches a compatible video producer on a
stable sender. `RtpTransceiver::stop` requires SDP renegotiation; adding a new producer
and negotiating again exposes a new receiver identity. Unrelated streams remain
active. `request_keyframe` submits a sender/receiver request; its success does
not mean a frame was generated. `take_keyframe_request()` reads and clears
feedback on the specific input source when WebRTC processes its next trigger,
including requests satisfied by an already supplied keyframe. The driver must
supply prerecorded keyframes; no compressed replacement is manufactured.

## Networking and replay

Outgoing packets are caller decisions. UDP IPv4/IPv6 delivery, duplication,
dropping, TCP client input and DNS answers are explicit safe operations. The
binding has no shaping policy. The seeded loss/delay/reorder/duplication policy
in `tests/controlled_media.rs` is fixture code, not a simulator integration or
library default. A pending external network deadline is not a native task
queue deadline.

Replay requires identical binding/artifact, builtin decoder configuration,
initial world time, seed, ordered input and external decisions. Compare logical
operation/state/stream outcomes, normalized SDP and ICE candidate contents and
observation times, virtual timing, PCM samples and visible packed
pixels. Exclude certificates, cryptographic material/ciphertext, native
thread-token diagnostics, pixel padding and wall-clock profiling values. Do
not exclude decoded contents or logical delivery times. Cross-build equivalence
and byte-identical cryptographic output are not promised.

## Qualification evidence

The focused encoded-media fixture exercises two observed peers, distinguishable bidirectional
Opus/VP8, gathered SDP, clean/impaired IPv4 and IPv6, exact in-process and
fresh-process replay, video replacement/receiver recreation, source-keyed
keyframe feedback, bounded input, invalid configuration/media and close/drop
with pending work. Native decoder/configure/output counters must identify the
calling thread and compressed input hashes must equal the prerecorded inputs.

`tools/check_controlled_lifecycle_trace.py` checks continuous `strace` creation
and real-socket events between lifecycle markers. Markers enclose acquisition,
native work and final root destruction, including rejected configurations.
Harness worker creation outside these markers is not library creation. Initial
and final thread snapshots are not used as proof. A watchdog only detects hangs
and never drives progress.

Common-construction, native alias teardown and standalone APM/mixing/G711/logging
fixtures also exercise the same application procedure with both runtimes. These
bounded constructor/lifecycle results do not qualify all media backends, NetEq,
VoIP or the full public SDK surface. See the
[capability matrix](capability-matrix.md) and
[audio runtime contracts](audio-runtime-contracts.md) for current limitations.
Source inspection or an earlier artifact is not substitute runtime evidence.
