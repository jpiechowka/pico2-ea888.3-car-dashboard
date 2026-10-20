"""Cargo guard inputs retain runtime validation under normal and optimized Python."""

import json
import os
import subprocess
import unittest
from unittest.mock import patch

from cargo_diagnostics import compiler_errors
from check_feature_profiles import CUSTOM_KEYS, compile_features, parse_profiles


class CargoInputs(unittest.TestCase):
    def test_compiler_error_code_and_rendered_fallback(self) -> None:
        output = json.dumps(
            {
                "reason": "compiler-message",
                "message": {
                    "level": "error",
                    "message": "borrow failed",
                    "code": {"code": "E0597"},
                },
            }
        )
        errors = compiler_errors("tool text\n" + output)
        self.assertEqual(
            [(error.code, error.message, error.rendered) for error in errors],
            [("E0597", "borrow failed", "borrow failed")],
        )
        self.assertEqual(compiler_errors('{"reason":"build-finished","success":true}'), [])
        for output in ("{", '{"reason":"compiler-message","message":null}', '{"reason":null}'):
            with self.subTest(output=output), self.assertRaises(AssertionError):
                compiler_errors(output)

    def test_profile_inventory_checks_shapes_and_identity(self) -> None:
        profiles = [
            {"profile": {"feature": f"cpu{i}", "aliases": [f"pico2-{i}"]}} for i in range(8)
        ]
        result = parse_profiles(json.dumps({"profiles": profiles}).encode())
        self.assertEqual([profile.feature for profile in result], [f"cpu{i}" for i in range(8)])
        invalid: tuple[object, ...] = (
            None,
            [],
            {},
            {"profiles": []},
            {"profiles": profiles[:-1]},
            {"profiles": profiles[:-1] + profiles[:1]},
            {"profiles": profiles[:-1] + [{"profile": {"feature": True, "aliases": []}}]},
            {"profiles": profiles[:-1] + [{"profile": {"feature": "cpu7", "aliases": None}}]},
        )
        for value in invalid:
            with self.subTest(value=value), self.assertRaises((AssertionError, ValueError)):
                parse_profiles(json.dumps(value).encode())
        with self.assertRaisesRegex(AssertionError, "bound"):
            parse_profiles(b" " * 65537)

    def test_compiler_process_clears_custom_environment_and_has_deadline(self) -> None:
        with (
            patch.dict(os.environ, dict.fromkeys(CUSTOM_KEYS, "stale")),
            patch(
                "check_feature_profiles.run_checked",
                return_value=subprocess.CompletedProcess([], 0, "", ""),
            ) as process,
        ):
            compile_features("demo")
            self.assertTrue(all(key not in process.call_args.kwargs["env"] for key in CUSTOM_KEYS))
            self.assertEqual(process.call_args.kwargs["timeout"], 600)
            compile_features("demo", ("50000000", "25000000", "1100"))
            self.assertEqual(
                tuple(process.call_args.kwargs["env"][key] for key in CUSTOM_KEYS),
                ("50000000", "25000000", "1100"),
            )


if __name__ == "__main__":
    unittest.main()
