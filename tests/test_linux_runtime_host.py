import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LinuxRuntimeHostTests(unittest.TestCase):
    def test_linux_runtime_targets_require_their_matching_native_hosts(self):
        allowed = (
            ("linux-x86_64", "Linux", "x86_64"),
            ("linux-arm64", "Linux", "aarch64"),
        )
        rejected = (
            ("linux-x86_64", "Linux", "aarch64"),
            ("linux-arm64", "Linux", "x86_64"),
            ("linux-x86_64", "Darwin", "x86_64"),
            ("linux-arm64", "Darwin", "aarch64"),
            ("linux-riscv64", "Linux", "riscv64"),
        )

        for target, system, machine in allowed:
            with self.subTest(target=target, system=system, machine=machine):
                result, extracted = self._run_runtime_test(target, system, machine)
                self.assertEqual(result.returncode, 55, result.stderr)
                self.assertTrue(extracted)

        for target, system, machine in rejected:
            with self.subTest(target=target, system=system, machine=machine):
                result, extracted = self._run_runtime_test(target, system, machine)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(f"{target} is not native to this runtime host", result.stderr)
                self.assertFalse(extracted)

    def _run_runtime_test(self, target, system, machine):
        with tempfile.TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            bin_dir = temp / "bin"
            bin_dir.mkdir()
            marker = temp / "extracted"
            archive = temp / "artifact.tar.gz"
            archive.touch()
            self._executable(
                bin_dir / "uname",
                f'''#!/usr/bin/env bash
case "$1" in
  -s) printf '%s\\n' '{system}' ;;
  -m) printf '%s\\n' '{machine}' ;;
  *) exit 2 ;;
esac
''',
            )
            self._executable(
                bin_dir / "tar",
                '''#!/usr/bin/env bash
touch "$RUNTIME_TAR_MARKER"
exit 55
''',
            )
            environment = os.environ | {
                "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                "RUNTIME_TAR_MARKER": str(marker),
                "XDG_RUNTIME_DIR": tempdir,
            }
            result = subprocess.run(
                ["just", "--justfile", str(ROOT / "Justfile"), "_runtime-test", "core", target, str(archive)],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            return result, marker.exists()

    @staticmethod
    def _executable(path, contents):
        path.write_text(contents, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)


if __name__ == "__main__":
    unittest.main()
