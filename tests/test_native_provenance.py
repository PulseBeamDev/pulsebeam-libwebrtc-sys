import hashlib
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
        self.assertEqual(fixture["schema_version"], 4)
        self.assertEqual(fixture["bridge"]["native_adapter_sha256"], native_provenance.adapter_digest(ROOT))

    def test_configuration_hash_uses_sorted_utf8_json(self):
        value = {"path": "é/路径"}
        expected = hashlib.sha256('{"path":"é/路径"}'.encode()).hexdigest()
        self.assertEqual(native_provenance.configuration_digest(value), expected)

    def test_configuration_rejects_rehashed_flavor_and_gn_substitution(self):
        path = ROOT / "tests/fixtures/artifact-support-manifest.json"
        for field, replacement in [("flavor", "native"), ("target", "linux-arm64")]:
            value = json.loads(path.read_text())
            value["native_configuration"][field] = replacement
            value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                native_provenance.validate_configuration(value, ROOT)
        value = json.loads(path.read_text())
        value["native_configuration"]["gn_args"]["rtc_use_h264"] = "false"
        value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
        with self.assertRaisesRegex(ValueError, "GN configuration"):
            native_provenance.validate_configuration(value, ROOT)

    def test_wrapper_identity_requires_the_command_not_a_digest_substring(self):
        path = ROOT / "tests/fixtures/artifact-support-manifest.json"
        fixture = json.loads(path.read_text())
        digest = fixture["native_configuration"]["floating_point"]["compiler_wrapper_sha256"]
        for command in [
            f"unrelated --contract-sha256 {digest} sccache",
            f"false tools/native_cc_wrapper.py --contract-sha256 {digest} sccache",
            f"python tools/unrelated.py --contract-sha256 {digest} sccache",
            f"python tools/native_cc_wrapper.py --contract-sha256 {digest} sccache extra",
        ]:
            with self.subTest(command=command):
                value = json.loads(path.read_text())
                value["native_configuration"]["gn_args"]["cc_wrapper"] = command
                value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
                with self.assertRaisesRegex(ValueError, "floating-point"):
                    native_provenance.validate_configuration(value, ROOT)
        fixture["native_configuration"]["gn_args"]["cc_wrapper"] = (
            f"'/host tools/bin/python3.13' '/relocated source/tools/native_cc_wrapper.py' "
            f"--contract-sha256 {digest} '/cache tools/sccache'")
        fixture["native_configuration_sha256"] = native_provenance.configuration_digest(fixture["native_configuration"])
        native_provenance.validate_configuration(fixture, ROOT)

    def test_identity_is_order_and_root_independent_but_path_and_byte_sensitive(self):
        with tempfile.TemporaryDirectory() as temporary:
            first, second = Path(temporary) / "a", Path(temporary) / "b"
            for root, order in [(first, ["z.h", "a.cc"]), (second, ["a.cc", "z.h"])]:
                (root / "native").mkdir(parents=True)
                (root / "Justfile").write_bytes(b"compile recipe\n")
                (root / "tools").mkdir()
                (root / "tools/native_cc_wrapper.py").write_bytes(b"compiler wrapper\n")
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
            (second / "tools/native_cc_wrapper.py").write_bytes(b"changed wrapper\n")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))
            (second / "tools/native_cc_wrapper.py").write_bytes(b"compiler wrapper\n")
            # Generated manifests must not participate in their own identity.
            (second / "native/manifest.json").write_bytes(b"not a source")
            self.assertEqual(original, native_provenance.adapter_digest(second))
            (second / "native/new.h").write_bytes(b"new header\n")
            self.assertNotEqual(original, native_provenance.adapter_digest(second))


if __name__ == "__main__":
    unittest.main()
