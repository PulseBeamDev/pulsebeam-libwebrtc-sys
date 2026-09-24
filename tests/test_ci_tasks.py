import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tests.test_release_audit import ReleaseAuditTests
from tools import ci_tasks


class CiTasksTests(unittest.TestCase):
    def test_revision_and_qualification_fail_closed(self):
        ci_tasks.revision("a" * 40)
        for invalid in ("A" * 40, "a" * 39, "a" * 41, "a" * 39 + "g"):
            with self.subTest(invalid=invalid), self.assertRaises(ci_tasks.TaskError):
                ci_tasks.revision(invalid)
        ci_tasks.require_success(["build=success", "runtime=success"])
        for invalid in ([], ["build=skipped"], ["build=cancelled"], ["build"]):
            with self.subTest(invalid=invalid), self.assertRaises(ci_tasks.TaskError):
                ci_tasks.require_success(invalid)

    def test_tag_is_same_commit_and_never_creates_marker_on_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            marker = Path(temp) / "tag"
            with patch.object(ci_tasks.subprocess, "run", return_value=type("Result", (), {"stdout": "b" * 40})()):
                with self.assertRaisesRegex(ci_tasks.TaskError, "does not resolve"):
                    ci_tasks.release_tag("v-test", "a" * 40, marker)
            self.assertFalse(marker.exists())
            with patch.object(ci_tasks.subprocess, "run", return_value=type("Result", (), {"stdout": "a" * 40})()):
                ci_tasks.release_tag("v-test", "a" * 40, marker)
            self.assertEqual(marker.read_text(), "v-test")
            with self.assertRaisesRegex(ci_tasks.TaskError, "prohibited"):
                ci_tasks.release_tag("v0.5.0", "a" * 40, marker)

    def test_closed_linux_audit_runs_locally(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            ReleaseAuditTests().build_release(root, {"linux-x86_64"})
            (root / "SHA256SUMS").unlink()
            ci_tasks.audit("linux", root, root / "SHA256SUMS", root / "manifest.json")
            self.assertEqual(json.loads((root / "manifest.json").read_text())["scope"], "linux")
            (root / "webrtc-core-linux-arm64.tar.gz").write_bytes(b"unexpected")
            with self.assertRaisesRegex(ci_tasks.TaskError, "two primary"):
                ci_tasks.audit("linux", root, root / "SHA256SUMS", root / "manifest.json")

    def test_candidate_uses_named_digest_not_ambiguous_grep(self):
        with tempfile.TemporaryDirectory() as temp:
            checksums = Path(temp) / "SHA256SUMS"
            checksums.write_text("a" * 64 + "  webrtc-core-linux-x86_64.tar.gz\n")
            with patch.object(ci_tasks.subprocess, "run") as run:
                ci_tasks.candidate("core", "linux-x86_64", checksums)
            self.assertIn("--sha256", run.call_args.args[0])
            self.assertEqual(run.call_args.args[0][-1], "core")
            with self.assertRaisesRegex(ci_tasks.TaskError, "missing checksum"):
                ci_tasks.candidate("native", "linux-x86_64", checksums)

    def test_rust_target_is_from_one_canonical_mapping(self):
        with patch.object(ci_tasks.subprocess, "run") as run:
            ci_tasks.install_target("ios-simulator-arm64")
        self.assertEqual(run.call_args.args[0], ["rustup", "target", "add", "aarch64-apple-ios-sim"])
        with self.assertRaisesRegex(ci_tasks.TaskError, "unsupported"):
            ci_tasks.install_target("nonsense")


if __name__ == "__main__":
    unittest.main()
