"""Scope and mapping invariants, plus an optional pinned-frontend boundary test."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import sdk_coverage

CLANG = ROOT / ".work/checkout/src/third_party/llvm-build/Release+Asserts/bin/clang++"


class ScopeTests(unittest.TestCase):
    def test_sdk_tables_control_depth_and_do_not_exclude_api_test_by_name(self):
        with tempfile.TemporaryDirectory() as temporary:
            sdk = Path(temporary)
            (sdk / "native-api.md").write_text(
                "`api` | Yes\n`legacy` | No\n"
            )
            for header in ("api/a.h", "api/test/public.h", "api/nested/new.hpp",
                           "legacy/a.h", "legacy/nested/not_public.h"):
                path = sdk / header
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("// header\n")
            scopes, headers = sdk_coverage.sdk_scope(sdk)
            self.assertEqual(scopes, [{"directory": "api", "recursive": True},
                                      {"directory": "legacy", "recursive": False}])
            self.assertEqual(headers, ["api/a.h", "api/nested/new.hpp", "api/test/public.h", "legacy/a.h"])
            (sdk / "api/new_public.h").write_text("struct NewPublic {};\n")
            self.assertIn("api/new_public.h", sdk_coverage.sdk_scope(sdk)[1])

    def test_gn_public_and_source_ownership_preserves_mixed_production_headers(self):
        properties = [
            {"//api:prod": {"sources": ["//api/test/public.h"]},
             "//api:tests": {"sources": ["//api/test/public.h", "//api/mock.h"]}},
            {"//api:prod": {"public": ["//api/explicit.h"]},
             "//api:tests": {"public": "*"}},
            {"//api:prod": {"testonly": False}, "//api:tests": {"testonly": True}},
            {"//api:prod": {"include_dirs": ["//third_party/example"]}},
        ]
        with patch.object(sdk_coverage.subprocess, "check_output", side_effect=map(json.dumps, properties)):
            owners, includes = sdk_coverage.gn_metadata(Path("/sdk"), Path("/build"))
        self.assertEqual(owners["api/test/public.h"], {"//api:prod": False, "//api:tests": True})
        self.assertEqual(owners["api/explicit.h"], {"//api:prod": False})
        self.assertTrue(all(owners["api/mock.h"].values()))
        self.assertEqual(includes["//api:prod"], ["/sdk/third_party/example"])

    def test_implementation_names_follow_sdk_tokens_not_substring_matches(self):
        for name in ("internal", "foo_internal", "CamelCaseImpl", "InternalState", "impl",
                     "RTC_INTERNAL_STATE", "RTC_IMPL_HELPER"):
            with self.subTest(name=name):
                self.assertTrue(sdk_coverage.implementation_name(name))
        for name in ("SimpleStringBuilder", "SimplifiedEncoder", "ImplicitCast", "PublicAPI"):
            with self.subTest(name=name):
                self.assertFalse(sdk_coverage.implementation_name(name))

    def test_compiler_substitution_cannot_be_reported_as_the_pinned_toolchain(self):
        with patch.object(sdk_coverage.subprocess, "check_output", return_value="other clang\n"):
            with self.assertRaisesRegex(ValueError, "qualified artifact toolchain"):
                sdk_coverage.compiler_arguments(Path("/sdk"), Path("/artifact"), Path("/build"),
                                                {"abi": {"toolchain": "pinned clang"}})

    def test_parser_failure_remains_unresolved_not_empty_or_unsupported(self):
        failure = subprocess.CompletedProcess([], 1, "", "missing generated header")
        with patch.object(sdk_coverage.subprocess, "run", return_value=failure):
            result = sdk_coverage.extract_header(["clang++"], Path("/sdk"), "api/public.h")
        self.assertEqual(result, {"status": "unresolved", "diagnostic": "missing generated header"})


class MappingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sdk = self.root / "sdk"
        (self.sdk / "api").mkdir(parents=True)
        self.header = self.sdk / "api/public.h"
        self.header.write_text("struct Public {};\n")
        (self.sdk / "native-api.md").write_text("`api` | Yes\n")
        (self.root / "binding.rs").write_text("pub fn binding() {}\nfn install() {}\nfn regression() {}\n")
        self.identifier = "api/public.h#c:@S@Public"
        self.inventory = {
            "schema": 1, "sdk_revision": "pin", "configuration_macros": ["WEBRTC_OPTION"],
            "profiles": {"core": {}, "native": {}},
            "scope": sdk_coverage.sdk_scope(self.sdk)[0],
            "scope_sha256": sdk_coverage.digest(self.sdk / "native-api.md"),
            "headers": {"api/public.h": {
                "sha256": hashlib.sha256(self.header.read_bytes()).hexdigest(),
                "profiles": {"core": {"status": "parsed", "declarations": [{"id": self.identifier}]},
                             "native": {"status": "parsed", "declarations": []}}
            }}
        }
        self.entry = {
            "declaration": self.identifier, "profiles": ["core"], "state": "implemented",
            "semantics": "Native ownership, not a local echo.",
            **{field: [{"tree": "repository", "path": "binding.rs", "anchor": anchor}]
               for field, anchor in (("rust", "pub fn binding"), ("installation", "fn install"),
                                     ("verification", "fn regression"))}
        }
        self.mapping = {"schema": 1, "sdk_revision": "pin", "mappings": [self.entry]}

    def check(self):
        return sdk_coverage.check_mapping(self.inventory, self.mapping, self.root, self.sdk)

    def test_added_declaration_is_unmapped_without_changing_the_rust_subset(self):
        report = self.check()
        self.assertEqual(report["mapped"], 1)
        self.assertEqual(report["unmapped_configuration_macros"], ["WEBRTC_OPTION"])
        other = {"id": "api/public.h#c:@S@NewPublic"}
        self.inventory["headers"]["api/public.h"]["profiles"]["core"]["declarations"].append(other)
        self.assertEqual(self.check()["unmapped"], [other["id"]])

    def test_partial_paths_are_not_counted_as_complete_mappings(self):
        self.entry.update(state="partial", gaps=["missing actual consumer"])
        self.assertEqual(self.check()["partial"], [self.identifier])
        del self.entry["gaps"]
        with self.assertRaisesRegex(ValueError, "partial mapping without gaps"):
            self.check()

    def test_unknown_duplicate_and_wrong_profile_mappings_fail(self):
        self.mapping["mappings"].append(copy.deepcopy(self.entry))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.check()
        self.mapping["mappings"].pop()
        self.entry["declaration"] = "not-a-native-declaration"
        with self.assertRaisesRegex(ValueError, "unknown"):
            self.check()
        self.entry["declaration"] = self.identifier
        self.entry["profiles"].append("native")
        with self.assertRaisesRegex(ValueError, "wrong declaration profiles"):
            self.check()

    def test_profile_removal_cannot_shrink_the_denominator(self):
        profiles = self.inventory["headers"]["api/public.h"]["profiles"]
        native = profiles.pop("native")
        with self.assertRaisesRegex(ValueError, "profile records"):
            self.check()
        profiles["native"] = native
        self.inventory["profiles"].pop("native")
        with self.assertRaisesRegex(ValueError, "both core/native profiles"):
            self.check()

    def test_unknown_or_unjustified_dispositions_cannot_hide_declarations(self):
        profiles = self.inventory["headers"]["api/public.h"]["profiles"]
        for metadata in (
            {"status": "ignored"},
            {"status": "gn_testonly", "gn_owners": {"//api:prod": False}},
            {"status": "gn_testonly", "gn_owners": {}},
            {"status": "non_linux"},
            {"status": "parsed"},
        ):
            with self.subTest(metadata=metadata):
                profiles["core"] = metadata
                with self.assertRaises(ValueError):
                    self.check()

    def test_missing_installation_or_stale_source_anchors_fail(self):
        self.entry["installation"] = []
        with self.assertRaisesRegex(ValueError, "missing installation"):
            self.check()
        self.entry["installation"] = [{"tree": "repository", "path": "binding.rs", "anchor": "fn removed"}]
        with self.assertRaisesRegex(ValueError, "exactly once"):
            self.check()

    def test_source_and_sdk_identity_drift_fail(self):
        self.mapping["sdk_revision"] = "other-pin"
        with self.assertRaisesRegex(ValueError, "SDK revision mismatch"):
            self.check()
        self.mapping["sdk_revision"] = "pin"
        self.header.write_text("struct Changed {};\n")
        with self.assertRaisesRegex(ValueError, "SDK header changed"):
            self.check()

    def test_stale_inventory_cannot_hide_a_new_header_or_changed_scope(self):
        new = self.sdk / "api/new.h"
        new.write_text("struct NewlyPublic {};\n")
        with self.assertRaisesRegex(ValueError, "header set changed"):
            self.check()
        new.unlink()
        (self.sdk / "native-api.md").write_text("`api` | Yes\nUpdated scope policy\n")
        with self.assertRaisesRegex(ValueError, "scope changed"):
            self.check()

    def test_unresolved_header_is_retained_in_the_report(self):
        self.inventory["headers"]["api/public.h"]["profiles"]["native"] = {
            "status": "unresolved", "diagnostic": "missing dependency"
        }
        self.assertEqual(self.check()["unresolved_headers"], ["native: api/public.h"])

    def test_configuration_requires_real_paths_and_cannot_be_silently_dropped(self):
        entry = {key: value for key, value in self.entry.items()
                 if key not in ("declaration", "state", "profiles")}
        entry["macro"] = "WEBRTC_OPTION"
        self.mapping["configuration_mappings"] = [entry]
        self.assertEqual(self.check()["unmapped_configuration_macros"], [])
        entry["installation"] = []
        with self.assertRaisesRegex(ValueError, "incomplete configuration"):
            self.check()


class MappingSourceTests(unittest.TestCase):
    def test_durable_mapping_source_anchors_are_real_and_unique(self):
        mapping = json.loads((ROOT / "docs/native-sdk-mapping.json").read_text())
        sdk = ROOT / ".work/checkout/src"
        references = []
        for entry in mapping["mappings"] + mapping.get("configuration_mappings", []) + mapping["retained_capabilities"]:
            for field in ("rust", "installation", "verification"):
                references.extend(entry[field])
        for entry in mapping["header_availability"].values():
            references.extend(entry["evidence"])
        for reference in references:
            if reference["tree"] == "sdk" and not sdk.is_dir():
                continue
            with self.subTest(reference=reference):
                sdk_coverage.source_reference(reference, ROOT, sdk)


@unittest.skipUnless(CLANG.is_file(), "pinned Clang is required for the ExtractAPI boundary")
class ExtractAPITests(unittest.TestCase):
    def test_real_frontend_preserves_overloads_templates_fields_and_macro_contracts(self):
        with tempfile.TemporaryDirectory() as temporary:
            sdk = Path(temporary)
            (sdk / "public.h").write_text('''#pragma once
#define PUBLIC_LIMIT 64
namespace webrtc {
namespace internal { struct Hidden { int field; }; }
class Example {
 private:
  struct Hidden { public: int nested; };
 protected:
  void Protected();
 public:
  enum class Mode { First, Second };
  int value;
  virtual int Call(int input) const = 0;
  virtual int Call(double input) const = 0;
};
template<class T> struct SimpleValue { T field; T Get() const; };
using Number = unsigned;
constexpr int Constant = 42;
int Function(int input = 4);
int Function(double input);
}
''')
            result = sdk_coverage.extract_header(
                [str(CLANG), "-extract-api", "-x", "c++-header", "-std=c++20"], sdk, "public.h"
            )
            self.assertEqual(result["status"], "parsed")
            declarations = result["declarations"]
            names = [item["name"] for item in declarations]
            self.assertEqual(names.count("webrtc::Example::Call"), 2)
            self.assertEqual(names.count("webrtc::Function"), 2)
            self.assertIn("webrtc::Example::value", names)
            self.assertIn("webrtc::Example::Mode::First", names)
            self.assertIn("webrtc::SimpleValue::field", names)
            self.assertIn("webrtc::SimpleValue::Get", names)
            self.assertIn("webrtc::Number", names)
            self.assertIn("webrtc::Constant", names)
            self.assertIn("PUBLIC_LIMIT", names)
            self.assertFalse(any("Hidden" in name or "Protected" in name for name in names))
            calls = [item["id"] for item in declarations if item["name"] == "webrtc::Example::Call"]
            self.assertEqual(len(set(calls)), 2)
            for item in declarations:
                self.assertGreater(item["line"], 0)
                self.assertEqual(item["header"], "public.h")
            self.assertGreater(result["excluded_symbols"]["private_or_protected"], 0)
            self.assertGreater(result["excluded_symbols"]["sdk_internal_or_impl"], 0)


if __name__ == "__main__":
    unittest.main()
