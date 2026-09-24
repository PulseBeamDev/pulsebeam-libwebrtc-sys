from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

from tests.test_release_audit import ReleaseAuditTests
from tools import audit_release, linux_release_publication as publication


class LinuxPublicationTests(unittest.TestCase):
    def prepared_bundle(self, root: Path) -> Path:
        checksums, _ = ReleaseAuditTests().build_release(root, {"linux-x86_64"})
        archives = root
        report = audit_release.audit(archives, checksums, Path("artifacts.lock.json"), "linux")
        audit = root / "LINUX-RELEASE-MANIFEST.json"
        audit.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        bundle = root / "bundle"
        publication.prepare(archives, checksums, audit, "v-test", bundle)
        return bundle

    def test_closed_bundle_and_first_publication(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = self.prepared_bundle(Path(temp))
            inventory = publication.bundle_inventory(bundle)
            self.assertEqual(set(inventory), publication.EXPECTED_BUNDLE)
            lock = json.loads((bundle / "artifacts.lock.json").read_text())
            self.assertEqual(lock["release_scope"], "linux")
            self.assertEqual(sum(item["url"] is not None for item in lock["artifacts"]), 2)
            self.assertEqual(sum(item["url"] is None for item in lock["artifacts"]), 16)
            state = publication.plan(bundle, "absent", None)
            self.assertTrue(state["create_draft"])
            self.assertEqual(state["upload"], sorted(publication.EXPECTED_BUNDLE))
            self.assertTrue(state["finalize"])

    def test_identical_retry_and_interrupted_draft_recover_without_replacement(self):
        with tempfile.TemporaryDirectory() as temp:
            root, observed = Path(temp), Path(temp) / "observed"
            bundle = self.prepared_bundle(root)
            observed.mkdir()
            for missing in publication.EXPECTED_BUNDLE:
                for path in observed.iterdir():
                    path.unlink()
                for name in publication.EXPECTED_BUNDLE - {missing}:
                    (observed / name).write_bytes((bundle / name).read_bytes())
                partial = publication.plan(bundle, "draft", observed)
                self.assertFalse(partial["create_draft"])
                self.assertEqual(partial["upload"], [missing])
                self.assertTrue(partial["finalize"])
            for name in publication.EXPECTED_BUNDLE:
                (observed / name).write_bytes((bundle / name).read_bytes())
            retry = publication.plan(bundle, "published", observed)
            self.assertEqual(retry["upload"], [])
            self.assertFalse(retry["finalize"])

    def test_conflicting_or_unexpected_bytes_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root, observed = Path(temp), Path(temp) / "observed"
            bundle = self.prepared_bundle(root)
            observed.mkdir()
            (observed / "SHA256SUMS").write_text("different\n")
            with self.assertRaisesRegex(publication.PublicationError, "immutable release conflict"):
                publication.plan(bundle, "draft", observed)
            (observed / "SHA256SUMS").unlink()
            (observed / "unexpected").write_text("different\n")
            with self.assertRaisesRegex(publication.PublicationError, "conflicting"):
                publication.plan(bundle, "draft", observed)

    def test_duplicate_or_unreadable_remote_inventory_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root, observed = Path(temp), Path(temp) / "observed"
            bundle = self.prepared_bundle(root)
            observed.mkdir()
            with self.assertRaisesRegex(publication.PublicationError, "ambiguous remote inventory"):
                publication.plan(bundle, "draft", observed, ["SHA256SUMS", "SHA256SUMS"])
            with self.assertRaisesRegex(publication.PublicationError, "inventory/download mismatch"):
                publication.plan(bundle, "draft", observed, ["SHA256SUMS"])

    def test_incomplete_published_release_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = self.prepared_bundle(Path(temp))
            with self.assertRaisesRegex(publication.PublicationError, "published release is incomplete"):
                publication.plan(bundle, "published", None)

    def test_noncanonical_repository_fails_before_publication_plan(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = self.prepared_bundle(Path(temp))
            with self.assertRaisesRegex(publication.PublicationError, "publication repository must be"):
                publication.plan(bundle, "absent", None, repository="fork/pulsebeam-libwebrtc-sys")

    def test_workflow_finalizes_only_after_attestation_reverification(self):
        workflow = Path(".github/workflows/linux.yml").read_text(encoding="utf-8")
        publish = workflow[workflow.index("  publish-linux:"):]
        self.assertIn('test "$RUNTIME_REPOSITORY" = "$CANONICAL_REPOSITORY"', publish)
        self.assertIn('gh api --paginate "repos/${CANONICAL_REPOSITORY}/releases?per_page=100"', publish)
        self.assertNotIn("${GITHUB_REPOSITORY}", publish)
        self.assertGreaterEqual(publish.count("--repo \"$CANONICAL_REPOSITORY\""), 7)
        gate = "if: steps.publication-plan.outputs.finalize == 'true'"
        self.assertEqual(publish.count(gate), 5)
        attestation = publish.index("      - name: Attest final release assets")
        reverify = publish.index("      - name: Reverify the complete attested draft immediately before advertisement")
        advertise = publish.index("      - name: Advertise only the verified complete Linux release")
        self.assertLess(attestation, reverify)
        self.assertLess(reverify, advertise)
        self.assertIn('attested-publication-plan.json', publish[reverify:advertise])

    def test_workflow_discovers_existing_draft_in_paginated_inventory(self):
        publish = Path('.github/workflows/linux.yml').read_text(encoding='utf-8').split('  publish-linux:', 1)[1]
        script = re.search(r"python3 -c '([^']+)' < releases.jsonl > release.json", publish)
        self.assertIsNotNone(script)
        releases = [
            {'tag_name': 'v0.5.2', 'draft': False, 'assets': []},
            {'tag_name': 'v0.5.3', 'draft': True, 'assets': []},
        ]
        result = subprocess.run(
            ['python3', '-c', script.group(1)],
            input='\n'.join(map(json.dumps, releases)) + '\n',
            text=True, capture_output=True, check=True,
            env={**os.environ, 'RELEASE_TAG': 'v0.5.3'},
        )
        self.assertEqual(json.loads(result.stdout), releases[1])
        self.assertIn('if test "$(python3 -c \'import json; print(bool(json.load(open("release.json"))["assets"]))\')" = True', publish)

    def test_workflow_uploads_one_asset_per_line(self):
        publish = Path('.github/workflows/linux.yml').read_text(encoding='utf-8').split('  publish-linux:', 1)[1]
        script = re.search(r"done < <\(python3 -c '([^']+)'\)", publish)
        self.assertIsNotNone(script)
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'publication-plan.json').write_text(json.dumps({'upload': ['SHA256SUMS', 'webrtc-core-linux-x86_64.tar.gz']}))
            result = subprocess.run(['python3', '-c', script.group(1)], cwd=temp, text=True, capture_output=True, check=True)
            Path(temp, 'publication-plan.json').write_text(json.dumps({'upload': []}))
            empty = subprocess.run(['python3', '-c', script.group(1)], cwd=temp, text=True, capture_output=True, check=True)
        self.assertEqual(result.stdout.splitlines(), ['SHA256SUMS', 'webrtc-core-linux-x86_64.tar.gz'])
        self.assertEqual(empty.stdout, '')
