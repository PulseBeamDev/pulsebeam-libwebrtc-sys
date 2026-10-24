import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class Arm64CompilerBuiltinsTests(unittest.TestCase):
    def _select(self, target, mode="valid"):
        with tempfile.TemporaryDirectory(prefix="compiler runtime space ") as temporary:
            src = Path(temporary)
            binary = src / "third_party/llvm-build/Release+Asserts/bin"
            binary.mkdir(parents=True)
            archive = src / "target builtins.a"
            if mode != "missing":
                archive.touch()
            log = src / "driver-arguments"
            clang = binary / "clang"
            clang.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$@" > "$TEST_DRIVER_LOG"\nprintf "%s\\n" "$TEST_BUILTINS"\n')
            nm = binary / "llvm-nm"
            nm.write_text('#!/usr/bin/env bash\nif test "$TEST_BUILTINS_MODE" = valid; then printf "000000 T __arm_tpidr2_save\\n"; fi\n')
            clang.chmod(0o755)
            nm.chmod(0o755)
            result = subprocess.run(
                ["just", "--justfile", str(ROOT / "Justfile"), "_export-compiler-builtins", target, str(src)],
                capture_output=True, text=True,
                env={**os.environ, "TEST_DRIVER_LOG": str(log), "TEST_BUILTINS": str(archive), "TEST_BUILTINS_MODE": mode},
            )
            return result, archive, log.read_text().splitlines() if log.exists() else []

    def test_each_arm64_runtime_is_selected_by_the_pinned_clang_driver(self):
        for target, triple in (("linux-arm64", "aarch64-linux-gnu"), ("android-arm64-v8a", "aarch64-linux-android26")):
            with self.subTest(target=target):
                result, archive, arguments = self._select(target)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), str(archive))
                self.assertEqual(arguments, ["--target=" + triple, "--rtlib=compiler-rt", "--print-libgcc-file-name"])

    def test_absent_or_incompatible_builtins_fail_at_export(self):
        for mode, message in (("missing", "missing target compiler-rt builtins"), ("no-sme", "missing SME ABI support")):
            for target in ("linux-arm64", "android-arm64-v8a"):
                with self.subTest(mode=mode, target=target):
                    result, _, _ = self._select(target, mode)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(message, result.stderr)

    def test_other_targets_do_not_admit_a_foreign_runtime(self):
        for target in ("linux-x86_64", "android-x86_64", "macos-arm64", "macos-x86_64", "ios-arm64", "ios-simulator-arm64", "windows-x86_64"):
            with self.subTest(target=target):
                result, _, arguments = self._select(target)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertEqual(arguments, [])

    def test_runtime_enters_the_merged_archive_not_consumer_link_flags(self):
        recipe = (ROOT / "Justfile").read_text().split("_export-static-closure flavor target", 1)[1].split("_export-compiler-builtins target src:", 1)[0]
        self.assertIn('_export-compiler-builtins "{{ target }}" "{{ src }}" >> "{{ archives }}"', recipe)


if __name__ == "__main__":
    unittest.main()
