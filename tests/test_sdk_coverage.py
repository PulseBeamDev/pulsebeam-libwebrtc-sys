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

    def deleted_constructor(self):
        self.header.write_text("struct Public {\n  Public() = delete;\n  static void Other();\n};\n")
        header = self.inventory["headers"]["api/public.h"]
        header["sha256"] = sdk_coverage.digest(self.header)
        identifier = self.identifier + "@F@Public#"
        declaration = {"id": identifier, "name": "webrtc::Public::Public",
                       "kind": "c++.method", "header": "api/public.h", "line": 2, "column": 3}
        header["profiles"]["core"]["declarations"].append(declaration)
        entry = {"declaration": identifier, "profiles": ["core"],
                 "availability": "deleted_default_constructor",
                 "reason": "The SDK explicitly deletes default construction.",
                 "evidence": {"tree": "sdk", "path": "api/public.h", "anchor": "Public() = delete;"},
                 "verification": copy.deepcopy(self.entry["verification"])}
        self.mapping["declaration_availability"] = [entry]
        return entry, declaration

    def test_deleted_constructor_stays_in_denominator_and_is_not_a_working_binding(self):
        entry, _ = self.deleted_constructor()
        report = self.check()
        self.assertEqual(report["declarations"], 2)
        self.assertEqual(report["mapped"], 1)
        self.assertEqual(report["unmapped"], [])
        self.assertEqual(report["unavailable_declarations"][0]["declaration"], entry["declaration"])
        self.assertEqual(report["unavailable_declarations"][0]["evidence"]["line"], 2)
        self.mapping["declaration_availability"] = []
        self.assertEqual(self.check()["unmapped"], [entry["declaration"]])

    def test_unavailability_cannot_waive_bindings_or_use_another_declarations_source(self):
        entry, _ = self.deleted_constructor()
        entry["availability"] = "missing_wrapper"
        with self.assertRaisesRegex(ValueError, "unsupported declaration availability"):
            self.check()
        entry["availability"] = "deleted_default_constructor"
        entry["evidence"]["anchor"] = "static void Other();"
        with self.assertRaisesRegex(ValueError, "exact deleted default constructor"):
            self.check()

    def test_deleted_constructor_requires_exact_source_not_a_delete_comment_or_parameters(self):
        entry, _ = self.deleted_constructor()
        for source in ("Public(); // = delete;", "Public(int) = delete;", "Public() = default;"):
            with self.subTest(source=source):
                self.header.write_text("struct Public {\n  " + source + "\n};\n")
                self.inventory["headers"]["api/public.h"]["sha256"] = sdk_coverage.digest(self.header)
                entry["evidence"]["anchor"] = source
                with self.assertRaisesRegex(ValueError, "exact deleted default constructor"):
                    self.check()

    def test_unavailability_must_match_all_profile_locations_and_require_verification(self):
        entry, declaration = self.deleted_constructor()
        native = copy.deepcopy(declaration)
        native["line"] = 3
        self.inventory["headers"]["api/public.h"]["profiles"]["native"]["declarations"] = [native]
        with self.assertRaisesRegex(ValueError, "wrong unavailable declaration profiles"):
            self.check()
        entry["profiles"].append("native")
        with self.assertRaisesRegex(ValueError, "exact deleted default constructor"):
            self.check()
        native["line"] = 2
        entry["verification"] = []
        with self.assertRaisesRegex(ValueError, "missing unavailable declaration verification"):
            self.check()

    def test_unknown_duplicate_and_already_mapped_unavailability_is_rejected(self):
        entry, _ = self.deleted_constructor()
        self.mapping["declaration_availability"].append(copy.deepcopy(entry))
        with self.assertRaisesRegex(ValueError, "duplicate or mapped unavailable"):
            self.check()
        self.mapping["declaration_availability"].pop()
        self.mapping["mappings"].append({**self.entry, "declaration": entry["declaration"]})
        with self.assertRaisesRegex(ValueError, "duplicate or mapped unavailable"):
            self.check()
        self.mapping["mappings"].pop()
        entry["declaration"] = "unknown"
        with self.assertRaisesRegex(ValueError, "unknown, duplicate"):
            self.check()

    def deleted_copy_operation(self, assignment=False):
        source = ("operator=(const Public& other) = delete;" if assignment
                  else "Public(const Public& other) = delete;")
        prefix = "Public& " if assignment else ""
        self.header.write_text("struct Public {\n  " + prefix + source + "\n};\n")
        header = self.inventory["headers"]["api/public.h"]
        header["sha256"] = sdk_coverage.digest(self.header)
        member = "operator=" if assignment else "Public"
        identifier = self.identifier + "@F@" + member + "#&1$@S@Public#"
        declaration = {"id": identifier, "name": "webrtc::Public::" + member,
                       "kind": "c++.method", "header": "api/public.h", "line": 2,
                       "column": 3 + len(prefix)}
        header["profiles"]["core"]["declarations"] = [{"id": self.identifier}, declaration]
        entry = {"declaration": identifier, "profiles": ["core"],
                 "availability": "deleted_copy_operation",
                 "reason": "The SDK explicitly deletes this copy operation.",
                 "evidence": {"tree": "sdk", "path": "api/public.h", "anchor": source},
                 "verification": copy.deepcopy(self.entry["verification"])}
        self.mapping["declaration_availability"] = [entry]
        return entry, declaration

    def test_deleted_copy_operations_remain_unavailable_not_implemented(self):
        for assignment in (False, True):
            with self.subTest(assignment=assignment):
                # Each operation is an independent declaration, not a class-wide
                # waiver of its other constructors or assignment overloads.
                entry, declaration = self.deleted_copy_operation(assignment)
                if not assignment:
                    declaration["name"] += "<T>"
                report = self.check()
                self.assertEqual(report["declarations"], 2)
                self.assertEqual(report["mapped"], 1)
                self.assertEqual(report["unmapped"], [])
                self.assertEqual(report["unavailable_declarations"][0]["declaration"], entry["declaration"])

    def test_deleted_copy_policy_rejects_other_operations_and_comment_spelling(self):
        entry, declaration = self.deleted_copy_operation()
        for source in ("Public(Public&& other) = delete;", "Public(Public other) = delete;",
                       "Public(const Other& other) = delete;", "Public() = delete;",
                       "Public(const Public& other); // = delete;",
                       "Public(const Public& other) = default;"):
            with self.subTest(source=source):
                self.header.write_text("struct Public {\n  " + source + "\n};\n")
                self.inventory["headers"]["api/public.h"]["sha256"] = sdk_coverage.digest(self.header)
                entry["evidence"]["anchor"] = source
                with self.assertRaisesRegex(ValueError, "exact deleted copy operation"):
                    self.check()
        entry, declaration = self.deleted_copy_operation(assignment=True)
        declaration["name"] = "webrtc::Public::Other"
        with self.assertRaisesRegex(ValueError, "exact deleted copy operation"):
            self.check()

    def test_deleted_copy_evidence_matches_every_profile_location(self):
        entry, declaration = self.deleted_copy_operation(assignment=True)
        native = copy.deepcopy(declaration)
        self.inventory["headers"]["api/public.h"]["profiles"]["native"]["declarations"] = [native]
        entry["profiles"].append("native")
        self.assertEqual(len(self.check()["unavailable_declarations"]), 1)
        native["column"] = 3
        with self.assertRaisesRegex(ValueError, "exact deleted copy operation"):
            self.check()

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
        for entry in mapping.get("declaration_availability", []):
            references.append(entry["evidence"])
            references.extend(entry["verification"])
        for reference in references:
            if reference["tree"] == "sdk" and not sdk.is_dir():
                continue
            with self.subTest(reference=reference):
                sdk_coverage.source_reference(reference, ROOT, sdk)


@unittest.skipUnless(CLANG.is_file(), "pinned Clang is required for the ExtractAPI boundary")
class ExtractAPITests(unittest.TestCase):
    def test_pinned_sdk_environment_deletes_default_construction_but_preserves_copy_and_move(self):
        work = ROOT / ".work"
        sdk = work / "checkout/src"
        manifests = [work / f"package/webrtc-{flavor}-linux-x86_64/manifest.json"
                     for flavor in sdk_coverage.PROFILES]
        if not all(path.is_file() for path in manifests):
            self.skipTest("matching core/native packages are required for the SDK type-trait boundary")
        with tempfile.TemporaryDirectory() as temporary:
            probe = Path(temporary) / "environment.cc"
            probe.write_text('''#include "api/environment/environment.h"
static_assert(!__is_constructible(webrtc::Environment));
static_assert(__is_constructible(webrtc::Environment, const webrtc::Environment&));
static_assert(__is_constructible(webrtc::Environment, webrtc::Environment&&));
static_assert(__is_assignable(webrtc::Environment&, const webrtc::Environment&));
static_assert(__is_assignable(webrtc::Environment&, webrtc::Environment&&));
''')
            for flavor, manifest in zip(sdk_coverage.PROFILES, manifests):
                with self.subTest(flavor=flavor):
                    arguments = sdk_coverage.compiler_arguments(
                        sdk, manifest.parent, work / f"out/{flavor}/linux-x86_64",
                        json.loads(manifest.read_text()))
                    arguments.remove("-extract-api")
                    arguments[arguments.index("c++-header")] = "c++"
                    result = subprocess.run([*arguments, "-fsyntax-only", str(probe)],
                                            text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)

    def test_pinned_sdk_result_deletes_copy_but_preserves_value_and_move_operations(self):
        work = ROOT / ".work"
        sdk = work / "checkout/src"
        manifests = [work / f"package/webrtc-{flavor}-linux-x86_64/manifest.json"
                     for flavor in sdk_coverage.PROFILES]
        if not all(path.is_file() for path in manifests):
            self.skipTest("matching core/native packages are required for the SDK type-trait boundary")
        with tempfile.TemporaryDirectory() as temporary:
            probe = Path(temporary) / "result.cc"
            probe.write_text('''#include "api/rtc_error.h"
#include <memory>
using Result = webrtc::RTCErrorOr<int>;
static_assert(!__is_constructible(Result, const Result&));
static_assert(!__is_assignable(Result&, const Result&));
static_assert(__is_constructible(Result));
static_assert(__is_constructible(Result, webrtc::RTCError&&));
static_assert(__is_constructible(Result, const int&));
static_assert(__is_constructible(Result, int&&));
static_assert(__is_constructible(Result, Result&&));
static_assert(__is_assignable(Result&, Result&&));
using OwnedResult = webrtc::RTCErrorOr<std::unique_ptr<int>>;
static_assert(!__is_constructible(OwnedResult, const OwnedResult&));
static_assert(!__is_assignable(OwnedResult&, const OwnedResult&));
static_assert(__is_constructible(OwnedResult, std::unique_ptr<int>&&));
static_assert(__is_constructible(OwnedResult, OwnedResult&&));
static_assert(__is_assignable(OwnedResult&, OwnedResult&&));
''')
            for flavor, manifest in zip(sdk_coverage.PROFILES, manifests):
                with self.subTest(flavor=flavor):
                    arguments = sdk_coverage.compiler_arguments(
                        sdk, manifest.parent, work / f"out/{flavor}/linux-x86_64",
                        json.loads(manifest.read_text()))
                    arguments.remove("-extract-api")
                    arguments[arguments.index("c++-header")] = "c++"
                    result = subprocess.run([*arguments, "-fsyntax-only", str(probe)],
                                            text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)

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
  Example() = delete;
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
            deleted = next(item for item in declarations if item["name"] == "webrtc::Example::Example")
            self.assertEqual(deleted["declaration"], "Example();")
            self.assertEqual((sdk / "public.h").read_text().splitlines()[deleted["line"] - 1].strip(),
                             "Example() = delete;")
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
