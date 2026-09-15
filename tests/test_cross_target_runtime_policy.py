import importlib.util
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
            'linux-arm64) platform=\'target_os="linux" target_cpu="arm64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_arm64-sysroot" use_custom_libcxx=true use_custom_libunwind=true\'',
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


if __name__ == "__main__":
    unittest.main()
