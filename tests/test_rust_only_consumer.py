from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("rust_only_consumer", ROOT / "tools/rust_only_consumer.py")
rust_only_consumer = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(rust_only_consumer)


class RustOnlyConsumerTests(unittest.TestCase):
    def test_renders_each_linux_coordinate(self):
        for target, flavor, cargo_target in [
            ("linux-x86_64", "core", "x86_64-unknown-linux-gnu"),
            ("linux-x86_64", "native", "x86_64-unknown-linux-gnu"),
            ("linux-arm64", "core", "aarch64-unknown-linux-gnu"),
            ("linux-arm64", "native", "aarch64-unknown-linux-gnu"),
        ]:
            with self.subTest(target=target, flavor=flavor):
                manifest = rust_only_consumer.render_consumer_manifest(
                    "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git",
                    "a" * 40,
                    target,
                    flavor,
                )
                self.assertIn(f'git = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys.git"', manifest)
                self.assertIn(f'rev = "{"a" * 40}"', manifest)
                self.assertEqual(rust_only_consumer.coordinate(target, flavor)[0], cargo_target)
                self.assertEqual('features = ["native"]' in manifest, flavor == "native")

    def test_released_rejects_noncanonical_repository_before_public_availability(self):
        with self.assertRaisesRegex(rust_only_consumer.ProofError, "canonical"):
            rust_only_consumer.validate_released("file:///fixture", "a" * 40, "linux-x86_64", "core")

    def test_released_validates_selected_public_lock_coordinate(self):
        with tempfile.TemporaryDirectory() as temporary:
            lock = json.loads((ROOT / "artifacts.lock.json").read_text(encoding="utf-8"))
            lock["release_scope"] = "linux"
            for entry in lock["artifacts"]:
                if entry["artifact_target"].startswith("linux-"):
                    entry["url"] = "https://github.com/PulseBeamDev/pulsebeam-libwebrtc-sys/releases/download/v1/" + entry["asset_name"]
                    entry["sha256"] = "b" * 64
            path = Path(temporary) / "artifacts.lock.json"
            path.write_text(json.dumps(lock), encoding="utf-8")
            rust_only_consumer.validate_released(
                rust_only_consumer.PUBLIC_REPOSITORY, "a" * 40, "linux-x86_64", "native", path
            )
            lock["artifacts"][1]["url"] = "https://example.invalid/substitution"
            path.write_text(json.dumps(lock), encoding="utf-8")
            with self.assertRaisesRegex(rust_only_consumer.ProofError, "noncanonical"):
                rust_only_consumer.validate_released(
                    rust_only_consumer.PUBLIC_REPOSITORY, "a" * 40, "linux-x86_64", "native", path
                )

    def test_rejects_host_that_does_not_match_selection(self):
        with mock.patch.object(rust_only_consumer.platform, "system", return_value="Linux"), mock.patch.object(
            rust_only_consumer.platform, "machine", return_value="x86_64"
        ):
            with self.assertRaisesRegex(rust_only_consumer.ProofError, "does not match"):
                rust_only_consumer.require_host("linux-arm64")

    def test_modes_do_not_accept_each_others_inputs(self):
        result = subprocess.run(
            [sys.executable, str(ROOT / "tools/rust_only_consumer.py"), "released", "--repository", "x", "--revision", "a" * 40, "--target", "linux-x86_64", "--flavor", "core", "--artifact", "x"],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unrecognized arguments", result.stderr)


if __name__ == "__main__":
    unittest.main()
