from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"
FLAVORS = ("core", "native")


class LinuxAsanConfigurationTests(unittest.TestCase):
    def test_asan_instruments_every_manual_bridge_compile_for_both_flavors(self):
        for flavor in FLAVORS:
            with self.subTest(flavor=flavor), tempfile.TemporaryDirectory() as temporary:
                calls = self._run_bridge(Path(temporary), flavor, sanitizer="address")
                self.assertEqual(len(calls), 9)
                self.assertTrue(all("-fsanitize=address" in call for call in calls), calls)

    def test_asan_instruments_cpp_smoke_for_both_flavors(self):
        for flavor in FLAVORS:
            with self.subTest(flavor=flavor), tempfile.TemporaryDirectory() as temporary:
                call = self._run_cpp_smoke(Path(temporary), flavor, sanitizer="address")
                self.assertIn("-fsanitize=address", call)

    def test_asan_retains_rust_boundaries(self):
        recipe = JUSTFILE.read_text(encoding="utf-8")
        rust_smoke = self._recipe("_rust-smoke flavor target kit:", "# Execute the complete Rust runtime suite")
        runtime_test = self._recipe("_runtime-test flavor target archive:", "")
        self.assertIn("rustflags+=(-C link-arg=-fsanitize=address)", rust_smoke)
        self.assertIn("rustflags=(-C link-arg=-fsanitize=address)", runtime_test)

    def test_ordinary_manual_bridge_commands_do_not_gain_sanitizer_flags(self):
        with tempfile.TemporaryDirectory() as temporary:
            calls = self._run_bridge(Path(temporary), "core", sanitizer=None)
        with tempfile.TemporaryDirectory() as temporary:
            cpp_call = self._run_cpp_smoke(Path(temporary), "core", sanitizer=None)
        self.assertEqual(len(calls), 9)
        self.assertTrue(all("-fsanitize=address" not in call for call in calls), calls)
        self.assertNotIn("-fsanitize=address", cpp_call)

    def _recipe(self, start: str, end: str) -> str:
        recipe = JUSTFILE.read_text(encoding="utf-8")
        offset = recipe.index(start)
        return recipe[offset:] if not end else recipe[offset:recipe.index(end, offset)]

    def _run_bridge(self, temporary: Path, flavor: str, sanitizer: str | None) -> list[list[str]]:
        work = temporary / "work"
        stage = temporary / "stage"
        tools = temporary / "tools"
        log = temporary / "compiler-arguments"
        compiler = work / "checkout/src/third_party/llvm-build/Release+Asserts/bin/clang++"
        compiler.parent.mkdir(parents=True)
        tools.mkdir()
        compiler.write_text(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\" >> \"$PULSEBEAM_TEST_COMPILER_LOG\"\nprintf '\\n' >> \"$PULSEBEAM_TEST_COMPILER_LOG\"\nfor ((i = 1; i <= $#; i++)); do if test \"${!i}\" = -o; then j=$((i + 1)); mkdir -p \"$(dirname \"${!j}\")\"; touch \"${!j}\"; fi; done\n",
            encoding="utf-8",
        )
        generator = tools / "cxxbridge"
        generator.write_text("#!/usr/bin/env bash\nprintf '// generated\\n'\n", encoding="utf-8")
        (tools / "python3").write_text(
            f"#!/usr/bin/env bash\nset -euo pipefail\ncase \"$*\" in *'cxx_import.py install-generator'*) printf '%s\\n' '{generator}' ;; *) exec /usr/bin/python3 \"$@\" ;; esac\n",
            encoding="utf-8",
        )
        for executable in (compiler, generator, tools / "python3"):
            executable.chmod(0o755)
        definitions = temporary / "definitions"
        definitions.write_text("-DTEST=1\n", encoding="utf-8")
        runtime = temporary / "runtime"
        runtime.mkdir()
        environment = {
            **os.environ,
            "WEBRTC_WORK": str(work),
            "PATH": f"{tools}:{os.environ['PATH']}",
            "PULSEBEAM_TEST_COMPILER_LOG": str(log),
            "XDG_RUNTIME_DIR": str(runtime),
        }
        if sanitizer:
            environment["PULSEBEAM_WEBRTC_SANITIZER"] = sanitizer
        result = subprocess.run(
            ["just", "--justfile", str(JUSTFILE), "_bridge-objects", flavor, "linux-x86_64", str(stage), str(definitions)],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return [call.splitlines() for call in log.read_text(encoding="utf-8").strip().split("\n\n")]

    def _run_cpp_smoke(self, temporary: Path, flavor: str, sanitizer: str | None) -> list[str]:
        work = temporary / "work"
        kit = temporary / "kit"
        log = temporary / "compiler-arguments"
        compiler = work / "checkout/src/third_party/llvm-build/Release+Asserts/bin/clang++"
        compiler.parent.mkdir(parents=True)
        compiler.write_text(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\" > \"$PULSEBEAM_TEST_COMPILER_LOG\"\n",
            encoding="utf-8",
        )
        compiler.chmod(0o755)
        (kit / "include/c++/v1").mkdir(parents=True)
        (kit / "lib").mkdir()
        (kit / "build.txt").write_text("cxx_defines=-DTEST=1\n", encoding="utf-8")
        runtime = temporary / "runtime"
        runtime.mkdir()
        environment = {
            **os.environ,
            "WEBRTC_WORK": str(work),
            "PULSEBEAM_TEST_COMPILER_LOG": str(log),
            "XDG_RUNTIME_DIR": str(runtime),
        }
        if sanitizer:
            environment["PULSEBEAM_WEBRTC_SANITIZER"] = sanitizer
        result = subprocess.run(
            ["just", "--justfile", str(JUSTFILE), "_cpp-smoke", flavor, "linux-x86_64", str(kit)],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return log.read_text(encoding="utf-8").splitlines()


if __name__ == "__main__":
    unittest.main()
