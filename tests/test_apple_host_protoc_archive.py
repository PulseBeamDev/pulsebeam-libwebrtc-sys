from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"


class AppleHostProtocArchiveTests(unittest.TestCase):
    def _workspace(self, directory: Path, shape: str) -> Path:
        work = directory / "host protoc workspace"
        src = work / "checkout" / "src"
        out = work / "out" / "core" / "macos-x86_64"
        ninja = src / "third_party" / "ninja" / "ninja"
        ar = src / "third_party" / "llvm-build" / "Release+Asserts" / "bin" / "llvm-ar"
        tools = directory / "tools"
        ninja.parent.mkdir(parents=True)
        ar.parent.mkdir(parents=True)
        tools.mkdir()
        ninja.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
        ar.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  t)
    case "${PULSEBEAM_TEST_ARCHIVE_SHAPE}" in
      valid|wrong-arch) printf '%s\\n' code_generator.o ;;
      symdef) printf '%s\\n' __.SYMDEF code_generator.o ;;
      qualified) printf '%s\\n' /private/build/code_generator.o ;;
      empty) ;;
      metadata-only) printf '%s\\n' __.SYMDEF '__.SYMDEF SORTED' ;;
      unreadable) echo 'fixture archive cannot be read' >&2; exit 2 ;;
    esac
    ;;
  p)
    test "$3" = code_generator.o || { echo "$3 was not found" >&2; exit 3; }
    printf object
    ;;
esac
""",
            encoding="utf-8",
        )
        file = tools / "file"
        file.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
if test "${1:-}" = -; then
  cat >/dev/null
  arch="${PULSEBEAM_TEST_MEMBER_ARCH:-$PULSEBEAM_TEST_HOST_ARCH}"
else
  arch="$PULSEBEAM_TEST_HOST_ARCH"
fi
printf 'Mach-O 64-bit object %s\\n' "$arch"
""",
            encoding="utf-8",
        )
        uname = tools / "uname"
        uname.write_text("#!/usr/bin/env bash\nprintf '%s\\n' \"$PULSEBEAM_TEST_HOST_ARCH\"\n", encoding="utf-8")
        for executable in (ninja, ar, file, uname):
            executable.chmod(0o755)
        if shape != "missing":
            support = out / "obj" / "third_party" / "protobuf" / "libprotoc_lib.a"
            support.parent.mkdir(parents=True)
            support.touch()
        out.mkdir(parents=True, exist_ok=True)
        protoc = out / "protoc"
        protoc.write_text("#!/usr/bin/env bash\n", encoding="utf-8")
        protoc.chmod(0o755)
        return work

    def _run(self, shape: str, host_arch: str, member_arch: str | None = None) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            work = self._workspace(directory, shape)
            runtime = directory / "runtime"
            runtime.mkdir()
            env = {
                **os.environ,
                "WEBRTC_WORK": str(work),
                "PULSEBEAM_TEST_ARCHIVE_SHAPE": shape,
                "PULSEBEAM_TEST_HOST_ARCH": host_arch,
                "PATH": f"{directory / 'tools'}:{os.environ['PATH']}",
                "XDG_RUNTIME_DIR": str(runtime),
            }
            if member_arch is not None:
                env["PULSEBEAM_TEST_MEMBER_ARCH"] = member_arch
            return subprocess.run(
                ["just", "--justfile", str(JUSTFILE), "_apple-host-protoc", "core", "macos-x86_64"],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )

    def test_archive_metadata_and_qualified_members_are_accepted(self):
        for shape, host_arch in (("symdef", "arm64"), ("qualified", "x86_64")):
            with self.subTest(shape=shape):
                result = self._run(shape, host_arch)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_valid_host_archives_support_both_architectures_in_space_path(self):
        for host_arch in ("x86_64", "arm64"):
            with self.subTest(host_arch=host_arch):
                result = self._run("valid", host_arch)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_invalid_host_archives_remain_actionable_failures(self):
        cases = {
            "missing": "missing host protoc support archive",
            "empty": "empty host protoc support archive",
            "unreadable": "unreadable host protoc support archive",
            "metadata-only": "invalid host protoc support archive",
            "wrong-arch": "host protoc support architecture mismatch",
        }
        for shape, expected in cases.items():
            with self.subTest(shape=shape):
                result = self._run(
                    shape,
                    "x86_64",
                    "arm64" if shape == "wrong-arch" else None,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
