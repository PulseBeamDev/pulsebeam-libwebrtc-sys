from pathlib import Path
import fnmatch
import json
import os
import re
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from tools import ci_tasks, mobile_runtime, platform_ci
from tools.release_inventory import OS_TARGETS


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"


def workflow_job(contents, name):
    match = re.search(rf"(?m)^  {re.escape(name)}:\n", contents)
    if match is None:
        raise ValueError(f"missing workflow job: {name}")
    start = match.start()
    following = re.search(r"\n  [^ \n][^\n]*:\n", contents[start + 1:])
    end = len(contents) if following is None else start + 1 + following.start()
    return contents[start:end]


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contents = WORKFLOW.read_text(encoding="utf-8")

    def job(self, name):
        return workflow_job(self.contents, name)

    def test_one_workflow_checks_prs_and_main_and_publishes_on_tag_pushes(self):
        self.assertEqual(sorted((ROOT / ".github/workflows").glob("*.yml")), [WORKFLOW.with_name("os-release.yml"), WORKFLOW])
        self.assertEqual(self.contents.splitlines()[0], "name: CI and release")
        triggers = self.contents.split("permissions: {}", 1)[0]
        self.assertIn("  pull_request:\n", triggers)
        self.assertIn("  push:\n    branches: [main]\n    tags: ['v*',", triggers)
        for scope in ("linux", "macos", "windows", "ios", "android"):
            self.assertIn(f"'!v*-{scope}'", triggers)
        self.assertIn("  workflow_dispatch:", triggers)
        self.assertIn("permissions: {}", self.contents)
        self.assertNotIn("\nconcurrency:", self.contents)
        self.assertIn("group: publish-linux-${{ github.ref }}", self.job("publish-linux"))
        self.assertIn("cancel-in-progress: false", self.contents)
        qualification = "github.event_name == 'workflow_dispatch' || github.ref_type == 'tag'"
        for name in ("linux-build", "lifetime-sanitizers", "macos-release", "windows-release", "ios-release", "android-release"):
            self.assertIn(f"if: {qualification}\n", self.job(name))
        self.assertIn(f"if: always() && ({qualification})\n", self.job("linux-qualification"))
        # A main push runs source checks, not native builds or publication.
        publication = "github.event_name == 'push' && github.ref_type == 'tag'"
        for name in ("linux-release-bundle", "publish-linux"):
            self.assertIn(f"if: {publication}\n", self.job(name))
        self.assertIn(f"if: always() && {publication}\n", self.job("release-result"))

    def test_validation_primes_mounted_cache_and_checks_before_release(self):
        validate = self.job("validate")
        self.assertIn("actions/checkout@", validate)
        self.assertIn("ref: ${{ github.sha }}", validate)
        self.assertNotIn("fetch-depth: 0", validate)
        self.assertNotIn("fetch-tags:", validate)
        self.assertIn("RELEASE_COMMIT: ${{ github.sha }}", validate)
        identity = 'just ci release-tag "$RELEASE_TAG" "$RELEASE_COMMIT" --repository "$RELEASE_REPOSITORY"'
        self.assertIn("RELEASE_TAG: ${{ github.ref_name }}", validate)
        self.assertIn("RELEASE_REPOSITORY: ${{ github.repository }}", validate)
        identity_step = validate[validate.index("      - name: Validate release identity"):validate.index("      - name: Validate release CLI")]
        self.assertIn("if: github.event_name == 'push' && github.ref_type == 'tag'\n", identity_step)
        tag_fetch = 'git fetch --no-tags --depth=1 origin "refs/tags/$RELEASE_TAG:refs/tags/$RELEASE_TAG"'
        self.assertLess(validate.index(tag_fetch), validate.index(identity))
        for expensive in ("just ci-preflight", "just ci podman", "just linux-image", "cargo fetch", "just check"):
            self.assertLess(validate.index(identity), validate.index(expensive))
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
                if name == "lifetime-sanitizers":
                    self.assertNotIn("linux-arm64", job)
        for name in ("linux-build", "candidate-consumer", "released-consumer"):
            job = self.job(name)
            self.assertEqual(job.count("target: linux-"), 4)
            self.assertEqual(job.count("runner: ubuntu-24.04-arm"), 2)
        # ARM64 runs on the producer host, with its pinned SDK checkout;
        # x86_64 retains the independent artifact-only runtime jobs.
        self.assertIn("if: matrix.target == 'linux-arm64'", self.job("linux-build"))
        self.assertIn("just ci runtime", self.job("linux-build"))
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
        self.assertNotIn("fetch-depth: 0", bundle)
        self.assertNotIn("just ci release-tag", bundle)
        self.assertNotIn(".release-tag", self.contents)
        self.assertIn('release-input/LINUX-RELEASE-MANIFEST.json "$RELEASE_TAG" linux-release', bundle)
        self.assertIn("RELEASE_TAG: ${{ github.ref_name }}-linux", bundle)
        self.assertIn("just ci release-prepare", bundle)
        self.assertIn("needs: [linux-qualification, linux-release-bundle]", publish)
        self.assertIn("contents: write", publish)
        self.assertIn("id-token: write", publish)
        self.assertIn("attestations: write", publish)
        self.assertNotIn("contents: write", bundle)
        self.assertIn('just ci-release ensure-tag --source-tag "$SOURCE_TAG" --scope linux --commit "$SOURCE_COMMIT"', publish)
        self.assertLess(publish.index("just ci-release ensure-tag"), publish.index("just ci-release classify"))
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
        self.assertEqual(consumer.count("target: linux-"), 4)
        self.assertIn("ref: ${{ github.sha }}", consumer)
        self.assertIn("just ci podman", consumer)
        self.assertIn("python3 -m tools.rust_only_consumer released", consumer)
        self.assertIn("--tag '${{ github.ref_name }}'", consumer)
        self.assertIn("https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git", consumer)
        self.assertNotIn("ARTIFACT_DIR", consumer)
        result = self.job("release-result")
        self.assertIn("needs: [publish-linux, released-consumer]", result)
        self.assertIn("if: always() && github.event_name == 'push' && github.ref_type == 'tag'\n", result)
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


class IndependentOSWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = WORKFLOW.read_text()
        cls.reusable = WORKFLOW.with_name("os-release.yml").read_text()

    def job(self, name):
        return workflow_job(self.reusable, name)

    def test_each_os_calls_an_independent_graph_after_only_source_checks(self):
        for scope in ("macos", "windows", "ios", "android"):
            caller = workflow_job(self.root, f"{scope}-release")
            self.assertIn("needs: validate", caller)
            self.assertIn("uses: ./.github/workflows/os-release.yml", caller)
            self.assertIn(f"with: {{scope: {scope}}}", caller)
            self.assertNotIn("linux-qualification", caller)
            for other in OS_TARGETS.keys() - {scope}:
                self.assertNotIn(f"{other}-release", caller)
        self.assertNotIn("\nconcurrency:", self.reusable)
        self.assertIn("group: publish-${{ inputs.scope }}-${{ github.ref }}", self.job("publish"))
        self.assertNotIn("continue-on-error", self.reusable)

    def test_complete_cold_and_published_matrices_and_mobile_baselines(self):
        selections = {(row["flavor"], row["target"]) for scope in OS_TARGETS if scope != "linux"
                      for row in platform_ci.matrix(scope)["include"]}
        self.assertEqual(len(selections), 14)
        self.assertEqual(selections, {(flavor, target) for scope, targets in OS_TARGETS.items() if scope != "linux"
                                      for target in targets for flavor in ("core", "native")})
        for name in ("build", "candidate", "released-consumer"):
            self.assertIn("fromJSON(needs.matrix.outputs.build)", self.job(name))
            self.assertIn("fail-fast: false", self.job(name))
        for scope, target in (("android", "android-x86_64"), ("ios", "ios-simulator-arm64")):
            rows = platform_ci.matrix(scope, runtime=True)["include"]
            self.assertEqual({(row["flavor"], row["target"]) for row in rows}, {("core", target), ("native", target)})
        for scope in ("linux", "complete", "arbitrary"):
            with self.assertRaises(platform_ci.SetupError):
                platform_ci.matrix(scope)

    def test_publication_requires_every_applicable_gate(self):
        gate = self.job("qualification")
        self.assertIn("if: always()", gate)
        self.assertIn("needs: [build, audit, candidate, mobile-runtime]", gate)
        self.assertIn('"build=$BUILD_RESULT" "audit=$AUDIT_RESULT" "candidate=$CANDIDATE_RESULT"', gate)
        self.assertIn('android|ios) just ci require-success "mobile-runtime=$MOBILE_RESULT"', gate)
        self.assertIn("needs: [matrix, qualification]", self.job("bundle"))
        self.assertIn("needs: [qualification, bundle]", self.job("publish"))
        self.assertIn("produce '${{ matrix.flavor }}' '${{ matrix.target }}'", self.job("build"))
        self.assertIn("just ci mobile-runtime", self.job("mobile-runtime"))
        self.assertIn("if-no-files-found: error", self.job("mobile-runtime"))

    def test_manual_qualification_cannot_mutate_tags_or_releases(self):
        for name in ("linux-release-bundle", "publish-linux"):
            self.assertIn("if: github.event_name == 'push' && github.ref_type == 'tag'\n", workflow_job(self.root, name))
        for name in ("bundle", "publish"):
            self.assertIn("if: github.event_name == 'push'", self.job(name))
        publish = self.job("publish")
        self.assertIn("contents: write", publish)
        for name in ("matrix", "build", "audit", "candidate", "mobile-runtime", "qualification", "bundle", "released-consumer"):
            self.assertIn("contents: read", self.job(name))
            self.assertNotIn("contents: write", self.job(name))
        self.assertIn("needs.publish.result == 'success'", self.job("released-consumer"))
        self.assertIn("--tag \"$SOURCE_TAG\"", self.job("released-consumer"))
        self.assertNotIn("ARTIFACT_DIR", self.job("released-consumer"))
        for name in ("candidate", "released-consumer"):
            self.assertNotIn("produce", self.job(name))
            self.assertNotIn("just build", self.job(name))

    def test_scoped_downloads_exclude_other_oses_and_runtime_receipts(self):
        for scope, targets in OS_TARGETS.items():
            if scope == "linux":
                continue
            pattern = f"*-{scope}-*"
            for target in targets:
                self.assertTrue(fnmatch.fnmatch(f"core-{target}", pattern))
            self.assertFalse(fnmatch.fnmatch(f"{scope}-runtime-core-0", pattern))
            for other, other_targets in OS_TARGETS.items():
                if other != scope:
                    for target in other_targets:
                        self.assertFalse(fnmatch.fnmatch(f"core-{target}", pattern))
        for name in ("audit", "bundle"):
            self.assertIn("pattern: '*-${{ inputs.scope }}-*'", self.job(name))
        publish = self.job("publish")
        self.assertLess(publish.index("ci-release ensure-tag"), publish.index("ci-release classify"))
        self.assertLess(publish.index("Attest final release assets"), publish.index("--output attested-plan.json"))
        self.assertLess(publish.index("--output attested-plan.json"), publish.index("ci-release advertise"))


class PlatformCITests(unittest.TestCase):
    def test_setup_cannot_proceed_without_explicit_ci_environment(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(platform_ci.rust_only_consumer, "require_host"), patch.object(platform_ci, "invoke") as invoke:
            with self.assertRaises(platform_ci.SetupError):
                platform_ci.setup("macos-arm64", "consumer")
            invoke.assert_not_called()

    def test_desktop_runtime_failure_prevents_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "dist").mkdir()
            (root / "dist/webrtc-core-macos-arm64.tar.gz").write_bytes(b"selected archive")
            with patch.object(platform_ci, "ROOT", root), patch.object(platform_ci.rust_only_consumer, "require_host"), patch.object(ci_tasks, "install_target"), patch.object(platform_ci, "invoke"), patch.object(ci_tasks, "runtime", side_effect=ci_tasks.TaskError("runtime failed")):
                with self.assertRaises(ci_tasks.TaskError):
                    platform_ci.produce("core", "macos-arm64")
            self.assertFalse((root / ".work/qualification/core-macos-arm64.json").exists())

    def test_android_sdk_installer_never_accepts_licenses_and_requires_actual_driver(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.dict(os.environ, {"ANDROID_SDK_ROOT": str(root)}), patch.object(platform_ci, "invoke") as invoke:
                with self.assertRaises(platform_ci.SetupError):
                    platform_ci.android_sdk(runtime=True)
                command = invoke.call_args.args[0]
                self.assertIn(f"ndk;{mobile_runtime.ANDROID_NDK}", command)
                self.assertIn(mobile_runtime.ANDROID_IMAGE, command)
                self.assertNotIn("--licenses", command)
                self.assertEqual(invoke.call_args.kwargs["input"], "")

    def test_ios_exact_version_download_is_imported_and_revalidated(self):
        available = {"version": "18.0", "identifier": "com.apple.CoreSimulator.SimRuntime.iOS-18-0", "isAvailable": True}
        for final in (available, {**available, "version": "18.1"}):
            calls = []
            def invoke(command, **kwargs):
                calls.append(command)
                if command[:4] == ["xcrun", "simctl", "list", "runtimes"]:
                    return SimpleNamespace(stdout=json.dumps({"runtimes": [] if len(calls) == 1 else [final]}))
                if "-downloadPlatform" in command:
                    (Path(command[-1]) / "runtime.dmg").write_bytes(b"fixture runtime")
                return SimpleNamespace(stdout="")
            with self.subTest(version=final["version"]), patch.object(platform_ci, "invoke", side_effect=invoke):
                if final["version"] == "18.0":
                    platform_ci.ensure_ios_runtime()
                else:
                    with self.assertRaises(mobile_runtime.RuntimeError):
                        platform_ci.ensure_ios_runtime()
            download = next(command for command in calls if "-downloadPlatform" in command)
            self.assertEqual(download[download.index("-buildVersion") + 1], "18.0")
            self.assertEqual(download[download.index("-architectureVariant") + 1], "arm64")
            self.assertTrue(any("-importPlatform" in command for command in calls))
            self.assertFalse(any("-license" in command for command in calls))

    def test_windows_uses_supported_vs_pinned_sdk_and_exports_only_sdk_values(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            python = root / "python.exe"
            python.write_bytes(b"fixture interpreter")
            installation = {"installationVersion": "17.14.0", "installationPath": str(root / "Visual Studio")}
            configured = "PATH=C:/sdk\nINCLUDE=C:/include\nLIB=C:/lib\nWindowsSDKVersion=10.0.26100.0\\\nMY_SECRET=not-exported\n"
            with patch.dict(os.environ, {"ProgramFiles(x86)": str(root)}), patch.object(platform_ci.sys, "executable", str(python)), patch.object(platform_ci, "invoke", return_value=SimpleNamespace(stdout=json.dumps([installation]))), patch.object(platform_ci.subprocess, "run", return_value=SimpleNamespace(stdout=configured)) as run:
                environment = platform_ci.windows_environment()
            self.assertEqual(environment["GYP_MSVS_VERSION"], "2022")
            self.assertEqual(environment["DEPOT_TOOLS_WIN_TOOLCHAIN"], "0")
            self.assertNotIn("MY_SECRET", environment)
            self.assertEqual((root / "python3.exe").read_bytes(), b"fixture interpreter")
            self.assertTrue(run.call_args.args[0].startswith('cmd.exe /d /s /c ""'))
            self.assertIn("-winsdk=10.0.26100.0", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
