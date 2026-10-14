# S16: Encode caller audio in the real send path

Status: **not started**. Future assignment required; this is not a resumed execution order.
Baseline: commit `52b4d80`; pinned SDK `ba469aa2093ba950066258ca0a59a6fbd1295582`.
Read [shared-contract](../shared-contract.md) and [checkpoint](../checkpoint.md) first.

## User-visible outcome

Install a caller-defined encoder product and encode valid send input with real output/failure behavior.

## Dependencies and parallel boundary

- Completion prerequisites: [S15](15-installed-audio-factories.md).
- You may investigate and implement against frozen prerequisite interfaces in your isolated worktree now, but cannot claim integrated completion before these contracts pass.
- Consumers of this slice: only applicable composed acceptance, not a blanket dependency for all lanes.
- Legacy task references: #38, #17. Reference only; read source instead of replaying old todos.

## Scope

- Prove nonfallible SDK Encode sample/growth checks and ACM buffering/packetization consumption at the actual caller path.
- Provide truthful failure admission for callback Err/panic/malformed output without zero-output success substitution or invented EncodedInfo.
- Retain the real product/environment and all native controls/metadata across queued send callbacks and migration.

## Existing work to preserve

Use the current implementations, fixtures and contracts below. Distinguish **source
exists**, **named checkpoint evidence**, and **remaining actual application path**.
Do not recreate owned value graphs or temporary drivers merely to restart.

- [docs/audio-encoder-contracts.md](../../docs/audio-encoder-contracts.md)

## Write ownership

Reserved lane files/regions, after confirming no competing lease:

- `src/audio_encoder_provider.rs`
- `src/audio_encoder_sdk_failure.rs`
- `tests/slice_16_caller_encoder.rs`

Shared bridge/codec/peer/actor/native/header/build seams are owned by the
integration coordinator as listed in the shared contract. Use one branch/worktree;
propose narrow, named seam patches and serialize their merge. If an existing file
above is split with another slice, the listed section lease is mandatory. New tests
must be slice-owned instead of concurrent edits to shared audio/video fixtures.

## End-to-end work checklist

- [ ] Reconcile the pinned public members, checkpoint source and applicable evidence into a local coverage table.
- [ ] Deliver the missing typed API/values and real native implementation for the outcome above.
- [ ] Install through the actual public builders/endpoints and ProductionSession where applicable; standalone-only APIs must be explicit.
- [ ] Preserve defaults/optional/native errors, buffer bounds, roles, callback reentry/concurrency and dependent-resource/final-drop retention.
- [ ] Integrate coordinator-owned seams and document the usable contract/examples within this slice.
- [ ] Compile and execute the real public workflow and failures on a matching candidate artifact through verify.
- [ ] Run applicable actor migration, controlled-mode and sanitizer/lifecycle cases; record exact scope or concrete blockers.
- [ ] Review and commit the coherent slice. An uninstalled provider, fake success or unrun fixture is not done.

## Specific acceptance

- Caller encoding produces valid real payload and metadata in a negotiated call.
- Err/panic/malformed output is distinguishable from legitimate buffering before unsafe SDK/packetization consumption; retirement and replacement preserve outcomes.

Start with existing related `tests/` fixtures, then execute the dedicated slice
fixture. Use the shared contract's verification rules. Name exact commands and
prerequisites when launching gates; do not assume checkpoint artifact freshness
after native/bridge/build changes. Hardware, controlled, core/native and unavailable
cases must be explicitly distinguished rather than silently skipped as success.

## Exclusions and stop conditions

Standalone Rust callbacks are not installed parity. No SDK patch, fake bytes or placeholder encoded success.

All shared exclusions apply. If a real native consumer makes the required outcome
unsafe or unsupported, record the exact branch/contract conflict and escalate it.
Do not broaden invalid-domain analysis, patch the SDK, normalize output, substitute
a stock provider or quietly reduce scope to make the test pass.

## Evidence receipt

Fill when assigned, without copying the old monolith:

- Source commit/worktree and SDK pin:
- Current member coverage and remaining omissions:
- Accepted prerequisite commits:
- Native artifact/configuration/compiler/manifest/archive SHA-256:
- Exact verify commands, results and retained logs:
- Ownership/role/failure/migration/control/sanitizer witnesses:
- Explicit unsupported capabilities, exclusions and unresolved blockers:
- Reviewed integrated slice commit:
