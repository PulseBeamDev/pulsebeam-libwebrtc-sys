from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"
BASELINE = "3daeec8ab2bfe396eed7fdcb38311abeb79e6880"
LABEL = "//buildtools/third_party/libc++abi"
LIBCXXABI_OBJECTS = ("cxa_exception.o", "cxa_guard.o")


class LinuxAsanStaticClosureTests(unittest.TestCase):
    def _workspace(self, directory: Path, mode: str = "valid") -> tuple[Path, Path, Path]:
        src = directory / "checkout" / "src"
        out = directory / "out"
        gn = src / "buildtools" / "linux64" / "gn"
        gn.parent.mkdir(parents=True)
        libcxxabi = out / "obj" / "buildtools" / "third_party" / "libc++abi" / "libc++abi"
        host_libcxxabi = out / "clang_x64" / "obj" / "buildtools" / "third_party" / "libc++abi" / "libc++abi"
        libcxxabi.mkdir(parents=True)
        host_libcxxabi.mkdir(parents=True)
        for object_name in LIBCXXABI_OBJECTS:
            if mode != "missing-target-object" or object_name != "cxa_guard.o":
                (libcxxabi / object_name).touch()
            (host_libcxxabi / object_name).touch()
        gn.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
label=''
what=''
for argument in "$@"; do
  case "$argument" in //*) label="$argument";; outputs|sources|deps) what="$argument";; esac
done
if test "${PULSEBEAM_WEBRTC_SANITIZER:-}" = address && test "$label" = '//buildtools/third_party/libc++abi' && test "$what" = outputs; then
  echo 'source_set targets do not have outputs' >&2
  exit 23
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = failure && test "$label" = '//buildtools/third_party/libc++abi' && test "$what" = sources; then
  echo 'GN fixture: libc++abi source metadata is unavailable' >&2
  exit 29
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = empty-libcxx && test "$label" = '//buildtools/third_party/libc++' && test "$what" = outputs; then
  exit 0
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = nonarchive-libcxx && test "$label" = '//buildtools/third_party/libc++' && test "$what" = outputs; then
  printf '%s\\n' "$PWD/not-an-archive.txt"
  exit 0
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = empty-libcxxabi && test "$label" = '//buildtools/third_party/libc++abi' && test "$what" = outputs; then
  exit 0
fi
if test "${PULSEBEAM_TEST_GN_MODE:-valid}" = nonarchive-libcxxabi && test "$label" = '//buildtools/third_party/libc++abi' && test "$what" = outputs; then
  printf '%s\\n' "$PWD/not-an-archive.txt"
  exit 0
fi
case "$what" in
  deps) printf '%s\\n' "$label";;
  sources) printf '%s\\n' '//third_party/libc++abi/src/src/cxa_exception.cpp' '//third_party/libc++abi/src/src/cxa_guard.cpp';;
  outputs) printf '%s\\n' "$PWD/obj/${label#//}/libfixture.a";;
esac
""",
            encoding="utf-8",
        )
        gn.chmod(0o755)
        return src, out, gn

    def _closure(self, flavor: str, directory: Path, mode: str = "valid") -> subprocess.CompletedProcess[str]:
        src, out, _ = self._workspace(directory, mode)
        archives = directory / "archives"
        objects = directory / "objects"
        runtime = directory / "runtime"
        runtime.mkdir()
        return subprocess.run(
            [
                "just", "--justfile", str(JUSTFILE), "_export-static-closure", flavor,
                "linux-x86_64", str(src), str(out), str(src), str(out), str(archives), str(objects),
            ],
            check=False,
            capture_output=True,
            text=True,
            env={
                **os.environ,
                "WEBRTC_WORK": str(directory),
                "PULSEBEAM_WEBRTC_SANITIZER": "address",
                "PULSEBEAM_TEST_GN_MODE": mode,
                "XDG_RUNTIME_DIR": str(runtime),
            },
        )

    def test_asan_libcxxabi_source_set_succeeds_for_both_flavors(self):
        for flavor in ("core", "native"):
            with self.subTest(flavor=flavor), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                result = self._closure(flavor, directory)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("libfixture.a", (directory / "archives").read_text())
                self.assertEqual(
                    (directory / "objects").read_text().splitlines(),
                    [str(directory / "out" / "obj" / "buildtools" / "third_party" / "libc++abi" / "libc++abi" / object_name) for object_name in LIBCXXABI_OBJECTS],
                )

    def test_asan_libcxxabi_source_set_fails_closed_without_host_object_fallback(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self._closure("core", Path(temp), "missing-target-object")
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        for expected in ("target=linux-x86_64", "flavor=core", "invariant=static-closure", LABEL, "missing libc++abi object output: cxa_guard.o"):
            self.assertIn(expected, result.stderr)

    def test_observed_baseline_output_query_fails_executably(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            src, out, gn = self._workspace(directory)
            runtime = directory / "runtime"
            runtime.mkdir()
            baseline = subprocess.run(
                ["git", "show", f"{BASELINE}:Justfile"],
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
            ).stdout
            match = re.search(r"    gn_outputs\(\) \{\n(?P<body>.*?)\n    \}", baseline, re.DOTALL)
            self.assertIsNotNone(match)
            historical_function = match.group(0).replace("{{ root }}", str(ROOT)).replace("{{ flavor }}", "core").replace("{{ target }}", "linux-x86_64")
            result = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", f'gn="{gn}"\nsrc_native="{src}"\nout_native="{out}"\n{historical_function}\ngn_outputs {LABEL}'],
                check=False,
                capture_output=True,
                text=True,
                env={**os.environ, "PULSEBEAM_WEBRTC_SANITIZER": "address", "XDG_RUNTIME_DIR": str(runtime)},
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("source_set targets do not have outputs", result.stderr)
        self.assertIn(f"actual=GN output query failed for {LABEL}", result.stderr)

    def test_ordinary_linux_closure_still_uses_libcxxabi_archive(self):
        for flavor in ("core", "native"):
            with self.subTest(flavor=flavor), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                src, out, _ = self._workspace(directory)
                archives = directory / "archives"
                objects = directory / "objects"
                runtime = directory / "runtime"
                runtime.mkdir()
                result = subprocess.run(
                    ["just", "--justfile", str(JUSTFILE), "_export-static-closure", flavor, "linux-x86_64", str(src), str(out), str(src), str(out), str(archives), str(objects)],
                    check=False,
                    capture_output=True,
                    text=True,
                    env={**os.environ, "WEBRTC_WORK": str(directory), "XDG_RUNTIME_DIR": str(runtime)},
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("libc++abi/libfixture.a", archives.read_text())
                self.assertFalse(objects.exists())

    def test_genuine_gn_failure_preserves_diagnostic_context(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self._closure("core", Path(temp), "failure")
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        for expected in ("GN fixture: libc++abi source metadata is unavailable", "target=linux-x86_64", "flavor=core", "invariant=static-closure", f"expected=GN sources for {LABEL}", f"actual=GN source query failed for {LABEL}"):
            self.assertIn(expected, result.stderr)

    def test_required_libcxx_output_is_not_silently_omitted(self):
        for mode in ("empty-libcxx", "nonarchive-libcxx"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                result = self._closure("native", Path(temp), mode)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("invariant=static-closure", result.stderr)
                self.assertIn("expected=a static archive output for //buildtools/third_party/libc++", result.stderr)
                self.assertIn("actual=no matching .a or .lib output for //buildtools/third_party/libc++", result.stderr)

    def test_required_ordinary_libcxxabi_output_is_not_silently_omitted(self):
        for mode in ("empty-libcxxabi", "nonarchive-libcxxabi"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                src, out, _ = self._workspace(directory)
                archives = directory / "archives"
                objects = directory / "objects"
                runtime = directory / "runtime"
                runtime.mkdir()
                result = subprocess.run(
                    ["just", "--justfile", str(JUSTFILE), "_export-static-closure", "native", "linux-x86_64", str(src), str(out), str(src), str(out), str(archives), str(objects)],
                    check=False,
                    capture_output=True,
                    text=True,
                    env={**os.environ, "WEBRTC_WORK": str(directory), "PULSEBEAM_TEST_GN_MODE": mode, "XDG_RUNTIME_DIR": str(runtime)},
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("invariant=static-closure", result.stderr)
                self.assertIn("expected=a static archive output for //buildtools/third_party/libc++abi", result.stderr)
                self.assertIn("actual=no matching .a or .lib output for //buildtools/third_party/libc++abi", result.stderr)


if __name__ == "__main__":
    unittest.main()
