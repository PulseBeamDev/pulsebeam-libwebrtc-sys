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
        self.assertIn("nonempty tag requests qualified Linux publication", self.contents)
        self.assertIn("        required: false\n", self.contents)
        self.assertIn("permissions: {}\n", self.contents)
        self.assertIn("group: linux-qualification-", self.contents)
        concurrency = self.contents[self.contents.index("concurrency:"):self.contents.index("jobs:")]
        self.assertIn("group: linux-qualification-", concurrency)
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.tag", concurrency)
        self.assertIn("cancel-in-progress: false", concurrency)

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

    def test_aggregate_is_fail_closed_and_no_runtime_escape_hatch_exists(self):
        aggregate = self._job("linux-qualification")
        self.assertIn("needs: [validate, linux-build, linux-runtime, lifetime-sanitizers, linux-audit, candidate-consumer]", aggregate)
        self.assertIn("if: always()", aggregate)
        for job in ("validate", "linux-build", "linux-runtime", "lifetime-sanitizers", "linux-audit", "candidate-consumer"):
            self.assertIn(f'"{job}=${{{{ needs.{job}.result }}}}"', aggregate)
        self.assertIn('test "${result#*=}" = success', aggregate)
        forbidden = (
            "apt", "docker", "podman push", "podman save", "podman load", "qemu", "binfmt",
            "--privileged", "podman.sock", "podman --remote", "CONTAINER_HOST", "container:",
            "tools/linux_container.py",
        )
        lowered = self.contents.lower()
        for token in forbidden:
            with self.subTest(token=token):
                self.assertNotIn(token, lowered)

    def test_closed_audit_and_cold_candidate_consumers_use_matching_native_images(self):
        audit = self._job("linux-audit")
        consumer = self._job("candidate-consumer")
        self.assertIn("needs: linux-build", audit)
        self.assertIn("name: linux-audit", audit)
        self.assertIn("SHA256SUMS", audit)
        self.assertIn("LINUX-RELEASE-MANIFEST.json", audit)
        self.assertIn("tools.audit_release", audit)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", audit)
        self.assertIn("needs: [linux-build, linux-audit]", consumer)
        self.assertEqual(consumer.count("target: linux-"), 4)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", consumer)
        self.assertIn("target: linux-arm64\n            runner: ubuntu-24.04-arm", consumer)
        self.assertIn("tools.rust_only_consumer candidate", consumer)
        self.assertIn("SHA256SUMS", consumer)
        self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", consumer)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", consumer)

    def test_explicit_tag_publication_is_serialized_and_mutation_is_isolated(self):
        bundle = self._job("linux-release-bundle")
        publish = self._job("publish-linux")
        condition = "github.event_name == 'workflow_dispatch' && inputs.tag != ''"
        self.assertIn(condition, bundle)
        self.assertIn(condition, publish)
        self.assertIn("needs: linux-qualification", bundle)
        self.assertIn("fetch-depth: 0", bundle)
        self.assertIn('git rev-parse --verify "refs/tags/${RELEASE_TAG}^{commit}"', bundle)
        self.assertIn('test "$tagged" = "$WORKFLOW_COMMIT"', bundle)
        self.assertIn("needs: [linux-qualification, linux-release-bundle]", publish)
        self.assertIn("cancel-in-progress: false", self.contents)
        self.assertIn("contents: write", publish)
        self.assertIn("id-token: write", publish)
        self.assertIn("attestations: write", publish)
        self.assertNotIn("contents: write", bundle)
        self.assertIn("tools.linux_release_publication prepare", bundle)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", bundle)
        self.assertIn("tools.linux_release_publication plan", publish)
        self.assertIn("actions/attest-build-provenance@", publish)
        self.assertLess(publish.index("Attest final release assets"), publish.index("Reverify the complete attested draft"))
        self.assertLess(publish.index("Reverify the complete attested draft"), publish.index("Advertise only the verified complete Linux release"))

    def _job(self, name):
        start = self.contents.index(f"  {name}:")
        following = re.search(r"\n  [^ \n][^\n]*:\n", self.contents[start + 1:])
        end = len(self.contents) if following is None else start + 1 + following.start()
        return self.contents[start:end]


if __name__ == "__main__":
    unittest.main()
