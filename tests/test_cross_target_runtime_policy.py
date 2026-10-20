import importlib.util
import io
import tarfile
import json
import os
import subprocess
import tempfile
from unittest.mock import patch

from tools import mobile_runtime as mobile
from tools import rust_only_consumer as consumer
from tools.release_inventory import OS_TARGETS, archive_names
from pathlib import Path
import re
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = (ROOT / "Justfile").read_text(encoding="utf-8")
sys.path.insert(0, str(ROOT / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "write_artifact_manifest", ROOT / "tools/write_artifact_manifest.py"
)
write_artifact_manifest = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(write_artifact_manifest)


class CrossTargetRuntimePolicyTests(unittest.TestCase):
    flavors = ("core", "native")
    affected = ("android-x86_64", "android-arm64-v8a", "linux-arm64")
    linux_native = (
        "-lX11 -lgio-2.0 -lglib-2.0 -lgobject-2.0 -lXcomposite -lXdamage "
        "-lXext -lXfixes -lXrandr -lXrender -lXtst -lgbm -ldrm"
    )
    expected_cxx = {
        "linux-x86_64": "-fuse-ld=lld -nostdlib++ -pthread -ldl -lrt -lm",
        "linux-arm64": "-fuse-ld=lld --rtlib=compiler-rt --unwindlib=none -nostdlib++ -pthread -ldl -lrt -lm",
        "windows-x86_64": "advapi32.lib bcrypt.lib crypt32.lib d3d11.lib dmoguids.lib dwmapi.lib dxgi.lib iphlpapi.lib msdmo.lib ole32.lib oleaut32.lib secur32.lib shcore.lib strmiids.lib user32.lib winmm.lib wmcodecdspuuid.lib ws2_32.lib",
        "macos-x86_64": "-framework Foundation -framework AppKit -framework ApplicationServices -framework CoreAudio -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework IOKit -framework IOSurface -framework OpenGL -framework VideoToolbox -weak_framework ScreenCaptureKit",
        "macos-arm64": "-framework Foundation -framework AppKit -framework ApplicationServices -framework CoreAudio -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework IOKit -framework IOSurface -framework OpenGL -framework VideoToolbox -weak_framework ScreenCaptureKit",
        "android-x86_64": "-fuse-ld=lld -nostdlib++ --unwindlib=none -llog -landroid -lGLESv2 -lOpenSLES -ldl -lm",
        "android-arm64-v8a": "-fuse-ld=lld -nostdlib++ --unwindlib=none -llog -landroid -lGLESv2 -lOpenSLES -ldl -lm",
        "ios-arm64": "-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework VideoToolbox -framework UIKit",
        "ios-simulator-arm64": "-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework VideoToolbox -framework UIKit",
    }
    expected_rust = {
        "linux-arm64": "rustflags=(-C link-arg=--target=aarch64-linux-gnu -C \"link-arg=--sysroot=$src/build/linux/debian_bullseye_arm64-sysroot\" -C link-arg=-fuse-ld=lld -C link-arg=--rtlib=compiler-rt -C link-arg=--unwindlib=none)",
        "android-x86_64": "rustflags=(-C \"link-arg=--target=${triple}-linux-android26\" -C \"link-arg=--sysroot=$prebuilt/sysroot\" -C link-arg=-fuse-ld=lld -C link-arg=--unwindlib=none)",
        "android-arm64-v8a": "rustflags=(-C \"link-arg=--target=${triple}-linux-android26\" -C \"link-arg=--sysroot=$prebuilt/sysroot\" -C link-arg=-fuse-ld=lld -C link-arg=--unwindlib=none)",
    }

    def test_complete_manifest_and_cxx_runtime_policies_are_stable(self):
        for flavor in self.flavors:
            for target in write_artifact_manifest.TARGETS:
                with self.subTest(flavor=flavor, target=target):
                    expected = self.expected_cxx[target]
                    if flavor == "native" and target.startswith("linux-"):
                        expected += " " + self.linux_native
                    if flavor == "native" and target.startswith("android-"):
                        expected += " -laaudio"
                    self.assertEqual(self._link_flags_policy(target, flavor), expected)
                    self.assertEqual(write_artifact_manifest.links_for(flavor, target), self._manifest_policy(target, flavor))

    def test_link_flags_policy_rejects_trailing_linux_arm64_library(self):
        recipe = self._recipe("_link-flags flavor target:", "_cpp-smoke flavor target kit:")
        branch = self._case_branch(recipe, "linux-arm64")
        mutated_recipe = recipe.replace(branch, branch + "; flags+=' -lbad'", 1)

        with self.assertRaisesRegex(AssertionError, "unparseable link policy"):
            self._link_flags_policy("linux-arm64", "core", mutated_recipe)

    def test_affected_targets_bind_complete_ordered_policies_to_smoke_branches(self):
        cxx = self._recipe("_cpp-smoke flavor target kit:", "_rust-smoke flavor target kit:")
        rust = self._recipe("_rust-smoke flavor target kit:", "_runtime-test flavor target archive:")
        self.assertIn('just --justfile "{{ root }}/Justfile" _link-flags "{{ flavor }}" "{{ target }}"', cxx)
        for target in self.affected:
            with self.subTest(consumer="cxx", target=target):
                self.assertIn(self._cxx_target_input(target), self._case_branch_for_target(cxx, target))
            with self.subTest(consumer="rust", target=target):
                branch = self._case_branch_for_target(rust, target)
                self.assertIn(self.expected_rust[target], branch)
                self.assertIn(self._rust_target_input(target), branch)
                for flavor in self.flavors:
                    self.assertEqual(
                        [item for item in write_artifact_manifest.links_for(flavor, target) if item["kind"] == "link_arg"],
                        [{"kind": "link_arg", "name": flag} for flag in self._manifest_runtime_flags(target)],
                    )

    def test_android_libunwind_is_android_guarded_once_without_host_runtime_admission(self):
        compile_recipe = self._recipe("_compile flavor target:", "_export-predicate-failed")
        export_recipe = self._recipe("_export flavor target:", "_bridge-objects flavor target stage definitions_file:")
        self.assertEqual(compile_recipe.count("buildtools/third_party/libunwind:libunwind"), 1)
        self.assertEqual(export_recipe.count("obj/buildtools/third_party/libunwind/libunwind"), 1)
        self.assertEqual(self._guarded_command(compile_recipe, "buildtools/third_party/libunwind:libunwind"), 'if [[ "{{ target }}" = android-* ]]; then "$src/third_party/ninja/ninja" -C "$out" buildtools/third_party/libunwind:libunwind; fi')
        self.assertEqual(self._guarded_command(export_recipe, "obj/buildtools/third_party/libunwind/libunwind"), 'if [[ "{{ target }}" = android-* ]]; then find "$out/obj/buildtools/third_party/libunwind/libunwind" -name \'*.o\' -print > "$objects"; fi')
        for target in self.affected[:2]:
            with self.subTest(target=target):
                self.assertEqual(self._link_flags_policy(target, "core"), self.expected_cxx[target])
                self.assertEqual(self._link_flags_policy(target, "native"), self.expected_cxx[target] + " -laaudio")
        self.assertNotIn("-lunwind", JUSTFILE)

    def test_linux_arm64_uses_its_bullseye_compiler_rt_branch(self):
        compile_recipe = self._recipe("_compile flavor target:", "_export-predicate-failed")
        cxx = self._recipe("_cpp-smoke flavor target kit:", "_rust-smoke flavor target kit:")
        rust = self._recipe("_rust-smoke flavor target kit:", "_runtime-test flavor target archive:")
        self.assertIn('--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot"', self._case_branch_for_target(cxx, "linux-arm64"))
        self.assertIn("debian_bullseye_arm64-sysroot", self._case_branch_for_target(rust, "linux-arm64"))
        self.assertIn(
            'if test "{{ target }}" = linux-arm64; then "$src/third_party/ninja/ninja" -C "$out" phony/buildtools/third_party/libunwind/libunwind.linkdeps; fi',
            compile_recipe,
        )
        self.assertNotIn("--rtlib=libgcc", JUSTFILE)
        self.assertIn(
            'linux-arm64) platform=\'target_os="linux" target_cpu="arm64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_arm64-sysroot" use_custom_libcxx=true use_custom_libunwind=true clang_use_chrome_plugins=false\'',
            JUSTFILE,
        )
        self.assertEqual(
            write_artifact_manifest.TARGETS["linux-arm64"]["cxx_runtime"],
            "bundled libc++, libc++abi, and libunwind",
        )

    @classmethod
    def _manifest_policy(cls, target, flavor):
        if target.startswith("linux-"):
            flags = ["-fuse-ld=lld"] + (["--rtlib=compiler-rt", "--unwindlib=none"] if target == "linux-arm64" else [])
            libraries = ["pthread", "dl", "rt", "m"] + ([name[2:] for name in cls.linux_native.split()] if flavor == "native" else [])
            return [{"kind": "link_arg", "name": flag} for flag in flags] + [{"kind": "dylib", "name": name} for name in libraries]
        if target == "windows-x86_64":
            return [{"kind": "dylib", "name": name.removesuffix(".lib")} for name in cls.expected_cxx[target].split()]
        if target.startswith("android-"):
            libraries = ["log", "android", "GLESv2", "OpenSLES", "dl", "m"] + (["aaudio"] if flavor == "native" else [])
            return [{"kind": "link_arg", "name": "-fuse-ld=lld"}, {"kind": "link_arg", "name": "--unwindlib=none"}] + [{"kind": "dylib", "name": name} for name in libraries]
        names, links, index = cls.expected_cxx[target].split(), [], 0
        while index < len(names):
            kind = "weak_framework" if names[index] == "-weak_framework" else "framework"
            if names[index] not in ("-framework", "-weak_framework"):
                raise AssertionError(f"unexpected platform link token: {names[index]}")
            links.append({"kind": kind, "name": names[index + 1]})
            index += 2
        return [{"kind": "dylib", "name": "c++"}] + links

    @staticmethod
    def _manifest_runtime_flags(target):
        return {
            "android-x86_64": ("-fuse-ld=lld", "--unwindlib=none"),
            "android-arm64-v8a": ("-fuse-ld=lld", "--unwindlib=none"),
            "linux-arm64": ("-fuse-ld=lld", "--rtlib=compiler-rt", "--unwindlib=none"),
        }[target]

    @staticmethod
    def _target_pattern(target):
        if target.startswith("android-"):
            return "android-*"
        if target.startswith("windows-"):
            return "windows-*"
        if target.startswith("macos-"):
            return "macos-*"
        if target.startswith("ios-"):
            return "ios-*"
        return target

    @staticmethod
    def _cxx_target_input(target):
        return {
            "android-x86_64": 'triple=$(case "{{ target }}" in android-x86_64) printf x86_64;; *) printf aarch64;; esac)',
            "android-arm64-v8a": 'triple=$(case "{{ target }}" in android-x86_64) printf x86_64;; *) printf aarch64;; esac)',
            "linux-arm64": '--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot"',
        }[target]

    @staticmethod
    def _rust_target_input(target):
        return {
            "android-x86_64": "cargo_target=x86_64-linux-android; triple=x86_64",
            "android-arm64-v8a": "cargo_target=aarch64-linux-android; triple=aarch64",
            "linux-arm64": "cargo_target=aarch64-unknown-linux-gnu",
        }[target]

    @staticmethod
    def _guarded_command(recipe, needle):
        return next(line.strip() for line in recipe.splitlines() if needle in line)

    @staticmethod
    def _case_branch(recipe, pattern):
        match = re.search(rf"^      {re.escape(pattern)}\) (?P<body>.*);;$", recipe, re.MULTILINE)
        if match is None:
            raise AssertionError(f"missing target branch: {pattern}")
        return match.group("body")

    @classmethod
    def _case_branch_for_target(cls, recipe, target):
        pattern = "android-x86_64|android-arm64-v8a" if target.startswith("android-") and "android-x86_64|android-arm64-v8a" in recipe else cls._target_pattern(target)
        return cls._case_branch(recipe, pattern)

    @staticmethod
    def _link_flags_policy(target, flavor, recipe=None):
        if recipe is None:
            recipe = CrossTargetRuntimePolicyTests._recipe("_link-flags flavor target:", "_cpp-smoke flavor target kit:")
        branch = CrossTargetRuntimePolicyTests._case_branch(recipe, CrossTargetRuntimePolicyTests._target_pattern(target))
        match = re.fullmatch(r"flags='(?P<core>[^']+)'(?:; test \"\{\{ flavor \}\}\" = core \|\| flags\+=' (?P<native>[^']+)')?", branch)
        if match is None:
            raise AssertionError(f"unparseable link policy for {target}: {branch}")
        return match.group("core") if flavor == "core" or match.group("native") is None else match.group("core") + " " + match.group("native")

    @staticmethod
    def _recipe(start, end):
        beginning = JUSTFILE.index(start)
        return JUSTFILE[beginning : JUSTFILE.index(end, beginning)]


class TargetConsumerPolicyTests(unittest.TestCase):
    targets = {
        "linux-x86_64": "x86_64-unknown-linux-gnu",
        "linux-arm64": "aarch64-unknown-linux-gnu",
        "windows-x86_64": "x86_64-pc-windows-msvc",
        "macos-x86_64": "x86_64-apple-darwin",
        "macos-arm64": "aarch64-apple-darwin",
        "android-x86_64": "x86_64-linux-android",
        "android-arm64-v8a": "aarch64-linux-android",
        "ios-arm64": "aarch64-apple-ios",
        "ios-simulator-arm64": "aarch64-apple-ios-sim",
    }

    def test_every_target_is_linked_and_only_desktop_targets_execute(self):
        for target, cargo in self.targets.items():
            with self.subTest(target=target):
                for flavor in ("core", "native"):
                    self.assertEqual(consumer.coordinate(target, flavor), (cargo, f"webrtc-{flavor}-{target}.tar.gz"))
                    manifest = consumer.render_consumer_manifest(consumer.PUBLIC_REPOSITORY, "v1.2.3", target, flavor, tag=True)
                    self.assertEqual('features = ["native"]' in manifest, flavor == "native")
                action = "build" if target.split('-')[0] in {"ios", "android"} else "run"
                self.assertEqual(consumer.consumer_command(target, "--locked", "--offline"),
                                 ["cargo", action, "--target", cargo, "--locked", "--offline"])
                with self.assertRaises(consumer.ProofError):
                    consumer.validate_released(consumer.PUBLIC_REPOSITORY, "v1.2.3-linux", target, "core")

    def test_android_uses_standalone_api26_linker_not_a_host_compiler(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for target, triple in (("android-x86_64", "x86_64"), ("android-arm64-v8a", "aarch64")):
                cargo = self.targets[target]
                key = f"CARGO_TARGET_{cargo.upper().replace('-', '_')}_LINKER"
                linker = root / f"toolchains/llvm/prebuilt/linux-x86_64/bin/{triple}-linux-android26-clang"
                linker.parent.mkdir(parents=True, exist_ok=True)
                linker.touch()
                environment = {"ANDROID_NDK_HOME": str(root), "RUSTFLAGS": "-C debuginfo=1"}
                consumer.configure_target_environment(environment, cargo)
                self.assertEqual(environment[key], str(linker))
                self.assertEqual(environment["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"),
                                 ["-C", "debuginfo=1", "-C", "link-arg=--unwindlib=none"])
                with self.assertRaisesRegex(consumer.ProofError, "Android consumer requires"):
                    consumer.configure_target_environment({}, cargo)
                override = {key: "/standalone-sdk/linker"}
                consumer.configure_target_environment(override, cargo)
                self.assertEqual(override[key], "/standalone-sdk/linker")

    def test_linux_arm64_requires_a_clang_linker_and_runtime_inputs(self):
        with patch.object(consumer.shutil, "which", return_value=None):
            with self.assertRaisesRegex(consumer.ProofError, "Clang with compiler-rt"):
                consumer.configure_target_environment({}, "aarch64-unknown-linux-gnu")
        with patch.object(consumer.shutil, "which", return_value="/sdk/clang-19"):
            environment = {}
            consumer.configure_target_environment(environment, "aarch64-unknown-linux-gnu")
        self.assertEqual(environment["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"], "/sdk/clang-19")
        self.assertIn("link-arg=--rtlib=compiler-rt", environment["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"))
        self.assertIn("link-arg=--unwindlib=none", environment["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"))

    def test_apple_uses_target_sdk_and_preserves_minima(self):
        with tempfile.TemporaryDirectory() as temporary:
            for target, sdk, key, minimum in (
                ("macos-arm64", "macosx", "MACOSX_DEPLOYMENT_TARGET", "12.0"),
                ("macos-x86_64", "macosx", "MACOSX_DEPLOYMENT_TARGET", "12.0"),
                ("ios-arm64", "iphoneos", "IPHONEOS_DEPLOYMENT_TARGET", "18.0"),
                ("ios-simulator-arm64", "iphonesimulator", "IPHONEOS_DEPLOYMENT_TARGET", "18.0"),
            ):
                with self.subTest(target=target), patch.object(consumer, "run", return_value=subprocess.CompletedProcess([], 0, temporary + "\n")) as run:
                    environment = {key: "99.0"}
                    consumer.configure_target_environment(environment, self.targets[target])
                    self.assertEqual(environment[key], minimum)
                    self.assertEqual(environment["SDKROOT"], temporary)
                    self.assertEqual(run.call_args.args[0], ["xcrun", "--sdk", sdk, "--show-sdk-path"])

    def test_missing_apple_sdk_is_not_a_silent_host_sdk_fallback(self):
        with patch.object(consumer, "run", return_value=subprocess.CompletedProcess([], 0, "")):
            with self.assertRaisesRegex(consumer.ProofError, "missing iphoneos SDK"):
                consumer.configure_target_environment({}, "aarch64-apple-ios")

    def test_windows_sentinels_are_cmd_files_and_keep_msvc_linker_separate(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(consumer.platform, "system", return_value="Windows"):
            root = Path(temporary)
            with patch.dict(os.environ, {"CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER": "link.exe"}):
                environment = consumer.cold_consumer_environment(root / "marker", root / "sentinels", root / "no-producer", "x86_64-pc-windows-msvc")
            self.assertEqual(environment["CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"], "link.exe")
            self.assertEqual(Path(environment["CC"]).suffix, ".cmd")
            self.assertIn("exit /b 97", Path(environment["CC"]).read_text())
            self.assertIn("target-feature=+crt-static", environment["CARGO_ENCODED_RUSTFLAGS"])
            self.assertTrue((root / "sentinels/gn.cmd").is_file())
            self.assertTrue((root / "sentinels/ninja.cmd").is_file())

    @unittest.skipIf(os.name == "nt", "POSIX sentinel execution")
    def test_posix_compiler_sentinel_fails_and_records_invocation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sentinel = consumer.write_sentinel(root, "cxx", root / "marker")
            result = subprocess.run([str(sentinel), "-c", "forbidden.cc"], capture_output=True)
            self.assertEqual(result.returncode, 97)
            self.assertIn("-c forbidden.cc", (root / "marker").read_text())

    def test_candidate_snapshots_have_only_the_selected_os_coordinates(self):
        for scope, targets in OS_TARGETS.items():
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = sorted(targets)[0]
                asset = f"webrtc-core-{target}.tar.gz"
                consumer.clean_snapshot(root, f"http://127.0.0.1:1/{asset}", "a" * 64, target, "core")
                lock = json.loads((root / "artifacts.lock.json").read_text())
                self.assertEqual(lock["release_scope"], scope)
                self.assertEqual({entry["asset_name"] for entry in lock["artifacts"] if entry["url"]}, archive_names(scope))
                report = json.loads((root / f"{scope}-audit.json").read_text())
                for evidence in report["assets"]:
                    expected = "applied" if scope == "ios" and evidence["name"].startswith("webrtc-core-") else "pristine"
                    self.assertEqual(evidence["source_state"], expected)


class MobileRuntimePolicyTests(unittest.TestCase):
    def test_android_baseline_is_real_emulator_api26_x86_64(self):
        properties = {"api": "26", "abi": "x86_64", "qemu": "1"}
        mobile.android_environment(properties)
        for key, value in (("api", "27"), ("api", "35"), ("abi", "arm64-v8a"), ("qemu", "0")):
            with self.subTest(key=key, value=value), self.assertRaises(mobile.RuntimeError):
                mobile.android_environment({**properties, key: value})

    def test_ios_requires_exact_available_baseline_not_newer_runtime(self):
        runtime = {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-18-0", "version": "18.0", "isAvailable": True}
        self.assertEqual(mobile.ios_runtime([runtime]), runtime["identifier"])
        for runtimes in ([], [{**runtime, "version": "18.1"}], [{**runtime, "isAvailable": False}], [runtime, runtime]):
            with self.subTest(runtimes=runtimes), self.assertRaises(mobile.RuntimeError):
                mobile.ios_runtime(runtimes)

    def test_mobile_success_requires_fresh_nonce_and_exact_archived_bytes(self):
        result = {"pulsebeam_status": 0, "pulsebeam_nonce": "nonce", "pulsebeam_archive_sha256": "a" * 64}
        mobile.validate_result(result, "nonce", "a" * 64)
        for key, value in (("pulsebeam_status", 1), ("pulsebeam_status", False),
                           ("pulsebeam_nonce", "stale"), ("pulsebeam_archive_sha256", "b" * 64)):
            with self.subTest(key=key, value=value), self.assertRaises(mobile.RuntimeError):
                mobile.validate_result({**result, key: value}, "nonce", "a" * 64)
        with self.assertRaises(mobile.RuntimeError):
            mobile.validate_result({}, "nonce", "a" * 64)

    def test_android_command_exit_zero_is_not_instrumentation_success(self):
        success = ("INSTRUMENTATION_RESULT: pulsebeam_status=0\n"
                   "INSTRUMENTATION_RESULT: pulsebeam_nonce=nonce\n"
                   f"INSTRUMENTATION_RESULT: pulsebeam_archive_sha256={'a' * 64}\n"
                   "INSTRUMENTATION_CODE: -1\n")
        self.assertEqual(mobile.android_result(success, "nonce", "a" * 64)["pulsebeam_status"], 0)
        for output in ("", success.replace("CODE: -1", "CODE: 0"),
                       success.replace("status=0", "status=1"),
                       success + "INSTRUMENTATION_RESULT: pulsebeam_status=0\n"):
            with self.subTest(output=output), self.assertRaises(mobile.RuntimeError):
                mobile.android_result(output, "nonce", "a" * 64)

    def test_mobile_archive_rejects_path_links_and_duplicate_members(self):
        for names, linked in ((["../escape"], False), (["/escape"], False),
                              (["C:\\escape"], False), (["link"], True),
                              (["manifest.json", "manifest.json"], False)):
            with self.subTest(names=names), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "input.tar.gz"
                with tarfile.open(archive, "w:gz") as output:
                    for name in names:
                        member = tarfile.TarInfo(name)
                        if linked:
                            member.type = tarfile.SYMTYPE
                            member.linkname = "../escape"
                            output.addfile(member)
                        else:
                            member.size = 2
                            output.addfile(member, io.BytesIO(b"{}"))
                with self.assertRaisesRegex(mobile.RuntimeError, "unsafe|duplicate"):
                    mobile.extract_kit(archive, mobile.digest(archive), root / "kit", "android-x86_64", "core")
                self.assertFalse((root / "escape").exists())

    def test_mobile_runtime_rejects_physical_mobile_targets_before_sdk_work(self):
        for target in ("android-arm64-v8a", "ios-arm64"):
            with self.subTest(target=target), self.assertRaises(mobile.RuntimeError):
                mobile.qualify(Path("absent"), "a" * 64, target, "core", Path("absent-output"))


if __name__ == "__main__":
    unittest.main()
