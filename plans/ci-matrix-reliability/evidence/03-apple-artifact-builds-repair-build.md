# Plan 03 repair build evidence

- Spec/plan: accepted `spec.md` revision 2026-09-13 and revised READY Plan 03.
- Base: `0ab1000af0ff12991024e9e6d66466885117a91b`.
- Starting candidate: `2f25a9d67716d03b772e0a0f638222181652a9a7`.
- Candidate: recorded by the commit for this repair.
- Changed behavior: core iOS alone applies the content-verified
  `patches/core-ios-remove-framework-objc.patch` delta; the patch SHA-256 is
  `c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037`.
  Producer metadata, Rust consumption, and release audit record/validate that
  state. Apple host protobuf output discovery and the export filter are fixed.
- Checks passed: `cargo fmt --check`; `git diff --check`; `just --list` and
  recipe rendering; locked offline Rust doc tests; fixture-backed locked offline
  Rust library tests; locked offline `execution`, `network`, `peer`,
  `data_channel`, and `video` integration tests.
- Limitation: no Apple build, GitHub Actions workflow, or CI dispatch was run;
  those remain human-owned execution evidence.
