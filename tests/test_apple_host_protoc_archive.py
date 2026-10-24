from __future__ import annotations

import os
from pathlib import Path
import shutil
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
        ninja.write_text('#!/usr/bin/env bash\nif test -n "${PULSEBEAM_TEST_ORDER_LOG:-}"; then printf "ninja %s\\n" "$*" >> "$PULSEBEAM_TEST_ORDER_LOG"; fi\nexit 0\n', encoding="utf-8")
        ar.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
if test -n "${PULSEBEAM_TEST_ORDER_LOG:-}"; then printf 'ar %s\\n' "$*" >> "$PULSEBEAM_TEST_ORDER_LOG"; fi
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
    expected=code_generator.o
    if test "$PULSEBEAM_TEST_ARCHIVE_SHAPE" = qualified; then expected=/private/build/code_generator.o; fi
    test "$3" = "$expected" || { echo "$3 was not found" >&2; exit 3; }
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

    def _run(self, shape: str, host_arch: str, member_arch: str | None = None, order: list[str] | None = None) -> subprocess.CompletedProcess[str]:
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
                "PULSEBEAM_TEST_ORDER_LOG": str(directory / "order"),
                "PATH": f"{directory / 'tools'}:{os.environ['PATH']}",
                "XDG_RUNTIME_DIR": str(runtime),
            }
            if member_arch is not None:
                env["PULSEBEAM_TEST_MEMBER_ARCH"] = member_arch
            result = subprocess.run(
                ["just", "--justfile", str(JUSTFILE), "_apple-host-protoc", "core", "macos-x86_64"],
                check=False,
                capture_output=True,
                text=True,
                env=env,
            )
            if order is not None:
                order.extend((directory / "order").read_text().splitlines())
            return result

    def test_protobuf_archives_are_built_and_reindexed_before_linking_protoc(self):
        order = []
        result = self._run("symdef", "arm64", order=order)
        self.assertEqual(result.returncode, 0, result.stderr)
        build_archives = next(i for i, line in enumerate(order) if line.startswith("ninja ") and line.endswith("libprotobuf_lite.a"))
        reindex = [i for i, line in enumerate(order) if line.startswith("ar --format=gnu s ")]
        link = next(i for i, line in enumerate(order) if line.startswith("ninja ") and line.endswith(" protoc"))
        self.assertEqual(len(reindex), 3)
        self.assertLess(build_archives, min(reindex))
        self.assertLess(max(reindex), link)

    def test_archive_metadata_and_qualified_members_are_accepted(self):
        for shape, host_arch in (("symdef", "arm64"), ("qualified", "x86_64")):
            with self.subTest(shape=shape):
                result = self._run(shape, host_arch)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_real_thin_archive_requires_the_listed_qualified_member(self):
        ar = shutil.which("llvm-ar-19") or shutil.which("llvm-ar")
        if not ar:
            self.skipTest("requires llvm-ar")
        with tempfile.TemporaryDirectory(prefix="thin archive space ") as temp:
            root = Path(temp)
            work = self._workspace(root, "valid")
            support = work / "out/core/macos-x86_64/obj/third_party/protobuf/libprotoc_lib.a"
            member = root / "object space" / "code_generator.o"
            member.parent.mkdir()
            member.write_bytes(b"object fixture")
            support.unlink()
            subprocess.run([ar, "rcT", str(support), str(member)], check=True, capture_output=True)
            for name in ("libprotobuf_full.a", "libprotobuf_lite.a"):
                shutil.copyfile(support, support.with_name(name))
            listed = subprocess.run([ar, "t", str(support)], check=True, capture_output=True, text=True).stdout.strip()
            self.assertEqual(Path(listed), member)
            self.assertNotEqual(subprocess.run([ar, "p", str(support), member.name], capture_output=True).returncode, 0)
            self.assertEqual(subprocess.run([ar, "p", str(support), listed], check=True, capture_output=True).stdout, member.read_bytes())
            fake_ar = work / "checkout/src/third_party/llvm-build/Release+Asserts/bin/llvm-ar"
            fake_ar.unlink()
            fake_ar.symlink_to(ar)
            result = subprocess.run(
                ["just", "--justfile", str(JUSTFILE), "_apple-host-protoc", "core", "macos-x86_64"],
                capture_output=True, text=True,
                env={**os.environ, "WEBRTC_WORK": str(work), "PULSEBEAM_TEST_HOST_ARCH": "x86_64",
                     "PATH": f"{root / 'tools'}:{os.environ['PATH']}"},
            )
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
            "unreadable": "cannot normalize Apple archive",
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
