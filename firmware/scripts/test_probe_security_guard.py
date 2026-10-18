"""Negative feature test must fail for the intentional guard, not another error."""

import json
import unittest
from check_probe_security_guard import verify_rejection, GUARD


def diagnostic(message):
    return json.dumps(
        {"reason": "compiler-message", "message": {"level": "error", "message": message}}
    )


class FeatureGuard(unittest.TestCase):
    def test_exact_guard(self):
        verify_rejection(101, diagnostic(GUARD))

    def test_unrelated_failures_never_count(self):
        for code, output in (
            (0, diagnostic(GUARD)),
            (101, ""),
            (101, diagnostic("unresolved import")),
            (101, diagnostic(GUARD) + "\n" + diagnostic("another error")),
        ):
            with self.subTest(code=code, output=output), self.assertRaises(AssertionError):
                verify_rejection(code, output)


if __name__ == "__main__":
    unittest.main()
