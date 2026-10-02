from contextlib import chdir
import json
import os
import subprocess
import sys
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

    def test_release_identity_rejects_static_errors_before_git(self):
        with tempfile.TemporaryDirectory() as temp, chdir(temp):
            Path("Cargo.toml").write_text('[package]\nversion = "0.5.6"\n')
            cases = [
                ("v0.6.0", "a" * 40, ci_tasks.linux_release_publication.CANONICAL_REPOSITORY, "must match Cargo package version v0.5.6"),
                ("v0.5.0", "a" * 40, ci_tasks.linux_release_publication.CANONICAL_REPOSITORY, "prohibited"),
                ("v0.5.6", "bad", ci_tasks.linux_release_publication.CANONICAL_REPOSITORY, "revision"),
                ("v0.5.6", "a" * 40, "fork/repo", "publication repository must be"),
                ("v0.5.6/invalid", "a" * 40, ci_tasks.linux_release_publication.CANONICAL_REPOSITORY, "prohibited"),
            ]
            for tag, commit, repository, diagnostic in cases:
                with self.subTest(tag=tag, repository=repository), patch.object(ci_tasks.subprocess, "run") as run:
                    with self.assertRaisesRegex(ci_tasks.TaskError, diagnostic):
                        ci_tasks.release_tag(tag, commit, repository)
                    run.assert_not_called()

    def test_release_identity_cli_checks_shallow_lightweight_and_annotated_tags(self):
        root = Path(__file__).resolve().parents[1]
        environment = {**os.environ, "PYTHONPATH": str(root)}
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            def git(*args, cwd=directory):
                return subprocess.run(["git", "-c", "user.name=CI Test", "-c", "user.email=ci@example.invalid", *args], cwd=cwd, check=True, text=True, capture_output=True).stdout.strip()

            git("init", "-q")
            (directory / "Cargo.toml").write_text('[package]\nversion = "1.2.3"\n')
            git("add", "Cargo.toml")
            git("commit", "-qm", "fixture")
            commit = git("rev-parse", "HEAD")
            for annotated in (False, True):
                with self.subTest(annotated=annotated):
                    if annotated:
                        git("tag", "-a", "v1.2.3", "-m", "fixture")
                    else:
                        git("tag", "v1.2.3")
                    checkout = directory / f"checkout-{annotated}"
                    git("init", "-q", str(checkout))
                    git("remote", "add", "origin", directory.as_uri(), cwd=checkout)
                    # Keep source pinned to the event SHA, then explicitly fetch
                    # the tag. SHA-only shallow fetches can omit the tag ref.
                    git("fetch", "--no-tags", "--depth=1", "origin", commit, cwd=checkout)
                    git("checkout", "--detach", "FETCH_HEAD", cwd=checkout)
                    git("fetch", "--no-tags", "--depth=1", "origin", "refs/tags/v1.2.3:refs/tags/v1.2.3", cwd=checkout)
                    self.assertEqual(git("rev-parse", "--is-shallow-repository", cwd=checkout), "true")
                    result = subprocess.run([sys.executable, "-m", "tools.ci_tasks", "release-tag", "v1.2.3", commit], cwd=checkout, env=environment, text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(f"v1.2.3 at {commit}", result.stdout)
                    with chdir(checkout), self.assertRaisesRegex(ci_tasks.TaskError, "does not resolve"):
                        ci_tasks.release_tag("v1.2.3", "b" * 40)
                    git("tag", "-d", "v1.2.3")
            result = subprocess.run([sys.executable, "-m", "tools.ci_tasks", "release-tag", "v1.2.3", commit], cwd=directory, env=environment, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("CI task:", result.stderr)

    def test_release_prepare_cli_passes_tag_without_marker_file(self):
        with patch.object(sys, "argv", ["ci_tasks", "release-prepare", "archives", "sums", "audit", "v1.2.3", "bundle"]):
            with patch.object(ci_tasks.linux_release_publication, "prepare") as prepare:
                self.assertEqual(ci_tasks.main(), 0)
            prepare.assert_called_once_with(Path("archives"), Path("sums"), Path("audit"), "v1.2.3", Path("bundle"))

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
