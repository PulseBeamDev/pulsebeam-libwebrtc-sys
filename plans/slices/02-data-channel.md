# S02: Exchange and close a data channel

Status: **not started**. Future assignment required; this is not a resumed execution order.
Baseline: commit `52b4d80`; pinned SDK `ba469aa2093ba950066258ca0a59a6fbd1295582`.
Read [shared-contract](../shared-contract.md) and [checkpoint](../checkpoint.md) first.

## User-visible outcome

Configure/open a channel, exchange ordered and unreliable messages, observe readiness/buffering/native outcomes, then close from a migrated session.

## Dependencies and parallel boundary

- Completion prerequisites: none beyond the frozen checkpoint and shared contract.
- This lane can start independently from the checkpoint; it does not wait for a global ledger, all documentation or another domain's implementation.
- Consumers of this slice: only applicable composed acceptance, not a blanket dependency for all lanes.
- Legacy task references: #20, #29, #30. Reference only; read source instead of replaying old todos.

## Scope

- Finish native properties, async outcomes, transport access and state/observer member coverage.
- Preserve transport semantics and bounded message/event retention without inventing SCTP consumption credit.
- Retain aliases and pending operations through close/drop and actor migration.

## Existing work to preserve

Use the current implementations, fixtures and contracts below. Distinguish **source
exists**, **named checkpoint evidence**, and **remaining actual application path**.
Do not recreate owned value graphs or temporary drivers merely to restart.

- [docs/capability-matrix.md](../../docs/capability-matrix.md)

## Write ownership

Reserved lane files/regions, after confirming no competing lease:

- `src/data_channel.rs`
- `native/data_channel.h`
- `native/data_channel.cc`
- `tests/slice_02_data_channel.rs`

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

- Two real peers exchange binary/text and fragmented messages under native reliability settings.
- Backpressure/readiness, rejected sends, close and overflow are observable and bounded.

Start with existing related `tests/` fixtures, then execute the dedicated slice
fixture. Use the shared contract's verification rules. Name exact commands and
prerequisites when launching gates; do not assume checkpoint artifact freshness
after native/bridge/build changes. Hardware, controlled, core/native and unavailable
cases must be explicitly distinguished rather than silently skipped as success.

## Exclusions and stop conditions

No dcSCTP pull/reassembly bridge or message-consumption-credit API.

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
