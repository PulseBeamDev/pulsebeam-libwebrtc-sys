import tempfile
import unittest
from pathlib import Path

from tools.select_upgrade_pin import select

OLD = "a" * 40
NEW = "b" * 40


class SelectUpgradePinTests(unittest.TestCase):
    def test_updates_only_declared_pin(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "Justfile"
            path.write_text(f'webrtc_commit := "{OLD}"\n# {OLD}\n')
            select(path, NEW)
            self.assertEqual(path.read_text(), f'webrtc_commit := "{NEW}"\n# {OLD}\n')

    def test_invalid_or_unchanged_candidate_does_not_write(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "Justfile"
            original = f'webrtc_commit := "{OLD}"\n'
            path.write_text(original)
            for candidate in (OLD, "BAD", "c" * 39):
                with self.assertRaises(ValueError):
                    select(path, candidate)
                self.assertEqual(path.read_text(), original)

    def test_ambiguous_pin_does_not_write(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "Justfile"
            original = f'webrtc_commit := "{OLD}"\n' * 2
            path.write_text(original)
            with self.assertRaisesRegex(ValueError, "exactly one"):
                select(path, NEW)
            self.assertEqual(path.read_text(), original)


if __name__ == "__main__":
    unittest.main()
