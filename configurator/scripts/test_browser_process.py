"""Disposable process-tree tests; no browser, USB or shared server is used."""
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest

from browser_process import OwnedProcess


class OwnershipTests(unittest.TestCase):
    def exercise(self, root_exits):
        with tempfile.TemporaryDirectory(prefix='studio-process-test-') as folder:
            marker = pathlib.Path(folder) / 'heartbeat'
            child = (
                'import pathlib,time,signal; '
                'signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                f'p=pathlib.Path({str(marker)!r}); '
                '\nwhile True:\n p.write_text(str(time.time_ns())); time.sleep(0.03)'
            )
            parent = (
                'import subprocess,sys,time; '
                f'subprocess.Popen([sys.executable,"-c",{child!r}]); '
                + ('time.sleep(0.2)' if root_exits else 'time.sleep(60)')
            )
            with open(pathlib.Path(folder) / 'log', 'w+b') as log:
                process = OwnedProcess([sys.executable, '-c', parent], stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 5
                    while not marker.exists() and time.monotonic() < deadline:
                        time.sleep(0.02)
                    self.assertTrue(marker.exists(), 'child did not start')
                    if root_exits:
                        self.assertEqual(process.wait(timeout=5), 0)
                    else:
                        with self.assertRaises(subprocess.TimeoutExpired):
                            process.wait(timeout=0.05)
                finally:
                    process.cleanup()
                # Observe the real child workload, not a recycled PID lookup.
                time.sleep(0.1)
                stopped = marker.read_text()
                time.sleep(0.2)
                self.assertEqual(marker.read_text(), stopped, 'descendant survived cleanup')
                self.assertIsNotNone(process.returncode)
                process.cleanup()  # Idempotent; cannot target a later reused PID.

    def test_root_exit_does_not_release_surviving_child(self):
        self.exercise(True)

    def test_deadline_stops_root_and_child(self):
        self.exercise(False)

    def test_failed_spawn_is_reported(self):
        with tempfile.TemporaryFile() as log:
            with self.assertRaises(OSError):
                OwnedProcess(['nonexistent-studio-test-executable-1739'], stdout=log, stderr=log)

    def test_cleanup_does_not_touch_another_owned_tree(self):
        with tempfile.TemporaryFile() as log:
            with OwnedProcess([sys.executable, '-c', 'import time; time.sleep(60)'],
                              stdout=log, stderr=log) as other:
                with OwnedProcess([sys.executable, '-c', 'import time; time.sleep(60)'],
                                  stdout=log, stderr=log):
                    pass
                self.assertIsNone(other.poll(), 'cleanup crossed process ownership')


if __name__ == '__main__':
    unittest.main()
