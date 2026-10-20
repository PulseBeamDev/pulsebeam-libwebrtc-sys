import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tests.test_linux_release_publication import PublicationTests
from tools import github_release as release
from tools import linux_release_publication as publication


class GithubReleaseTests(unittest.TestCase):
    def bundle(self, root):
        return PublicationTests().prepared_bundle(root)

    def test_paginates_drafts_and_classifies_missing_assets_without_mutation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan_file, github_output = root / "plan.json", root / "output"
            releases = [
                {"tag_name": "older", "draft": False, "assets": []},
                {"tag_name": "v1.2.3-linux", "draft": True, "assets": []},
            ]
            with patch.object(release, "gh", return_value="\n".join(map(json.dumps, releases)) + "\n") as gh, patch.dict(release.os.environ, {"GITHUB_OUTPUT": str(github_output)}):
                plan = release.classify("v1.2.3-linux", release.REPOSITORY, bundle, plan_file)
            self.assertEqual(gh.call_count, 1)
            self.assertIn("--paginate", gh.call_args.args)
            self.assertEqual(plan["upload"], sorted(publication.expected_bundle("linux")))
            self.assertFalse(plan["create_draft"])
            self.assertEqual(github_output.read_text(), "finalize=true\n")
            self.assertEqual(json.loads(plan_file.read_text()), plan)

    def test_fork_refused_before_network_and_duplicate_identity_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            with patch.object(release, "gh") as gh:
                with self.assertRaises(publication.PublicationError):
                    release.classify("v1.2.3-linux", "someone/fork", bundle, root / "plan")
                gh.assert_not_called()
            duplicate = json.dumps({"tag_name": "v1.2.3-linux", "draft": True, "assets": []})
            with patch.object(release, "gh", return_value=duplicate + "\n" + duplicate + "\n"):
                with self.assertRaisesRegex(publication.PublicationError, "ambiguous release identity"):
                    release.inventory("v1.2.3-linux")

    def test_create_notes_only_at_create_and_edit_has_no_generate_notes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan_file = root / "plan.json"
            plan = publication.plan(bundle, "absent", None)
            release.write_plan(plan, plan_file)
            with patch.object(release, "gh") as gh:
                release.upload("v1.2.3-linux", bundle, plan_file)
            self.assertEqual(gh.call_count, 1 + len(publication.expected_bundle("linux")))
            self.assertEqual(gh.call_args_list[0].args[:3], ("release", "create", "v1.2.3-linux"))
            self.assertIn("--generate-notes", gh.call_args_list[0].args)
            self.assertTrue(all(call.args[1] == "upload" for call in gh.call_args_list[1:]))
            complete = publication.plan(bundle, "draft", bundle)
            release.write_plan(complete, plan_file)
            with patch.object(release, "gh") as gh:
                release.advertise("v1.2.3-linux", plan_file)
            self.assertEqual(gh.call_args.args, ("release", "edit", "v1.2.3-linux", "--repo", release.REPOSITORY, "--draft=false"))
            self.assertNotIn("--generate-notes", gh.call_args.args)

    def test_verify_rejects_missing_or_conflicting_assets_before_advertisement(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            names = sorted(publication.expected_bundle("linux"))
            def fake_gh(*args):
                if args[1] == "view":
                    return json.dumps({"assets": [{"name": name} for name in names]})
                if args[1] == "download":
                    directory = Path(args[-1])
                    for name in names:
                        (directory / name).write_bytes((bundle / name).read_bytes())
                return ""
            with patch.object(release, "gh", side_effect=fake_gh):
                result = release.verify("v1.2.3-linux", bundle, root / "verified.json")
                self.assertFalse(result["upload"])
                names.pop()
                with self.assertRaisesRegex(publication.PublicationError, "incomplete"):
                    release.verify("v1.2.3-linux", bundle, root / "incomplete.json")
            self.assertFalse((root / "incomplete.json").exists())

    def test_derived_tag_is_created_once_and_never_moved(self):
        commit = "a" * 40
        missing = subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 404")
        ref = json.dumps({"type": "commit", "sha": commit})
        conflict = json.dumps({"type": "commit", "sha": "b" * 40})
        for scope in release.OS_TARGETS:
            with self.subTest(scope=scope):
                with patch.object(release, "gh", side_effect=[ref, missing, ""]) as gh:
                    self.assertEqual(release.ensure_tag("v1.2.3", scope, commit), f"v1.2.3-{scope}")
                    self.assertIn("POST", gh.call_args.args)
                    self.assertIn(f"sha={commit}", gh.call_args.args)
                with patch.object(release, "gh", side_effect=[ref, ref]) as gh:
                    release.ensure_tag("v1.2.3", scope, commit)
                    self.assertEqual(gh.call_count, 2)
                with patch.object(release, "gh", side_effect=[ref, conflict]) as gh:
                    with self.assertRaisesRegex(ValueError, "immutable derived tag conflict"):
                        release.ensure_tag("v1.2.3", scope, commit)
                    self.assertEqual(gh.call_count, 2)
        with patch.object(release, "gh", side_effect=[ref, subprocess.CalledProcessError(1, ["gh"], stderr="HTTP 503")]) as gh:
            with self.assertRaises(subprocess.CalledProcessError):
                release.ensure_tag("v1.2.3", "linux", commit)
            self.assertEqual(gh.call_count, 2)

    def test_tag_identity_uses_exact_refs_and_peels_annotated_tags(self):
        commit, annotation = "a" * 40, "b" * 40
        with patch.object(release, "gh", side_effect=[
            json.dumps({"type": "tag", "sha": annotation}),
            json.dumps({"type": "commit", "sha": commit}),
        ]) as gh:
            self.assertEqual(release.tag_commit("v1.2.3"), commit)
            self.assertIn("/git/ref/tags/v1.2.3", gh.call_args_list[0].args[1])
            self.assertIn(f"/git/tags/{annotation}", gh.call_args_list[1].args[1])
        for obj in ({"type": "tree", "sha": commit}, {"type": "tag", "sha": annotation}):
            with patch.object(release, "gh", return_value=json.dumps(obj)):
                with self.assertRaises(publication.PublicationError):
                    release.tag_commit("v1.2.3")

    def test_wrong_os_bundle_fails_before_network(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            with patch.object(release, "gh") as gh:
                with self.assertRaises(ValueError):
                    release.classify("v1.2.3-macos", release.REPOSITORY, bundle, root / "plan")
                gh.assert_not_called()

    def test_upload_plan_rejects_unexpected_names(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan = root / "plan.json"
            release.write_plan({"upload": ["unexpected"], "finalize": True, "create_draft": True}, plan)
            with patch.object(release, "gh") as gh:
                with self.assertRaisesRegex(publication.PublicationError, "unsafe"):
                    release.upload("v1.2.3-linux", bundle, plan)
                gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
