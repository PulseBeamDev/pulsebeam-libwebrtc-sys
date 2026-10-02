"""Compile and exercise the adapter's standalone C++20 carrier accounting."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest


class OpusCarrierBudgetTests(unittest.TestCase):
    def test_recipient_slot_receipts(self):
        compiler = shlex.split(os.environ.get("CXX", "c++"))
        self.assertTrue(compiler and shutil.which(compiler[0]),
                        "C++20 compiler required (set CXX)")
        source = Path(__file__).with_name("native_opus_carrier_budget.cc")
        with tempfile.TemporaryDirectory(prefix="opus-carrier-budget-") as temp:
            output = Path(temp) / "budget"
            subprocess.run([
                *compiler, "-std=c++20", "-Wall", "-Wextra", "-Werror",
                "-UNDEBUG", "-pthread", str(source), "-o", str(output),
            ], check=True, capture_output=True, text=True)
            subprocess.run([str(output)], check=True,
                           capture_output=True, text=True)


if __name__ == "__main__":
    unittest.main()
