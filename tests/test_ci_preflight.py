import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools import ci_preflight, github_release


class CiPreflightTests(unittest.TestCase):
    def test_release_flags_are_local_and_create_only_flag_never_used_on_edit(self):
        self.assertIn("--generate-notes", github_release.GH_OPTIONS["release create"])
        self.assertNotIn("--generate-notes", github_release.GH_OPTIONS["release edit"])
        workflow = (ci_preflight.WORKFLOWS / "release.yml").read_text()
        self.assertIn("just ci-release classify", workflow)
        self.assertIn("just ci-release advertise", workflow)
        self.assertNotIn("gh release ", workflow)

    def test_host_preflight_precedes_expensive_linux_images(self):
        workflow = (ci_preflight.WORKFLOWS / "release.yml").read_text()
        validate = workflow.split("  validate:", 1)[1].split("  linux-build:", 1)[0]
        self.assertLess(validate.index("run: just ci-preflight"), validate.index("just linux-image"))

    def test_rejects_inline_release_scripts_and_unsupported_options(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "publish.yml"
            path.write_text('run: gh release edit "$RELEASE_TAG" --draft=false --generate-notes\n')
            def fake_help(args, **kwargs):
                command = " ".join(args[1:-1])
                supported = github_release.GH_OPTIONS[command] - ({"--draft"} if command == "release edit" else set())
                return type("Result", (), {"returncode": 0, "stdout": "Flags:\n" + "".join(f"      {flag}  Example\n" for flag in supported), "stderr": ""})()
            with patch.object(ci_preflight.subprocess, "run", side_effect=fake_help):
                errors = ci_preflight.check_release_options(Path(temp))
            self.assertIn(f"{path}:1: move gh release scripting behind just ci-release", errors)
            self.assertIn("gh release edit does not support --draft", errors)

    def test_preflight_detects_options_added_to_actual_release_calls(self):
        self.assertEqual(ci_preflight.release_options_in_source(), github_release.GH_OPTIONS)
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / "github_release.py"
            source.write_text(Path(github_release.__file__).read_text().replace('"--draft=false")', '"--draft=false", "--generate-notes")'))
            with patch.object(github_release, "__file__", str(source)):
                self.assertIn("--generate-notes", ci_preflight.release_options_in_source()["release edit"])
                with patch.object(ci_preflight.subprocess, "run", return_value=type("Result", (), {"returncode": 0, "stdout": "Flags:\n", "stderr": ""})()):
                    errors = ci_preflight.check_release_options(Path(temp))
                self.assertTrue(any("preflight flags differ from actual gh calls" in error for error in errors))

    def test_helps_are_required(self):
        with tempfile.TemporaryDirectory() as temp:
            with patch.object(ci_preflight.subprocess, "run", return_value=type("Result", (), {"returncode": 1, "stderr": "missing gh", "stdout": ""})()):
                self.assertIn("--help failed", ci_preflight.check_release_options(Path(temp))[0])


if __name__ == "__main__":
    unittest.main()
