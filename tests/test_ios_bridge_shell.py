from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"


class IosBridgeShellTests(unittest.TestCase):
    flavors = ("core", "native")
    targets = {
        "ios-arm64": ("iphoneos", "-miphoneos-version-min=18.0"),
        "ios-simulator-arm64": ("iphonesimulator", "-mios-simulator-version-min=18.0"),
    }

    def test_all_ios_bridge_commands_preserve_ordered_arguments_and_space_paths(self):
        for flavor in self.flavors:
            for target, (sdk, minimum) in self.targets.items():
                with self.subTest(flavor=flavor, target=target):
                    calls, stage = self._run_bridge(flavor, target)
                    self.assertEqual(len(calls), 9)
                    expected_prefix = [
                        "-arch", "arm64", "-isysroot", "/fake SDK path", minimum,
                        "-std=c++20", "-fno-exceptions", "-fno-rtti",
                        "-Wno-nullability-completeness", f"-I{stage}/include", "-DTEST=1", "-c",
                    ]
                    for call in calls:
                        self.assertEqual(call[: len(expected_prefix)], expected_prefix)
                        self.assertEqual(call[-2], "-o")
                        self.assertIn(" ", call[-1])

    def test_android_bridge_target_selection_and_arguments_are_preserved(self):
        recipe = JUSTFILE.read_text(encoding="utf-8")
        start = recipe.index("_bridge-objects flavor target stage definitions_file:")
        end = recipe.index("_link-flags flavor target:", start)
        bridge = recipe[start:end]
        self.assertIn('case "{{ target }}" in android-x86_64) triple=x86_64;; *) triple=aarch64;; esac', bridge)
        self.assertIn(
            'args=(--target="${triple}-linux-android26" --sysroot="$prebuilt/sysroot" -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness -nostdinc++ -isystem "{{ stage }}/include/c++/v1")',
            bridge,
        )

    def _run_bridge(self, flavor: str, target: str) -> tuple[list[list[str]], Path]:
        with tempfile.TemporaryDirectory() as temporary:
            temp = Path(temporary)
            work = temp / "work space"
            stage = temp / "stage space" / flavor / target
            tools = temp / "tools"
            log = temp / "compiler-arguments"
            compiler = work / "checkout/src/third_party/llvm-build/Release+Asserts/bin/clang++"
            compiler.parent.mkdir(parents=True)
            tools.mkdir()
            compiler.write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\" >> \"$PULSEBEAM_TEST_COMPILER_LOG\"\nprintf '\\n' >> \"$PULSEBEAM_TEST_COMPILER_LOG\"\nfor ((i = 1; i <= $#; i++)); do if test \"${!i}\" = -o; then j=$((i + 1)); mkdir -p \"$(dirname \"${!j}\")\"; touch \"${!j}\"; fi; done\n",
                encoding="utf-8",
            )
            (tools / "xcrun").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' '/fake SDK path'\n",
                encoding="utf-8",
            )
            generator = tools / "cxxbridge"
            generator.write_text("#!/usr/bin/env bash\nprintf '// generated\\n'\n", encoding="utf-8")
            (tools / "python3").write_text(
                f"#!/usr/bin/env bash\nset -euo pipefail\ncase \"$*\" in *'cxx_import.py install-generator'*) printf '%s\\n' '{generator}' ;; *) exec /usr/bin/python3 \"$@\" ;; esac\n",
                encoding="utf-8",
            )
            for executable in (compiler, tools / "xcrun", generator, tools / "python3"):
                executable.chmod(0o755)
            definitions = temp / "definitions"
            definitions.write_text("-DTEST=1\n", encoding="utf-8")
            runtime = temp / "runtime"
            runtime.mkdir()
            environment = {
                **os.environ,
                "WEBRTC_WORK": str(work),
                "PATH": f"{tools}:{os.environ['PATH']}",
                "PULSEBEAM_TEST_COMPILER_LOG": str(log),
                "XDG_RUNTIME_DIR": str(runtime),
            }
            result = subprocess.run(
                ["just", "--justfile", str(JUSTFILE), "_bridge-objects", flavor, target, str(stage), str(definitions)],
                cwd=ROOT,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return [call.splitlines() for call in log.read_text(encoding="utf-8").strip().split("\n\n")], stage


if __name__ == "__main__":
    unittest.main()
