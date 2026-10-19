import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class WebRtcSourceStateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)
        self.src = self.work / "checkout/src"
        self.src.mkdir(parents=True)
        self.paths = ["media/engine/webrtc_video_engine.cc",
                      "media/engine/webrtc_video_engine_unittest.cc"]
        for name in ["BUILD.gn", *self.paths]:
            path = self.src / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("original\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                 "commit", "-qm", "fixture")
        self.revision = self.git("rev-parse", "HEAD").strip()
        self.encoder_patch = self.work / "encoder.patch"
        for name in self.paths:
            (self.src / name).write_text("encoder fixed\n")
        self.encoder_patch.write_text(self.git("diff", "--binary", "--full-index"))
        self.git("restore", ".")
        self.ios_patch = self.work / "ios.patch"
        (self.src / "BUILD.gn").write_text("ios fixed\n")
        self.ios_hash = self.digest(self.src / "BUILD.gn")
        self.ios_patch.write_text(self.git("diff", "--binary", "--full-index"))
        self.git("restore", ".")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.src), *args], text=True)

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def run_state(self, flavor="core", target="linux-x86_64"):
        values = {
            "webrtc_commit": self.revision,
            "webrtc_encoder_switch_patch": str(self.encoder_patch),
            "webrtc_encoder_switch_patch_sha256": self.digest(self.encoder_patch),
            "webrtc_core_ios_patch": str(self.ios_patch),
            "webrtc_core_ios_patch_sha256": self.digest(self.ios_patch),
            "webrtc_core_ios_build_gn_sha256": self.ios_hash,
        }
        command = ["just", "--justfile", str(ROOT / "Justfile")]
        for key, value in values.items():
            command.extend(["--set", key, value])
        return subprocess.run(
            [*command, "_source-state", flavor, target],
            env=os.environ | {"WEBRTC_WORK": str(self.work),
                              "WEBRTC_DIST": str(self.work / "dist")},
            text=True, capture_output=True,
        )

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_runtime_patch_is_applied_for_every_flavor_and_is_idempotent(self):
        for flavor in ["core", "native"]:
            self.assert_success(self.run_state(flavor))
            self.assert_success(self.run_state(flavor))
            for name in self.paths:
                self.assertEqual((self.src / name).read_text(), "encoder fixed\n")
            self.assertEqual((self.src / "BUILD.gn").read_text(), "original\n")

    def test_ios_patch_transitions_do_not_remove_runtime_patch(self):
        self.assert_success(self.run_state(target="ios-arm64"))
        self.assertEqual((self.src / "BUILD.gn").read_text(), "ios fixed\n")
        self.assert_success(self.run_state())
        self.assertEqual((self.src / "BUILD.gn").read_text(), "original\n")
        for name in self.paths:
            self.assertEqual((self.src / name).read_text(), "encoder fixed\n")

    def test_rejects_additional_edits_inside_patched_files(self):
        self.assert_success(self.run_state())
        (self.src / self.paths[0]).write_text("encoder fixed\nunexpected\n")
        result = self.run_state()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differs from pinned patch", result.stderr)

    def test_rejects_untracked_or_staged_edits(self):
        extra = self.src / "extra"
        extra.write_text("untracked\n")
        self.assertNotEqual(self.run_state().returncode, 0)
        self.git("add", "extra")
        self.assertNotEqual(self.run_state().returncode, 0)


if __name__ == "__main__":
    unittest.main()
