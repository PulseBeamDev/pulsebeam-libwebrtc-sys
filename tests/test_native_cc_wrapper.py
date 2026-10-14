import importlib.util
import json
from pathlib import Path
import shlex
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_cc_wrapper", ROOT / "tools/native_cc_wrapper.py")
wrapper = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(wrapper)


class NativeCompilerWrapperTests(unittest.TestCase):
    def test_contract_flags_are_final_without_rewriting_native_arguments(self):
        arguments = ["/cache/sccache", "/llvm/clang++", "-ffast-math", "-ffp-contract=fast",
                     "@native.rsp", "-c", "common_audio/smoothing_filter.cc", "-o", "filter.o"]
        command = wrapper.compiler_command(arguments)
        self.assertEqual(command[:-2], arguments)
        self.assertEqual(command[-2:], ["-fno-fast-math", "-ffp-contract=off"])
        self.assertEqual(arguments[-1], "filter.o")

    def test_gn_command_retains_quoted_paths_and_wrapper_identity(self):
        cache = "/a path/it's sccache"
        encoded = wrapper.gn_wrapper(cache)
        command = shlex.split(json.loads(encoded))
        self.assertEqual(command[1], str(ROOT / "tools/native_cc_wrapper.py"))
        self.assertEqual(command[2:4], ["--contract-sha256", wrapper.source_digest()])
        self.assertEqual(command[-1], cache)
        configuration = wrapper.floating_point_configuration(f"target_cpu=\"x64\" cc_wrapper={encoded}")
        self.assertEqual(configuration["compiler_wrapper_sha256"], wrapper.source_digest())
        self.assertEqual(configuration["final_compiler_flags"], ["-fno-fast-math", "-ffp-contract=off"])

    def test_manifest_contract_rejects_missing_duplicate_or_stale_wrapper(self):
        valid = wrapper.gn_wrapper("/cache/sccache")
        cases = ["target_cpu=\"x64\"", 'cc_wrapper="/cache/sccache"',
                 f"cc_wrapper={valid} cc_wrapper={valid}",
                 f"cc_wrapper={valid.replace(wrapper.source_digest(), '0' * 64)}"]
        for gn_args in cases:
            with self.subTest(gn_args=gn_args), self.assertRaises(ValueError):
                wrapper.floating_point_configuration(gn_args)

    def test_compiler_is_required(self):
        for arguments in ([], ["/cache/sccache"]):
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                wrapper.compiler_command(arguments)


if __name__ == "__main__":
    unittest.main()
