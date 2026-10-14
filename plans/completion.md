# Parallel completion index

## Current authority

The human replaced the old implementation goal with: **commit the current work,
then split this project into independently executable parallel vertical slices**.
The source checkpoint is committed as `52b4d80`. This planning pass does not resume
implementation or mark remaining bindings complete. Assign a slice explicitly
before executing it. Old task queues do not override the new scope.

Read [shared-contract.md](shared-contract.md) for constraints, write ownership and
per-slice definition of done. Read [checkpoint.md](checkpoint.md) before changing
source; it records delivered work and the 35 selected standalone core tests.
The former monolith is preserved locally in
`archive/completion-before-split-52b4d80.md`, not as an active execution plan.

## Vertical slices

Every slice includes its public API, actual native/application installation,
actor path where applicable, ownership/error/lifecycle semantics, documentation,
matching-artifact tests and reviewed commit. There is no global "implement all,
then document all, then test all" chain. Standalone-only and unsupported contracts
are explicit, not disguised as installed parity.

| Slice | Completion prerequisites | Old task traceability |
| --- | --- | --- |
| [S01: Negotiate and retire a peer](slices/01-peer-lifecycle-sdp.md) | Checkpoint only | #36, #20, #30 |
| [S02: Exchange and close a data channel](slices/02-data-channel.md) | Checkpoint only | #20, #29, #30 |
| [S03: Install sender RTP parameters safely](slices/03-rtp-parameters-allocation.md) | Checkpoint only | #4, #14 |
| [S04: Observe and control RTP endpoints](slices/04-rtp-observers-transceivers.md) | Checkpoint only | #14, #15 |
| [S05: Inspect and operate a retained transport](slices/05-transports-ice-dtls.md) | Checkpoint only | #16 |
| [S06: Send and receive PCM audio](slices/06-pcm-audio-call.md) | Checkpoint only | #18, #20, #29 |
| [S07: Send and receive raw video](slices/07-raw-video-call.md) | Checkpoint only | #18, #20, #29 |
| [S08: Use selected video codec products](slices/08-video-codecs-selectors.md) | Checkpoint only | #6, #7, #8, #33, #34, #35 |
| [S09: Publish and receive encoded Opus](slices/09-encoded-opus.md) | Checkpoint only | #21, #29 |
| [S10: Publish concurrent encoded video profiles](slices/10-encoded-video.md) | Checkpoint only | #21, #4, #7, #8 |
| [S11: Replace and retire sources independently](slices/11-source-replacement-retention.md) | S09, S10 | #22, #21, #30 |
| [S12: Transform live encoded frames safely](slices/12-frame-transformers.md) | Checkpoint only | #9, #10 |
| [S13: Encrypt and decrypt frames through public endpoints](slices/13-frame-crypto-sframe.md) | Checkpoint only | #15, #20, #29 |
| [S14: Use owning audio factory and codec products](slices/14-standalone-audio-codecs.md) | Checkpoint only | #17 |
| [S15: Select a caller audio factory in a live call](slices/15-installed-audio-factories.md) | S14 | #37, #17, #20 |
| [S16: Encode caller audio in the real send path](slices/16-caller-audio-encoder.md) | S15 | #38, #17 |
| [S17: Decode caller frames in the real receive path](slices/17-caller-audio-decoder.md) | S15 | #39, #17 |
| [S18: Use stock encoder controls and safely enable ANA](slices/18-stock-audio-ana.md) | Checkpoint only | #11, #12 |
| [S19: Drive an owning standalone NetEq](slices/19-standalone-neteq.md) | Checkpoint only | #13 |
| [S20: Run with a caller-owned execution environment](slices/20-execution-environment.md) | Checkpoint only | #19, #20 |
| [S21: Create a peer with selected network/security providers](slices/21-peer-network-providers.md) | Checkpoint only | #19, #20 |
| [S22: Record events through an installed caller log](slices/22-event-log.md) | Checkpoint only | #20, #25 |
| [S23: Adapt a live call with selected congestion/FEC providers](slices/23-congestion-fec.md) | Checkpoint only | #20, #29 |
| [S24: Use selected audio processing and device components](slices/24-audio-device-processing.md) | Checkpoint only | #18, #20 |
| [S25: Use selected Linux capture and video sources](slices/25-video-capture.md) | Checkpoint only | #18, #20 |
| [S26: Drive and replay a controlled peer world](slices/26-controlled-execution.md) | Checkpoint only | #23, #30 |
| [S27: Replay impaired mixed media in a controlled world](slices/27-controlled-media.md) | S09, S10, S11, S26 | #23, #29, #30 |
| [S28: Build and consume matched Linux SDK artifacts](slices/28-artifact-consumers-tooling.md) | Checkpoint only | #5, #24, #26, #27, #28, #31 |

All slices start **not started** in [slices.json](slices.json). A dependency means
an accepted contract/commit is needed for integrated completion, not a prohibition
on independent investigation or implementation in a separate worktree. Existing
checkpoint interfaces can be used without waiting for every related slice to finish.

## Parallel launch batches

- **Independent lanes (23):** S01-S10, S12-S14, S18-S26 and S28. Launch according
  to available owners/resources, not as 23 concurrent writers in one checkout.
- **Composition after source contracts:** S11 after S09/S10; S15 after S14;
  S16/S17 after S15; S27 after S09/S10/S11/S26.
- **Safety conditions:** new/changed sender-allocation paths in S07/S08/S10/S11
  must use S03's admitted seam or report the specific blocked path. Do not hold
  unrelated baseline/read/fixture work behind the entire allocator project.
  Installed audio Query/Encode/decoder safety is deliberately separate in
  S15/S16/S17. The old #37/#38/#39 gates are not waived.
- **No shared-output races:** coordinator serializes shared-file integration,
  worker-owned matching artifact builds and process-global fixtures. SDK sources
  may be shared read-only. Whole-file ownership/conflict rules are in the shared
  contract; temporary same-file section leases must be explicit.

## Coordinator responsibilities, not a horizontal implementation slice

- Assign one owner per slice and reserve leaf files. Give each worker its own
  branch/worktree and source/candidate output paths.
- Integrate narrow bridge/builder/actor/native/header/build patches at the shared
  seams; avoid concurrently edited `src/lib.rs`, `native/codec.*`,
  `native/peer.*`, `src/production.rs` and `Justfile`.
- Combine each lane's pinned-member ledger into whole-surface coverage. Route
  newly found requirements to the responsible slice instead of restarting a
  global inventory that blocks delivery.
- Review a slice only when its actual workflow and applicable lifecycle gates
  pass; retain exact evidence and honest blockers. Preserve completed checkpoint
  source rather than reproducing it.
- Execute [final-integration.md](final-integration.md) after accepted slices are
  integrated. Combined behavior may invalidate prior evidence, so run affected
  composed gates on the integrated candidate, not just individual branches.

## Boundaries

Full Linux application parity remains the future outcome. Custom peer/actor
NetEq registration, public unsafe legacy decoder methods, producer/received VLA
substitutes, downstream simulation integration, other-platform expansion,
real-SFU interoperability and publication remain excluded/later goals as defined
in the shared contract. No safety, installation or native behavior requirement
is relaxed by this split.
