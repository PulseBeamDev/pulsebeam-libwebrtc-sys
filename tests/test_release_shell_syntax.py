"""Parse actual Justfile shell recipes with the host's Bash, including Apple 3.2."""
import os
from pathlib import Path
import re
import subprocess
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
BASH = os.environ.get("PULSEBEAM_TEST_BASH", "bash")
TARGETS = ("linux-x86_64", "linux-arm64", "windows-x86_64", "macos-x86_64", "macos-arm64",
           "android-x86_64", "android-arm64-v8a", "ios-arm64", "ios-simulator-arm64")


class ReleaseShellSyntaxTests(unittest.TestCase):
    def test_every_bash_recipe_parses_on_the_runner_shell(self):
        recipes = re.findall(r"^([\w-]+)[^\n:]*:\n((?:    .*\n|\n)*)", (ROOT / "Justfile").read_text(), re.MULTILINE)
        parsed = []
        for name, body in recipes:
            body = textwrap.dedent(body)
            if not body.startswith("#!/usr/bin/env bash\n"):
                continue
            # Shell parsing does not need installed SDKs, downloads or a build.
            for target in TARGETS if name == "_export" else ("ios-arm64",):
                script = body.replace("{{ target }}", target)
                script = re.sub(r"\{\{[^}]*\}\}", "fixture", script)
                with self.subTest(recipe=name, target=target):
                    result = subprocess.run([BASH, "-n"], input=script, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
            parsed.append(name)
        self.assertIn("_export", parsed)
        self.assertIn("_apple-host-protoc", parsed)
        self.assertGreater(len(parsed), 20)

    def test_export_library_selection_executes_for_every_target_with_space_paths(self):
        lines = (ROOT / "Justfile").read_text().splitlines()
        selection = next(line.strip() for line in lines if line.strip().startswith('case "{{ target }}" in windows-*) library='))
        for target in TARGETS:
            with self.subTest(target=target):
                result = subprocess.run([BASH, "-euc", selection.replace("{{ target }}", target) + '\nprintf "%s" "$library"'],
                                        env={**os.environ, "stage": "/stage with spaces"}, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "/stage with spaces/lib/" + ("webrtc.lib" if target.startswith("windows-") else "libwebrtc.a"))


if __name__ == "__main__":
    unittest.main()
