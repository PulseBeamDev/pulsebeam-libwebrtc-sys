import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_consumer_metadata", ROOT / "tools/check_consumer_metadata.py"
)
check_consumer_metadata = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(check_consumer_metadata)


class ConsumerMetadataTests(unittest.TestCase):
    def metadata(self):
        packages = [
            {"id": "consumer", "name": "pulsebeam-webrtc-sys-rust-only-consumer", "targets": []},
            {"id": "sys", "name": "pulsebeam-webrtc-sys", "targets": [], "source": "git+file"},
            {
                "id": "cxx",
                "name": "cxx",
                "targets": [{"kind": ["lib"]}],
                "source": "git+file",
                "manifest_path": "/cargo/git/checkouts/repository/vendor/cxx/Cargo.toml",
            },
            {"id": "macro", "name": "cxxbridge-macro", "targets": [], "source": "registry"},
        ]
        nodes = [
            {"id": "consumer", "deps": [{"pkg": "sys"}]},
            {"id": "sys", "deps": [{"pkg": "cxx"}]},
            {"id": "cxx", "deps": [{"pkg": "macro"}]},
            {"id": "macro", "deps": []},
        ]
        return {"packages": packages, "resolve": {"nodes": nodes}}

    def test_accepts_rust_only_bridge_graph(self):
        check_consumer_metadata.validate(self.metadata())

    def test_rejects_local_cxx_custom_build_target(self):
        metadata = self.metadata()
        metadata["packages"][2]["targets"].append({"kind": ["custom-build"]})
        with self.assertRaisesRegex(check_consumer_metadata.MetadataError, "custom-build"):
            check_consumer_metadata.validate(metadata)

    def test_rejects_compiler_running_bridge_dependencies(self):
        for forbidden in ("cc", "cxx-build"):
            with self.subTest(forbidden):
                metadata = self.metadata()
                metadata["packages"].append(
                    {"id": forbidden, "name": forbidden, "targets": [], "source": "registry"}
                )
                metadata["resolve"]["nodes"].append({"id": forbidden, "deps": []})
                metadata["resolve"]["nodes"][1]["deps"].append({"pkg": forbidden})
                with self.assertRaisesRegex(
                    check_consumer_metadata.MetadataError, "forbidden native build packages"
                ):
                    check_consumer_metadata.validate(metadata)


if __name__ == "__main__":
    unittest.main()
