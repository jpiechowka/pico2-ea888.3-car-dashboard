"""Radio provenance rejects changed, missing and additional source bytes."""
import hashlib
import tempfile
import unittest
from pathlib import Path

from check_radio_vendor import verify_tree


class Provenance(unittest.TestCase):
    def test_only_top_level_cargo_output_is_ignored(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'src').mkdir()
            (root / 'src/lib.rs').write_bytes(b'source')
            expected = {'src/lib.rs': hashlib.sha256(b'source').hexdigest()}
            (root / 'target').mkdir()
            (root / 'target/build-output').write_bytes(b'generated')
            verify_tree(root, expected, {})
            (root / 'src/target').mkdir()
            (root / 'src/target/extra.rs').write_bytes(b'not generated')
            with self.assertRaises(AssertionError):
                verify_tree(root, expected, {})

    def test_exact_bytes_and_reviewed_overlay(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'src').mkdir()
            (root / 'src/lib.rs').write_bytes(b'patched\n')
            digest = lambda data: hashlib.sha256(data).hexdigest()
            original = {'src/lib.rs': digest(b'original\n')}
            patched = {'src/lib.rs': digest(b'patched\n')}
            verify_tree(root, original, patched)
            for contents in [b'patched\r\n', b'changed']:
                (root / 'src/lib.rs').write_bytes(contents)
                with self.assertRaises(AssertionError):
                    verify_tree(root, original, patched)
            (root / 'src/lib.rs').write_bytes(b'patched\n')
            (root / 'extra.rs').write_bytes(b'ignored?')
            with self.assertRaises(AssertionError):
                verify_tree(root, original, patched)
            (root / 'extra.rs').unlink()
            (root / 'src/lib.rs').unlink()
            with self.assertRaises(AssertionError):
                verify_tree(root, original, patched)

    def test_paths_hashes_and_empty_manifest_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name in ['../outside', '/absolute', 'a\\b', 'a/../b', 'a//b', 'C:/outside', '.']:
                with self.assertRaises(AssertionError, msg=name):
                    verify_tree(root, {name: '0' * 64}, {})
            for manifest in [{}, {'a': '0' * 63}, {'a': 'g' * 64}, {'a': None}]:
                with self.assertRaises(AssertionError):
                    verify_tree(root, manifest, {})


if __name__ == '__main__':
    unittest.main()
