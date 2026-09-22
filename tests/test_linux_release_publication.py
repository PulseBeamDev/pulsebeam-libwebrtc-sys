from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tests.test_release_audit import ReleaseAuditTests
from tools import audit_release, linux_release_publication as publication


class LinuxPublicationTests(unittest.TestCase):
    def test_prior_automatic_success_requires_earlier_exact_revision(self):
        repository = publication.CANONICAL_REPOSITORY
        sha = "a" * 40
        current = dict(id=42, repository={"full_name": repository}, workflow_id=7,
                       path=".github/workflows/linux.yml@refs/heads/main", head_sha=sha,
                       event="workflow_dispatch", created_at="2026-09-22T12:00:00Z")
        prior = dict(id=41, repository={"full_name": repository}, workflow_id=7,
                     head_sha=sha, event="push", status="completed", conclusion="success",
                     updated_at="2026-09-22T11:59:59Z",
                     html_url=f"https://github.com/{repository}/actions/runs/41")
        def validate(run):
            return publication.prior_automatic_success(current, [{"workflow_runs": [run]}], repository, sha, 42)
        self.assertEqual(validate(prior), prior["html_url"])
        for change in (
            {"updated_at": current["created_at"]}, {"conclusion": "failure"},
            {"event": "workflow_dispatch"}, {"head_sha": "b" * 40},
            {"id": 42}, {"workflow_id": 8}, {"repository": {"full_name": "other/repo"}},
        ):
            with self.subTest(change=change), self.assertRaises(publication.PublicationError):
                validate({**prior, **change})
        with self.assertRaises(publication.PublicationError):
            publication.prior_automatic_success(current, [], repository, sha, 42)
        with self.assertRaises(publication.PublicationError):
            publication.prior_automatic_success(current, [{"workflow_runs": [prior]}], "other/repo", sha, 42)

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
        self.assertIn('gh api --include "repos/${CANONICAL_REPOSITORY}/releases/tags/${RELEASE_TAG}"', publish)
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
