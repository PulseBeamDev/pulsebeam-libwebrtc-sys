# Final composed acceptance and merge gate

This is a coordinator gate, not a replacement for per-slice vertical acceptance.
It is future work after explicit assignment. Legacy traceability: #1, #2, #3,
#25-#32 and #40. Start auditing incoming receipts as slices land; do not defer
all compilation, documentation or behavior checks until this gate.

## Prerequisites

- Accepted commits and per-slice evidence from S01-S28, or an explicit human
  scope amendment for a genuinely unsupported requirement. A blocked native
  consumer is not automatically an exclusion.
- Combined pinned-header/member ledger covering all in-scope application roots
  and transitive public contracts, including domains discovered during execution.
- Clean integrated candidate identity and fresh matching core/native artifacts.

## Composed gates

- [ ] Native values/defaults/errors, public builder/actor installation and all
  required callable members are genuinely usable, not only CXX declarations or
  standalone providers. Review actual native consumption and installation.
- [ ] Repository source/format/lint checks and Rust/CXX/native compilation pass
  on the integrated candidate, including relevant protobuf on/off and core/native
  paths. Meaningful diagnostics are not missing-include cascades or launch receipts.
- [ ] Checksum-pinned archives pass manifest/configuration/license/symbol closure,
  actual advertised codec execution and Rust-only/SDK/real-Rust companion consumers.
- [ ] Public workflows cover peer operations, data channels, RTP/transport,
  raw/encoded media, selected providers, crypto/transforms, ANA and standalone
  NetEq. Stock peer NetEq selection remains stock; no custom registration is added.
- [ ] Required concurrent H264 L1T1/L1T3/three-rung/mixed and dual-fraction sources
  work without rebuilding peers; source replacement does not perturb other sources.
- [ ] Actor migration, bounded pending/unconsumed terminals, callback/resource
  retirement and final drop pass with actual roles and dependencies retained.
- [ ] Controlled bidirectional VP8/Opus and required mixed H264 impairment/replay
  pass with continuous no-thread/process/socket tracing and no hidden progress.
- [ ] Applicable matching-artifact sanitizers pass for buffers, callback aliases,
  replacement, close/drop and actual installed consumption. Successful source
  tests or standalone failure receipts cannot waive installed safety gates.
- [ ] Documentation and public examples match usable outcomes, scope and genuine
  native unsupported behavior. No wrapper-count, setter-echo or stock-substitution
  parity claims remain.
- [ ] Review integrated commits, record source/SDK/compiler/GN/manifest/archive
  identities plus exact verify commands/results/evidence, and report remaining
  limits. No publication/tag replacement/patent clearance is implied.

## Evidence and failure handling

Preserve per-slice receipts and archive hashes. Re-run affected gates when shared
native/bridge inputs or composition changes. A failed gate goes back to the
owning slice with a precise reproduction; do not rewrite a passing-looking
progress narrative, remove assertions, invent output or patch the SDK.

The old monolith's "implementation finished before testing" order is superseded.
This gate certifies integrated composition only after independently tested slices.
