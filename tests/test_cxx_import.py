import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import socket
import tarfile
import tempfile
import threading
import unittest
from unittest import mock
import urllib.error


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("cxx_import", ROOT / "tools/cxx_import.py")
cxx_import = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(cxx_import)


class CxxImportTests(unittest.TestCase):
    version = "1.0.200"

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "tools/cxx").mkdir(parents=True)
        shutil.copy(ROOT / "tools/cxx/Cargo.toml.in", self.root / "tools/cxx/Cargo.toml.in")
        (self.root / "vendor/cxx").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text(
            '[dependencies]\ncxx = { version = "=1.0.200", path = "vendor/cxx" }\n'
        )
        (self.root / "Cargo.lock").write_text(
            'version = 4\n\n'
            '[[package]]\nname = "cxx"\nversion = "1.0.200"\n'
            'dependencies = ["cxxbridge-macro"]\n\n'
            '[[package]]\nname = "cxxbridge-macro"\nversion = "1.0.200"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
            'checksum = "test"\n'
        )
        (self.root / "vendor/cxx/Cargo.toml").write_bytes(
            cxx_import.render_manifest(self.root, self.version)
        )
        self.payload = {
            "LICENSE-APACHE": b"apache\n",
            "LICENSE-MIT": b"mit\n",
            "include/cxx.h": b"header\n",
            "src/cxx.cc": b"runtime\n",
            "src/lib.rs": b"rust runtime\n",
        }
        self.archive = self._archive(self.version, self.version)
        self.provenance = {
            "schema": 1,
            "package": {
                "name": "cxx",
                "registry": "https://crates.io",
                "version": self.version,
                "checksum": self._sha256(self.archive),
            },
            "generator": {
                "name": "cxxbridge-cmd",
                "registry": "https://crates.io",
                "version": self.version,
                "checksum": "1" * 64,
                "source_revision": "0" * 40,
            },
            "upstream": {
                "repository": "https://github.com/dtolnay/cxx",
                "tag": self.version,
                "commit": "0" * 40,
            },
            "licenses": [],
            "overlay": {},
            "imported_files": {},
        }
        self._write_provenance(self.provenance)
        cxx_import.refresh(self.root, self.archive)

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def _sha256(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def _write_provenance(self, provenance):
        (self.root / "vendor/cxx/provenance.json").write_text(json.dumps(provenance))

    def _archive(self, prefix_version, manifest_version):
        archive = self.root / f"cxx-{prefix_version}-{manifest_version}.crate"
        manifest = (
            f'[package]\nname = "cxx"\nversion = "{manifest_version}"\n'
            f'[dependencies]\ncxxbridge-macro = {{ version = "={manifest_version}" }}\n'
        ).encode()
        with tarfile.open(archive, "w:gz") as output:
            for relative, data in {**self.payload, "Cargo.toml.orig": manifest}.items():
                info = tarfile.TarInfo(f"cxx-{prefix_version}/{relative}")
                info.size = len(data)
                output.addfile(info, io.BytesIO(data))
        return archive

    def test_refresh_is_idempotent(self):
        before = {
            path.relative_to(self.root / "vendor/cxx"): path.read_bytes()
            for path in (self.root / "vendor/cxx").rglob("*")
            if path.is_file()
        }
        cxx_import.refresh(self.root, self.archive)
        after = {
            path.relative_to(self.root / "vendor/cxx"): path.read_bytes()
            for path in (self.root / "vendor/cxx").rglob("*")
            if path.is_file()
        }
        self.assertEqual(before, after)
        cxx_import.verify(self.root)

    def test_verify_names_altered_missing_and_unexpected_files(self):
        cases = {
            "altered": ("src/lib.rs", lambda: (self.root / "vendor/cxx/src/lib.rs").write_bytes(b"changed")),
            "missing": ("include/cxx.h", lambda: (self.root / "vendor/cxx/include/cxx.h").unlink()),
            "unexpected": ("src/extra.rs", lambda: (self.root / "vendor/cxx/src/extra.rs").write_bytes(b"extra")),
        }
        for expected, (path, mutate) in cases.items():
            with self.subTest(expected):
                cxx_import.refresh(self.root, self.archive)
                mutate()
                errors = "\n".join(cxx_import.verification_errors(self.root))
                self.assertIn(expected, errors)
                self.assertIn(path, errors)

    def test_verify_rejects_changed_generated_overlay(self):
        (self.root / "vendor/cxx/Cargo.toml").write_text("[package]\nname = 'cxx'\n")
        errors = "\n".join(cxx_import.verification_errors(self.root))
        self.assertIn("generated overlay differs", errors)

    def test_wrong_checksum_is_rejected_before_replacement(self):
        provenance = copy.deepcopy(self.provenance)
        provenance["package"]["checksum"] = "0" * 64
        self._write_provenance(provenance)
        marker = self.root / "vendor/cxx/src/lib.rs"
        marker.write_bytes(b"preserve me")
        with self.assertRaisesRegex(cxx_import.ImportError, "checksum mismatch"):
            cxx_import.refresh(self.root, self.archive)
        self.assertEqual(marker.read_bytes(), b"preserve me")

    def test_inconsistent_component_version_is_rejected_before_replacement(self):
        archive = self._archive(self.version, "1.0.199")
        provenance = copy.deepcopy(self.provenance)
        provenance["package"]["checksum"] = self._sha256(archive)
        self._write_provenance(provenance)
        marker = self.root / "vendor/cxx/src/lib.rs"
        marker.write_bytes(b"preserve me")
        with self.assertRaisesRegex(cxx_import.ImportError, "version disagrees"):
            cxx_import.refresh(self.root, archive)
        self.assertEqual(marker.read_bytes(), b"preserve me")

    def test_substituted_generator_version_is_rejected(self):
        install_root = self.root / "generator"
        binary = cxx_import.generator_binary(install_root)
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/bin/sh\nprintf 'cxxbridge 1.0.199\\n'\n")
        binary.chmod(0o755)
        (install_root / cxx_import.GENERATOR_STAMP).write_text(
            json.dumps(self.provenance["generator"])
        )
        with self.assertRaisesRegex(cxx_import.ImportError, "version must be"):
            cxx_import.verify_generator(self.root, install_root)

    def test_wrong_generator_package_checksum_is_rejected(self):
        archive = self.root / "cxxbridge-cmd.crate"
        archive.write_bytes(b"not the recorded package")
        with self.assertRaisesRegex(cxx_import.ImportError, "generator package checksum mismatch"):
            cxx_import.unpack_generator(
                archive, self.root / "unpacked", self.provenance["generator"]
            )

    def test_download_reuses_verified_cache_without_network(self):
        archive = cxx_import.cached_archive(self.root, self.provenance["package"])
        archive.parent.mkdir(parents=True)
        shutil.copy(self.archive, archive)
        with mock.patch.object(cxx_import.urllib.request, "urlopen") as urlopen:
            self.assertEqual(cxx_import.download(self.root, self.provenance["package"]), archive)
        urlopen.assert_not_called()

    def test_download_replaces_corrupt_cache_after_transient_failure(self):
        archive = cxx_import.cached_archive(self.root, self.provenance["package"])
        archive.parent.mkdir(parents=True)
        archive.write_bytes(b"corrupt")
        response = mock.MagicMock()
        response.read.return_value = self.archive.read_bytes()
        response.__enter__.return_value = response
        with (
            mock.patch.object(
                cxx_import.urllib.request,
                "urlopen",
                side_effect=[socket.timeout("temporary"), response],
            ) as urlopen,
            mock.patch.object(cxx_import.time, "sleep"),
        ):
            self.assertEqual(cxx_import.download(self.root, self.provenance["package"]), archive)
        self.assertEqual(urlopen.call_count, 2)
        self.assertEqual(archive.read_bytes(), self.archive.read_bytes())

    def test_download_does_not_retry_permanent_or_checksum_failures(self):
        permanent = urllib.error.HTTPError("https://example.invalid", 404, "missing", {}, None)
        with mock.patch.object(cxx_import.urllib.request, "urlopen", side_effect=permanent) as urlopen:
            with self.assertRaisesRegex(cxx_import.ImportError, "deterministic-retrieval"):
                cxx_import.download(self.root, self.provenance["package"])
        permanent.close()
        urlopen.assert_called_once()

        response = mock.MagicMock()
        response.read.return_value = b"wrong"
        response.__enter__.return_value = response
        with mock.patch.object(cxx_import.urllib.request, "urlopen", return_value=response) as urlopen:
            with self.assertRaisesRegex(cxx_import.ImportError, "checksum"):
                cxx_import.download(self.root, self.provenance["package"])
        urlopen.assert_called_once()
        archive = cxx_import.cached_archive(self.root, self.provenance["package"])
        self.assertFalse(archive.exists())
        self.assertEqual(list(archive.parent.glob("*.tmp")), [])

    def test_download_exhausts_transient_failures_and_concurrent_success_converges(self):
        with (
            mock.patch.object(
                cxx_import.urllib.request,
                "urlopen",
                side_effect=socket.timeout("temporary"),
            ) as urlopen,
            mock.patch.object(cxx_import.time, "sleep"),
        ):
            with self.assertRaisesRegex(cxx_import.ImportError, "transient-transport-exhausted"):
                cxx_import.download(self.root, self.provenance["package"])
        self.assertEqual(urlopen.call_count, cxx_import.DOWNLOAD_ATTEMPTS)

        barrier = threading.Barrier(2)
        def response_for_concurrent_download(*_args, **_kwargs):
            barrier.wait()
            response = mock.MagicMock()
            response.read.return_value = self.archive.read_bytes()
            response.__enter__.return_value = response
            return response

        errors = []
        with mock.patch.object(cxx_import.urllib.request, "urlopen", side_effect=response_for_concurrent_download):
            threads = [threading.Thread(target=lambda: self._download_in_thread(errors)) for _ in range(2)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
        self.assertEqual(errors, [])
        archive = cxx_import.cached_archive(self.root, self.provenance["package"])
        self.assertEqual(archive.read_bytes(), self.archive.read_bytes())
        self.assertEqual(list(archive.parent.glob("*.tmp")), [])

    def _download_in_thread(self, errors):
        try:
            cxx_import.download(self.root, self.provenance["package"])
        except Exception as error:
            errors.append(error)


if __name__ == "__main__":
    unittest.main()
