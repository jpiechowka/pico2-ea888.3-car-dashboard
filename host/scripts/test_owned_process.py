"""Disposable child trees exercise bounded supervision without build tools or USB."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import owned_process
from owned_process import run_bounded


class BoundedTests(unittest.TestCase):
    def test_bounded_stdin_has_exact_bytes_and_eof(self) -> None:
        payload = ("typed fixture Ω" * 10000).encode()
        result = run_bounded(
            [sys.executable, "-c", "import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())"],
            input_bytes=payload,
        )
        self.assertEqual(result.stdout, payload)
        with self.assertRaisesRegex(ValueError, "stdin"):
            run_bounded(["must-not-launch"], input_bytes=b"x" * (8 * 1024 * 1024 + 1))

    def exercise_failure(self, mode: str) -> None:
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "heartbeat"
            descendant = (
                f"import pathlib,time; p=pathlib.Path({str(marker)!r}); "
                "\nwhile True: p.write_text(str(time.time_ns())); time.sleep(.02)"
            )
            producer = (
                "import subprocess,sys,time,pathlib; "
                f"subprocess.Popen([sys.executable,'-c',{descendant!r}]); "
                f"p=pathlib.Path({str(marker)!r}); "
                "\nwhile not p.exists(): time.sleep(.01)\n"
            )
            if mode == "deadline":
                producer += "time.sleep(30)"
            else:
                producer += "\nwhile True: print('x'*4096,flush=True)"

            def fail(_: bytes) -> None:
                if mode == "interrupt":
                    raise KeyboardInterrupt()
                raise OSError("injected sink failure")

            error = (
                subprocess.TimeoutExpired
                if mode == "deadline"
                else (
                    ValueError
                    if mode == "quota"
                    else KeyboardInterrupt
                    if mode == "interrupt"
                    else OSError
                )
            )
            start = time.monotonic()
            with self.assertRaises(error):
                run_bounded(
                    [sys.executable, "-c", producer],
                    timeout=1,
                    stdout_limit=5000,
                    on_stdout=fail if mode in ("callback", "interrupt") else None,
                )
            self.assertLess(time.monotonic() - start, 7)
            self.assertTrue(marker.exists())
            time.sleep(0.1)
            stopped = marker.read_bytes()
            time.sleep(0.15)
            self.assertEqual(marker.read_bytes(), stopped, "descendant survived supervisor failure")

    def test_deadline_closes_inherited_pipe_and_descendant(self) -> None:
        self.exercise_failure("deadline")

    def test_quota_does_not_drain_an_infinite_writer(self) -> None:
        self.exercise_failure("quota")

    def test_callback_failure_closes_descendant(self) -> None:
        self.exercise_failure("callback")

    def test_interrupted_reader_closes_descendant(self) -> None:
        self.exercise_failure("interrupt")

    def test_spawn_failure_and_pipe_reader_failure(self) -> None:
        with self.assertRaises(OSError):
            run_bounded(["nonexistent-owned-test-executable-1739"])
        with patch.object(owned_process, "_read_available", side_effect=OSError("reader failed")):
            with self.assertRaisesRegex(OSError, "reader failed"):
                run_bounded([sys.executable, "-c", "import time; time.sleep(30)"])

    def test_both_streams_cwd_environment_and_exact_caps(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment = dict(os.environ, OWNED_PROCESS_TEST="passed")
            stderr: list[bytes] = []
            result = run_bounded(
                [
                    sys.executable,
                    "-c",
                    "import os,sys; "
                    "assert os.environ['OWNED_PROCESS_TEST']=='passed'; "
                    f"assert os.getcwd()=={directory!r}; "
                    "sys.stdout.buffer.write(b'x'*8192); sys.stderr.buffer.write(b'y'*8192)",
                ],
                cwd=directory,
                env=environment,
                stdout_limit=8192,
                stderr_limit=8192,
                on_stderr=stderr.append,
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"x" * 8192)
            self.assertEqual(b"".join(stderr), b"y" * 8192)

    def test_machine_cli_never_discloses_failed_stdout(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                owned_process.__file__,
                "--json",
                "--",
                sys.executable,
                "-c",
                "print('PRIVATE_SENTINEL'); raise SystemExit(7)",
            ],
            capture_output=True,
            timeout=10,
        )
        self.assertEqual(result.returncode, 7)
        self.assertNotIn(b"PRIVATE_SENTINEL", result.stdout + result.stderr)

    def test_quota_preserves_prefix_and_observed_loss(self) -> None:
        chunks: list[bytes] = []
        with self.assertRaises(owned_process.OutputLimitError) as caught:
            run_bounded(
                [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'x'*10000)"],
                stdout_limit=5000,
                on_stdout=chunks.append,
            )
        self.assertEqual(b"".join(chunks), b"x" * 5000)
        self.assertGreater(caught.exception.observed, 5000)
        self.assertEqual(caught.exception.known_discarded, caught.exception.observed - 5000)


if __name__ == "__main__":
    unittest.main()
