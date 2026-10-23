import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

from tests.native_manifest import current_manifest
from tools import native_provenance

ROOT = Path(__file__).resolve().parents[1]


class NativeProvenanceTests(unittest.TestCase):
    def setUp(self):
        # Mutation tests must start valid: an unrelated stale hash must not
        # satisfy or mask the rejection they are supposed to exercise.
        native_provenance.validate_configuration(current_manifest(), ROOT)

    def test_producer_records_current_sources_without_refreshing_fixtures(self):
        for target in ("linux-x86_64", "android-x86_64", "android-arm64-v8a"):
            with self.subTest(target=target):
                self.produce_manifest(target)

    def produce_manifest(self, target):
        fixture = current_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "lib").mkdir()
            (root / "lib/libwebrtc.a").write_bytes(b"synthetic archive\n")
            if target.startswith("android-"):
                (root / "lib/libunwind.a").write_bytes(b"synthetic unwind\n")
            (root / "LICENSES").mkdir()
            (root / "LICENSES/NOTICE").write_text("synthetic notice\n")
            (root / "generated.h").write_text("synthetic bridge header\n")
            (root / "generated.cc").write_text("synthetic bridge source\n")
            (root / "toolchain").write_text(fixture["abi"]["toolchain"])
            (root / "defines").write_text("\n".join(fixture["native_configuration"]["cxx_defines"]))
            (root / "gn").write_text(shlex.join([
                f"{key}={value}" for key, value in fixture["native_configuration"]["gn_args"].items()
            ]))
            command = [sys.executable, str(ROOT / "tools/write_artifact_manifest.py")]
            options = {
                "flavor": "core", "target": target,
                "bridge-identity": fixture["bridge"]["identity"],
                "source-repository": fixture["sources"]["webrtc"]["repository"],
                "source-revision": fixture["sources"]["webrtc"]["revision"],
                "source-patch-sha256": fixture["sources"]["webrtc"]["patch_sha256"],
                "encoder-switch-patch-sha256": fixture["sources"]["webrtc"]["encoder_switch_patch_sha256"],
                "runtime-resources-patch-sha256": fixture["sources"]["webrtc"]["runtime_resources_patch_sha256"],
                "libvpx-repository": native_provenance.LIBVPX_SOURCE["repository"],
                "libvpx-revision": native_provenance.LIBVPX_SOURCE["revision"],
                "libvpx-worker-patch-sha256": native_provenance.LIBVPX_SOURCE["worker_patch_sha256"],
                "source-state": "pristine",
                "depot-tools-repository": fixture["sources"]["depot_tools"]["repository"],
                "depot-tools-revision": fixture["sources"]["depot_tools"]["revision"],
                "bridge-source": ROOT / "src/lib.rs", "generated-header": root / "generated.h",
                "generated-source": root / "generated.cc", "toolchain-file": root / "toolchain",
                "gn-args-file": root / "gn", "defines-file": root / "defines",
                "licenses": root / "LICENSES", "output": root / "manifest.json",
            }
            for key, value in options.items():
                command.extend([f"--{key}", str(value)])
            result = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((root / "manifest.json").read_text())
            self.assertEqual(manifest["schema_version"], 4)
            self.assertEqual(manifest["bridge"]["native_adapter_sha256"], native_provenance.adapter_digest(ROOT))
            native_provenance.validate_configuration(manifest, ROOT)
            if target.startswith("android-"):
                self.assertEqual(manifest["unwind_archive"], {
                    "member": "lib/libunwind.a", "name": "unwind",
                    "sha256": hashlib.sha256(b"synthetic unwind\n").hexdigest(),
                })
            else:
                self.assertNotIn("unwind_archive", manifest)

    def test_android_requires_exact_bundled_unwind_identity(self):
        value = current_manifest()
        value["artifact"].update(target="android-x86_64", cargo_target="x86_64-linux-android")
        value["native_configuration"].update(value["artifact"])
        value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
        valid = {"member": "lib/libunwind.a", "name": "unwind", "sha256": "1" * 64}
        for unwind in (None, {}, {**valid, "member": "../libunwind.a"}, {**valid, "sha256": "bad"}):
            with self.subTest(unwind=unwind):
                value["unwind_archive"] = unwind
                with self.assertRaisesRegex(ValueError, "Android unwind archive"):
                    native_provenance.validate_configuration(value, ROOT)
        value["unwind_archive"] = valid
        native_provenance.validate_configuration(value, ROOT)

    def test_source_edits_invalidate_existing_artifacts_not_test_inputs(self):
        for path in ("native/adapter.cc", "Justfile", "tools/native_cc_wrapper.py"):
            with self.subTest(path=path), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "native").mkdir()
                (root / "tools").mkdir()
                (root / "native/adapter.cc").write_text("original source\n")
                shutil.copyfile(ROOT / "Justfile", root / "Justfile")
                shutil.copyfile(ROOT / "tools/native_cc_wrapper.py", root / "tools/native_cc_wrapper.py")
                manifest = current_manifest(root)
                native_provenance.validate_configuration(manifest, root)
                (root / path).write_text("changed source\n")
                with self.assertRaisesRegex(ValueError, "identity mismatch"):
                    native_provenance.validate_configuration(manifest, root)
                native_provenance.validate_configuration(current_manifest(root), root)

    def test_configuration_hash_uses_sorted_utf8_json(self):
        value = {"path": "é/路径"}
        expected = hashlib.sha256('{"path":"é/路径"}'.encode()).hexdigest()
        self.assertEqual(native_provenance.configuration_digest(value), expected)

    def test_configuration_rejects_rehashed_flavor_and_gn_substitution(self):
        for field, replacement in [("flavor", "native"), ("target", "linux-arm64")]:
            value = current_manifest()
            value["native_configuration"][field] = replacement
            value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                native_provenance.validate_configuration(value, ROOT)
        value = current_manifest()
        value["native_configuration"]["gn_args"]["rtc_use_h264"] = "false"
        value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
        with self.assertRaisesRegex(ValueError, "GN configuration"):
            native_provenance.validate_configuration(value, ROOT)

    def test_child_source_pin_matches_tracked_patch_and_rejects_rehashed_substitution(self):
        self.assertEqual(
            hashlib.sha256((ROOT / "patches/runtime-libvpx-workers.patch").read_bytes()).hexdigest(),
            native_provenance.LIBVPX_SOURCE["worker_patch_sha256"])
        for field in ["repository", "revision", "worker_patch_sha256"]:
            value = current_manifest()
            value["sources"]["libvpx"][field] = "unrecognized"
            if field != "repository":
                value["native_configuration"]["libvpx_" + field] = "unrecognized"
            value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "child source"):
                native_provenance.validate_configuration(value, ROOT)
        value = current_manifest()
        del value["sources"]["libvpx"]
        with self.assertRaisesRegex(ValueError, "child source"):
            native_provenance.validate_configuration(value, ROOT)
        for field in ["libvpx_revision", "libvpx_worker_patch_sha256"]:
            value = current_manifest()
            value["native_configuration"][field] = "unrecognized"
            value["native_configuration_sha256"] = native_provenance.configuration_digest(value["native_configuration"])
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "identity mismatch"):
                native_provenance.validate_configuration(value, ROOT)

    def test_wrapper_identity_requires_the_command_not_a_digest_substring(self):
        fixture = current_manifest()
        digest = fixture["native_configuration"]["floating_point"]["compiler_wrapper_sha256"]
        for command in [
            f"unrelated --contract-sha256 {digest} sccache",
            f"false tools/native_cc_wrapper.py --contract-sha256 {digest} sccache",
            f"python tools/unrelated.py --contract-sha256 {digest} sccache",
            f"python tools/native_cc_wrapper.py --contract-sha256 {digest} sccache extra",
        ]:
            with self.subTest(command=command):
                value = current_manifest()
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
