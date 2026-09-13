import importlib.util
from pathlib import Path
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
    affected = ("android-x86_64", "android-arm64-v8a", "linux-arm64")
    flavors = ("core", "native")

    def test_affected_manifest_and_smoke_policies_match(self):
        link_flags = self._recipe("_link-flags flavor target:", "_cpp-smoke flavor target kit:")
        cxx = self._recipe("_cpp-smoke flavor target kit:", "_rust-smoke flavor target kit:")
        rust = self._recipe("_rust-smoke flavor target kit:", "_runtime-test flavor target archive:")
        expected = {
            "android-x86_64": ("-fuse-ld=lld", "--unwindlib=none"),
            "android-arm64-v8a": ("-fuse-ld=lld", "--unwindlib=none"),
            "linux-arm64": ("-fuse-ld=lld", "--rtlib=compiler-rt"),
        }
        for flavor in self.flavors:
            for target in self.affected:
                with self.subTest(flavor=flavor, target=target):
                    manifest_flags = [
                        link["name"]
                        for link in write_artifact_manifest.links_for(flavor, target)
                        if link["kind"] == "link_arg"
                    ]
                    self.assertEqual(manifest_flags, list(expected[target]))
                    self.assertIn(expected[target][1], link_flags)
                    self.assertIn('_link-flags "{{ flavor }}" "{{ target }}"', cxx)
                    self.assertIn(f"link-arg={expected[target][1]}", rust)

    def test_android_packages_only_target_unwind_and_uses_no_unwind_library(self):
        compile_recipe = self._recipe("_compile flavor target:", "_export-predicate-failed")
        export = self._recipe("_export flavor target:", "_bridge-objects flavor target stage definitions_file:")
        self.assertEqual(
            compile_recipe.count("buildtools/third_party/libunwind:libunwind"), 1
        )
        self.assertEqual(
            export.count("obj/buildtools/third_party/libunwind/libunwind"), 1
        )
        self.assertIn('"$prebuilt/sysroot"', self._recipe("_cpp-smoke flavor target kit:", "_rust-smoke flavor target kit:"))
        self.assertIn('"link-arg=--sysroot=$prebuilt/sysroot"', self._recipe("_rust-smoke flavor target kit:", "_runtime-test flavor target archive:"))
        self.assertNotIn("-lunwind", JUSTFILE)

    def test_linux_arm64_keeps_bullseye_compiler_rt_policy(self):
        link_flags = self._recipe("_link-flags flavor target:", "_cpp-smoke flavor target kit:")
        cxx = self._recipe("_cpp-smoke flavor target kit:", "_rust-smoke flavor target kit:")
        rust = self._recipe("_rust-smoke flavor target kit:", "_runtime-test flavor target archive:")
        self.assertIn("debian_bullseye_arm64-sysroot", cxx)
        self.assertIn("debian_bullseye_arm64-sysroot", rust)
        self.assertIn("--rtlib=compiler-rt", link_flags)
        self.assertIn("link-arg=--rtlib=compiler-rt", rust)
        self.assertNotIn("--rtlib=libgcc", JUSTFILE)

    def test_unaffected_manifest_runtime_policies_remain_unchanged(self):
        for flavor in self.flavors:
            for target in sorted(set(write_artifact_manifest.TARGETS) - set(self.affected)):
                with self.subTest(flavor=flavor, target=target):
                    flags = [
                        link["name"]
                        for link in write_artifact_manifest.links_for(flavor, target)
                        if link["kind"] == "link_arg"
                    ]
                    self.assertNotIn("--unwindlib=none", flags)
                    self.assertNotIn("--rtlib=compiler-rt", flags)

    @staticmethod
    def _recipe(start, end):
        beginning = JUSTFILE.index(start)
        return JUSTFILE[beginning : JUSTFILE.index(end, beginning)]


if __name__ == "__main__":
    unittest.main()
