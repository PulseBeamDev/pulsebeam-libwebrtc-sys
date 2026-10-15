# Connectivity implementation batch and final verification

Contract: `01-connectivity.md`. Platform: Linux x86_64 headless core.

## Workflow

Per the user's instruction, finish the implementation batch first. Record
verification obligations here. Do not run incremental tests/builds or generate
per-feature artifacts. After all implementations are complete, freeze the
candidate, refresh provenance pins once, and run final qualification against
matching artifacts. Launch receipts are not passing evidence.

## Current checkpoint

- `3bb3e8c`: owning ICE configuration/candidates/stats/packet operations.
- `f2257b6`: delayed description overloads, owning Plan B streams, actual
  native sender/receiver/transceiver collection snapshots and source leases.
- Both checkpoints have matching release, ASan and cold-consumer evidence.
- Final batch adds native peer-wide data-channel message
  observation, legacy renegotiation, receiving-change and usage-pattern hooks,
  bandwidth-estimation reconfiguration, and generic audio/video SetTrack.
  A compile check launched before the workflow change passed for an earlier
  observer-only candidate. It does not qualify the current batch.
- Final source inventories c0e75f13, 4797bedc and c8d09199 completed. They found
  bandwidth-estimation reconfiguration and generic SetTrack gaps, now implemented;
  no remaining concrete transport gap was reported. Final review and the
  qualification sequence below are complete.

## Completed implementation and evidence checklist

### Peer observer and peer-wide data-channel message batch

- [x] Implement local and ProductionSession owning APIs, byte-bearing message
  values, native ID/timestamp/direction/type, bounded retention and drop counts.
  Qualified by the final native runtime fixtures below.
- [x] Preserve SDK observer install/replace/uninstall sequencing on network.
  Reject replacement from SDK network callbacks before any reverse blocking
  invoke; controlled roles remain explicitly caller-pumped. SDK-owned state
  survives queued install/replacement/teardown.
- [x] Document that native channel IDs and labels are not session resource IDs.
- [x] Document that this pin declares receiving-change but has no source invoker.
- [x] Exercise actual text and binary sends/receives with distinguishable bytes,
  non-UTF-8/NUL-bearing labels, exact SDK channel IDs and plausible Unix times.
- [x] Observe independent channel events and peer-wide observations together.
- [x] Exercise queued copies after disable, no new callbacks after fenced disable,
  overflow/loss accounting, and caller-thread migration/close/drop.
- [x] Exercise final actor retirement from an actual network-role readiness wake,
  plus rejected reentrant observer replacement without a blocking-invoke cycle.
- [x] Observe actual scheduled usage reports and legacy renegotiation callbacks;
  preserve distinct interesting-usage log occurrences as bounded non-advisory
  records, not latest-state snapshots. Do not synthesize receiving-change
  notifications for the unemitting SDK hook.

### Bandwidth estimation and generic SetTrack batch

- [x] Bind native ReconfigureBandwidthEstimation with typed SDK-default settings
  and local/actor operations. Void native invocation is not an emitted-probe claim.
- [x] Bind generic SDK SetTrack for existing audio/video senders, preserving its
  boolean rejection separately from binding errors and supporting None detach.
- [x] Preserve original Rust source owners by actual native track/sender identity;
  normalize sender-backed video aliases without strong peer/source cycles.
  Generic replacement shares existing controlled video and layout admission.
- [x] Verify accepted audio replacement and detachment through actual new-source
  delivery/old-source detachment, not only track getter echoes. Cover alias reuse,
  wrong media-kind SDK false result, migration, close and source retirement.
- [x] Verify generic video replacement does not bypass controlled source/layout
  guards and preserves source leases through sender-backed aliases.
- [x] Verify default/explicit BWE settings before media and on a connected
  native controller, plus
  post-close rejection. On an RTX-capable sending video transceiver with no media,
  observe actual native padding/probe behavior under allow_probe_without_media;
  do not certify control effects with a setter echo.
- [x] Include callback, replacement, payload and destruction paths in final
  release/ASan and cold-consumer coverage.

BWE evidence basis: the pinned transport controller applies the setting to
repeated initial probing and to the pacer's allow-without-media flag only when
its packet router supports RTX payload padding. If a controller already exists,
it recreates it and updates network availability. Use negotiated RTX capability
and actual packet/native-stat observation, not an invented effective-settings
getter. Source: `call/rtp_transport_controller_send.cc:278-304`.

### Final scope inventory

- [x] Source inventory: peer/factory/description/candidate/state/stats members.
  c0e75f13 identified ReconfigureBandwidthEstimation; implemented in this batch.
- [x] Source inventory: channel/RTP/DTMF/track/stream members.
  4797bedc identified generic SetTrack audio support; implemented in this batch.
- [x] Source inventory: DTLS/SCTP/ICE, certificates/addresses/internal operations.
  c8d09199 found no further concrete supported gap.
- [x] Account explicitly for permitted exclusions and genuinely unsupported SDK
  operations. Source presence alone is not installed-runtime proof.

## Implementation review

- Source review cf21f0e8 found no concrete generic SetTrack/source-owner issue.
- Source review 05caf717 found interesting-usage occurrences incorrectly grouped
  with latest-state advisories. Fixed by making usage records non-advisory;
  their overflow remains bounded and counted.
- Final fixtures were added after all implementations completed:
  `tests/production_connectivity.rs` extends the migrated channel fixture with
  actual bidirectional log metadata, bounded overflow/disable and network-wake
  final retirement, and adds a combined actual RTX-padding/audio replacement
  fixture with distinguishable committed Opus payloads.
  `src/media_stream.rs` adds a sender-backed alias/source-retirement regression.
- Formatted Rust; regenerated pinned CXX once. Bridge-source SHA256:
  `01cf6fe8e2de10d5bf0cef2359a5f90af2e74165210b68361683c2961c69ef98`.
  Native adapter SHA256:
  `4ddf9c0da6e1822717f01e94f9e610bd40214868348fac4f5d6036c939aceeaa`.
- The implementation candidate stayed frozen during qualification runs. All
  subsequent repairs were test/doc-only and waited for active runs to finish;
  no native/bridge repair or per-feature artifact rebuild was needed.

## Final qualification sequence

1. Finish implementations and update this checklist. Then add/extend the
   narrowest faithful regression fixtures for the final batch.
2. Format, regenerate the CXX bridge with the pinned tool, and refresh the bridge
   source/header hashes and native adapter digest in
   `build_support/manifest.rs` and the artifact-support fixture.
3. Freeze the source candidate. Run `just check` and whitespace checks through
   the verifier.
4. Produce exactly matching release and address-sanitized `core linux-x86_64`
   exports using the existing candidate/ASan workspaces. Record their provenance
   and archive SHA256. Do not reuse the historical archive as changed-native proof.
5. Run affected connectivity suites against each corresponding artifact,
   including final-batch observer/BWE/generic-SetTrack fixtures and source-lease
   regressions. ASan must include
   leak detection and halt on error. Existing affected suites:
   `production_connectivity`, `production_plan_b`, `ice_values`,
   `production_sdk_wake_drop`, `production_session`,
   `production_legacy_selectors`, `production_media_references`,
   `peer_signaling`, `stats`, plus any final inventory additions.
6. Run the isolated Rust-only consumer against the final release archive and its
   actual SHA256, with networking/toolchain isolation as implemented by
   `tools/rust_only_consumer.py`.
7. Reconcile all evidence and source review findings. Repair failures only after
   applicable verifier runs have stopped, then repeat only invalidated gates.
8. Commit the completed batch and report exact scope, limitations and evidence.

## Known historical broad-suite failures, not a green-suite claim

Earlier full `--lib` release run `dda154d5` had five failures: audio-encoder
narrowing, SDK environment clone identity, two NetEq extreme-sentinel expectations,
and video-factory Create/context assumptions. It had 122 passes and one ignored
test. ASan full `--lib` run `36cd9544` aborted in an audio-encoder extreme-sentinel
case. No baseline counterfactual was established. These are not evidence that the
entire suite is green, and they must not silently disappear from the final report.
Do not expand unrelated speculative numeric obligations into this slice.

Retained historical logs:
- `/tmp/pi-verifier-l5ZhNJ/output.log`
- `/tmp/pi-verifier-uRz8rB/output.log`

## Final evidence

Final source candidate qualification started after implementation completion.

- `374df00a`: `just check` and `git diff --check` passed.
  The separate C++ syntax command failed because its temporary include overlay
  omitted current native headers, selecting stale installed headers instead.
  Corrected the overlay by copying current `native/*.h`; no implementation or
  provenance pin changed. The subsequent full native builds supersede this
  auxiliary check.
- `2be3b9f6`: auxiliary syntax retry raced release export's regeneration
  of the installed include tree (missing SDK/standard/CXX headers). Superseded by
  successful full native compilation in both actual exports, not a source fix.
- `5eb35022`: matching release export passed; `abf8ca73`: matching ASan export
  passed. Native/bridge candidate is unchanged by subsequent test-only repairs.
  Release archive SHA256:
  `33a5ab755a11cccd7d7afa06323c30f837c1dcdef64e4f726eb52c2d75a65984`.
  ASan archive SHA256:
  `b6e41d9daeb903c74950b08c96da9f55cbc31c2f1d8d21e27af7a6cd506c1465`.
- `e0ce476c`: isolated offline Rust-only consumer passed against that release
  archive and SHA256.
- `895ad00d` / `9dfc9829`: initial release/ASan runtime batches failed only
  at the new fixture's assertion that detached producer input is accepted.
  Actual RTX padding, distinguishable audio replacement, kind rejection and
  network-wake teardown reached their successful assertions. Corrected fixture
  to expect the existing producer's Backpressure result after SDK detachment.
- `40c4d632` / `80a08efc`: extended release/ASan regressions found an
  unchanged capability fixture expecting a default video formatter. Pinned
  `api/rtp_parameters.h:190` initializes RtpCodec.kind=AUDIO, and the unchanged
  native default constructor/formatter returned `[audio//0/1]`. Corrected that
  assertion and added an explicit default-kind assertion. No implementation,
  native input, bridge pin or artifact changed.
- `4c2e0dd2` / `f9a75910`: all new feature and migration fixtures passed;
  extended integration stopped at another unchanged fixture that confused
  RTCError.message() with its full builder storage. Parent inspection verified
  StringBuilder already passed the full-byte assertion; failure was specifically
  at `tests/rtp_values.rs:784` (RTCError.message_bytes).
  Pinned `api/rtc_error.cc:64-65` returns message_.str().c_str(); its public
  string_builder() retains full bytes. Corrected only the two error-view
  assertions, and separately assert full builder bytes remain intact. Do not
  attribute truncation to AbslStringify or change the binding to synthesize bytes.
- `03dc00ad`: repeated `just check` and whitespace checks passed.
- The source-retirement fixture now drops its deliberately owning public source
  observation before asserting final retirement through a pure SDK source facade.
  This avoids mistaking the observation's required source ownership for a leak.
- `51157193` / `e407f525`: source-lease and retention unit groups passed
  in release/ASan, including original-source survival through sender-backed
  aliases and final native ended-state after stream/input lease retirement.
- All runs above finished before subsequent test/doc-only repairs.
- `c47de1a3`: final release integration passed, **99 tests in 26 suites**.
- `c756a33d`: final ASan integration passed, **99 tests in 26 suites**, with
  `detect_leaks=1:halt_on_error=1`.
- `b09da9b6`: final `just check` and whitespace checks passed.
- Final integration/source tested candidate:
  `f2257b69329e0a8ea5c0e3cd233ae69ee706ca3f:31598e8a34edb96cac3343a584527d5465230a7f78f08e3922de04ad3fa7e0e6`.
  The six focused lease/retention tests passed on the same production code in
  `51157193` / `e407f525`; later changes were an unrelated RTP test oracle
  and a documentation line, not those tests or their implementation.
- Historical full-library failures above remain outside this green **targeted**
  qualification claim. No full-crate green claim is made.

## Evidence interpretation and limits

- The channel fixture observes actual send/receive callbacks, copied bytes,
  native channel IDs, wall-clock timestamps, independent channel delivery,
  bounded overflow, fenced disable and deferred final-owner network-role drop.
- BWE evidence distinguishes default no-media behavior from actual RTX padding
  after enabling the native setting. It does not invent an effective-settings
  getter or promise bandwidth allocation. Reconfiguration forwards to the SDK
  before/after estimation without a binding-local state cache.
- Audio replacement delivers distinguishable committed 440/880-Hz Opus packet
  bytes through the real native transport. Wrong-kind rejection remains the
  SDK boolean; detachment produces real producer backpressure/no further media.
- Generic video replacement reuses the already-qualified convenience's exact
  controlled-source/layout guard helper. Its new alias/source retirement path
  has separate native state and ASan unit evidence; no new profile is claimed.
- Receiving-change has no SDK source invoker in this pin. It is wired, documented
  and not fabricated. Usage reports are actual scheduled SDK callbacks; distinct
  log records are non-advisory by source review.
- Existing SDK/contract exclusions remain: sender parameter writes/allocation,
  selector/crypto/transformer installation, private ICE algorithms/pointers,
  unrelated helpers, new source/device/provider products, other platforms and
  real-SFU composition. Baseline media selections remain usable.
- Controlled execution remains caller-owned with no hidden pump. An SDK network
  wake may retire its final actor safely, but observer replacement in that
  callback is rejected before a reverse blocking invoke. Wakers must schedule
  ordinary engine operations rather than run them inside native callbacks.
- The isolated consumer and both exports match unchanged production/native
  source; later fixes changed only test or documentation text. Archive hashes
  and embedded provenance are not relabeled to a historical source checkpoint.
