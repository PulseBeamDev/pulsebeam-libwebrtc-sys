import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "cxx_provenance", ROOT / "tools/cxx_provenance.py"
)
cxx_provenance = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(cxx_provenance)


class CxxProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.generated_header = self.root / "lib.rs.h"
        self.generated_source = self.root / "lib.rs.cc"
        self.generated_header.write_bytes(b"generated header\n")
        self.generated_source.write_bytes(b"generated source\n")

    def tearDown(self):
        self.temporary.cleanup()

    def provenance(self):
        return cxx_provenance.artifact_provenance(
            ROOT, ROOT / "src/lib.rs", self.generated_header, self.generated_source
        )

    def test_identical_generation_outputs_have_identical_digests(self):
        first = self.provenance()
        second = self.provenance()
        self.assertEqual(first["generated_header_sha256"], second["generated_header_sha256"])
        self.assertEqual(first["generated_source_sha256"], second["generated_source_sha256"])

    def test_rejects_missing_malformed_and_inconsistent_fields(self):
        valid = self.provenance()
        cases = []
        missing = copy.deepcopy(valid)
        del missing["generated_source_sha256"]
        cases.append(missing)
        malformed = copy.deepcopy(valid)
        malformed["generated_source_sha256"] = "bad"
        cases.append(malformed)
        inconsistent = copy.deepcopy(valid)
        inconsistent["runtime_package_sha256"] = "0" * 64
        cases.append(inconsistent)
        for value in cases:
            with self.subTest(value=value):
                with self.assertRaises(cxx_provenance.ProvenanceError):
                    cxx_provenance.validate_artifact_provenance(value, ROOT)

    def test_archive_inspection_rejects_altered_packaged_header(self):
        artifact = self.root / "artifact"
        (artifact / "include/rust").mkdir(parents=True)
        generated = artifact / "include/pulsebeam-webrtc-sys/src/lib.rs.h"
        generated.parent.mkdir(parents=True)
        shutil.copy(ROOT / "vendor/cxx/include/cxx.h", artifact / "include/rust/cxx.h")
        shutil.copy(self.generated_header, generated)
        provenance = self.provenance()
        cxx_provenance.validate_artifact_provenance(provenance, ROOT, artifact)
        generated.write_bytes(b"altered\n")
        with self.assertRaisesRegex(cxx_provenance.ProvenanceError, "disagrees"):
            cxx_provenance.validate_artifact_provenance(provenance, ROOT, artifact)


if __name__ == "__main__":
    unittest.main()
