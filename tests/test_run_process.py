import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import run_process


ROOT = Path(__file__).resolve().parents[1]


class ProcessLaunchTests(unittest.TestCase):
    def test_cli_preserves_exit_arguments_and_working_directory(self):
        with tempfile.TemporaryDirectory(prefix="process space ") as temporary:
            result = subprocess.run([
                sys.executable, str(ROOT / "tools/run_process.py"), "--timeout", "10",
                "--cwd", temporary, "--", sys.executable, "-c",
                "import os, sys; print(os.getcwd()); print(sys.argv[1]); sys.exit(27)",
                "spaces & shell $ syntax",
            ], capture_output=True, text=True)
            self.assertEqual(result.returncode, 27, result.stderr)
            self.assertEqual(result.stdout.splitlines(), [str(Path(temporary).resolve()), "spaces & shell $ syntax"])

    def test_timeout_is_portable_and_reports_failure_without_gnu_timeout(self):
        result = subprocess.run([
            sys.executable, str(ROOT / "tools/run_process.py"), "--timeout", "0.1", "--",
            sys.executable, "-c", "import threading; threading.Event().wait()",
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertIn("timed out", result.stderr)

    @unittest.skipIf(os.name == "nt", "POSIX process groups")
    def test_timeout_kills_only_the_launched_process_group(self):
        with patch.object(run_process.subprocess, "Popen") as popen, patch.object(run_process.os, "killpg") as kill:
            child = popen.return_value.__enter__.return_value
            child.pid = 1234
            child.wait.side_effect = [subprocess.TimeoutExpired("git", 2), -9]
            self.assertEqual(run_process.run(["git"], timeout=2), 124)
            kill.assert_called_once_with(1234, run_process.signal.SIGKILL)
            self.assertTrue(popen.call_args.kwargs["start_new_session"])

    def test_invalid_launches_fail_before_starting_a_child(self):
        with patch.object(run_process.subprocess, "Popen") as popen:
            for command, timeout in [([], None), (["git"], 0), (["git"], -1)]:
                with self.subTest(command=command, timeout=timeout), self.assertRaises(ValueError):
                    run_process.run(command, timeout=timeout)
            popen.assert_not_called()

    def test_batch_command_uses_cmd_quoting_not_crt_backslashes(self):
        with patch.dict(os.environ, {"COMSPEC": r"C:\Windows\System32\cmd.exe"}):
            self.assertEqual(
                run_process.batch_command([r"D:\work space\depot_tools\cipd.bat", "version"]),
                r'"C:\Windows\System32\cmd.exe" /d /s /c ""D:\work space\depot_tools\cipd.bat" "version""',
            )
        for bad in ['quote"', "new\nline", "%PATH%", "!expansion!"]:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                run_process.batch_command(["script.bat", bad])

    @unittest.skipUnless(os.name == "nt", "requires native cmd.exe; exercised on Windows producers")
    def test_native_batch_launch_handles_spaces_arguments_cwd_and_failure(self):
        with tempfile.TemporaryDirectory(prefix="batch space & ") as temporary:
            root = Path(temporary)
            batch = root / "test script.bat"
            batch.write_text('@echo off\necho %~1>"%~2"\nexit /b 27\n')
            result = run_process.run([str(batch), "two words", "result.txt"],
                                     batch=True, timeout=10, cwd=str(root))
            self.assertEqual(result, 27)
            self.assertEqual((root / "result.txt").read_text().strip(), "two words")


if __name__ == "__main__":
    unittest.main()
