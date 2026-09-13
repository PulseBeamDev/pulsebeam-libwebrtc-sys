# Plan 04 Windows bootstrap and path-boundary repair evidence

- Specification/plan: accepted `spec.md` source-patch and validation revision
  (2026-09-13); READY `04-windows-bootstrap-and-paths.md`.
- Review repaired: `evidence/review-04.md`.
- Base: `adc6bd1f285280bb740ca046311db4ecd04c8be4`.
- Candidate: recorded after the committed repair.

## Changed behavior

- Every Git-Bash-to-`cmd.exe` boundary now sets
  `MSYS2_ARG_CONV_EXCL='*'`, preserving `/d /s /c` and the quoted native
  command payload for CIPD validation, `gclient sync`, `gclient runhooks`,
  and license generation.
- Every direct `clang-cl` invocation now sets the same per-process guard. The
  bridge compilation and C++ smoke link therefore preserve `/MT`, `/c`, `/Fo`,
  `/I`, and `/Fe` arguments without changing the explicit native-path values.
- The existing CIPD pin and `windows-amd64` validation, Windows pristine
  source-state/provenance behavior, path conversion points, archive contract,
  and declared Windows libraries are unchanged.

## Local verification (Linux x86-64)

- `just --fmt --check`, `cargo fmt --check`, and base-range plus worktree
  `git diff --check` — passed.
- `CARGO_HOME=.work/cargo-home PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test
  --doc --locked --offline` — passed (35 tests).
- Fixture-backed Rust tests with `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR` set to an
  extracted `tests/fixtures/webrtc-core-linux-x86_64.tar.gz` — passed: `cargo
  test --lib` (18), and integration targets `execution` (5), `network` (3),
  `peer` (3), `data_channel` (3), and `video` (2), all `--locked --offline
  -- --test-threads=1`.

No Windows build or GitHub Actions workflow was triggered or requested.
Windows execution remains the human-owned Plan 06 evidence boundary; no
standalone Windows/path/bootstrap harness was added.
