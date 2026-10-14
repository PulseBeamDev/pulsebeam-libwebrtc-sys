import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from tools.native_compile_commands import compilation_commands


class NativeCompileCommandsTests(unittest.TestCase):
    def fixture(self, base, flavor="core", target="linux-x86_64"):
        root, kit, checkout, overlay = (base / name for name in ("root space", "kit space", "checkout space", "overlay"))
        for name in ("native/value.cc", "native/value.h", "consumer/smoke.cc", "src/lib.rs"):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("// source\n")
        for name in ("include/rust/cxx.h", "include/pulsebeam-webrtc-sys/src/lib.rs.h"):
            path = kit / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.touch()
        (kit / "include/c++/v1").mkdir(parents=True)
        (kit / "build.txt").write_text("cxx_defines=-DWEBRTC_LINUX -DNDEBUG\n")
        (kit / "manifest.json").write_text(json.dumps({"artifact": {"target": target, "flavor": flavor},
            "bridge": {"cxx": {"bridge_source_sha256": hashlib.sha256((root / "src/lib.rs").read_bytes()).hexdigest(),
                                 "generated_header_sha256": hashlib.sha256((kit / "include/pulsebeam-webrtc-sys/src/lib.rs.h").read_bytes()).hexdigest()}}}))
        compiler = checkout / "third_party/llvm-build/Release+Asserts/bin/clang++"
        compiler.parent.mkdir(parents=True)
        compiler.touch()
        sysroot = checkout / "build/linux/debian_bullseye_amd64-sysroot"
        sysroot.mkdir(parents=True)
        if flavor == "native":
            (sysroot / "usr/include/glib-2.0").mkdir(parents=True)
            (sysroot / "usr/lib/x86_64-linux-gnu/glib-2.0/include").mkdir(parents=True)
        return root, kit, checkout, overlay

    def test_uses_pinned_inputs_exported_defines_and_argument_arrays(self):
        with tempfile.TemporaryDirectory() as directory:
            root, kit, checkout, overlay = self.fixture(Path(directory))
            commands = compilation_commands(root, kit, checkout, overlay)
            self.assertEqual({Path(entry["file"]).name for entry in commands}, {"value.cc", "value.h", "smoke.cc"})
            for entry in commands:
                args = entry["arguments"]
                self.assertEqual(entry["directory"], str(root))
                self.assertEqual(args[0], str(checkout / "third_party/llvm-build/Release+Asserts/bin/clang++"))
                self.assertIn(f"--sysroot={checkout}/build/linux/debian_bullseye_amd64-sysroot", args)
                self.assertIn("-nostdinc++", args)
                self.assertIn(str(kit / "include/c++/v1"), args)
                self.assertIn("-DWEBRTC_LINUX", args)
                self.assertLess(args.index(f"-I{overlay}"), args.index(f"-I{kit}/include"))
                self.assertNotIn("-DPULSEBEAM_WEBRTC_NATIVE_AUDIO=1", args)
                self.assertEqual(args[-4:], ["-x", "c++", "-c", entry["file"]])

    def test_native_flavor_adds_only_actual_native_audio_and_glib_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            args = compilation_commands(*self.fixture(Path(directory), flavor="native"))[0]["arguments"]
            self.assertIn("-DPULSEBEAM_WEBRTC_NATIVE_AUDIO=1", args)
            self.assertTrue(any(value.endswith("usr/include/glib-2.0") for value in args))
            self.assertTrue(any(value.endswith("usr/lib/x86_64-linux-gnu/glib-2.0/include") for value in args))

    def test_rejects_stale_bridge_and_unsupported_target(self):
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.fixture(Path(directory))
            (inputs[0] / "src/lib.rs").write_text("// changed\n")
            with self.assertRaisesRegex(ValueError, "does not match"):
                compilation_commands(*inputs)
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "supports Linux x86_64"):
                compilation_commands(*self.fixture(Path(directory), target="linux-arm64"))

    def test_rejects_altered_generated_header_and_missing_native_glib(self):
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.fixture(Path(directory))
            (inputs[1] / "include/pulsebeam-webrtc-sys/src/lib.rs.h").write_text("// altered\n")
            with self.assertRaisesRegex(ValueError, "header digest"):
                compilation_commands(*inputs)
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.fixture(Path(directory), flavor="native")
            (inputs[2] / "build/linux/debian_bullseye_amd64-sysroot/usr/include/glib-2.0").rmdir()
            with self.assertRaisesRegex(ValueError, "missing pinned.*glib"):
                compilation_commands(*inputs)

    def test_rejects_missing_sdk_input_and_non_definition_argument(self):
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.fixture(Path(directory))
            (inputs[1] / "include/rust/cxx.h").unlink()
            with self.assertRaisesRegex(ValueError, "missing pinned"):
                compilation_commands(*inputs)
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.fixture(Path(directory))
            (inputs[1] / "build.txt").write_text("cxx_defines=-I/host/include\n")
            with self.assertRaisesRegex(ValueError, "exported -D"):
                compilation_commands(*inputs)


if __name__ == "__main__":
    unittest.main()
