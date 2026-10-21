from __future__ import annotations

import copy
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
    def test_runtime_resource_patch_is_mandatory_and_pinned(self):
        for patch in (None, "0" * 64):
            with self.subTest(patch=patch), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"}, runtime_resources_patch=patch)
                with self.assertRaisesRegex(audit_release.AuditError, "source identity|runtime-resources patch"):
                    audit_release.audit(root, checksums, lock, "linux")

    def build_release(self, root: Path, targets: set[str] | None = None, *,
                      native_adapter: str | None = None, schema: int = 4,
                      encoder_switch_patch: str | None = audit_release.ENCODER_SWITCH_PATCH_SHA256,
                      runtime_resources_patch: str | None = audit_release.RUNTIME_RESOURCES_PATCH_SHA256,
                      license_path: str = "LICENSES/NOTICE.txt") -> tuple[Path, Path]:
        lock = json.loads((ROOT / "artifacts.lock.json").read_text(encoding="utf-8"))
        checksums = []
        license_contents = b"archive" if license_path == "lib/libwebrtc.a" else b"license\n"
        template = json.loads((ROOT / "tests/fixtures/artifact-support-manifest.json").read_text())
        entries = [
            entry
            for entry in lock["artifacts"]
            if targets is None or entry["artifact_target"] in targets
        ]
        for index, entry in enumerate(entries):
            manifest = {
                "schema_version": schema,
                "bridge": {"identity": lock["bridge_identity"], "cxx": {"version": "1"},
                           "native_adapter_sha256": native_adapter or audit_release.adapter_digest(ROOT)},
                "sources": {
                    "webrtc": {
                        "repository": "source-repository",
                        "revision": "source",
                        "patch_sha256": audit_release.CORE_IOS_PATCH_SHA256,
                        "encoder_switch_patch_sha256": encoder_switch_patch,
                        "runtime_resources_patch_sha256": runtime_resources_patch,
                        "state": "applied" if entry["flavor"] == "core" and entry["artifact_target"] in {"ios-arm64", "ios-simulator-arm64"} else "pristine",
                    },
                    "depot_tools": {"repository": "tools-repository", "revision": "tools"},
                    "libvpx": copy.deepcopy(audit_release.LIBVPX_SOURCE),
                },
                "artifact": {
                    "cargo_target": entry["cargo_target"],
                    "target": entry["artifact_target"],
                    "flavor": entry["flavor"],
                },
                "native_configuration": {"test_configuration": index},
                "native_configuration_sha256": audit_release.manifest_digest({"test_configuration": index}),
                "archive": {"member": "lib/libwebrtc.a", "sha256": hashlib.sha256(b"archive").hexdigest()},
                "licenses": {
                    "sha256": audit_release.manifest_digest(
                        [
                            {
                                "path": license_path,
                                "sha256": hashlib.sha256(license_contents).hexdigest(),
                            }
                        ]
                    ),
                    "files": [
                        {
                            "path": license_path,
                            "sha256": hashlib.sha256(license_contents).hexdigest(),
                        }
                    ],
                },
            }
            config = copy.deepcopy(template["native_configuration"])
            config.update(**manifest["artifact"], native_adapter_sha256=manifest["bridge"]["native_adapter_sha256"],
                          bridge_identity=manifest["bridge"]["identity"], cxx=manifest["bridge"]["cxx"],
                          source_revision=manifest["sources"]["webrtc"]["revision"],
                          source_patch_sha256=manifest["sources"]["webrtc"]["patch_sha256"],
                          source_encoder_switch_patch_sha256=manifest["sources"]["webrtc"]["encoder_switch_patch_sha256"],
                          source_runtime_resources_patch_sha256=manifest["sources"]["webrtc"]["runtime_resources_patch_sha256"],
                          source_state=manifest["sources"]["webrtc"]["state"],
                          depot_tools_revision=manifest["sources"]["depot_tools"]["revision"])
            manifest["abi"] = {"toolchain": config["toolchain"]}
            if entry["flavor"] == "native":
                config["gn_args"].update(rtc_include_internal_audio_device="true", rtc_use_x11="true", rtc_use_pipewire="true")
            manifest["native_configuration"] = config
            manifest["native_configuration_sha256"] = audit_release.manifest_digest(config)
            path = root / entry["asset_name"]
            with tarfile.open(path, "w:gz") as archive:
                for name, contents in (
                    ("manifest.json", json.dumps(manifest).encode()),
                    (license_path, license_contents),
                    ("lib/libwebrtc.a", b"archive"),
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

    def test_rejects_stale_adapters_and_legacy_schema_even_with_valid_checksums(self):
        for kwargs, expected in [({"native_adapter": "0" * 64}, "stale native"),
                                 ({"schema": 3}, "schema 4")]:
            with self.subTest(kwargs=kwargs), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"}, **kwargs)
                with self.assertRaisesRegex(audit_release.AuditError, expected):
                    audit_release.audit(root, checksums, lock, "linux")

    def test_rejects_missing_or_substituted_runtime_patch_with_valid_checksums(self):
        for patch in (None, "f" * 64):
            with self.subTest(patch=patch), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                checksums, lock = self.build_release(
                    root, {"linux-x86_64", "linux-arm64"}, encoder_switch_patch=patch,
                )
                with self.assertRaisesRegex(audit_release.AuditError,
                                            "source identity|encoder-switch patch"):
                    audit_release.audit(root, checksums, lock, "linux")

    def test_rejects_rehashed_license_paths_outside_the_notices_directory(self):
        for path in ["lib/libwebrtc.a", "LICENSES/../NOTICE.txt", "LICENSES//NOTICE.txt"]:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"}, license_path=path)
                with self.assertRaisesRegex(audit_release.AuditError, "license inventory path"):
                    audit_release.audit(root, checksums, lock, "linux")

    def test_rejects_duplicate_lock_selections(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checksums, lock = self.build_release(root, {"linux-x86_64", "linux-arm64"})
            value = json.loads(lock.read_text())
            value["artifacts"].append(value["artifacts"][0])
            duplicate = root / "duplicate-lock.json"
            duplicate.write_text(json.dumps(value))
            with self.assertRaisesRegex(audit_release.AuditError, "duplicate"):
                audit_release.audit(root, checksums, duplicate, "linux")

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
