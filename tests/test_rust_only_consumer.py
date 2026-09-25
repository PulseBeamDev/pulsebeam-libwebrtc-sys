from __future__ import annotations

import importlib.util
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tarfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("rust_only_consumer", ROOT / "tools/rust_only_consumer.py")
rust_only_consumer = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(rust_only_consumer)


class RustOnlyConsumerTests(unittest.TestCase):
    def write_mismatched_archive(self, source: Path, destination: Path, field: str, value: str) -> None:
        with tarfile.open(source, "r:gz") as input_archive, tarfile.open(destination, "w:gz") as output_archive:
            for member in input_archive.getmembers():
                content = input_archive.extractfile(member) if member.isfile() else None
                if member.name == "manifest.json":
                    assert content is not None
                    manifest = json.loads(content.read())
                    if field == "bridge_identity":
                        manifest["bridge"]["identity"] = value
                    else:
                        manifest["artifact"][field] = value
                    payload = json.dumps(manifest).encode("utf-8")
                    member.size = len(payload)
                    output_archive.addfile(member, io.BytesIO(payload))
                else:
                    output_archive.addfile(member, content)

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
                tagged = rust_only_consumer.render_consumer_manifest(
                    rust_only_consumer.PUBLIC_REPOSITORY, "v0.5.6", target, flavor, tag=True
                )
                self.assertIn('tag = "v0.5.6"', tagged)
                self.assertNotIn('rev =', tagged)
                self.assertEqual(rust_only_consumer.coordinate(target, flavor)[0], cargo_target)
                self.assertEqual('features = ["native"]' in manifest, flavor == "native")

    def test_released_rejects_noncanonical_repository_before_public_availability(self):
        with self.assertRaisesRegex(rust_only_consumer.ProofError, "canonical"):
            rust_only_consumer.validate_released("file:///fixture", "v0.5.6", "linux-x86_64", "core")

    def test_released_requires_valid_tag(self):
        rust_only_consumer.validate_released(
            rust_only_consumer.PUBLIC_REPOSITORY, "v0.5.6", "linux-x86_64", "native"
        )
        with self.assertRaisesRegex(rust_only_consumer.ProofError, "invalid release tag"):
            rust_only_consumer.validate_released(
                rust_only_consumer.PUBLIC_REPOSITORY, "a" * 40, "linux-x86_64", "native"
            )

    def test_rejects_host_that_does_not_match_selection(self):
        with mock.patch.object(rust_only_consumer.platform, "system", return_value="Linux"), mock.patch.object(
            rust_only_consumer.platform, "machine", return_value="x86_64"
        ):
            with self.assertRaisesRegex(rust_only_consumer.ProofError, "does not match"):
                rust_only_consumer.require_host("linux-arm64")

    def test_released_rejects_consumer_metadata_graph_failure(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            rust_only_consumer, "require_host"
        ), mock.patch.object(rust_only_consumer, "validate_released"), mock.patch.object(
            rust_only_consumer, "run"
        ) as run:
            def invoke(command, **_kwargs):
                if command[:2] == ["cargo", "vendor"]:
                    return subprocess.CompletedProcess(command, 0, "")
                if command[:2] == ["cargo", "metadata"]:
                    return subprocess.CompletedProcess(command, 0, '{"packages": []}')
                if command[0] == sys.executable:
                    metadata = json.loads(Path(command[-1]).read_text(encoding="utf-8"))
                    self.assertEqual(metadata, {"packages": []})
                    raise subprocess.CalledProcessError(1, command)
                self.fail(f"released proof continued after rejected metadata: {command}")

            run.side_effect = invoke
            with self.assertRaises(subprocess.CalledProcessError):
                rust_only_consumer.prove_released(
                    rust_only_consumer.PUBLIC_REPOSITORY, "v0.5.6", "linux-x86_64", "core"
                )

    def test_released_fixture_rejects_mismatched_archives(self):
        source = ROOT / "dist/webrtc-core-linux-x86_64.tar.gz"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cargo_home = ROOT / ".work/cargo-home"
            vendor = root / "registry"
            cargo_config = subprocess.run(
                ["cargo", "vendor", "--locked", str(vendor)],
                cwd=ROOT,
                env={**os.environ, "CARGO_HOME": str(cargo_home)},
                check=True,
                text=True,
                stdout=subprocess.PIPE,
            ).stdout
            for field, value, message in [
                ("bridge_identity", "wrong-bridge", "wrong bridge identity"),
                ("flavor", "native", "wrong flavor"),
                ("target", "linux-arm64", "wrong target"),
            ]:
                with self.subTest(field=field):
                    artifact = root / "fixture.tar.gz"
                    self.write_mismatched_archive(source, artifact, field, value)
                    with tarfile.open(artifact, "r:gz") as archive:
                        manifest_member = archive.getmember("manifest.json")
                        manifest_file = archive.extractfile(manifest_member)
                        assert manifest_file is not None
                        manifest = json.loads(manifest_file.read())
                    actual = manifest["bridge"]["identity"] if field == "bridge_identity" else manifest["artifact"][field]
                    self.assertEqual(actual, value)
                    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
                    with rust_only_consumer.ArtifactServer(artifact.read_bytes(), "webrtc-core-linux-x86_64.tar.gz") as server:
                        repository = root / f"repository-{field}"
                        repository.mkdir()
                        revision = rust_only_consumer.clean_snapshot(
                            repository, server.url, digest, "linux-x86_64", "core"
                        )
                        with mock.patch.dict(os.environ, {"CARGO_HOME": str(cargo_home)}):
                            rust_only_consumer.prove_released_fixture(
                                repository.resolve().as_uri(), revision, "linux-x86_64", "core", message,
                                cargo_config,
                            )

    def test_candidate_snapshot_populates_only_primary_linux(self):
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            revision = rust_only_consumer.clean_snapshot(
                repository, "http://127.0.0.1:1234/webrtc-core-linux-x86_64.tar.gz",
                "a" * 64, "linux-x86_64", "core",
            )
            self.assertRegex(revision, r"^[0-9a-f]{40}$")
            lock = json.loads((repository / "artifacts.lock.json").read_text())
            self.assertEqual(lock["release_scope"], "linux")
            self.assertEqual(
                {entry["asset_name"] for entry in lock["artifacts"] if entry["url"] is not None},
                {"webrtc-core-linux-x86_64.tar.gz", "webrtc-native-linux-x86_64.tar.gz"},
            )
            self.assertTrue(all(entry["sha256"] is None for entry in lock["artifacts"] if entry["artifact_target"] != "linux-x86_64"))
            report = json.loads((repository / "linux-audit.json").read_text())
            self.assertEqual(len(report["assets"]), 2)

    def test_modes_do_not_accept_each_others_inputs(self):
        result = subprocess.run(
            [sys.executable, str(ROOT / "tools/rust_only_consumer.py"), "released", "--repository", "x", "--tag", "v0.5.6", "--target", "linux-x86_64", "--flavor", "core", "--artifact", "x"],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unrecognized arguments", result.stderr)


if __name__ == "__main__":
    unittest.main()
