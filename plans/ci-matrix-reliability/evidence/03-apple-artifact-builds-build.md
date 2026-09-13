# Plan 03 build evidence

- Spec/plan: `plans/ci-matrix-reliability/spec.md` (accepted 2026-09-13) and
  revised READY `03-apple-artifact-builds.md`.
- Base: `0ab1000af0ff12991024e9e6d66466885117a91b`.
- Candidate: recorded by the implementation commit.
- Behavior: Apple builds now build and inspect the GN host-toolchain `protoc`
  executable and `libprotoc_lib.a` before target compilation, reject an
  `__.SYMDEF` member or architecture mismatch, omit that host-only archive
  from the exported target archive, and preserve bridge SDK options as separate
  Bash array elements.
- Passed locally on Linux x86-64: `cargo fmt --check`; locked offline Rust
  doc tests (35); fixture-backed library tests (18); integration tests
  `execution` (5), `network` (3), `peer` (3), `data_channel` (3), and
  `video` (2); `git diff --check`; Justfile parse/show check.
- Not run: Apple builds, smoke links, runtime jobs, and GitHub Actions, per the
  accepted validation boundary. The human owns that workflow evidence.
- Additional non-required Rust-only-consumer attempt did not complete because
  `/tmp` had about 1 GiB free and its isolated artifact extraction failed with
  `ENOSPC`; an earlier attempt reached a host linker bus error. This does not
  affect the required Plan 03 Linux Rust test set.
