"""Corrupted source and accidentally accepted invalid profiles must fail at -O."""
from pathlib import Path
import runpy
import subprocess
import unittest
from unittest.mock import patch
import check_vendor
import check_memory_layout


class ValidationPolicy(unittest.TestCase):
    def test_configuration_stack_budget_has_exact_64k_boundary(self):
        for headroom in (-1, 0, 32768, 65535):
            with self.assertRaises(AssertionError):
                check_memory_layout.check_headroom(headroom)
        for headroom in (65536, 65544, 131072):
            check_memory_layout.check_headroom(headroom)

    def test_modified_vendor_source_is_rejected(self):
        original = Path.read_bytes
        changed = check_vendor.ROOT / 'embassy-rp/src/lib.rs'
        with patch.object(Path, 'read_bytes', lambda p: original(p) + (b'changed' if p == changed else b'')):
            with self.assertRaisesRegex(AssertionError, 'vendor mismatch'):
                check_vendor.check()

    def test_invalid_feature_compile_success_is_rejected(self):
        accepted = subprocess.CompletedProcess([], 0, '', '')
        with patch('subprocess.run', return_value=accepted):
            with self.assertRaises(AssertionError):
                runpy.run_path(str(Path(__file__).with_name('check_feature_profiles.py')), run_name='__main__')


if __name__ == '__main__':
    unittest.main()
