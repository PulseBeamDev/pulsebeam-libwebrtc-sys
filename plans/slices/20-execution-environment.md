# S20: Run with a caller-owned execution environment

Status: **not started**. Future assignment required; this is not a resumed execution order.
Baseline: commit `52b4d80`; pinned SDK `ba469aa2093ba950066258ca0a59a6fbd1295582`.
Read [shared-contract](../shared-contract.md) and [checkpoint](../checkpoint.md) first.

## User-visible outcome

Select clocks, field trials, queues/metronomes and retained resources through real creation APIs while preserving production and controlled semantics.

## Dependencies and parallel boundary

- Completion prerequisites: none beyond the frozen checkpoint and shared contract.
- This lane can start independently from the checkpoint; it does not wait for a global ledger, all documentation or another domain's implementation.
- Consumers of this slice: only applicable composed acceptance, not a blanket dependency for all lanes.
- Legacy task references: #19, #20. Reference only; read source instead of replaying old todos.

## Scope

- Finish public execution/environment factory/product inputs and role-thread dependency injection with real native default selection.
- Preserve exact trial lookup/copy contracts, durable clock identity/bounds, relative/absolute scheduling and explicit optional resources.
- Retain native dependencies and Rust randomness leases across callbacks/migration/retirement, rejecting incompatible controlled combinations.

## Existing work to preserve

Use the current implementations, fixtures and contracts below. Distinguish **source
exists**, **named checkpoint evidence**, and **remaining actual application path**.
Do not recreate owned value graphs or temporary drivers merely to restart.

- [docs/clock-contracts.md](../../docs/clock-contracts.md)
- [docs/environment-contracts.md](../../docs/environment-contracts.md)
- [docs/controlled-scheduling.md](../../docs/controlled-scheduling.md)

## Write ownership

Reserved lane files/regions, after confirming no competing lease:

- `src/clock.rs`
- `src/time_delta.rs`
- `src/timestamp.rs`
- `src/ntp_time.rs`
- `src/field_trials.rs`
- `src/task_queue_provider.rs`
- `src/metronome.rs`
- `native/clock.h`
- `native/clock.cc`
- `native/task_queue_provider.h`
- `native/task_queue_provider.cc`
- `native/metronome.h`
- `native/metronome.cc`
- `tests/slice_20_execution.rs`

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

- Public environment/provider selections affect actual native creation and observed clock/queue behavior.
- Panic/reentry/concurrency, scheduled cancellation and final release retain correct roles without hidden progress.

Start with existing related `tests/` fixtures, then execute the dedicated slice
fixture. Use the shared contract's verification rules. Name exact commands and
prerequisites when launching gates; do not assume checkpoint artifact freshness
after native/bridge/build changes. Hardware, controlled, core/native and unavailable
cases must be explicitly distinguished rather than silently skipped as success.

## Exclusions and stop conditions

Do not reimplement clocks/schedulers or allow a borrowed callback view to escape; S26 owns composed deterministic replay.

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
