# Caller-driven scheduling and prerecorded media

`ControlledWorld::acquire(seed, initial_time)` acquires the process-global
WebRTC clock and non-cryptographic random generator together. Conflicting
acquisition leaves active hooks untouched. A world is sequence-bound, as are
its peers, networks and queues. Production peers and parallel worlds must not
coexist with it. Crypto keys, certificates and ciphertext are not seeded.

Use `world.create_network()` and register an endpoint for each peer. Supply
that endpoint's network-manager and packet-socket providers to
`world.peer_factory_builder()`. The builder supplies the matching environment
and driver. Retain the **endpoint owner** for the intended link lifetime:
provider recipes retain state, but dropping or closing the endpoint explicitly
shuts down its link. Mixing providers from different endpoints or networks, or
using a closed endpoint, fails before native peer construction.

Queues created with `world.create_queue` retain the driver, just as peer
factories and controlled networks do. Dropping the world handle alone does not
invalidate those resources. The final retained driver tears down on the calling
thread and restores the global hooks.

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

`TaskQueue::post_local` accepts sequence-bound closures only for cooperative
queues, enabling self-reposting caller work. Threaded posting retains its
`Send` bound. Dispatch budgets bound task count, not the execution time of
arbitrary caller-provided closures.

## Explicit media profile

Ordinary controlled peers still reject media. Opt in with
`PeerConnectionFactoryBuilder::controlled_media()` and all four selections:

- `AudioEncoderFactory::with_opus_frames()` for prerecorded Opus carriers;
- `AudioDecoderFactory::builtin_opus()` for the pinned software Opus decoder;
- `EncodedVideoInput::new_for_format(VideoCodecFormat::new("VP8"))` and its
  `encoder_factory()` for direct prerecorded VP8 input;
- `VideoDecoderFactoryHandle::builtin_vp8()` for the pinned libvpx VP8 decoder.

The profile requires a seeded world. It rejects raw-input/software compression,
H.264 or other encoded-video formats, substitute/unavailable decoders, PCM
processing, physical devices and separately threaded configurations. Codec
names alone are insufficient: sealed binding provenance determines admission.
The core build does not expose physical-device APIs. Builtin VP8 configuration
forces one libvpx decoding thread. Production defaults remain threaded and do
not acquire these profile guarantees.

Create `EncodedAudioSource` and encoded audio tracks using the admitted peer
factory, and create VP8 sources/tracks through the same `EncodedVideoInput`.
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

`RtpSender::set_track` replaces or detaches an admitted VP8 producer on a stable
sender. `RtpTransceiver::stop` requires SDP renegotiation; adding a new producer
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
operation/state/stream outcomes, virtual timing, PCM samples and visible packed
pixels. Exclude certificates, cryptographic material/ciphertext, native
thread-token diagnostics, pixel padding and wall-clock profiling values. Do
not exclude decoded contents or logical delivery times. Cross-build equivalence
and byte-identical cryptographic output are not promised.

## Qualification evidence

The safe fixture exercises two observed peers, distinguishable bidirectional
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

The exact acceptance contract is `plans/single-thread-simulation/spec.md`.
Recorded source/artifact identities and verification results, including the
status of sanitizer and production regression gates, are maintained in
`plans/single-thread-simulation/progress.md`. Source inspection or an earlier
artifact is not substitute runtime evidence.
