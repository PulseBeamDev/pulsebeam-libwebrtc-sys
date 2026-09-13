# Plan 04 Windows bootstrap/path repair-02 build evidence

- Specification/plan: accepted `spec.md` source-patch and validation revision
  (2026-09-13); READY `04-windows-bootstrap-and-paths.md`.
- Repair authority: the human-approved one-off Plan 04 repair exception recorded
  in `state.md`; review finding `evidence/review-04-repair-02.md`.
- Base: `db50c46585ba0fd3f47d9147f750318413c21e37`.
- Candidate: recorded after the scoped repair commit.

## Changed behavior

- Restored the missing Windows `_bridge-objects` compilation of
  `native/network.cc` to `network.obj` between the existing `execution.cc` and
  `codec.cc` compilations.
- The restored direct `clang-cl` invocation uses the existing per-process
  `MSYS2_ARG_CONV_EXCL='*'` guard, so its native arguments retain the same
  Windows path boundary behavior as every adjacent call.
- No other workflow, pin, path-conversion, source-state/provenance, archive,
  ABI, or test behavior changed.

## Local verification (Linux x86-64)

- `cargo fmt --check` and `just --fmt --check` — passed.
- `CARGO_HOME=.work/cargo-home PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test
  --doc --locked --offline` — passed (35 tests).
- Fixture-backed locked/offline Rust tests with
  `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR` set to an extracted
  `tests/fixtures/webrtc-core-linux-x86_64.tar.gz` — passed: `cargo test --lib`
  (18), and integration targets `execution` (5), `network` (3), `peer` (3),
  `data_channel` (3), and `video` (2), all with `--locked --offline --
  --test-threads=1`.
- `git diff --check db50c46585ba0fd3f47d9147f750318413c21e37` and `git diff
  --check` — passed.

No Windows build, GitHub Actions workflow, `just check`, or standalone
regression harness was run or added. Windows execution remains human-owned
external evidence.
