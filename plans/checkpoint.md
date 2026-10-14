# Frozen starting checkpoint

## Source identity

- Commit: `52b4d80d9f5ebe18064515c952ec8e6382d789a0`.
- Subject: `chore: checkpoint in-progress WebRTC bindings`.
- Captures 250 files, 60,669 additions and 676 deletions from the prior worktree.
- This is an in-progress source checkpoint, not a declaration of full parity.
- Pinned SDK: `ba469aa2093ba950066258ca0a59a6fbd1295582`.
- SDK checkout was pristine. No SDK patch, download, installation or publication
  was performed during the checkpoint operation.

## Evidence that must not be replayed as missing implementation

A matching Linux x86_64 core artifact with H264/protobuf enabled was built and
exported successfully. Archive SHA-256:
`31e974b0921528ec92bbac5f7dc110ef410fec7984b2dc4b6ae3d6fefd8dc9ca`.
Local path: `dist/webrtc-core-linux-x86_64.tar.gz`. The archive is not in Git;
its presence in a new worktree is not guaranteed. Rebuild when source/configuration
changes. Never relabel this archive for a changed native/bridge candidate.

Passed evidence before the checkpoint:

| Gate | Retained evidence |
| --- | --- |
| Matching core SDK build and export, SDK clone-factory closure, CXX provenance and C++/Rust companion linking | verifier `b1824639-a70c-4dd3-b02e-3782c396adb9`, `/tmp/pi-verifier-TeeMR0/output.log` |
| SDK C++ consumer execution including H264 encode/decode, callback-slot/metadata checks, and real Rust-linked transformable-frame ownership/cloning | verifier `55a2e0ef-3fd7-437e-aff6-73aee0f269d8`, `/tmp/pi-verifier-LkUlaG/output.log` |
| 13 exact standalone native Rust factory/product/frame/handoff/forwarding tests | verifier `22a8b978-0cc4-49bb-8732-0c526f5feb18`, `/tmp/pi-verifier-EuLpwY/output.log` |
| 18 exact standalone failure/retirement/reentry/concurrency tests | verifier `1c9d7bf2-481a-4c9d-bb68-6417bf2a6f77`, `/tmp/pi-verifier-EpFnnA/output.log` |
| Four raw-metadata, private no-write legacy-refusal, nested-forwarding and isolated randomness-lifetime witnesses | verifier `90b760e9-0f7f-4fcf-8ffe-999454ca355c`, `/tmp/pi-verifier-gxJsna/output.log` |
| Core/native offline Rust all-target integration checks, artifact-support tests | verifier `e7d1b09b-83d6-46ef-bb2a-572f560b9def`, `/tmp/pi-verifier-GG92y8/output.log` |
| 21 scoped Python policy/provenance tests and 25 artifact-support tests | verifier `a14280d1-e333-47af-8171-c6deb3df3d40`, `/tmp/pi-verifier-WIhQHC/output.log` |
| Latest five clangd-database generator prerequisite tests and whitespace check | verifier `031b8419-7631-4399-a863-b689f8bbf4f8`, `/tmp/pi-verifier-bIXOFh/output.log` |

These are named historical candidate receipts. The `/tmp` logs are local retained
paths, not guaranteed portable artifacts; preserve relevant receipts when handing
off. Broad/heavy-file clangd checks timed out. Sampled native diagnostics became
meaningful; this is not complete native lint qualification.

## Source foundations to retain

Owning native time/environment/media/RTP/transport/error/codec values; byte-safe
SDP/ICE; factory/peer configuration; provider scaffolding; selector/factory values;
transformable frame owners; standalone audio SDK dispatch, products and retained
forwarding; actor configuration choices; controlled-world infrastructure; and
bundled Linux H264/protobuf/provenance changes are already present.

Historical narrower production/controlled metadata work at
`9841c58730bff038447e234d9e5c93aaf22fd52f` is a baseline, not proof of the newer
whole scope. Keep it; do not recreate existing values or drivers just to restart.

## Unfinished, not certified by the checkpoint

Installed caller audio factory Query stability (#37), encoder Encode failure
admission (#38), decoder/internal-CNG consumer admission (#39); allocator-safe
sender installation; live transform delivery; full factory/selector/product
installation; ANA; complete standalone NetEq qualification; full actor/dependency,
RTP/transport/device/media graphs; generalized mixed sources/replacement; native
flavor, composed controlled replay, sanitizers and final public-member coverage.

The explicit empty-payload SDK parser edge case remains outside the passed
standalone batches. Standalone no-write legacy refusal is not DSP/CNG support.
Source-wired or statically reviewed is not runtime-qualified.
