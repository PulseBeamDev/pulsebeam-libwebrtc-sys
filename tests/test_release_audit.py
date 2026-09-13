from __future__ import annotations

import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from tools import audit_release


ROOT = Path(__file__).resolve().parents[1]


class ReleaseAuditTests(unittest.TestCase):
    def build_release(self, root: Path, targets: set[str] | None = None) -> tuple[Path, Path]:
        lock = json.loads((ROOT / "artifacts.lock.json").read_text(encoding="utf-8"))
        checksums = []
        license_contents = b"license\n"
        entries = [
            entry
            for entry in lock["artifacts"]
            if targets is None or entry["artifact_target"] in targets
        ]
        for index, entry in enumerate(entries):
            manifest = {
                "bridge": {"identity": lock["bridge_identity"], "cxx": {"version": "1"}},
                "sources": {
                    "webrtc": {
                        "repository": "source-repository",
                        "revision": "source",
                        "patch_sha256": audit_release.CORE_IOS_PATCH_SHA256,
                        "state": "applied" if entry["flavor"] == "core" and entry["artifact_target"] in {"ios-arm64", "ios-simulator-arm64"} else "pristine",
                    },
                    "depot_tools": {"revision": "tools"},
                },
                "artifact": {
                    "cargo_target": entry["cargo_target"],
                    "target": entry["artifact_target"],
                    "flavor": entry["flavor"],
                },
                "native_configuration_sha256": f"{index:064x}",
                "licenses": {
                    "sha256": audit_release.manifest_digest(
                        [
                            {
                                "path": "LICENSES/NOTICE.txt",
                                "sha256": hashlib.sha256(license_contents).hexdigest(),
                            }
                        ]
                    ),
                    "files": [
                        {
                            "path": "LICENSES/NOTICE.txt",
                            "sha256": hashlib.sha256(license_contents).hexdigest(),
                        }
                    ],
                },
            }
            path = root / entry["asset_name"]
            with tarfile.open(path, "w:gz") as archive:
                for name, contents in (
                    ("manifest.json", json.dumps(manifest).encode()),
                    ("LICENSES/NOTICE.txt", license_contents),
                ):
                    member = tarfile.TarInfo(name)
                    member.size = len(contents)
                    archive.addfile(member, io.BytesIO(contents))
            checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
        checksum_path = root / "SHA256SUMS"
        checksum_path.write_text("".join(checksums), encoding="utf-8")
        return checksum_path, ROOT / "artifacts.lock.json"

    def test_accepts_only_complete_consistent_matrix(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root)
            report = audit_release.audit(root, checksums, lock)
            self.assertEqual(len(report["assets"]), 18)
            self.assertEqual(report["scope"], "complete")
            self.assertEqual(report["bridge"]["identity"], "pulsebeam-webrtc-sys-bridge-v2")
            self.assertEqual(
                sum(asset["source_state"] == "applied" for asset in report["assets"]), 2
            )

    def test_accepts_exact_linux_scope_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            report = audit_release.audit(root, checksums, lock, "linux")
            self.assertEqual(report["scope"], "linux")
            self.assertEqual(len(report["assets"]), 4)

    def test_rejects_missing_extra_or_substituted_linux_assets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            (root / "webrtc-core-linux-x86_64.tar.gz").unlink()
            with self.assertRaisesRegex(audit_release.AuditError, "asset sets differ"):
                audit_release.audit(root, checksums, lock, "linux")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            (root / "webrtc-core-windows-x86_64.tar.gz").write_bytes(b"not an archive")
            with self.assertRaisesRegex(audit_release.AuditError, "asset sets differ"):
                audit_release.audit(root, checksums, lock, "linux")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            lines = checksums.read_text(encoding="utf-8").splitlines()
            lines[0] = lines[0].replace("webrtc-core-linux-x86_64.tar.gz", "webrtc-core-windows-x86_64.tar.gz")
            checksums.write_text("\n".join(lines) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(audit_release.AuditError, "asset sets differ"):
                audit_release.audit(root, checksums, lock, "linux")

    def test_linux_assets_cannot_pass_complete_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            with self.assertRaisesRegex(audit_release.AuditError, "asset sets differ"):
                audit_release.audit(root, checksums, lock)

    def test_rejects_a_checksum_substitution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root)
            lines = checksums.read_text(encoding="utf-8").splitlines()
            lines[0] = f"{'0' * 64}  {lines[0].split()[1]}"
            checksums.write_text("\n".join(lines) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(audit_release.AuditError, "checksum mismatch"):
                audit_release.audit(root, checksums, lock)


if __name__ == "__main__":
    unittest.main()
