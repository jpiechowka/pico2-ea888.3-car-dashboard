"""Frame metadata and cumulative-budget regressions, including rejected fixes."""
import struct
import unittest
from check_stack_budget import frames, budgets, task_frames


class StackBudget(unittest.TestCase):
    def test_outlined_future_body_cannot_hide_behind_small_poll_shim(self):
        functions = {'TaskStorage_dashboard_task_4poll': (101, 1, 0),
                     'dashboard_task_body': (201, 1, 0),
                     'dashboard_task_alias': (201, 1, 0),
                     'dashboard_task_write_in_place': (301, 1, 0)}
        self.assertEqual(task_frames(functions, {100: 16, 200: 17936}, 'dashboard_task'), 17952)
        with self.assertRaises(AssertionError):
            task_frames(functions, {100: 16}, 'dashboard_task')
        with self.assertRaises(AssertionError):
            task_frames({}, {}, 'dashboard_task')

    def test_metadata_decodes_thumb_addresses_and_variable_sizes(self):
        self.assertEqual(frames(struct.pack('<I', 0x10000101) + b'\xac\x02'), {0x10000100: 300})

    def test_metadata_rejects_truncation_overflow_and_conflicting_duplicates(self):
        for data in (b'', b'abc', struct.pack('<I', 1), struct.pack('<I', 1) + b'\x80',
                     struct.pack('<I', 1) + b'\xff\xff\xff\xff\x10',
                     struct.pack('<I', 1) + b'\x80\x00',
                     struct.pack('<IBIB', 1, 2, 1, 3)):
            with self.subTest(data=data), self.assertRaises(AssertionError):
                frames(data)

    def test_original_overflow_and_incomplete_helper_fix_are_rejected(self):
        for poll, constructor in ((38576, 0), (22528, 31808), (5400, 31520)):
            with self.subTest(poll=poll), self.assertRaises(AssertionError):
                budgets(50344, poll + constructor + 1024, 10056, 30000, 68648)

    def test_boot_nested_decode_regression_is_rejected(self):
        with self.assertRaises(AssertionError):
            budgets(87124, 25456, 10056, 30000, 68648)

    def test_final_candidate_and_exact_reserve_boundaries(self):
        budgets(50344, 25456, 10056, 30000, 68648)
        budgets(65536 - 8192, 32768 - 4096, 32768 - 4096, 65536 - 8192, 65536)
        for boot, construct, runtime, dashboard, headroom in (
            (65536 - 8192 + 1, 0, 0, 0, 65536),
            (0, 32768 - 4096 + 1, 0, 0, 65536),
            (0, 0, 32768 - 4096 + 1, 0, 65536),
            (0, 0, 0, 65536 - 8192 + 1, 65536),
        ):
            with self.assertRaises(AssertionError):
                budgets(boot, construct, runtime, dashboard, headroom)


if __name__ == '__main__':
    unittest.main()
