from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "linux.yml"


class LinuxWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contents = WORKFLOW.read_text(encoding="utf-8")

    def test_automatic_qualification_triggers_are_linux_only(self):
        self.assertIn("  pull_request:\n", self.contents)
        self.assertIn("  push:\n    branches: [main]\n", self.contents)
        self.assertIn("  workflow_dispatch:\n", self.contents)
        self.assertIn("      tag:\n", self.contents)
        self.assertIn("        required: false\n", self.contents)
        self.assertIn("permissions: {}\n", self.contents)
        self.assertIn("group: linux-qualification-", self.contents)
        self.assertIn("cancel-in-progress: false", self.contents)

    def test_qualification_jobs_use_checked_out_native_podman_images(self):
        for job in ("validate", "linux-build", "linux-runtime", "lifetime-sanitizers"):
            with self.subTest(job=job):
                section = self._job(job)
                self.assertIn("actions/checkout@", section)
                self.assertIn("ref: ${{ github.sha }}", section)
                self.assertIn("extractions/setup-just@", section)
                self.assertIn("just-version: 1.43.1", section)
                self.assertIn("command -v podman", section)
                self.assertIn("podman --version", section)
                self.assertIn(".Host.Security.Rootless", section)
                self.assertIn('= true', section)
                self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", section)
                self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", section)

    def test_native_architecture_matrices_and_all_required_proofs_exist(self):
        build = self._job("linux-build")
        runtime = self._job("linux-runtime")
        asan = self._job("lifetime-sanitizers")
        self.assertEqual(build.count("target: linux-"), 4)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", build)
        self.assertIn("target: linux-arm64\n            runner: ubuntu-24.04-arm", build)
        self.assertEqual(runtime.count("target: linux-"), 4)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", runtime)
        self.assertIn("target: linux-arm64\n            runner: ubuntu-24.04-arm", runtime)
        self.assertIn("flavor: [core, native]", asan)
        self.assertIn("PULSEBEAM_WEBRTC_SANITIZER: address", asan)
        self.assertIn("ASAN_OPTIONS: detect_leaks=1", asan)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just check", self._job("validate"))
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just build", build)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just _runtime-test", runtime)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just build", asan)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just _runtime-test", asan)

    def test_aggregate_is_fail_closed_and_no_release_or_runtime_escape_hatch_exists(self):
        aggregate = self._job("linux-qualification")
        self.assertIn("needs: [validate, linux-build, linux-runtime, lifetime-sanitizers]", aggregate)
        self.assertIn("if: always()", aggregate)
        for job in ("validate", "linux-build", "linux-runtime", "lifetime-sanitizers"):
            self.assertIn(f'"{job}=${{{{ needs.{job}.result }}}}"', aggregate)
        self.assertIn('test "${result#*=}" = success', aggregate)
        forbidden = (
            "apt", "docker", "podman push", "podman save", "podman load", "qemu", "binfmt",
            "--privileged", "podman.sock", "--remote", "CONTAINER_HOST", "container:",
            "tools/linux_container.py", "linux-release-bundle", "publish-linux", "attest",
            "gh release", "id-token: write", "contents: write",
        )
        lowered = self.contents.lower()
        for token in forbidden:
            with self.subTest(token=token):
                self.assertNotIn(token, lowered)

    def _job(self, name):
        start = self.contents.index(f"  {name}:")
        following = re.search(r"\n  [^ \n][^\n]*:\n", self.contents[start + 1:])
        end = len(self.contents) if following is None else start + 1 + following.start()
        return self.contents[start:end]


if __name__ == "__main__":
    unittest.main()
