"""Exercise benchmark cancellation and failures with disposable local producers."""

import contextlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from capture_benchmark import run_benchmark
from owned_process import OutputLimitError


class CaptureProcess(unittest.TestCase):
    def test_deadline_owns_descendant_and_keeps_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "heartbeat"
            child = (
                f"import pathlib,time; p=pathlib.Path({str(marker)!r}); "
                "\nwhile True: p.write_text(str(time.time_ns())); time.sleep(.02)"
            )
            parent = (
                "import subprocess,sys,time,pathlib; "
                f"subprocess.Popen([sys.executable,'-c',{child!r}]); "
                f"p=pathlib.Path({str(marker)!r}); "
                "\nwhile not p.exists(): time.sleep(.01)\n"
                "print('benchmark-stage-ready',flush=True); time.sleep(30)"
            )
            output = io.StringIO()
            with contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(subprocess.TimeoutExpired):
                    run_benchmark([sys.executable, "-c", parent], Path(directory), os.environ, 1)
            self.assertIn("benchmark-stage-ready", output.getvalue())
            self.assertTrue(marker.exists())
            time.sleep(0.1)
            stopped = marker.read_bytes()
            time.sleep(0.15)
            self.assertEqual(marker.read_bytes(), stopped)

    def test_error_and_output_limit_fail_capture(self) -> None:
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(subprocess.CalledProcessError) as error:
                run_benchmark(
                    [sys.executable, "-c", "print('compile failed'); raise SystemExit(7)"],
                    Path.cwd(),
                    os.environ,
                    10,
                )
            self.assertEqual(error.exception.returncode, 7)
            self.assertIn("compile failed", output.getvalue())
            output.seek(0)
            output.truncate()
            with self.assertRaises(OutputLimitError):
                run_benchmark(
                    [
                        sys.executable,
                        "-c",
                        "import sys; sys.stdout.buffer.write(b'x'*(9*1024*1024))",
                    ],
                    Path.cwd(),
                    os.environ,
                    10,
                )
            self.assertEqual(len(output.getvalue()), 8 * 1024 * 1024)


if __name__ == "__main__":
    unittest.main()
