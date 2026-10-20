from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

from tools import audit_release
from tools.release_inventory import OS_TARGETS
from tests.test_release_audit import ReleaseAuditTests

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("write_artifact_lock", ROOT / "tools/write_artifact_lock.py")
write_artifact_lock = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(write_artifact_lock)


class ArtifactLockTests(unittest.TestCase):
    def test_checked_in_lock_is_an_unpublished_selection_template(self):
        lock = write_artifact_lock.template(ROOT / "artifacts.lock.json")
        self.assertEqual(lock["release_scope"], "none")
        self.assertEqual(len(lock["artifacts"]), 18)
        self.assertTrue(all(entry["url"] is None and entry["sha256"] is None for entry in lock["artifacts"]))

    def report(self, root: Path, scope: str) -> Path:
        checksums, lock = ReleaseAuditTests().build_release(root, OS_TARGETS.get(scope))
        report = audit_release.audit(root, checksums, lock, scope)
        path = root / "audit.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        return path

    def render_report(self, root: Path, scope: str) -> dict:
        return json.loads(write_artifact_lock.render(ROOT / "artifacts.lock.json", None, f"v1.2.3-{scope}", self.report(root, scope)))

    def test_each_os_audit_renders_exactly_its_architectures_and_flavors(self):
        for scope, targets in OS_TARGETS.items():
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as temporary:
                lock = self.render_report(Path(temporary), scope)
                self.assertEqual(lock["schema_version"], 2)
                self.assertEqual(lock["release_scope"], scope)
                self.assertEqual(sum(item["url"] is not None for item in lock["artifacts"]), len(targets) * 2)
                for item in lock["artifacts"]:
                    self.assertEqual(item["url"] is not None, item["artifact_target"] in targets)
                    if item["url"]:
                        self.assertEqual(item["url"], f"{write_artifact_lock.BASE_URL}/v1.2.3-{scope}/{item['asset_name']}")

    def test_os_audit_rejects_root_and_foreign_os_tags(self):
        for scope in OS_TARGETS:
            with self.subTest(scope=scope), tempfile.TemporaryDirectory() as temporary:
                report = self.report(Path(temporary), scope)
                for tag in ("v1.2.3", "v1.2.3-other", "v1.2.3-linux-linux"):
                    with self.assertRaises(write_artifact_lock.LockError):
                        write_artifact_lock.render(ROOT / "artifacts.lock.json", None, tag, report)

    def test_closed_complete_audit_and_checksum_compatibility_are_complete(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lock = self.render_report(root, "complete")
            self.assertEqual(lock["release_scope"], "complete")
            self.assertEqual(sum(item["url"] is not None for item in lock["artifacts"]), 18)
            checksums = root / "SHA256SUMS"
            checksums.write_text("".join(f"{'1' * 64}  {item['asset_name']}\n" for item in lock["artifacts"]), encoding="utf-8")
            legacy = json.loads(write_artifact_lock.render(ROOT / "artifacts.lock.json", checksums, "webrtc-bridge-v1"))
            self.assertEqual(legacy["release_scope"], "complete")

    def test_rejects_incomplete_or_inconsistent_audit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = json.loads(self.report(root, "linux").read_text())
            report["assets"].pop()
            bad = root / "bad.json"; bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "asset set mismatch"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)
            report = json.loads(self.report(root, "linux").read_text())
            report["assets"].append(report["assets"][0])
            bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "duplicate"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)
            report = json.loads(self.report(root, "linux").read_text())
            report["bridge"]["identity"] = "wrong"
            bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "bridge"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)

    def test_rejects_malformed_checksums_and_does_not_fill_linux(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); checksums = root / "SHA256SUMS"
            checksums.write_text("not-a-hash  asset\n", encoding="utf-8")
            with self.assertRaisesRegex(write_artifact_lock.LockError, "malformed"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", checksums, "tag")
            linux = self.render_report(root, "linux")
            self.assertEqual(sum(item["url"] is not None for item in linux["artifacts"]), 4)

    def test_rejects_noncanonical_template_and_weakened_linux_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            template = json.loads((ROOT / "artifacts.lock.json").read_text())
            template["artifacts"][0]["cargo_target"] = "not-a-supported-target"
            template_path = root / "template.json"
            template_path.write_text(json.dumps(template))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "noncanonical"):
                write_artifact_lock.render(template_path, None, "tag", self.report(root, "linux"))

            report = json.loads(self.report(root, "linux").read_text())
            report["sources"]["depot_tools"]["repository"] = ""
            bad = root / "bad.json"
            bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "source identity"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)

            report = json.loads(self.report(root, "linux").read_text())
            report["assets"][1]["native_configuration_sha256"] = report["assets"][0]["native_configuration_sha256"]
            bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "duplicate native configuration"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)

            report = json.loads(self.report(root, "linux").read_text())
            report["assets"][0]["source_state"] = "applied"
            bad.write_text(json.dumps(report))
            with self.assertRaisesRegex(write_artifact_lock.LockError, "inconsistent audit source"):
                write_artifact_lock.render(ROOT / "artifacts.lock.json", None, "tag", bad)

if __name__ == "__main__": unittest.main()
