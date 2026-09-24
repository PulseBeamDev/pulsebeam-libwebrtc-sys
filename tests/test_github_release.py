import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tests.test_linux_release_publication import LinuxPublicationTests
from tools import github_release as release
from tools import linux_release_publication as publication


class GithubReleaseTests(unittest.TestCase):
    def bundle(self, root):
        return LinuxPublicationTests().prepared_bundle(root)

    def test_paginates_drafts_and_classifies_missing_assets_without_mutation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan_file, github_output = root / "plan.json", root / "output"
            releases = [
                {"tag_name": "older", "draft": False, "assets": []},
                {"tag_name": "v-test", "draft": True, "assets": []},
            ]
            with patch.object(release, "gh", return_value="\n".join(map(json.dumps, releases)) + "\n") as gh, patch.dict(release.os.environ, {"GITHUB_OUTPUT": str(github_output)}):
                plan = release.classify("v-test", release.REPOSITORY, bundle, plan_file)
            self.assertEqual(gh.call_count, 1)
            self.assertIn("--paginate", gh.call_args.args)
            self.assertEqual(plan["upload"], sorted(publication.EXPECTED_BUNDLE))
            self.assertFalse(plan["create_draft"])
            self.assertEqual(github_output.read_text(), "finalize=true\n")
            self.assertEqual(json.loads(plan_file.read_text()), plan)

    def test_fork_refused_before_network_and_duplicate_identity_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            with patch.object(release, "gh") as gh:
                with self.assertRaises(publication.PublicationError):
                    release.classify("v-test", "someone/fork", bundle, root / "plan")
                gh.assert_not_called()
            duplicate = json.dumps({"tag_name": "v-test", "draft": True, "assets": []})
            with patch.object(release, "gh", return_value=duplicate + "\n" + duplicate + "\n"):
                with self.assertRaisesRegex(publication.PublicationError, "ambiguous release identity"):
                    release.inventory("v-test")

    def test_create_notes_only_at_create_and_edit_has_no_generate_notes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan_file = root / "plan.json"
            plan = publication.plan(bundle, "absent", None)
            release.write_plan(plan, plan_file)
            with patch.object(release, "gh") as gh:
                release.upload("v-test", bundle, plan_file)
            self.assertEqual(gh.call_count, 1 + len(publication.EXPECTED_BUNDLE))
            self.assertEqual(gh.call_args_list[0].args[:3], ("release", "create", "v-test"))
            self.assertIn("--generate-notes", gh.call_args_list[0].args)
            self.assertTrue(all(call.args[1] == "upload" for call in gh.call_args_list[1:]))
            complete = publication.plan(bundle, "draft", bundle)
            release.write_plan(complete, plan_file)
            with patch.object(release, "gh") as gh:
                release.advertise("v-test", plan_file)
            self.assertEqual(gh.call_args.args, ("release", "edit", "v-test", "--repo", release.REPOSITORY, "--draft=false"))
            self.assertNotIn("--generate-notes", gh.call_args.args)

    def test_verify_rejects_missing_or_conflicting_assets_before_advertisement(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            names = sorted(publication.EXPECTED_BUNDLE)
            def fake_gh(*args):
                if args[1] == "view":
                    return json.dumps({"assets": [{"name": name} for name in names]})
                if args[1] == "download":
                    directory = Path(args[-1])
                    for name in names:
                        (directory / name).write_bytes((bundle / name).read_bytes())
                return ""
            with patch.object(release, "gh", side_effect=fake_gh):
                result = release.verify("v-test", bundle, root / "verified.json")
                self.assertFalse(result["upload"])
                names.pop()
                with self.assertRaisesRegex(publication.PublicationError, "incomplete"):
                    release.verify("v-test", bundle, root / "incomplete.json")
            self.assertFalse((root / "incomplete.json").exists())

    def test_upload_plan_rejects_unexpected_names(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = self.bundle(root)
            plan = root / "plan.json"
            release.write_plan({"upload": ["unexpected"], "finalize": True, "create_draft": True}, plan)
            with patch.object(release, "gh") as gh:
                with self.assertRaisesRegex(publication.PublicationError, "unsafe"):
                    release.upload("v-test", bundle, plan)
                gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
