# Shared contract for every vertical slice

## Authority and scope

The current request is checkpointing and planning only. Start implementation of a
slice only after it is assigned/authorized. The old implementation loop is not
resumed by this split. Slice IDs replace the old global execution chain; old todo
IDs remain traceability references, not an instruction to run queued work.

Future outcome: usable, owning, typed Rust bindings for the pinned application-
facing WebRTC surface on Linux x86_64, core headless and native device opt-in.
Include transitive public contracts needed to construct, implement, use and
observe application roots, including applicable experimental/deprecated APIs.
Private/test-only internals and unrelated algorithms are not parity obligations.

Pinned SDK: `ba469aa2093ba950066258ca0a59a6fbd1295582`. Other revisions are leads,
not evidence. Use [checkpoint.md](checkpoint.md) as the starting inventory.

## Non-negotiables

- Public APIs may break. Prefer simple ownership, typed inputs and documented
  validation for native-supported inputs, not exhaustive preservation of invalid
  numeric representations. Preserve legitimate optional/default/native-error
  distinctions and open protocol tokens.
- CXX, raw pointers and unsafe consumer adapters remain private. No generic
  configuration bag masquerading as complete public APIs.
- No fabricated success, synthetic media/metadata, provider substitution,
  silent clamping/normalization, hidden registry lookup, guessed source/SSRC joins,
  raw-RTP escape hatch, new SDK/dependency runtime patches or substitute peers.
- Guard genuinely reached native abort/UB and Rust memory hazards. Do not expand
  speculative invalid-domain proofs into unrelated work. Block unresolved unsafe
  installation honestly; escalate a concrete conflict rather than invent a pass.
- Retain native and Rust dependent resources through callbacks, queues and final
  destruction. Environment ownership alone does not prove randomness-lease
  retention. Cover aliases, migration, reentry, concurrency and retirement.
- ProductionSession remains move-only Send, non-Sync and single mutable-owner,
  usable after caller-thread migration. Every applicable slice includes real
  builder and actor/session paths, not a separate later actor-integration phase.
- Controlled execution is explicit, caller-owned and sequence-bound: no hidden
  pumping/time advance, worker thread, real-time wait or production fallback.
  Reject incompatible combinations before unsafe native consumption.
- Native packetization, RTP/RTCP, crypto, feedback, retransmission, reassembly and
  codec behavior remain engine-owned.

## Explicit exclusions and limitations

- Custom NetEq installation into peers or ProductionSession is excluded.
  Standalone owning NetEq remains in scope; stock peer selection stays intact.
- Public unsafe legacy decoder Decode/DecodeRedundant/DecodePlc and PLC query
  hooks are excluded. Internal SDK compatibility is not thereby removed.
  Standalone negative-return/no-write refusal is allowed with typed observation,
  but is not genuine DSP or internal CNG support.
- No producer-VLA override; stock WebRTC owns VLA generation/serialization.
  Received VLA is unavailable through selected stock application roots: no
  guessed peer/SSRC join. Do not claim simultaneous three-rung stock reception
  merely because sender capability is supported.
- Keyframe feedback uses actual encoder callbacks, not idle-source/passive RTCP
  stand-ins. No SCTP consumption-credit or generic dcSCTP pull/reassembly bridge.
- Downstream simulator integration, other-platform expansion, real-SFU
  interoperability, downstream migration and publication are later goals.
  Preserve existing platform packaging contracts; do not break them casually.
- H264 is required in headless Linux core where non-device implementations are
  supported. Technical codec qualification is not patent/license clearance or
  publication authorization. Keep notices and documented prerequisites.

## Parallel work and write ownership

1. Use one branch/worktree per assigned slice, starting at the same checkpoint
   plus explicitly accepted prerequisite commits. Do not concurrently edit one
   live worktree. A slice may start its bounded work before all other APIs finish.
2. Exclusive files listed by a slice are its lane. New modules/tests use the
   slice prefix or another explicitly reserved name. Request a lease before
   editing another lane's file.
3. A single integration coordinator owns shared seams: `src/lib.rs`,
   `src/codec.rs`, `src/peer.rs`, `src/production.rs`, `src/execution.rs`,
   `src/audio.rs`, `src/video.rs`, `native/codec.{h,cc}`, `native/peer.{h,cc}`,
   `native/audio.{h,cc}`, `native/video.{h,cc}`, `native/execution.{h,cc}`,
   `native/rtp_values.{h,cc}`, `Justfile`, build/manifest pins, global docs and
   shared `tests/audio.rs`, `tests/video.rs`, `tests/production_session.rs`.
   Workers can propose narrow, named integration patches from their branches;
   the coordinator serializes/cherry-picks them. Do not share-lock entire domains
   indefinitely. Extract a seam into a dedicated module when that reduces conflict.
4. Shared-seam integration is part of the slice's completion, not a reason to
   claim done on uninstalled code. Independent worktrees may carry local seam
   edits to make their own complete acceptance executable.
5. SDK checkout/toolchain may be shared read-only. Build outputs, staging,
   candidate archives, Cargo target dirs and test traces must be worker-owned.
   Never let two agents overwrite `.work/package`, GN output or a candidate
   archive under another running verification. Coordinate process-global tests.
6. A blocked slice reports the exact missing contract/evidence and continues only
   independent safe work. It cannot silently narrow the feature or waive a gate.

## Per-slice definition of done

Each plan is a vertical deliverable: public values/API, actual consumer/builders,
actor forwarding where applicable, ownership/error/lifecycle behavior, fixture,
contract documentation, matching-artifact qualification and reviewed commit.
A wrapper count, CXX declaration, standalone object, setter echo or unsupported
stub is not application completion.

- Maintain a slice-local pinned-header/member coverage table and explicit
  exclusions. The coordinator combines tables, rather than requiring a huge
  up-front audit to unblock all workers.
- Inspect the checkpoint before editing. Record delivered/qualified/source-only
  parts separately. Do not replay completed foundations as missing features.
- Compile/test small increments through verify, including failed checks and known
  repairs. Use repository build/export procedures, not a new parallel build system.
- A fresh source/bridge/native configuration requires a matching artifact. Record
  pin, compiler/FP flags, GN args, manifest identity and archive SHA-256. Do not
  relabel the checkpoint archive for a changed native candidate.
- All tests/builds/lints/benchmarks/acceptance gates go through verify. Provisioning,
  source edits and Git changes are separate. Keep a tested candidate unchanged
  until completion; receipts alone are not evidence.
- Execute a real public-path success case and rejected/error case plus lifecycle
  stress relevant to the slice. Run production migration, controlled mode and
  matching-artifact sanitizer checks where applicable; hardware absence must be
  distinguished from missing binding support.
- Record exact commands, candidate/artifact IDs, results and portable evidence
  references in the slice. Leave unresolved gates visible. Commit a coherent
  reviewed increment, not a blanket claim that all slices are complete.
