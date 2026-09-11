from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "write_artifact_lock", ROOT / "tools/write_artifact_lock.py"
)
write_artifact_lock = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(write_artifact_lock)


class ArtifactLockTests(unittest.TestCase):
    def test_records_exact_complete_checksum_set(self):
        with tempfile.TemporaryDirectory() as temporary:
            checksums = Path(temporary) / "SHA256SUMS"
            lock = __import__("json").loads(
                (ROOT / "artifacts.lock.json").read_text(encoding="utf-8")
            )
            checksums.write_text(
                "".join(
                    f"{'1' * 64}  {entry['asset_name']}\n"
                    for entry in lock["artifacts"]
                ),
                encoding="utf-8",
            )
            rendered = write_artifact_lock.render(
                ROOT / "artifacts.lock.json", checksums, "webrtc-bridge-v1"
            ).decode()
            self.assertEqual(rendered.count("/webrtc-bridge-v1/webrtc-"), 18)
            self.assertNotIn('"sha256": null', rendered)
            self.assertIn('"release_ready": true', rendered)

    def test_rejects_missing_or_malformed_checksums(self):
        with tempfile.TemporaryDirectory() as temporary:
            checksums = Path(temporary) / "SHA256SUMS"
            checksums.write_text("not-a-hash  asset\n", encoding="utf-8")
            with self.assertRaisesRegex(write_artifact_lock.LockError, "malformed"):
                write_artifact_lock.render(
                    ROOT / "artifacts.lock.json", checksums, "webrtc-bridge-v1"
                )


if __name__ == "__main__":
    unittest.main()
