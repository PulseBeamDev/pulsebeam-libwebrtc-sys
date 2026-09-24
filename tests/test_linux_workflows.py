from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "linux.yml"
CONSUMER_WORKFLOW = ROOT / ".github" / "workflows" / "linux-consumer.yml"
LEGACY_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
CHECK_WORKFLOW = ROOT / ".github" / "workflows" / "check.yml"
UPGRADE_WORKFLOW = ROOT / ".github" / "workflows" / "upgrade-rehearsal.yml"


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

    def test_validation_checkouts_need_no_historical_commit(self):
        self.assertNotIn("fetch-depth: 0", self._job("validate"))
        self.assertNotIn("fetch-depth: 0", CHECK_WORKFLOW.read_text(encoding="utf-8"))
        self.assertIn("fetch-depth: 0", self._job("linux-release-bundle"))

    def test_validate_primes_the_mounted_cache_before_offline_checks(self):
        validate = self._job("validate")
        prime = "just linux-run pulsebeam-linux-${{ github.sha }} env CARGO_HOME=/workspace/.work/cargo-home cargo fetch --locked"
        checks = "just linux-run pulsebeam-linux-${{ github.sha }} just check"
        self.assertIn(prime, validate)
        self.assertIn(checks, validate)
        self.assertLess(validate.index(prime), validate.index(checks))

    def test_builds_restore_only_the_primary_x86_64_compiler_cache(self):
        build = self._job("linux-build")
        self.assertEqual(build.count("actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830"), 1)
        self.assertIn("path: .work/sccache/${{ matrix.target }}", build)
        self.assertIn("key: linux-sccache-v2-${{ matrix.flavor }}-${{ matrix.target }}-${{ github.sha }}", build)
        self.assertIn("linux-sccache-v2-${{ matrix.flavor }}-${{ matrix.target }}-", build)
        self.assertIn("linux-sccache-v1-${{ matrix.target }}-", build)
        self.assertNotIn("linux-arm64", build)
        self.assertNotIn("ubuntu-24.04-arm", build)

    def test_asan_builds_persist_separate_compiler_caches(self):
        asan = self._job("lifetime-sanitizers")
        self.assertEqual(asan.count("actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830"), 1)
        self.assertIn("path: .work/sccache/linux-x86_64", asan)
        self.assertIn("key: linux-sccache-asan-v1-${{ matrix.flavor }}-linux-x86_64-${{ github.sha }}", asan)
        self.assertIn("restore-keys: linux-sccache-asan-v1-${{ matrix.flavor }}-linux-x86_64-", asan)
        self.assertLess(asan.index("Restore ASan compiler object cache"), asan.index("Build and run ASan tests"))

    def test_native_architecture_matrices_and_all_required_proofs_exist(self):
        build = self._job("linux-build")
        runtime = self._job("linux-runtime")
        asan = self._job("lifetime-sanitizers")
        self.assertEqual(build.count("target: linux-"), 2)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", build)
        self.assertNotIn("linux-arm64", build)
        self.assertEqual(runtime.count("target: linux-"), 2)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", runtime)
        self.assertNotIn("linux-arm64", runtime)
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
        self.assertEqual(consumer.count("target: linux-"), 2)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", consumer)
        self.assertNotIn("linux-arm64", consumer)
        self.assertIn("tools.rust_only_consumer candidate", consumer)
        self.assertIn('checksum_line="$(grep -F "  $(basename "$archive")" audit/SHA256SUMS)"', consumer)
        self.assertIn('digest="${checksum_line%% *}"', consumer)
        self.assertNotIn('"$2 == name { print $1 }"', consumer)
        self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", consumer)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", consumer)

    def test_explicit_tag_publication_requires_same_run_qualification(self):
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
        self.assertNotIn("actions: read", bundle)
        self.assertNotIn("tools.linux_release_publication qualify", bundle)
        self.assertNotIn("actions/workflows/linux.yml/runs", bundle)
        self.assertIn("test \"$RELEASE_TAG\" != v0.5.0", bundle)
        self.assertLess(bundle.index("Validate the requested immutable release tag"), bundle.index("Construct one closed audited"))
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


class LinuxConsumerWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contents = CONSUMER_WORKFLOW.read_text(encoding="utf-8")

    def test_dispatch_only_released_consumer_proof_is_read_only_and_native(self):
        self.assertIn("  workflow_dispatch:\n", self.contents)
        self.assertNotIn("  pull_request:\n", self.contents)
        self.assertNotIn("  push:\n", self.contents)
        self.assertIn("      revision:\n", self.contents)
        self.assertIn("        required: true\n", self.contents)
        self.assertIn("permissions:\n  contents: read\n", self.contents)
        self.assertIn("^[0-9a-f]{40}$", self.contents)

        consumer = self._job("released-consumer")
        self.assertEqual(consumer.count("target: linux-"), 2)
        self.assertIn("target: linux-x86_64\n            runner: ubuntu-24.04", consumer)
        self.assertNotIn("linux-arm64", consumer)
        self.assertIn("ref: ${{ inputs.revision }}", consumer)
        self.assertIn("just-version: 1.43.1", consumer)
        self.assertIn("command -v podman", consumer)
        self.assertIn("podman --version", consumer)
        self.assertIn(".Host.Security.Rootless", consumer)
        self.assertIn("just linux-image pulsebeam-linux-${{ inputs.revision }}", consumer)
        self.assertIn("just linux-run pulsebeam-linux-${{ inputs.revision }} python3 -m tools.rust_only_consumer released", consumer)
        self.assertIn("https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", consumer)
        self.assertIn("--revision '${{ inputs.revision }}'", consumer)

        aggregate = self._job("linux-consumer")
        self.assertIn("needs: released-consumer", aggregate)
        self.assertIn("if: always()", aggregate)
        self.assertIn('"released-consumer=${{ needs.released-consumer.result }}"', aggregate)
        self.assertIn('test "${result#*=}" = success', aggregate)

        forbidden = (
            "apt", "docker", "podman push", "podman save", "podman load", "qemu", "binfmt",
            "--privileged", "podman.sock", "podman --remote", "container_host", "container:",
            "tools/linux_container.py", "upload-artifact", "attest", "gh release",
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


class LinuxWorkflowMigrationTests(unittest.TestCase):
    def test_workflow_names_describe_their_distinct_purposes(self):
        names = (
            (CHECK_WORKFLOW, "Fast checks"),
            (WORKFLOW, "Linux qualification + manual release"),
            (LEGACY_WORKFLOW, "Complete-matrix checks (manual)"),
            (CONSUMER_WORKFLOW, "Released Linux consumer check"),
            (UPGRADE_WORKFLOW, "WebRTC upgrade check"),
        )
        for path, name in names:
            with self.subTest(path=path.name):
                self.assertEqual(path.read_text(encoding="utf-8").splitlines()[0], f"name: {name}")

    def test_complete_matrix_removes_only_the_checkout_only_gate(self):
        contents = LEGACY_WORKFLOW.read_text(encoding="utf-8")
        self.assertNotIn("  validate:", contents)
        self.assertNotIn("needs.validate.result", contents)
        for job in ("linux-build", "non-linux-build", "lifetime-sanitizers"):
            section = self._job(contents, job)
            self.assertIn("actions/checkout@", section)
            self.assertNotIn("needs: validate", section)
        aggregate = self._job(contents, "complete-matrix-qualification")
        self.assertIn("if: always()", aggregate)
        required = (
            "linux-build", "non-linux-build", "linux-desktop-runtime",
            "non-linux-desktop-runtime", "lifetime-sanitizers", "complete-audit",
        )
        self.assertIn(f"needs: [{', '.join(required)}]", aggregate)
        for job in required:
            self.assertIn(f"needs.{job}.result", aggregate)
        self.assertIn('test "$result" = success', aggregate)

    def test_legacy_workflow_is_manually_dispatchable(self):
        contents = LEGACY_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("on:\n  workflow_dispatch:\n", contents)

    def test_legacy_non_linux_build_and_runtime_contracts_are_preserved(self):
        contents = LEGACY_WORKFLOW.read_text(encoding="utf-8")
        build = self._job(contents, "non-linux-build")
        for target, rust_target in (
            ("windows-x86_64", "x86_64-pc-windows-msvc"),
            ("macos-x86_64", "x86_64-apple-darwin"),
            ("macos-arm64", "aarch64-apple-darwin"),
            ("android-x86_64", "x86_64-linux-android"),
            ("android-arm64-v8a", "aarch64-linux-android"),
            ("ios-arm64", "aarch64-apple-ios"),
            ("ios-simulator-arm64", "aarch64-apple-ios-sim"),
        ):
            self.assertIn(f"{target}) cargo_target={rust_target}", build)
        self.assertIn('rustup target add "$cargo_target"', build)
        runtime = self._job(contents, "non-linux-desktop-runtime")
        self.assertIn("needs: non-linux-build", runtime)
        self.assertEqual(runtime.count("target:"), 6)
        self.assertIn("timeout-minutes: 45", runtime)
        self.assertIn("just _runtime-test", runtime)
        aggregate = self._job(contents, "complete-matrix-qualification")
        self.assertIn("non-linux-desktop-runtime", aggregate)
        self.assertIn("needs.non-linux-desktop-runtime.result", aggregate)

    def test_legacy_workflow_has_no_linux_publication_authority_or_shared_concurrency(self):
        contents = LEGACY_WORKFLOW.read_text(encoding="utf-8")
        self.assertNotIn("concurrency:", contents)
        self.assertNotIn("  linux-release-bundle:", contents)
        self.assertNotIn("  publish-linux:", contents)
        self.assertNotIn("contents: write", contents)
        self.assertNotIn("id-token: write", contents)
        self.assertNotIn("attestations: write", contents)

    def test_legacy_linux_jobs_use_direct_native_podman_recipes(self):
        contents = LEGACY_WORKFLOW.read_text(encoding="utf-8")
        for job in ("linux-build", "linux-desktop-runtime", "lifetime-sanitizers"):
            with self.subTest(job=job):
                section = self._job(contents, job)
                self.assertIn("extractions/setup-just@", section)
                self.assertIn("just-version: 1.43.1", section)
                self.assertIn("command -v podman", section)
                self.assertIn("podman --version", section)
                self.assertIn(".Host.Security.Rootless", section)
                self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", section)
                self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", section)
        build = self._job(contents, "linux-build")
        self.assertIn("target: linux-arm64, runner: ubuntu-24.04-arm", build)
        self.assertIn("path: .work/sccache/${{ matrix.target }}", build)
        self.assertIn("Prepare ARM64 checkout for toolchain restore", build)
        self.assertIn("path: .work/checkout/src/third_party/llvm-build/Release+Asserts", build)
        self.assertNotIn("apt", contents.lower())
        self.assertNotIn("linux_release_publication", contents)

    def test_fast_checks_and_upgrade_rehearsal_run_repository_commands_in_image(self):
        check = CHECK_WORKFLOW.read_text(encoding="utf-8")
        upgrade = UPGRADE_WORKFLOW.read_text(encoding="utf-8")
        for contents in (check, upgrade):
            self.assertIn("extractions/setup-just@", contents)
            self.assertIn("just-version: 1.43.1", contents)
            self.assertIn("command -v podman", contents)
            self.assertIn("podman --version", contents)
            self.assertIn(".Host.Security.Rootless", contents)
            self.assertIn("just linux-image pulsebeam-linux-${{ github.sha }}", contents)
            self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }}", contents)
            self.assertIsNone(re.search(r"\bapt(?:-get)?\b", contents.lower()))
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} env CARGO_HOME=/workspace/.work/cargo-home cargo fetch --locked", check)
        self.assertIn("CARGO_HOME=/workspace/.work/cargo-home", check)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just check", check)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just refresh-cxx", upgrade)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} just build core linux-x86_64", upgrade)
        self.assertIn("just linux-run pulsebeam-linux-${{ github.sha }} bash -c", upgrade)
        self.assertIn("CANDIDATE_COMMIT: ${{ inputs.webrtc_commit }}", upgrade)
        select_start = upgrade.index("      - name: Select the candidate source pin")
        select_end = upgrade.index("      - name: Verify the pinned CXX import refresh is mechanical")
        select = upgrade[select_start:select_end]
        self.assertNotIn("inputs.webrtc_commit", select[select.index("run:"):])
        self.assertIn('-- "$CANDIDATE_COMMIT"', select)

    def test_linux_container_docs_and_check_wiring_cover_workflow_proofs(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        justfile = (ROOT / "Justfile").read_text(encoding="utf-8")
        self.assertIn("Linux qualification + manual release", readme)
        self.assertIn("linux-consumer.yml", readme)
        self.assertIn("exact 40-character revision", readme)
        self.assertIn('pulsebeam-libwebrtc-sys = { git = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", rev = "<consumer-revision>" }', readme)
        self.assertIn('features = ["native"]', readme)
        self.assertIn("locally built", readme)
        self.assertIn("tests/test_linux_workflows.py", justfile)

    @staticmethod
    def _job(contents, name):
        start = contents.index(f"  {name}:")
        following = re.search(r"\n  [^ \n][^\n]*:\n", contents[start + 1:])
        end = len(contents) if following is None else start + 1 + following.start()
        return contents[start:end]


if __name__ == "__main__":
    unittest.main()
