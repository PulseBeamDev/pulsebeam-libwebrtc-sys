# Plan 04 Windows bootstrap and path-boundary build evidence

- Specification/plan: accepted `spec.md` source-patch and validation revision
  (2026-09-13); READY `04-windows-bootstrap-and-paths.md`.
- Base: `3318dc17b3a604483d30a690ed3cad9daad0aced`.
- Candidate: recorded after the committed implementation.

## Changed behavior

- The existing pinned depot-tools revision
  `ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4` is retained. Inspection of that
  pinned tree found its `cipd.bat` `windows-amd64` bootstrap and corresponding
  SHA-256 entry in `cipd_client_version.digests`, so a pin update was neither
  needed nor permitted.
- Windows synchronization validates the selected `windows-amd64` bootstrap
  against that pinned digest metadata, then starts the native CIPD client
  before `gclient runhooks`. Failures name the requested host and pinned
  depot-tools identity.
- Git/Bash checkout and source-state operations retain POSIX paths. Native
  CIPD, gclient, GN, Ninja, LLVM tools, Python/vpython, clang-cl, and Cargo
  receive explicitly converted Windows paths. The Plan 03 exact source-state
  guard remains Git/Bash-side and Windows artifacts continue to declare the
  pristine state.
- Windows bridge and C++ smoke links retain `/MT` and the existing declared
  system-library contract.

## Local verification (Linux x86-64)

- `just --fmt --check`, rendered recipe checks, `cargo fmt --check`, and
  `git diff --check` — passed.
- `CARGO_HOME=.work/cargo-home PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test
  --doc --locked --offline` — passed (35 tests).
- Fixture-backed Rust tests with
  `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR` set to an extracted
  `tests/fixtures/webrtc-core-linux-x86_64.tar.gz` — passed: `cargo test
  --lib` (18), and integration targets `execution` (5), `network` (3),
  `peer` (3), `data_channel` (3), and `video` (2), all `--locked --offline
  -- --test-threads=1`.

No Windows build or GitHub Actions workflow was triggered. Production Windows
execution remains the human-owned Plan 06 evidence boundary; no standalone
Windows/path/bootstrap harness was added.
