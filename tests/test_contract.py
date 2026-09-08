from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from webrtc.contract import FULL_SHA, REF, ContractError, patch_entries, recipe_hash, target_config
from webrtc.patches import apply_all
from webrtc.runner import run


class ContractTests(unittest.TestCase):
    def fixture(self) -> Path:
        root = Path(tempfile.mkdtemp())
        (root / "patches").mkdir()
        (root / "tools").mkdir()
        (root / "tests").mkdir()
        for name in ("Justfile",):
            (root / name).write_text(name)
        (root / "config").mkdir()
        for name in ("toolchains.lock.json", "targets.json"):
            (root / "config" / name).write_text("{}")
        (root / "upstream.lock.json").write_text('{"identity":null,"recipe_sha256":null}')
        (root / "patches/series").write_text("")
        (root / "patches/manifest.json").write_text('{"schema_version":1,"patches":{}}')
        return root

    def test_manifest_series_files_must_agree(self):
        root = self.fixture()
        (root / "patches/series").write_text("one.patch\n")
        with self.assertRaisesRegex(ContractError, "agree one-to-one"):
            patch_entries(root)

    def test_patch_and_toolchain_change_invalidate_digest(self):
        root = self.fixture()
        before = recipe_hash(root)
        (root / "config/toolchains.lock.json").write_text('{"changed":true}')
        after_tool = recipe_hash(root)
        self.assertNotEqual(before, after_tool)
        data = b"diff --git a/a b/a\n"
        (root / "patches/x.patch").write_bytes(data)
        (root / "patches/series").write_text("x.patch\n")
        meta = {"schema_version": 1, "patches": {"x.patch": {
            "checkout": "src", "base_sha": "0" * 40, "rationale": "test",
            "sha256": hashlib.sha256(data).hexdigest()}}}
        (root / "patches/manifest.json").write_text(json.dumps(meta))
        self.assertNotEqual(after_tool, recipe_hash(root))

    def test_unknown_and_unsupported_inputs(self):
        with self.assertRaisesRegex(ContractError, "unknown target"):
            target_config("nope", "production")
        with self.assertRaisesRegex(ContractError, "reserved but not implemented"):
            target_config("linux-arm64", "production")
        with self.assertRaisesRegex(ContractError, "unknown profile"):
            target_config("linux-x86_64", "nope")

    def test_malformed_refs_and_truncated_shas_are_rejected(self):
        self.assertIsNone(REF.fullmatch("m150_release"))
        self.assertIsNone(FULL_SHA.fullmatch("030ad13afd0a"))

    def test_subprocess_failure_is_actionable(self):
        with self.assertRaisesRegex(ContractError, "command failed"):
            run(["sh", "-c", "exit 7"])

    def test_strict_order_and_conflict_removes_checkout(self):
        root = self.fixture()
        checkout = root / "checkout"
        src = checkout / "src"
        src.mkdir(parents=True)
        subprocess.run(["git", "init", "-q"], cwd=src, check=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=src, check=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=src, check=True)
        (src / "value").write_text("base\n")
        subprocess.run(["git", "add", "value"], cwd=src, check=True)
        subprocess.run(["git", "commit", "-qm", "base"], cwd=src, check=True)
        base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=src, text=True).strip()
        p1 = b"diff --git a/value b/value\n--- a/value\n+++ b/value\n@@ -1 +1 @@\n-base\n+first\n"
        p2 = b"diff --git a/value b/value\n--- a/value\n+++ b/value\n@@ -1 +1 @@\n-base\n+second\n"
        manifest = {"schema_version": 1, "patches": {}}
        for name, data in (("01.patch", p1), ("02.patch", p2)):
            (root / "patches" / name).write_bytes(data)
            manifest["patches"][name] = {"checkout": "src", "base_sha": base,
                "rationale": "ordering test", "sha256": hashlib.sha256(data).hexdigest()}
        (root / "patches/series").write_text("01.patch\n02.patch\n")
        (root / "patches/manifest.json").write_text(json.dumps(manifest))
        with self.assertRaises(ContractError):
            apply_all(checkout, root)
        self.assertFalse(checkout.exists())


if __name__ == "__main__":
    unittest.main()
