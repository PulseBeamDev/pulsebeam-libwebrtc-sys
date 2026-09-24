import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.repair_incomplete_git import repair


class RepairIncompleteGitTests(unittest.TestCase):
    def test_only_removes_partial_dependency_checkouts(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "src"
            good = root / "third_party" / "good"
            bad = root / "third_party" / "bad"
            other = root / "generated"
            for path in (good, bad, other):
                path.mkdir(parents=True)
            subprocess.run(["git", "init", "-q", str(good)], check=True)
            subprocess.run(["git", "-C", str(good), "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-qm", "valid"], check=True)
            subprocess.run(["git", "init", "-q", str(bad)], check=True)
            (bad / "partial").write_text("incomplete")
            (other / "input").write_text("keep")
            self.assertEqual(repair(root), [bad])
            self.assertTrue(good.is_dir())
            self.assertTrue((other / "input").is_file())
            self.assertFalse(bad.exists())
            self.assertEqual(repair(root), [])

    def test_missing_root_is_noop(self):
        with tempfile.TemporaryDirectory() as temp:
            self.assertEqual(repair(Path(temp) / "missing"), [])


if __name__ == "__main__":
    unittest.main()
