from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"
LABEL = "//buildtools/third_party/libunwind:libunwind"
UNWIND_OBJECTS = (
    "UnwindLevel1-gcc-ext.o",
    "UnwindLevel1.o",
    "UnwindRegistersSave.o",
)


class LinuxArm64UnwindTests(unittest.TestCase):
    def _workspace(self, directory: Path, mode: str) -> tuple[Path, Path]:
        src = directory / "checkout" / "src"
        out = directory / "out"
        gn = src / "buildtools" / "linux64" / "gn"
        gn.parent.mkdir(parents=True)
        abi = out / "obj" / "buildtools" / "third_party" / "libc++abi"
        unwind = out / "obj" / "buildtools" / "third_party" / "libunwind" / "libunwind"
        host_unwind = out / "clang_x64" / "obj" / "buildtools" / "third_party" / "libunwind" / "libunwind"
        abi.mkdir(parents=True)
        unwind.mkdir(parents=True)
        host_unwind.mkdir(parents=True)
        (abi / "cxa_exception.o").touch()
        for object_name in UNWIND_OBJECTS:
            if mode != "partial-missing-object" or object_name != "UnwindLevel1.o":
                (unwind / object_name).touch()
        (host_unwind / "host-only.o").touch()
        gn.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
label=''; what=''
for argument in "$@"; do
  case "$argument" in //*) label="$argument";; outputs|sources|deps) what="$argument";; esac
done
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = gn-error && test "$label" = '//buildtools/third_party/libunwind:libunwind' && test "$what" = sources; then
  echo 'GN fixture: libunwind source metadata is unavailable' >&2
  exit 29
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = missing-source && test "$label" = '//buildtools/third_party/libunwind:libunwind' && test "$what" = sources; then exit 0; fi
case "$what" in
  deps) printf '%s\\n' "$label";;
  sources) if test "$label" = '//buildtools/third_party/libunwind:libunwind'; then printf '%s\\n' '//buildtools/third_party/libunwind/src/UnwindLevel1-gcc-ext.c' '//buildtools/third_party/libunwind/src/UnwindLevel1.c' '//buildtools/third_party/libunwind/src/UnwindRegistersSave.S'; else printf '%s\\n' '//third_party/libc++abi/src/src/cxa_exception.cpp'; fi;;
  outputs) printf '%s\\n' "$PWD/obj/${label#//}/libfixture.a";;
esac
""",
            encoding="utf-8",
        )
        gn.chmod(0o755)
        return src, out

    def _closure(self, flavor: str, mode: str) -> tuple[subprocess.CompletedProcess[str], Path]:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        src, out = self._workspace(directory, mode)
        runtime = directory / "runtime"
        runtime.mkdir()
        result = subprocess.run(
            ["just", "--justfile", str(JUSTFILE), "_export-static-closure", flavor, "linux-arm64", str(src), str(out), str(src), str(out), str(directory / "archives"), str(directory / "objects")],
            check=False, capture_output=True, text=True,
            env={**os.environ, "WEBRTC_WORK": str(directory), "PULSEBEAM_TEST_GN_MODE": mode, "XDG_RUNTIME_DIR": str(runtime)},
        )
        return result, directory

    def test_linux_arm64_libunwind_source_set_is_exported_for_both_flavors(self):
        for flavor in ("core", "native"):
            with self.subTest(flavor=flavor):
                result, directory = self._closure(flavor, "valid")
                self.assertEqual(result.returncode, 0, result.stderr)
                objects = (directory / "objects").read_text().splitlines()
                self.assertEqual(
                    objects,
                    [str(directory / "out" / "obj" / "buildtools" / "third_party" / "libunwind" / "libunwind" / object_name) for object_name in UNWIND_OBJECTS],
                )
                self.assertNotIn("host-only.o", "\n".join(objects))

    def test_linux_arm64_libunwind_source_set_fails_closed(self):
        expected = {"gn-error": "GN source query failed", "missing-source": "no libunwind source metadata", "partial-missing-object": "missing libunwind object output: UnwindLevel1.o"}
        for mode, actual in expected.items():
            with self.subTest(mode=mode):
                result, _ = self._closure("core", mode)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                for fragment in ("target=linux-arm64", "flavor=core", "invariant=static-closure", LABEL, actual):
                    self.assertIn(fragment, result.stderr)


if __name__ == "__main__":
    unittest.main()
