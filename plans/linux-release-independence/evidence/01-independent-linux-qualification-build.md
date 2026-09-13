# Build evidence — Plan 01

- Specification: plans/linux-release-independence/spec.md, accepted revision 4.
- Plan: 01-independent-linux-qualification.md, READY at baseline
  5780b4924112ef95657bc24bd203198184992a8d.
- Implementation commit: 4fc622a47e0e833a135390be800db9f2ce93e93f.

## Delivered behavior

- The dispatch tag is optional; a tag is validated only before the existing
  publishing path, which remains the sole job with write, attestation, and
  identity-token permissions.
- Linux build/runtime evidence is split from non-Linux matrices. Linux
  aggregation observes validation, four Linux artifacts, two Linux runtime
  suites, ASan, the cold consumer, and a closed four-asset audit only.
- Complete-matrix aggregation observes both matrix groups and the closed
  eighteen-asset audit.
- The release auditor has explicit linux and complete scopes. Workflow and
  synthetic audit tests cover the graph and closed asset sets.

## Checks

- python3 -m unittest tests/test_release_audit.py tests/test_release_workflow.py — passed (9 tests).
- yq '.jobs | keys' .github/workflows/release.yml — parsed successfully.
- env XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp just check — passed.
- git diff --check — passed before commit.

## Remaining required external evidence

No GitHub Actions dispatch was run from this build environment. The required
controlled non-publishing run evidence (successful Linux with failed,
cancelled, or skipped non-Linux work, plus failed/missing Linux prerequisites)
therefore remains outstanding. It must be recorded before this slice can be
accepted.
