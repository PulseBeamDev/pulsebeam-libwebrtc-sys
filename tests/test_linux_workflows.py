from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contents = WORKFLOW.read_text(encoding="utf-8")

    def job(self, name):
        start = self.contents.index(f"  {name}:")
        following = re.search(r"\n  [^ \n][^\n]*:\n", self.contents[start + 1:])
        end = len(self.contents) if following is None else start + 1 + following.start()
        return self.contents[start:end]

    def test_one_workflow_checks_prs_and_publishes_on_tag_pushes(self):
        self.assertEqual(sorted((ROOT / ".github/workflows").glob("*.yml")), [WORKFLOW])
        self.assertEqual(self.contents.splitlines()[0], "name: CI and release")
        triggers = self.contents.split("permissions: {}", 1)[0]
        self.assertIn("  pull_request:\n", triggers)
        self.assertIn("  push:\n    tags: ['v*']", triggers)
        self.assertNotIn("workflow_dispatch", triggers)
        self.assertNotIn("branches: [main]", triggers)
        self.assertIn("permissions: {}", self.contents)
        self.assertIn("group: linux-qualification-${{ github.ref }}", self.contents)
        self.assertIn("cancel-in-progress: false", self.contents)
        self.assertIn("if: github.event_name == 'push'", self.job("linux-build"))
        self.assertIn("if: github.event_name == 'push'", self.job("lifetime-sanitizers"))
        self.assertIn("if: always() && github.event_name == 'push'", self.job("linux-qualification"))

    def test_validation_primes_mounted_cache_and_checks_before_release(self):
        validate = self.job("validate")
        self.assertIn("actions/checkout@", validate)
        self.assertIn("ref: ${{ github.sha }}", validate)
        self.assertNotIn("fetch-depth: 0", validate)
        self.assertIn("run: just ci-preflight", validate)
        self.assertIn("run: just ci podman", validate)
        self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", validate)
        prime = "just linux-run pulsebeam-linux-${{ github.sha }} env CARGO_HOME=/workspace/.work/cargo-home cargo fetch --locked"
        checks = "just linux-run pulsebeam-linux-${{ github.sha }} just check"
        self.assertLess(validate.index(prime), validate.index(checks))

    def test_linux_qualification_is_closed_and_uses_native_images(self):
        for name in ("linux-build", "linux-runtime", "lifetime-sanitizers", "candidate-consumer"):
            with self.subTest(job=name):
                job = self.job(name)
                self.assertIn("actions/checkout@", job)
                self.assertIn("just ci podman", job)
                self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", job)
                if name == "lifetime-sanitizers":
                    self.assertIn("just ci asan pulsebeam-linux-${{ github.sha }}", job)
                else:
                    self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", job)
                self.assertNotIn("linux-arm64", job)
        self.assertEqual(self.job("linux-build").count("target: linux-"), 2)
        self.assertEqual(self.job("linux-runtime").count("target: linux-"), 2)
        self.assertIn("flavor: [core, native]", self.job("lifetime-sanitizers"))
        self.assertIn("PULSEBEAM_WEBRTC_SANITIZER: address", self.job("lifetime-sanitizers"))
        audit = self.job("linux-audit")
        self.assertIn("needs: linux-build", audit)
        self.assertIn("just ci audit linux release-input", audit)
        self.assertIn("LINUX-RELEASE-MANIFEST.json", audit)
        candidate = self.job("candidate-consumer")
        self.assertIn("needs: [linux-build, linux-audit]", candidate)
        self.assertIn("just ci candidate", candidate)
        aggregate = self.job("linux-qualification")
        for name in ("validate", "linux-build", "linux-runtime", "lifetime-sanitizers", "linux-audit", "candidate-consumer"):
            self.assertIn(f"'{name}=${{{{ needs.{name}.result }}}}'", aggregate)
        self.assertIn("just ci require-success", aggregate)
        self.assertNotIn("--privileged", self.contents)
        self.assertNotIn("podman.sock", self.contents)

    def test_tag_push_publishes_exactly_the_audited_bundle(self):
        bundle = self.job("linux-release-bundle")
        publish = self.job("publish-linux")
        self.assertIn("needs: linux-qualification", bundle)
        self.assertIn("fetch-depth: 0", bundle)
        self.assertIn('just ci release-tag "$RELEASE_TAG" "$(git rev-parse HEAD)"', bundle)
        self.assertIn("RELEASE_TAG: ${{ github.ref_name }}", bundle)
        self.assertIn("just ci release-prepare", bundle)
        self.assertIn("needs: [linux-qualification, linux-release-bundle]", publish)
        self.assertIn("contents: write", publish)
        self.assertIn("id-token: write", publish)
        self.assertIn("attestations: write", publish)
        self.assertNotIn("contents: write", bundle)
        self.assertIn("just ci-release classify", publish)
        self.assertIn("just ci-release upload", publish)
        self.assertIn("actions/attest-build-provenance@", publish)
        self.assertLess(publish.index("Attest final release assets"), publish.index("Reverify the complete attested draft"))
        self.assertLess(publish.index("Reverify the complete attested draft"), publish.index("Advertise only the verified complete Linux release"))

    def test_publication_proves_tag_as_real_git_consumer(self):
        publish = self.job("publish-linux")
        self.assertNotIn("publish_consumer_revision", publish)
        self.assertIn("Advertise only the verified complete Linux release", publish)
        consumer = self.job("released-consumer")
        self.assertIn("needs: publish-linux", consumer)
        self.assertIn("flavor: [core, native]", consumer)
        self.assertIn("ref: ${{ github.sha }}", consumer)
        self.assertIn("just ci podman", consumer)
        self.assertIn("python3 -m tools.rust_only_consumer released", consumer)
        self.assertIn("--tag '${{ github.ref_name }}'", consumer)
        self.assertIn("https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", consumer)
        self.assertNotIn("ARTIFACT_DIR", consumer)
        result = self.job("release-result")
        self.assertIn("needs: [publish-linux, released-consumer]", result)
        self.assertIn("if: always() && github.event_name == 'push'", result)
        self.assertIn("'released-consumer=${{ needs.released-consumer.result }}'", result)
        self.assertIn("GITHUB_STEP_SUMMARY", result)
        self.assertIn("Git tag $RELEASE_TAG", result)

    def test_ci_tasks_jobs_checkout_and_install_just(self):
        for match in re.finditer(r"(?m)^  ([a-z][a-z-]*):\n", self.contents):
            job = self.job(match.group(1))
            if "just ci " in job or "just ci-release " in job:
                with self.subTest(job=match.group(1)):
                    self.assertIn("actions/checkout@", job)
                    self.assertIn("extractions/setup-just@", job)

    def test_documentation_explains_consumer_tag(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        justfile = (ROOT / "Justfile").read_text(encoding="utf-8")
        self.assertIn("The single `release.yml` workflow", readme)
        self.assertNotIn("consumers/<tag>", readme)
        self.assertIn('pulsebeam-webrtc-sys = { git = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", tag = "<tag>" }', readme)
        self.assertIn('features = ["native"]', readme)
        self.assertIn("locally built", readme)
        self.assertIn("tests/test_linux_workflows.py", justfile)


if __name__ == "__main__":
    unittest.main()
