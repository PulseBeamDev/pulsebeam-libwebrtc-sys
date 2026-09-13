# Plan 03 repair 02 build evidence

- Specification/plan: accepted `spec.md` source-patch and validation revision
  (2026-09-13); READY `03-apple-artifact-builds.md`.
- Review repaired: `review-03-replanned.md` findings 1 and 2.
- Base: `131195ad5a67843b5646fb4fcefd2897294effbd`.
- Candidate: recorded after the committed repair.

## Changed behavior

- `_source-state` now rejects untracked, non-ignored files before source-state
  classification, state transition, packaging, and provenance claims. Existing
  pristine and exact approved `BUILD.gn` patch classification is unchanged.
- `build.txt` now records `webrtc_repository` alongside its pinned revision,
  patch digest, and source state, matching the manifest source identity.
- The artifact-contract documentation now lists the repository in `build.txt`
  provenance.

## Local verification (Linux x86-64)

- `just --fmt --check` — passed.
- `cargo fmt --check` — passed.
- `git diff --check` — passed.
- `CARGO_HOME=.work/cargo-home PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test --doc --locked --offline` — passed (35 tests).
- Fixture-backed Rust tests with `PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR` set to an
  extracted `tests/fixtures/webrtc-core-linux-x86_64.tar.gz` — passed:
  `cargo test --lib` (18), and integration targets `execution` (5), `network`
  (3), `peer` (3), `data_channel` (3), and `video` (2), all `--locked
  --offline -- --test-threads=1`.

No Apple build or GitHub/CI workflow was triggered; those remain human-owned
execution evidence under the accepted validation boundary. No standalone
source-cleanliness or export harness was added.
