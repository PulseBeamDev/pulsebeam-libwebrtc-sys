from __future__ import annotations

from pathlib import Path
import re
import unittest


WORKFLOW = (
    Path(__file__).resolve().parents[1] / ".github" / "workflows" / "release.yml"
).read_text(encoding="utf-8")


def job(name: str) -> str:
    match = re.search(
        rf"^  {re.escape(name)}:\n(.*?)(?=^  [a-z][a-z-]+:\n|\Z)",
        WORKFLOW,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing workflow job: {name}")
    return match.group(1)


def matrix(body: str) -> str:
    match = re.search(r"    matrix:\n(.*?)\n    runs-on:", body, re.DOTALL)
    if match is None:
        raise AssertionError("job has no matrix")
    return match.group(1)


def pairs(body: str) -> set[tuple[str, str]]:
    return set(
        re.findall(
            r"flavor:\s*(core|native).*?target:\s*([a-z0-9_-]+)",
            body,
            re.DOTALL,
        )
    )


LINUX_ARTIFACTS = {
    (flavor, target)
    for flavor in ("core", "native")
    for target in ("linux-x86_64", "linux-arm64")
}
NON_LINUX_ARTIFACTS = {
    (flavor, target)
    for flavor in ("core", "native")
    for target in (
        "windows-x86_64",
        "macos-x86_64",
        "macos-arm64",
        "android-x86_64",
        "android-arm64-v8a",
        "ios-arm64",
        "ios-simulator-arm64",
    )
}


class ReleaseWorkflowTests(unittest.TestCase):
    def test_artifact_and_runtime_matrices_are_separate_exact_groups(self):
        self.assertEqual(pairs(matrix(job("linux-build"))), LINUX_ARTIFACTS)
        self.assertEqual(pairs(matrix(job("non-linux-build"))), NON_LINUX_ARTIFACTS)
        self.assertEqual(
            pairs(matrix(job("linux-desktop-runtime"))),
            {("core", "linux-x86_64"), ("native", "linux-x86_64")},
        )
        self.assertEqual(
            pairs(matrix(job("non-linux-desktop-runtime"))),
            {
                ("core", "windows-x86_64"),
                ("native", "windows-x86_64"),
                ("core", "macos-x86_64"),
                ("native", "macos-x86_64"),
                ("core", "macos-arm64"),
                ("native", "macos-arm64"),
            },
        )

    def test_linux_closure_excludes_non_linux_work(self):
        linux = job("linux-qualification")
        self.assertIn("if: always()", linux)
        self.assertNotIn("non-linux", linux)
        self.assertIn("needs: linux-build", job("linux-desktop-runtime"))
        self.assertIn("needs: linux-build", job("rust-only-consumer"))
        self.assertIn("needs: linux-build", job("linux-audit"))
        self.assertIn("--scope linux", job("linux-audit"))

    def test_complete_closure_requires_every_matrix_group(self):
        complete = job("complete-matrix-qualification")
        self.assertIn("if: always()", complete)
        for required in (
            "linux-build",
            "non-linux-build",
            "linux-desktop-runtime",
            "non-linux-desktop-runtime",
            "lifetime-sanitizers",
            "rust-only-consumer",
            "complete-audit",
        ):
            self.assertIn(required, complete)
        self.assertIn("--scope complete", job("complete-audit"))

    def test_qualification_dispatch_has_no_write_path(self):
        self.assertIn("required: false", WORKFLOW)
        self.assertIn('test -z "$RELEASE_TAG" && exit 0', job("validate"))
        publish = job("publish")
        self.assertIn("if: inputs.tag != ''", publish)
        self.assertIn("contents: write", publish)
        self.assertIn("attestations: write", publish)


if __name__ == "__main__":
    unittest.main()
