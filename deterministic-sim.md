# Caller-driven WebRTC media simulation

The canonical, explicitly approved specification is now
[`plans/single-thread-simulation/spec.md`](plans/single-thread-simulation/spec.md).

The source-grounded current-state audit is
[`plans/single-thread-simulation/analysis.md`](plans/single-thread-simulation/analysis.md).

The current scope qualifies prerecorded Opus/VP8 input with real libwebrtc
decoding and caller-driven, thread-free execution through independent public
interfaces. It supersedes this document’s earlier H.264 scope and required
simulator integration fixture. Downstream simulator integration is deferred.

Spec approval is not evidence that the implementation meets the contract.
The explicit direct-input Opus/VP8 profile, lifecycle/replay fixture and
continuous thread-creation observation now have matching Linux core runtime
proof. Full regression and sanitizer qualification remain outstanding. Public
contracts are in [`docs/controlled-scheduling.md`](docs/controlled-scheduling.md);
exact verification identities and status are recorded in the plan's progress
log.
