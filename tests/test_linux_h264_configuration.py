from __future__ import annotations

import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LinuxH264ConfigurationTests(unittest.TestCase):
    def gn_args(self, flavor: str, target: str, protobuf: str = "true") -> dict[str, str]:
        with tempfile.TemporaryDirectory() as temporary:
            # The recipe needs the wrapper's path, not a compiler or SDK checkout.
            wrapper = Path(temporary) / "sccache"
            wrapper.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
            wrapper.chmod(0o755)
            environment = {**os.environ, "PATH": f"{temporary}:{os.environ['PATH']}"}
            environment.pop("PULSEBEAM_WEBRTC_SANITIZER", None)
            environment["PULSEBEAM_WEBRTC_PROTOBUF"] = protobuf
            result = subprocess.run(
                ["just", "--justfile", str(ROOT / "Justfile"), "_gn-args", flavor, target],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            arguments = shlex.split(result.stdout)
            keys = [argument.split("=", 1)[0] for argument in arguments]
            self.assertEqual(len(keys), len(set(keys)), "GN arguments must not override one another")
            return dict(argument.split("=", 1) for argument in arguments)

    def test_both_linux_x86_64_flavors_bundle_real_h264_dependencies(self):
        for flavor in ("core", "native"):
            with self.subTest(flavor=flavor):
                arguments = self.gn_args(flavor, "linux-x86_64")
                self.assertEqual(arguments["rtc_use_h264"], "true")
                self.assertEqual(arguments["proprietary_codecs"], "true")
                self.assertEqual(arguments["ffmpeg_branding"], "Chrome")
                self.assertEqual(arguments["rtc_system_openh264"], "false")
                self.assertEqual(arguments["rtc_include_internal_audio_device"],
                                 "false" if flavor == "core" else "true")
                self.assertEqual(arguments["rtc_use_x11"], "false" if flavor == "core" else "true")
                compiler_wrapper = shlex.split(arguments["cc_wrapper"])
                self.assertEqual(len(compiler_wrapper), 5)
                self.assertEqual(compiler_wrapper[1], str(ROOT / "tools/native_cc_wrapper.py"))
                self.assertEqual(compiler_wrapper[2], "--contract-sha256")
                self.assertEqual(compiler_wrapper[3], hashlib.sha256(
                    (ROOT / "tools/native_cc_wrapper.py").read_bytes()).hexdigest())

    def test_protobuf_modes_preserve_software_implementations_and_flavor(self):
        for flavor in ("core", "native"):
            enabled = self.gn_args(flavor, "linux-x86_64")
            disabled = self.gn_args(flavor, "linux-x86_64", "false")
            self.assertEqual(enabled.pop("rtc_enable_protobuf"), "true")
            self.assertEqual(disabled.pop("rtc_enable_protobuf"), "false")
            # The per-invocation sccache path is deliberately machine-local.
            enabled.pop("cc_wrapper")
            disabled.pop("cc_wrapper")
            self.assertEqual(enabled, disabled)

    def test_other_targets_keep_their_existing_h264_boundary(self):
        for target in ("linux-arm64", "windows-x86_64", "macos-x86_64", "macos-arm64",
                       "android-x86_64", "android-arm64-v8a", "ios-arm64", "ios-simulator-arm64"):
            for flavor in ("core", "native"):
                with self.subTest(target=target, flavor=flavor):
                    arguments = self.gn_args(flavor, target)
                    self.assertEqual(arguments["rtc_use_h264"], "false")
                    self.assertNotIn("proprietary_codecs", arguments)
                    self.assertNotIn("ffmpeg_branding", arguments)


if __name__ == "__main__":
    unittest.main()
