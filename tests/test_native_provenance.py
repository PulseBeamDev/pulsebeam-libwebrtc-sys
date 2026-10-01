import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_provenance", ROOT / "tools/native_provenance.py")
native_provenance = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(native_provenance)


class NativeProvenanceTests(unittest.TestCase):
    def test_fixture_agrees_with_current_sources_and_build_recipe(self):
        fixture = json.loads((ROOT / "native/manifest.core-linux-x86_64.json").read_text())
        self.assertEqual(fixture["schema_version"], 3)
        self.assertEqual(fixture["bridge"]["native_adapter_sha256"], native_provenance.adapter_digest(ROOT))

    def test_identity_is_order_and_root_independent_but_path_and_byte_sensitive(self):
        with tempfile.TemporaryDirectory() as temporary:
            first, second = Path(temporary) / "a", Path(temporary) / "b"
            for root, order in [(first, ["z.h", "a.cc"]), (second, ["a.cc", "z.h"])]:
                (root / "native").mkdir(parents=True)
                (root / "Justfile").write_bytes(b"compile recipe\n")
                for name in order:
                    (root / "native" / name).write_bytes(b"same bytes\n")
            original = native_provenance.adapter_digest(first)
            self.assertEqual(original, native_provenance.adapter_digest(second))
            (second / "native/a.cc").rename(second / "native/b.cc")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))
            (second / "native/b.cc").rename(second / "native/a.cc")
            (second / "native/a.cc").write_bytes(b"different bytes\n")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))
            (second / "native/a.cc").write_bytes(b"same bytes\n")
            (second / "Justfile").write_bytes(b"different recipe\n")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))
            (second / "Justfile").write_bytes(b"compile recipe\n")
            # Generated manifests must not participate in their own identity.
            (second / "native/manifest.json").write_bytes(b"not a source")
            self.assertEqual(original, native_provenance.adapter_digest(second))
            (second / "native/new.h").write_bytes(b"new header\n")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))


if __name__ == "__main__":
    unittest.main()
