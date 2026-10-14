# Five independently executable WebRTC completion plans

Status: APPROVED

## Motivation

Replace the long serial completion effort with five coarse, self-contained
application contracts that can be completed and verified independently in parallel.

## Outcome

`plans/parallel-slices/` contains five plan files and this active decomposition
spec. Each plan is one complete Markdown contract; it requires no shared planning
document or sibling changes. This spec and the five linked contracts are the
approval set, not a separate execution queue or progress ledger.

| Plan | Independently accepted outcome |
| --- | --- |
| [01: Peer connectivity, data channels and RTP transports](01-connectivity.md) | Applications create and negotiate owning peers, exchange native data-channel messages, observe/control supported RTP endpoints and retain/use actual transports safely through actor migration and teardown. |
| [02: Native media, video codecs and controlled replay](02-media.md) | Applications exchange raw PCM/video and concurrent caller-encoded native media, select video products, transform/protect frames, replace individual sources and deterministically replay the supported workflows, with sender-admission and lifecycle safety included. |
| [03: Audio codecs, adaptation, standalone NetEq and processing](03-audio.md) | Applications use and install owning stock/caller audio codec products, safely enable supported stock adaptation, drive standalone NetEq and select actual audio processing or explicitly opted-in Linux audio devices. |
| [04: Execution, network providers, observability and capture](04-providers.md) | Applications select owning execution, network/security, event-log, congestion/FEC/bitrate and opt-in Linux video-capture components that actually participate in native creation and media behavior, with truthful failures and lifecycle ownership. |
| [05: Matching Linux artifacts, packaging and consumer tooling](05-artifacts.md) | Applications build and consume truthful core/native Linux native packages with actual advertised implementations, provenance/configuration validation, real consumer execution and useful native diagnostics. |

## Behavior

Every plan is completable and verifiable against the same frozen software
checkpoint. Prerequisites are available in that checkpoint or included in the
plan's own contract. Native safety, installation, actor and proof gaps reached
by its outcome belong to it; they cannot be deferred to a sibling.

Peer connectivity and provider workflows can use baseline supported media.
Advanced media owns sender admission, video products, raw/encoded sources,
replacement, transformations/crypto and controlled replay. The audio plan owns
standalone/installed caller products, stock ANA, standalone NetEq and audio
processing/devices. Runtime/provider selection and package consumers use baseline
calls/implementations, not future media or codec extensions.

Independence is about acceptance, not a guarantee of conflict-free edits to one
repository or independently publishable packages. Ordinary merge reconciliation
and later composed checks cannot waive a plan's incomplete acceptance.

## Decisions

- The human requires a new folder with each plan self-contained in one Markdown.
- The human requests exactly five coarser plans.
- Independently complete and verifiable means no sibling changes are prerequisites.
- Coupled outcomes may be merged rather than hide completion dependencies.
- This pass specifies only; no binding implementation or crate restructuring.

## Invariants

Preserve the complete intended application capabilities, existing delivered
foundations, pinned native behavior, owning typed APIs and native defaults/
optional/errors. Preserve actual consumer safety, resource retention, production
actor migration and explicitly caller-owned controlled progress.

No scope reduction, synthesized output, invented success, hidden joins/provider
fallback or unqualified unsafe installation can manufacture independence. Source
presence or selected standalone successes are not installed-runtime proof. Changed
native candidates need matching artifact evidence.

After explicit approval, this folder is the only active semantic split. Old
execution plans remain history, not another dependency graph/shared contract/
progress manifest that builders must read or satisfy.

## Constraints

Source checkpoint: `52b4d80d9f5ebe18064515c952ec8e6382d789a0`. Plan-only
commit `042579b` adds no binding implementation. SDK pin:
`ba469aa2093ba950066258ca0a59a6fbd1295582`. Scope is Linux x86_64 headless
core plus explicitly opted-in native devices, preserving other packaging
contracts. Public APIs may change for simple ownership and typed native delegation.

No SDK/dependency runtime patch, public unsafe escape or substitute native engine.
Private/test-only unrelated internals are not public parity obligations. Native
unsupported behavior stays truthful and cannot substitute for unfinished bindings.

## Non-goals

Implementation, routine internal architecture/files/worktrees/fixtures, main
integration, publication, history rewrites, other-platform expansion, downstream
migration/simulator integration, real-SFU interoperability and patent clearance.

Custom peer/actor NetEq registration, public unsafe legacy decoder families,
producer-VLA override/guessed received VLA, SCTP credit/reassembly bridges,
passive keyframe substitutes and fabricated absent SFrame behavior remain excluded.

## Verification

- Each plan alone plus repository/SDK facts defines its behavior, operation scope,
  failure/lifetime constraints, exclusions and falsifiable proof.
- No installation/success/failure/lifecycle proof requires another plan's changes.
- The five contracts preserve all intended capabilities and reached safety duties;
  coupled work is included rather than removed to claim independence.
- Risk-based observable evidence rejects ignored selection, setter-only echoes,
  lost identity/state, synthetic media, unsafe lifetimes and stale artifacts.
  Real native/continuous lifecycle boundaries are required where narrower proof
  cannot faithfully protect the outcome, not as a blanket testing inventory.
- Drafting does not approve the revised grouping or authorize implementation.

## Risks

Pinned allocator and nonfallible caller-codec consumer safety may block their
owning plan. Independence removes sibling prerequisites, not native feasibility
risks. A concrete incompatibility needs a human decision, not silent scope
narrowing, SDK patches or fabricated completion.

## Deferred

Feature implementation and final integrated composition after independently
accepted outcomes are delivered. Later semantic changes require renewed approval.
