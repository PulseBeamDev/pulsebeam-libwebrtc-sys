import re
import unittest
from pathlib import Path


JUSTFILE = Path(__file__).resolve().parents[1] / "Justfile"


class InputRetrievalBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.justfile = JUSTFILE.read_text()

    def test_acquisition_retries_are_bounded_and_keep_pins(self):
        sync = self._recipe("_sync flavor target:", "_target-dependencies target:")
        self.assertIn("for attempt in 1 2 3; do", sync)
        self.assertIn('retry_acquisition depot_tools "{{ depot_tools_commit }}"', sync)
        self.assertIn('fetch --depth=1 origin "{{ depot_tools_commit }}"', sync)
        self.assertIn('retry_acquisition webrtc_sync "{{ webrtc_commit }}"', sync)
        self.assertIn('--revision "src@{{ webrtc_commit }}"', sync)
        self.assertIn('retry_acquisition webrtc_hooks "{{ webrtc_commit }}"', sync)
        self.assertIn(
            'actual_webrtc=$(git -C "$src" rev-parse HEAD 2>/dev/null || printf \'%s\' missing)',
            sync,
        )
        self.assertIn("expected={{ webrtc_commit }} actual=$actual_webrtc", sync)

        dependencies = self._recipe("_target-dependencies target:", "_gn-args flavor target:")
        self.assertIn("for attempt in 1 2 3; do", dependencies)
        self.assertIn('install-sysroot.py" --arch=arm64', dependencies)
        self.assertIn("pin={{ webrtc_commit }}", dependencies)
        self.assertRegex(
            dependencies,
            r"attempts=3 failure=upstream-acquisition-exhausted",
        )

    def test_retry_helper_reports_each_pinned_boundary(self):
        sync = self._recipe("_sync flavor target:", "_target-dependencies target:")
        self.assertRegex(
            sync,
            r"retrying pinned input=\$input pin=\$pin attempt=\$attempt/3",
        )
        self.assertRegex(
            sync,
            r"pinned input retrieval failed input=\$input pin=\$pin attempts=3",
        )
        self.assertEqual(len(re.findall(r"retry_acquisition (?:depot_tools|webrtc_sync|webrtc_hooks)", sync)), 5)

    def test_artifact_proof_verifies_its_snapshot_before_execution(self):
        proof = self._recipe("verify-artifact archive='' sha256='':", "# Download, checksum")
        copy = proof.index('cp -- "{{ archive }}" "$snapshot"')
        digest = proof.index('actual=$(sha256sum "$snapshot"')
        runtime = proof.index(' _runtime-test core linux-x86_64 "$snapshot"')
        consumer = proof.index('tools/rust_only_consumer.py --artifact "$snapshot"')
        self.assertLess(copy, digest)
        self.assertLess(digest, runtime)
        self.assertLess(digest, consumer)

    def _recipe(self, start, end):
        beginning = self.justfile.index(start)
        return self.justfile[beginning : self.justfile.index(end, beginning)]


if __name__ == "__main__":
    unittest.main()
