"""Test discovery must never start the opt-in live helper harness."""

import importlib.util
from contextlib import redirect_stdout
import io
from pathlib import Path
import unittest
from unittest.mock import patch
from collections.abc import Mapping

from test_server_http import exercise, job_identity, response_body


class HarnessImport(unittest.TestCase):
    def test_import_has_no_process_or_network_side_effect(self) -> None:
        spec = importlib.util.spec_from_file_location(
            "http_harness", Path(__file__).with_name("test_server_http.py")
        )
        if spec is None or spec.loader is None:
            raise AssertionError("HTTP harness module unavailable")
        module = importlib.util.module_from_spec(spec)
        with patch("subprocess.Popen") as process, patch("urllib.request.urlopen") as network:
            spec.loader.exec_module(module)
            process.assert_not_called()
            network.assert_not_called()
        self.assertTrue(callable(module.main))

    def test_bad_status_or_capability_fails_without_further_requests(self) -> None:
        cases: tuple[tuple[int, object], ...] = (
            (500, {"capability": "a" * 64}),
            (200, None),
            (200, {"capability": 64}),
            (200, {"capability": "a" * 63}),
        )
        for result in cases:
            with self.subTest(status=result[0]):
                with patch("test_server_http.request", return_value=result) as request:
                    with self.assertRaises(AssertionError):
                        exercise()
                    request.assert_called_once()

    def test_happy_path_and_failed_prerequisite_both_check_auth_and_cancel(self) -> None:
        for cause in ("failed", "succeeded"):
            replies: list[tuple[str, int, object]] = [
                ("/api/v1/connect", 200, {"capability": "a" * 64}),
                ("/api/v1/jobs", 403, {}),
                ("/api/v1/connect", 403, {}),
                ("/api/v1/session", 200, {"ready": False}),
                ("/api/v1/jobs", 200, []),
                ("/api/v1/prerequisites/check", 202, {"job_id": "job-1"}),
                ("/api/v1/prerequisites/check", 409, {}),
                (
                    "/api/v1/jobs/job-1/events?after=0&limit=128",
                    200,
                    {
                        "summary": {"events_closed": True, "terminal_cause": cause},
                        "records": [{}],
                    },
                ),
                (
                    "/api/v1/prerequisites",
                    200,
                    {
                        "readiness_scope": "installed_tool_versions_only",
                    },
                ),
                ("/api/v1/prerequisites/check", 202, {"job_id": "job-2"}),
                ("/api/v1/jobs/job-2/cancel", 200, {}),
                (
                    "/api/v1/jobs/job-2/events?after=0&limit=128",
                    200,
                    {
                        "summary": {"events_closed": True, "terminal_cause": "cancelled"},
                    },
                ),
                ("/api/v1/builds", 415, {}),
            ]

            def reply(
                path: str, method: str, headers: Mapping[str, str] | None
            ) -> tuple[int, object]:
                expected, status, body = replies.pop(0)
                self.assertEqual(path, expected)
                self.assertIn(method, ("GET", "POST"))
                if path not in ("/api/v1/connect", "/api/v1/jobs"):
                    self.assertEqual(headers, {"X-Studio-Capability": "a" * 64})
                return status, body

            with (
                self.subTest(cause=cause),
                patch("test_server_http.request", side_effect=reply),
                redirect_stdout(io.StringIO()),
            ):
                exercise()
                self.assertEqual(replies, [])

    def test_auth_rejection_is_not_a_removable_python_assert(self) -> None:
        with patch(
            "test_server_http.request",
            side_effect=[(200, {"capability": "a" * 64}), (200, [])],
        ) as request:
            with self.assertRaisesRegex(AssertionError, "unauthenticated"):
                exercise()
            self.assertEqual(request.call_count, 2)

    def test_response_bounds_and_job_identity(self) -> None:
        with patch("test_server_http.RESPONSE_LIMIT", 4):
            self.assertEqual(response_body(io.BytesIO(b"null")), None)
            with self.assertRaisesRegex(AssertionError, "byte bound"):
                response_body(io.BytesIO(b"null "))
            with self.assertRaisesRegex(AssertionError, "JSON"):
                response_body(io.BytesIO(b"{"))
        invalid: tuple[object, ...] = (
            None,
            [],
            {},
            {"job_id": ""},
            {"job_id": "../jobs"},
            {"job_id": 1},
            {"job_id": "a" * 65},
        )
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(AssertionError):
                job_identity(value)
        self.assertEqual(job_identity({"job_id": "job-123"}), "job-123")


if __name__ == "__main__":
    unittest.main()
