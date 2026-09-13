# Plan 02 build evidence

- Spec/plan revision: `spec.md` SHA-256 `12a7dbfcac0d2739dc7fede4fbaaa62a2d252d0d6f9f0ff3aa950039d173774c`; `02-cross-target-runtime-linking.md` SHA-256 `42430285aa46f7169adf5396ffd3d3dbd0c8892635661e96822d0642ca2bc9a6`.
- Base: `0ad92d89b0c71033ace31a5ea21bc732c4c2b39c`.
- Behavior: Android manifests now carry `--unwindlib=none`, matching the C++ smoke policy while retaining the one bundled libunwind implementation. Linux arm64 manifests and C++ smoke links now select the pinned Clang compiler-rt with `--rtlib=compiler-rt`, avoiding the Bullseye sysroot's older libgcc.
- Checks: `cargo fmt --check` passed; `git diff --check` passed; Linux x86-64 `cargo test --doc --locked --offline` passed (35); Linux x86-64 fixture-backed `cargo test --lib` and `execution`, `network`, `peer`, `data_channel`, and `video` integration tests passed (34 total).
- Limitation: no cross-target build or GitHub Actions run was triggered, requested, or awaited; Plan 06 requires human-dispatched evidence for the six affected artifact jobs.
