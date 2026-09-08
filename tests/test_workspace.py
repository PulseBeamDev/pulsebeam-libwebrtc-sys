from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from webrtc.errors import ContractError
from webrtc.workspace import dependency_manifest, patch_state


class WorkspaceTests(unittest.TestCase):
    def test_dependency_identity_mismatch_is_detectable(self):
        root = Path(tempfile.mkdtemp())
        marker = root / "pkg/.cipd/pins"
        marker.parent.mkdir(parents=True)
        marker.write_text("instance one")
        first = dependency_manifest(root)
        marker.write_text("instance two")
        second = dependency_manifest(root)
        self.assertNotEqual(first, second)

    def test_patch_state_rejects_unstaged_checkout_mutation(self):
        root = Path(tempfile.mkdtemp())
        (root / "patches").mkdir()
        (root / "patches/series").write_text("")
        (root / "patches/manifest.json").write_text('{"schema_version":1,"patches":{}}')
        checkout = root / "checkout"
        src = checkout / "src"
        src.mkdir(parents=True)
        import subprocess
        subprocess.run(["git", "init", "-q"], cwd=src, check=True)
        (src / "value").write_text("changed\n")
        with self.assertRaises(ContractError):
            patch_state(checkout, root)
