import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
CONTAINERFILE = ROOT / "Containerfile"
DOCKERIGNORE = ROOT / ".dockerignore"

NATIVE_PACKAGES = (
    "libdrm-dev", "libgbm-dev", "libglib2.0-dev", "libx11-dev",
    "libxcomposite-dev", "libxdamage-dev", "libxext-dev", "libxfixes-dev",
    "libxrandr-dev", "libxrender-dev", "libxtst-dev",
)
BUILD_TOOLS = (
    "build-essential", "ca-certificates", "curl", "file", "git", "lsb-release",
    "perl", "pkg-config", "python3", "tar", "unzip", "xz-utils",
)


class LinuxContainerTests(unittest.TestCase):
    def test_containerfile_is_pinned_minimal_and_context_excludes_checkout(self):
        contents = CONTAINERFILE.read_text(encoding="utf-8")
        self.assertIn(
            "FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc",
            contents,
        )
        self.assertIn("cargo install just --version 1.43.1 --locked", contents)
        for package in (*NATIVE_PACKAGES, *BUILD_TOOLS):
            with self.subTest(package=package):
                self.assertIn(package, contents)
        self.assertIn("--no-install-recommends", contents)
        self.assertIn("rm -rf /var/lib/apt/lists/*", contents)
        self.assertNotIn("COPY", contents)
        self.assertNotIn("PULSEBEAM", contents)
        self.assertNotIn("EXPOSE", contents)
        self.assertEqual(DOCKERIGNORE.read_text(encoding="utf-8"), "*\n!Containerfile\n")

    def test_public_recipes_are_direct_engine_interfaces(self):
        recipes = (ROOT / "Justfile").read_text(encoding="utf-8")
        self.assertIn("linux-image engine image:", recipes)
        self.assertIn("linux-run engine image command *args:", recipes)
        self.assertNotIn("tools/linux_container.py", recipes)
        self.assertFalse((ROOT / "tools" / "linux_container.py").exists())

    def test_build_uses_root_containerfile_and_context_for_both_engines(self):
        for engine in ("docker", "podman"):
            with self.subTest(engine=engine), tempfile.TemporaryDirectory() as temp:
                command = self._invoke_engine(Path(temp), engine, "linux-image", engine, "example:test")
                self.assertEqual(
                    command,
                    ["build", "--file", str(CONTAINERFILE), "--tag", "example:test", str(ROOT)],
                )

    def test_image_arguments_are_shell_data_and_rejected_engines_have_no_side_effects(self):
        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            marker = temp_path / "expanded"
            image = f"example:$(touch {marker})"
            command = self._invoke_engine(temp_path, "podman", "linux-image", "podman", image)
            self.assertEqual(
                command,
                ["build", "--file", str(CONTAINERFILE), "--tag", image, str(ROOT)],
            )
            self.assertFalse(marker.exists())

        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            result = self._run(temp_path, "invalid", "linux-image", "invalid", "example:test")
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((temp_path / "engine-command.json").exists())

    def test_run_preserves_arguments_and_uses_only_allowed_environment(self):
        for engine, namespace_args in (("docker", []), ("podman", ["--userns=keep-id"])):
            with self.subTest(engine=engine), tempfile.TemporaryDirectory() as temp:
                command = self._invoke_engine(
                    Path(temp), engine, "linux-run", engine, "pulsebeam-linux-task3", "env",
                    "-u", "PULSEBEAM_WEBRTC_SANITIZER", "just", "build", "core", "linux-x86_64",
                )
            self.assertEqual(
                command,
                [
                    "run", "--rm", *namespace_args, "--user", f"{os.getuid()}:{os.getgid()}",
                    "--volume", f"{ROOT}:/workspace:rw,z", "--workdir", "/workspace", "--env",
                    "HOME=/tmp", "--env", "XDG_RUNTIME_DIR=/tmp", "--env",
                    "PULSEBEAM_WEBRTC_SANITIZER", "--env", "ASAN_OPTIONS", "pulsebeam-linux-task3",
                    "env", "-u", "PULSEBEAM_WEBRTC_SANITIZER", "just", "build", "core",
                    "linux-x86_64",
                ],
            )

    def test_run_propagates_engine_exit_and_rejects_bad_inputs(self):
        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            result = self._run(temp_path, "podman", "linux-run", "podman", "example:test", "true", exit_code=37)
            self.assertEqual(result.returncode, 37, result.stderr)
            missing = self._run(temp_path, "podman", "linux-run", "podman", "example:test")
            self.assertNotEqual(missing.returncode, 0)
            unsupported = self._run(temp_path, "podman", "linux-image", "invalid", "example:test")
            self.assertNotEqual(unsupported.returncode, 0)

    def _invoke_engine(self, temp, engine, *arguments):
        result = self._run(temp, engine, *arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads((temp / "engine-command.json").read_text(encoding="utf-8"))

    def _run(self, temp, engine, *arguments, exit_code=0):
        output = temp / "engine-command.json"
        executable = temp / engine
        executable.write_text(
            "#!/usr/bin/env python3\nimport json, os, sys\n"
            "Path = __import__('pathlib').Path\n"
            "Path(os.environ['ENGINE_OUTPUT']).write_text(json.dumps(sys.argv[1:]))\n"
            "raise SystemExit(int(os.environ.get('ENGINE_EXIT', '0')))\n",
            encoding="utf-8",
        )
        executable.chmod(executable.stat().st_mode | stat.S_IXUSR)
        environment = os.environ | {
            "PATH": f"{temp}{os.pathsep}{os.environ['PATH']}",
            "ENGINE_OUTPUT": str(output),
            "ENGINE_EXIT": str(exit_code),
            "XDG_RUNTIME_DIR": str(temp),
        }
        return subprocess.run(
            ["just", "--justfile", str(ROOT / "Justfile"), *arguments],
            cwd=ROOT,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )


if __name__ == "__main__":
    unittest.main()
