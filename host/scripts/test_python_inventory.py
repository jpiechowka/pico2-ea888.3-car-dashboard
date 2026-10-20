"""Coverage inventory must include newly added scripts and reject unsafe paths."""

from pathlib import Path
import tempfile
import unittest

from check_python import source_paths


class PythonInventory(unittest.TestCase):
    def test_nested_tests_vendor_and_untracked_names_are_all_included(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = ["new.py", "vendor/generate.py", "tests/test_case.py", "-option.py"]
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("", encoding="utf-8")
            listed = ("\0".join([*names, "new.py"]) + "\0").encode()
            self.assertEqual(
                {p.relative_to(root.resolve()).as_posix() for p in source_paths(root, listed)},
                set(names),
            )
            self.assertTrue(all(path.is_absolute() for path in source_paths(root, listed)))

    def test_invalid_empty_traversal_missing_and_oversized_inventory_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.py").write_text("", encoding="utf-8")
            for listing in (
                b"",
                b"a.py",
                b"\0",
                b"../a.py\0",
                b"/a.py\0",
                b"a//b.py\0",
                b"./a.py\0",
                b"a\\b.py\0",
                b"C:/a.py\0",
                b"a\n.py\0",
                b"missing.py\0",
                b"a.rs\0",
                b"a.py\0" * 4097,
                b"x" * (4 * 1024 * 1024) + b"\0",
            ):
                with self.subTest(listing=listing[:40]), self.assertRaises(ValueError):
                    source_paths(root, listing)
            self.assertEqual(source_paths(root, b"a.py\0"), [root.resolve() / "a.py"])


if __name__ == "__main__":
    unittest.main()
