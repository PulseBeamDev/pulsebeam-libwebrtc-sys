import base64
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SCANNER = ROOT / "tools/check_tracked_payloads.py"


def tar_bytes(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for name, data in entries.items():
            item = tarfile.TarInfo(name)
            item.size = len(data)
            archive.addfile(item, io.BytesIO(data))
    return output.getvalue()


def zip_bytes(entries):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        for name, data in entries.items():
            archive.writestr(name, data)
    return output.getvalue()


class TrackedPayloadTests(unittest.TestCase):
    def scan(self, tracked, untracked=None):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            subprocess.run(["git", "init", "--quiet"], cwd=repo, check=True)
            for name, data in tracked.items():
                target = repo / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            for name, data in (untracked or {}).items():
                target = repo / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            if untracked:
                for name in untracked:
                    subprocess.run(["git", "rm", "--cached", "--quiet", name], cwd=repo, check=True)
            return subprocess.run(["python3", str(SCANNER)], cwd=repo, text=True, capture_output=True)

    def assert_rejected(self, tracked):
        result = self.scan(tracked)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("tracked native payload", result.stderr)

    def test_rejects_fixture_shape_and_renamed_tarball(self):
        payload = tar_bytes({"manifest.json": b"{}", "lib/libwebrtc.a": b"!<arch>\n"})
        self.assert_rejected({"tests/fixtures/webrtc-core-linux-x86_64.tar.gz": payload})
        self.assert_rejected({"nothing-to-see": payload})

    def test_rejects_direct_and_nested_native_payloads(self):
        self.assert_rejected({"source-data": b"\x7fELFcompiled"})
        nested = zip_bytes({"inner.tar.gz": tar_bytes({"lib/object.o": b"!<arch>\n"})})
        self.assert_rejected({"ordinary-data": nested})

    def test_rejects_base64_payload(self):
        self.assert_rejected({"encoded.txt": base64.b64encode(b"!<arch>\n")})

    def test_allows_source_and_untracked_artifact(self):
        result = self.scan({"fixture.json": b'{"source": true}', "README": b"ordinary source"}, {"dist/external": b"\x7fELFcompiled"})
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
