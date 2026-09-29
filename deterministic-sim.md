# Caller-driven WebRTC media simulation

Status: APPROVED

## Motivation
PulseBeam's simulator owns virtual time, seeded scheduling and shaped networks. Real WebRTC peers would add receive-side fidelity absent from its current str0m peer paths. A driver that connects data channels but rejects media is not sufficient.

## Outcome
Qualify the Linux x86_64 core library for multiple real WebRTC peers in one simulation process, with all library peer and decoder execution on the simulator's single calling thread. Send prerecorded, pre-encoded Opus audio and H.264 video and receive decoded PCM and CPU video frames. Retain data-channel support. Supply public low-level controls sufficient for a narrow integration fixture in the existing PulseBeam simulator, not a replacement simulator or client migration.

## Decisions
- Human: audio and video are required, not a data-only qualification.
- Human: media input is always prerecorded and pre-encoded; no actual audio/video encoding is involved.
- Human: decoded output is required; encoded reception alone is insufficient.
- Preserve the no-upstream-fork boundary. Reuse the pinned upstream simulated-time/yield mechanisms where needed; their availability is a candidate solution, not acceptance evidence.
- Native calls may cooperatively execute other ready internal queues on the same thread at unchanged virtual time to complete synchronous lifecycle waits. This is part of explicitly invoked library execution, not autonomous progress.

## Behavior
### Execution and time
- Construction, negotiation, packetization, transport, decoding, stream changes and shutdown execute on the same caller thread. No library-owned background thread or helper process may execute these operations.
- The external simulator alone advances time and schedules library invocations, media input and network delivery. Pumping and internal cooperative yielding must not advance time, sleep, wait for an OS event, or require an independently progressing queue. A yielded queue is not reentered while its current task is suspended.
- Public controls report ready work and the earliest pending deadline. A bounded pump returns control even when work reposts itself; remaining ready work is observable. Within a queue, equal-deadline tasks preserve posting order. Across queues, selection is stable for identical creation/posting order and external scheduling; callers are given a documented policy, not OS-scheduling dependence. The simulator need not inspect private CXX objects.
- The library clock and the simulator clock share one monotonic timeline with a documented epoch mapping and timestamp precision. Unsupported timestamp ranges fail explicitly, rather than wrapping or silently progressing on real time.

### Media
- Inject valid prerecorded Opus packets and H.264 Annex-B access units through the existing encoded-input contract, with capture timing, format and stream identity. Input bypasses actual compression; internal pass-through codec adapters do not count as encoding.
- Decode received Opus to PCM and H.264 to owned CPU frames on the calling thread. Use a fixed decoder configuration for replay. No bundled H.264 implementation is required; a caller-supplied synchronous decoder is permitted.
- Several peers and concurrent audio/video streams remain distinguishable. Stop, replace and renegotiate one stream without stale delivery from its retired instance or interruption of unrelated streams.
- Preserve format validation, bounded buffering and explicit backpressure/loss behavior. Invalid compressed input and unavailable codec configurations yield observable errors or documented media drops, never an execution hang or fallback to background work.
- Keyframe requests are observable with stream identity. The fixture responds with prerecorded keyframes; the library does not manufacture or encode replacements.

### Network and simulator boundary
- Use the existing controlled network extension points. Outgoing datagrams are available to the caller; incoming datagrams and DNS results are injected by it. No real sockets, DNS, devices or remote service are required for qualification.
- The first integration fixture uses UDP and gathered SDP, with IPv4 and IPv6 exercised separately. Delay, loss, reorder and duplicate decisions come from the external simulation network, not a second library network policy.
- A downstream fixture maps these public controls to Turmoil host execution, socket delivery and timer wakeups. It preserves the existing simulator's seeded scheduling and shaping rules. The library neither owns a Tokio runtime nor replaces `sim.step()`.
- Existing controlled TCP/TURN APIs are preserved. Their full simulator integration and TURN servers are not requirements of this slice.

### Replay, ownership and failure
- One controlled world exclusively owns the library's process-global clock and seeded non-cryptographic randomness hooks. Conflicting acquisition fails explicitly and atomically. Deterministic construction cannot silently omit seeded randomness.
- The same source/artifact/decoder identities, initial state, seed, ordered inputs and external scheduling produce the same normalized logical trace, including virtual timestamps, operation outcomes, connection states, stream identities, media delivery and decoded contents.
- Certificates, secrets, cryptographic fingerprints and ciphertext are excluded from trace equality. Exclusions cannot remove logical outcomes, media contents or their delivery times. Byte-identical crypto across seeds or runs is not promised by libc randomness interposition.
- Unsupported autonomous codec/device configurations fail before starting work. Controlled mode never quietly selects threaded execution.
- Accepted asynchronous operations end exactly once in success, failure or cancellation. Explicit close is idempotent. Close/drop with pending signaling, media and network work must complete safely on the same thread, without needing inaccessible work to run.
- After all controlled resources are released, global state is restored and a second world can acquire it. Coexistence with production peers or concurrent independent worlds in one process is excluded.

## Invariants
- Controlled and production modes use the same pinned WebRTC protocol/media engine, not fake peers or prerecorded outputs.
- Merely removing current media rejection guards is not a solution: the blocking lifecycle paths must be resolved and exercised.
- No wall-clock sleeps, arbitrary retries, altered seeds or weakened oracles substitute for deterministic progress.
- Safe public Rust calls retain their lifetime and thread-affinity guarantees. Events and media retention remain bounded, and callbacks do not outlive retained dependencies.

## Constraints
- Preserve production behavior, the private Rust/CXX boundary, Rust-only consumption and matching native artifact identity checks.
- H.264 decoder acquisition/licensing remains downstream; no new bundled codec.
- Existing no-WebRTC/BoringSSL-fork and source-pin boundaries remain in force unless explicitly changed by the human.

## Non-goals
- Raw-input encoding, live media capture, native devices, hardware codecs, GPU output and physical playback.
- Whole-harness or native-agent migration, SFU signaling policy, exhaustive codec/profile coverage, and general RTP injection.
- Deterministic cryptographic bytes, parallel controlled worlds, and non-Linux platform qualification.

## Acceptance evidence
- A fixture using only safe public Rust APIs drives at least two real peers on one calling thread, with virtual-time-only gathered-SDP negotiation, bidirectional data, concurrent prerecorded Opus/H.264 publication and decoded reception. Verify recognizable audio/video content and correct recipient/stream identity, not packet counts alone.
- Use fixed valid media fixtures and a fixed software decoder configuration. Replay the same plan twice from clean worlds in the same process and in fresh processes. Compare the complete normalized logical trace, including timestamps and decoded PCM/visible pixel content. Padding bytes and wall-clock profiling metrics are not trace fields.
- Exercise a clean link and a fixed seeded impaired link with delayed/lost/reordered/duplicated packets. Require positive media delivery under the chosen profile, correct stream isolation and reproducible recovery; loss does not imply every source frame must be delivered.
- Exercise stream stop/replacement, renegotiation that recreates receive streams, keyframe recovery and close/drop with queued decode work and pending offer/description operations. A watchdog may fail a hung test but may not drive peer progress. Verify exactly-once terminal outcomes and absence of post-close delivery.
- Demonstrate bounded pumping, ready/deadline reporting, no peer progress outside explicit library invocation or input delivery, equal-deadline ordering, and no hidden time advancement, including during cooperative lifecycle waits.
- Observe thread creation for the full controlled lifecycle, including transient threads, and record thread identity for peer/decoder callbacks. Construction/final thread snapshots alone are insufficient evidence.
- Prove actual encoding is bypassed, and actual decoding occurs with nonempty PCM/pixel outputs. Include unavailable/threaded-decoder, malformed input, clock/seed conflict and mismatched environment/network rejection cases.
- Run a narrow fixture in `/home/lukas/workspace/pulsebeamdev/pulsebeam/crates/pulsebeam-simulator/` through its real Turmoil scheduler and shaped sockets. It must show aligned deadlines/time, positive decoded media and deterministic replay without replacing existing peers throughout the harness.
- Record exact library, simulator, decoder and native artifact identities. Relevant source checks, matching Linux core runtime tests and sanitizer lifetime tests must pass. Stale artifacts or unavailable integration infrastructure are blockers, not passing evidence.

## Risks
- Current upstream media lifecycle code synchronously waits for encoder/decode queues. Encoded input removes compression, not these waits. Pinned upstream simulated-time execution provides same-thread cooperative yield mechanisms, but no runtime proof currently establishes this entire lifecycle. The global controller wrapper is test-only and its unbounded run-ready operation is not itself the required public pump. Failure to integrate these mechanisms under the no-fork boundary blocks completion rather than authorizing threads or reduced media scope.
- The simulator's libc clock/randomness shims do not establish control of every native clock or RNG path. One agreed timeline and representative replay evidence are required.
- H.264 software decoders can introduce threading or machine-dependent output; qualification must identify and constrain the chosen decoder, not promise equivalence across unrelated builds or hardware.

## Deferred
- Controlled raw-media encoding and device simulation.
- Full TCP/TURN/TLS simulator integration and complete SFU/native-agent adoption.
- Cross-platform and cross-decoder/build replay equivalence.
