"""Negative feature test must fail for the intentional guard, not another error."""

import json
import unittest
from check_probe_security_guard import verify_rejection, GUARD


def diagnostic(message: str) -> str:
    return json.dumps(
        {"reason": "compiler-message", "message": {"level": "error", "message": message}}
    )


class FeatureGuard(unittest.TestCase):
    def test_exact_guard(self) -> None:
        verify_rejection(101, diagnostic(GUARD))

    def test_unrelated_failures_never_count(self) -> None:
        for code, output in (
            (0, diagnostic(GUARD)),
            (101, ""),
            (101, diagnostic("unresolved import")),
            (101, diagnostic(GUARD) + "\n" + diagnostic("another error")),
        ):
            with self.subTest(code=code, output=output), self.assertRaises(AssertionError):
                verify_rejection(code, output)

    def test_malformed_diagnostic_is_a_controlled_rejection(self) -> None:
        malformed: tuple[object, ...] = (None, [], 1, {"level": "error", "message": None})
        for message in malformed:
            with self.subTest(message=message), self.assertRaises(AssertionError):
                verify_rejection(
                    101, json.dumps({"reason": "compiler-message", "message": message})
                )

    def test_non_error_diagnostics_do_not_change_the_guard(self) -> None:
        warning = json.dumps(
            {"reason": "compiler-message", "message": {"level": "warning", "message": "notice"}}
        )
        verify_rejection(101, "building\n" + warning + "\n" + diagnostic(GUARD))


if __name__ == "__main__":
    unittest.main()
