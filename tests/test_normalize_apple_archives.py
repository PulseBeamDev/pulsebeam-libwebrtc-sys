import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools.normalize_apple_archives import normalize


class AppleArchiveNormalizationTests(unittest.TestCase):
    def test_only_darwin_index_members_are_deleted_by_their_exact_names(self):
        members = "__.SYMDEF\n__.SYMDEF SORTED\n/private/build/code_generator.o\nother/code_generator.o\n"
        with patch("tools.normalize_apple_archives.subprocess.run") as run:
            run.return_value.stdout = members
            normalize("llvm-ar", Path("space path/libprotoc_lib.a"))
        self.assertEqual([call.args[0] for call in run.call_args_list], [
            ["llvm-ar", "t", "space path/libprotoc_lib.a"],
            ["llvm-ar", "--format=gnu", "d", "space path/libprotoc_lib.a", "__.SYMDEF", "__.SYMDEF SORTED"],
            ["llvm-ar", "--format=gnu", "s", "space path/libprotoc_lib.a"],
        ])

    def test_an_archive_without_metadata_is_still_reindexed_in_gnu_format(self):
        with patch("tools.normalize_apple_archives.subprocess.run") as run:
            run.return_value.stdout = "object.o\n"
            normalize("llvm-ar", Path("lib.a"))
        self.assertEqual(len(run.call_args_list), 2)
        self.assertEqual(run.call_args_list[-1].args[0], ["llvm-ar", "--format=gnu", "s", "lib.a"])

    def test_real_archives_retain_both_same_named_macho_objects_and_link(self):
        bin_dir = os.environ.get("PULSEBEAM_TEST_LLVM_BIN")
        def tool(name):
            if bin_dir:
                return str(Path(bin_dir) / name)
            return shutil.which(name + "-19") or shutil.which(name)
        clang, ar, linker = (tool(name) for name in ("clang", "llvm-ar", "ld64.lld"))
        if not all((clang, ar, linker)):
            self.skipTest("requires Clang, llvm-ar and ld64.lld")
        with tempfile.TemporaryDirectory(prefix="Apple archive space ") as temp:
            root = Path(temp)
            for arch in ("arm64", "x86_64"):
                objects = []
                for name, code in (("one", "int one(void) { return 1; }"),
                                   ("two", "int two(void) { return 2; }"),
                                   ("main", "int one(void); int two(void); int main(void) { return one()+two(); }")):
                    directory = root / arch / name
                    directory.mkdir(parents=True)
                    source, obj = directory / "entry.c", directory / "entry.o"
                    source.write_text(code)
                    subprocess.run([clang, "--target=" + arch + "-apple-macos12", "-c", str(source), "-o", str(obj)], check=True, capture_output=True)
                    objects.append(obj)
                metadata = root / arch / "__.SYMDEF"
                metadata.write_bytes(b"stale Darwin index")
                for shape, flags in (("thin", ["qcT"]), ("darwin", ["--format=darwin", "qc"])):
                    archive = root / (arch + "-" + shape + ".a")
                    subprocess.run([ar, *flags, str(archive), *map(str, objects[:2]), str(metadata)], check=True, capture_output=True)
                    link = [linker, "-arch", arch, "-platform_version", "macos", "12.0", "15.5",
                            "-all_load", "-fatal_warnings", "-e", "_main", "-o", str(root / (arch + "-binary")), str(objects[2]), str(archive)]
                    broken = subprocess.run(link, capture_output=True, text=True)
                    self.assertNotEqual(broken.returncode, 0, f"{arch}/{shape}")
                    self.assertIn("__.SYMDEF", broken.stderr)
                    normalize(ar, archive)
                    expected_magic = b"!<thin>\n" if shape == "thin" else b"!<arch>\n"
                    self.assertTrue(archive.read_bytes().startswith(expected_magic))
                    members = subprocess.run([ar, "t", str(archive)], check=True, capture_output=True, text=True).stdout.splitlines()
                    expected = list(map(str, objects[:2])) if shape == "thin" else ["entry.o", "entry.o"]
                    self.assertEqual(members, expected)
                    subprocess.run(link, check=True, capture_output=True)
                    # Repeat safely, including when no stray index remains.
                    normalize(ar, archive)
                    subprocess.run(link, check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
