# Caller-driven scheduling

`ControlledWorld::acquire(seed, initial_time)` acquires the process-global
WebRTC clock and non-cryptographic random generator together. Conflicting
acquisition leaves active hooks untouched. A world is sequence-bound, as are
its peers, networks and queues. Production peers and parallel worlds must not
coexist with it. Crypto keys, certificates and ciphertext are not seeded.

Use `world.peer_factory_builder()` with packet providers from
`ControlledSimulatedNetwork::new(world.driver())`. The builder supplies the
matching environment and driver. Queues created with `world.create_queue`
retain the driver, just as peer factories and controlled networks do. Dropping
the world handle alone does not invalidate those resources. The final retained
driver tears down on the calling thread and restores the global hooks.

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
dispatches. Supported lifecycle waits require finite ready dependency work that signals
the event before the upstream wait continues. A yield hook does not itself
turn arbitrary upstream event waits into nonblocking operations.

The lower-level `TaskQueueFactory::pump(budget)` handles one factory and rejects
calls away from the cooperative factory's creator thread before dispatch.
`ControlledPeerDriver::pump(budget)` handles every registered factory sharing
its clock. The compatibility `run_ready` methods dispatch at most 1,024 tasks
and may leave work ready; use the explicit pump for a caller-selected budget
and remaining-work snapshot. Cooperative factory `run_ready` rejects
wrong-thread dispatch by returning zero; the explicit factory pump returns
`BuildEnvironmentError::WrongThread`.

`TaskQueue::post_local` accepts sequence-bound closures only for cooperative
queues, enabling self-reposting caller work. Threaded posting retains its
`Send` bound. Library dispatch budgets bound task count, not the execution time
of arbitrary caller-provided closures.

## Qualification boundary

These controls do not by themselves qualify media lifecycle support or prove
zero thread creation across a call. The full selected outcome and required
runtime, replay, thread-observation and sanitizer evidence are in
`plans/single-thread-simulation/spec.md`. Media guards remain until the
supported waits and real decoding paths have matching native runtime proof.
