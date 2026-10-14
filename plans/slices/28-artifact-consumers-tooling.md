# S28: Build and consume matched Linux SDK artifacts

Status: **not started**. Future assignment required; this is not a resumed execution order.
Baseline: commit `52b4d80`; pinned SDK `ba469aa2093ba950066258ca0a59a6fbd1295582`.
Read [shared-contract](../shared-contract.md) and [checkpoint](../checkpoint.md) first.

## User-visible outcome

Build core/native Linux packages with required real implementations and consume them through Rust and SDK/native companion programs.

## Dependencies and parallel boundary

- Completion prerequisites: none beyond the frozen checkpoint and shared contract.
- This lane can start independently from the checkpoint; it does not wait for a global ledger, all documentation or another domain's implementation.
- Consumers of this slice: only applicable composed acceptance, not a blanket dependency for all lanes.
- Legacy task references: #5, #24, #26, #27, #28, #31. Reference only; read source instead of replaying old todos.

## Scope

- Preserve passed core export and SDK clone-root/CXX real-runtime fixes; qualify remaining native flavor/protobuf on-off/tooling paths.
- Bundle publicly constructible Linux VP8/VP9/AV1/audio/APM/H264 and required event/extension helpers; validate symbol closure, FP/compiler flags, manifests and licenses.
- Finish meaningful clangd/source setup and real Rust-only/SDK/bridge consumers using exact source/candidate checksums.

## Existing work to preserve

Use the current implementations, fixtures and contracts below. Distinguish **source
exists**, **named checkpoint evidence**, and **remaining actual application path**.
Do not recreate owned value graphs or temporary drivers merely to restart.

- [docs/capability-matrix.md](../../docs/capability-matrix.md)

## Write ownership

Reserved lane files/regions, after confirming no competing lease:

- `tools/native_compile_commands.py`
- `tools/native_cc_wrapper.py`
- `tools/native_provenance.py`
- `tools/write_artifact_manifest.py`
- `consumer/bridge_smoke.cc`
- `consumer/frame_callback_slot.cc`
- `consumer/transformable_frame.cc`
- `consumer/video_frame_metadata.cc`
- `tests/slice_28_artifact_consumers.rs`

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

- Fresh matching packages link and run real advertised codec/consumer workflows, including H264 encode and decode, for applicable core/native configurations.
- Stale/malformed artifacts are rejected; generator/include/provenance/sanitizer gates use the actual pinned runtime and configuration.

Start with existing related `tests/` fixtures, then execute the dedicated slice
fixture. Use the shared contract's verification rules. Name exact commands and
prerequisites when launching gates; do not assume checkpoint artifact freshness
after native/bridge/build changes. Hardware, controlled, core/native and unavailable
cases must be explicitly distinguished rather than silently skipped as success.

## Required per-flavor evidence matrix

Maintain separate core and native receipts for every row. A single successful
archive or H264 run does not certify all implementations or both flavors.

| Implementation | Headless core | Native Linux | Required evidence |
| --- | --- | --- | --- |
| VP8/VP9 | Required | Required | Pinned public factory/product construction, compile/link closure and real advertised encode/decode execution |
| AV1 | Required where the pinned public implementation is constructible | Same supported implementation, explicit configuration | Compile/link and real advertised execution; document exact native unsupported constructor/configuration, not a substitute |
| Built-in audio codecs | Required | Required | Discovery/create, compile/link and advertised encode/decode consumer execution |
| APM/software audio processing | Required, no device activation | Required | Builder/product compile/link plus actual processing workflow |
| H264 non-device implementations | Required | Required | Factory/product compile/link and real encode/decode, required DD/scalability and notices/prerequisites |
| Event-log/RTP-extension public helpers | Required | Required | Symbol closure and actual public helper consumers, with protobuf-on/off behavior separately recorded |
| Physical audio/camera/screen backends | Explicitly excluded from core | Explicit opt-in | Compile/link and available-device/provider workflows; hardware absence is an observed limitation, not a passing device test |

Record SDK/compiler/FP flags, GN/protobuf args, source/artifact manifest and SHA-256,
exact commands, results and limits for each flavor/configuration. Disabled-feature
outcomes must be truthful and must not enter inherited native assertions. Hardware
absence does not waive software codec/processing tests.

## Exclusions and stop conditions

No other-platform expansion, release/tag rewriting, patent clearance or publication. Each slice still builds/tests its own changed candidate.

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
