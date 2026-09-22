import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = ROOT / "Justfile"
TOOLS = ("clang", "clang++", "ld.lld", "llvm-ar", "llvm-nm")


class LinuxToolchainHostTests(unittest.TestCase):
    def test_linux_targets_require_matching_native_hosts_before_sync(self):
        for target, machine, accepted in (
            ("linux-x86_64", "x86_64", True),
            ("linux-arm64", "aarch64", True),
            ("linux-x86_64", "aarch64", False),
            ("linux-arm64", "x86_64", False),
            ("linux-arm64", "riscv64", False),
        ):
            with self.subTest(target=target, machine=machine):
                result = self._run_validate_host(target, machine)
                self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_build_orders_linux_toolchain_after_sync_before_dependencies(self):
        contents = JUSTFILE.read_text(encoding="utf-8")
        build = self._recipe("build flavor target:", "_validate-flavor flavor:")
        self.assertIn(' _linux-toolchain "{{ target }}"', build)
        self.assertLess(build.index(' _sync "{{ flavor }}" "{{ target }}"'), build.index(' _linux-toolchain "{{ target }}"'))
        self.assertLess(build.index(' _linux-toolchain "{{ target }}"'), build.index(' _target-dependencies "{{ target }}"'))

    def test_x86_64_accepts_matching_prebuilt_without_source_build(self):
        with self._fixture("x86-64") as fixture:
            result = fixture.run("linux-x86_64")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(fixture.build_log.exists())
            self.assertEqual(fixture.tool_log.read_text(encoding="utf-8").splitlines(), [
                "clang --version", "clang++ --version", "ld.lld --version", "llvm-ar --version",
                "llvm-nm --version",
            ])

    def test_arm64_rejects_wrong_elf_before_execution_then_rebuilds_with_pinned_script(self):
        with self._fixture("x86-64", rebuild_arch="ARM aarch64") as fixture:
            result = fixture.run("linux-arm64")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(fixture.build_log.read_text(encoding="utf-8").splitlines(), [
                "--host-cc=/usr/bin/gcc", "--host-cxx=/usr/bin/g++", "--no-tools", "--without-android",
                "--without-fuchsia", "--use-system-cmake", "--with-ml-inliner-model=", "--preserve-gcs-signature",
            ])
            self.assertNotIn("wrong", fixture.tool_log.read_text(encoding="utf-8"))
            self.assertEqual(fixture.tool_log.read_text(encoding="utf-8").splitlines(), [
                "clang --version", "clang++ --version", "ld.lld --version", "llvm-ar --version",
                "llvm-nm --version",
            ])

    def test_arm64_reuses_valid_rebuilt_toolchain(self):
        with self._fixture("ARM aarch64") as fixture:
            result = fixture.run("linux-arm64")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(fixture.build_log.exists())

    def test_invalid_required_tools_fail_closed(self):
        for problem in ("missing", "stale", "not-executable"):
            with self.subTest(problem=problem), self._fixture("x86-64", problem=problem) as fixture:
                result = fixture.run("linux-x86_64")
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(fixture.build_log.exists())

    def test_early_tool_version_failure_fails_closed(self):
        with self._fixture("x86-64", version_fail_tool="clang") as fixture:
            result = fixture.run("linux-x86_64")
            self.assertNotEqual(result.returncode, 0)

    def test_arm64_build_and_post_build_validation_failures_propagate(self):
        with self._fixture("x86-64", build_exit=35) as fixture:
            result = fixture.run("linux-arm64")
            self.assertEqual(result.returncode, 35, result.stderr)
        with self._fixture("x86-64", rebuild_arch="x86-64") as fixture:
            result = fixture.run("linux-arm64")
            self.assertNotEqual(result.returncode, 0)

    def test_non_linux_targets_do_not_enter_toolchain_gate(self):
        with self._fixture("x86-64") as fixture:
            result = fixture.run("macos-x86_64")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(fixture.tool_log.exists())
            self.assertFalse(fixture.build_log.exists())

    def _run_validate_host(self, target, machine):
        with tempfile.TemporaryDirectory() as temporary:
            temp = Path(temporary)
            self._executable(temp / "uname", f"#!/usr/bin/env bash\ncase \"$1\" in -s) echo Linux;; -m) echo {machine};; esac\n")
            return subprocess.run(
                ["just", "--justfile", str(JUSTFILE), "_validate-host", target], cwd=ROOT,
                env=os.environ | {"PATH": f"{temp}{os.pathsep}{os.environ['PATH']}", "XDG_RUNTIME_DIR": temporary},
                text=True, capture_output=True, check=False,
            )

    def _recipe(self, start, end):
        contents = JUSTFILE.read_text(encoding="utf-8")
        return contents[contents.index(start):contents.index(end, contents.index(start))]

    @staticmethod
    def _executable(path, contents):
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)


class ToolchainFixture:
    def __init__(self, temporary, arch, rebuild_arch=None, problem=None, build_exit=0, version_fail_tool=None):
        self.root = Path(temporary)
        self.work = self.root / "work"
        self.src = self.work / "checkout" / "src"
        self.bin = self.root / "bin"
        self.arch, self.rebuild_arch, self.problem, self.build_exit = arch, rebuild_arch, problem, build_exit
        self.version_fail_tool = version_fail_tool
        self.tool_log = self.root / "tool.log"
        self.build_log = self.root / "build.log"

    def __enter__(self):
        scripts = self.src / "tools" / "clang" / "scripts"
        scripts.mkdir(parents=True)
        self.bin.mkdir()
        (self.work / "depot_tools").mkdir(parents=True)
        (scripts / "update.py").write_text("print('pinned-revision')\n", encoding="utf-8")
        (scripts / "build.py").write_text(
            "import os, pathlib, sys\n"
            "if str(pathlib.Path(os.environ['WEBRTC_WORK'], 'depot_tools')) not in os.environ['PATH'].split(os.pathsep): raise SystemExit(49)\n"
            "if '--with-ml-inliner-model=' not in sys.argv[1:]: raise SystemExit(51)\n"
            "pathlib.Path(os.environ['BUILD_LOG']).write_text('\\n'.join(sys.argv[1:]) + '\\n')\n"
            "pathlib.Path(os.environ['WEBRTC_WORK'], 'checkout', 'src', 'rebuilt').touch()\n"
            "raise SystemExit(int(os.environ['BUILD_EXIT']))\n",
            encoding="utf-8",
        )
        toolchain = self.src / "third_party" / "llvm-build" / "Release+Asserts"
        (toolchain / "bin").mkdir(parents=True)
        (toolchain / "cr_build_revision").write_text("stale" if self.problem == "stale" else "pinned-revision", encoding="utf-8")
        for tool in TOOLS:
            if self.problem == "missing" and tool == "ld.lld":
                continue
            LinuxToolchainHostTests._executable(
                toolchain / "bin" / tool,
                f"#!/usr/bin/env bash\nprintf '%s %s\\n' '{tool}' \"$*\" >> \"$TOOL_LOG\"\nif test '{tool}' = \"$VERSION_FAIL_TOOL\" && test \"$1\" = --version; then exit 47; fi\n",
            )
        if self.problem == "not-executable":
            path = toolchain / "bin" / "clang"
            path.chmod(path.stat().st_mode & ~stat.S_IXUSR)
        LinuxToolchainHostTests._executable(
            self.bin / "file",
            "#!/usr/bin/env bash\n"
            "if test -e \"$WEBRTC_WORK/checkout/src/rebuilt\"; then arch=$REBUILD_ARCH; else arch=$INITIAL_ARCH; fi\n"
            "printf 'ELF 64-bit LSB executable, %s\\n' \"$arch\"\n",
        )
        return self

    def __exit__(self, *unused):
        return False

    def run(self, target):
        environment = os.environ | {
            "PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}", "WEBRTC_WORK": str(self.work),
            "TOOL_LOG": str(self.tool_log), "BUILD_LOG": str(self.build_log), "BUILD_EXIT": str(self.build_exit),
            "INITIAL_ARCH": self.arch, "REBUILD_ARCH": self.rebuild_arch or self.arch,
            "VERSION_FAIL_TOOL": self.version_fail_tool or "",
            "XDG_RUNTIME_DIR": str(self.root),
        }
        return subprocess.run(
            ["just", "--justfile", str(JUSTFILE), "_linux-toolchain", target], cwd=ROOT,
            env=environment, text=True, capture_output=True, check=False,
        )


def fixture(arch, **kwargs):
    temporary = tempfile.TemporaryDirectory()
    return _FixtureContext(temporary, ToolchainFixture(temporary.name, arch, **kwargs))


class _FixtureContext:
    def __init__(self, temporary, fixture):
        self.temporary, self.fixture = temporary, fixture

    def __enter__(self):
        return self.fixture.__enter__()

    def __exit__(self, *args):
        self.fixture.__exit__(*args)
        self.temporary.cleanup()


LinuxToolchainHostTests._fixture = staticmethod(fixture)


if __name__ == "__main__":
    unittest.main()
