"""Supplied stage fixtures; real enclosing readers, no complete producer."""

import hashlib
import unittest
import io
import json
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch
from copy import deepcopy
import test_validation_policy as fixtures
import check_memory_layout as image
import capture_target_bench as capture
from owned_process import run_checked
from config_build_source import SourceContext
from config_platform import StackRecord
from image_test_support import QualifiedImageFixture
from test_target_capture import fixture_fields, fixture_items


class OrdinaryResultMigration(unittest.TestCase):
    @staticmethod
    def fixture() -> tuple[QualifiedImageFixture, bytes, StackRecord, SourceContext]:
        fixture = fixtures.MemoryQualification()
        elf, eav, stack = fixture.fixture()
        source = elf.fixture_source_context
        return elf, eav, stack, source

    def test_old_source_unqualified_result_rejects_even_with_matching_source_pair(self) -> None:
        fixture = fixtures.MemoryQualification()
        elf, eav, stack = fixture.fixture()
        source = fixture.source_context(elf)
        legacy_stack: dict[str, object] = dict(deepcopy(stack))
        legacy_stack["schema"] = 1
        legacy_stack.pop("execution_scope", None)
        static, headroom = image.geometry(elf)
        record = dict(
            schema=1,
            gate="full_memory_layout",
            status="CONDITIONAL_IMAGE_PASS",
            platform_policy="assumed_external",
            hardware_qualified=False,
            elf_sha256=hashlib.sha256(elf.data).hexdigest(),
            static_ram_bytes=static,
            headroom=headroom,
            stack=legacy_stack,
        )
        with self.assertRaisesRegex(AssertionError, "execution|schema"):
            image.read_image_result(record, elf, eav, source_context=source)

    def test_actual_image_stack_and_capture_readers_retain_one_nested_execution_record(
        self,
    ) -> None:
        elf, eav, stack, source = self.fixture()
        stdout = io.StringIO()
        with (
            patch.object(image, "ElfImage", return_value=elf),
            patch.object(image, "check_stack_budget", return_value=stack) as supplied,
            patch.object(image, "check_flash_lockout"),
            redirect_stdout(stdout),
        ):
            record = image.check("supplied-stage.elf", source_context=source)
        supplied.assert_called_once_with(elf, 0x7FE00, source_context=source)
        self.assertEqual(record["schema"], 2)
        self.assertEqual(record["stack"]["schema"], 2)
        self.assertEqual(record["stack"]["execution_scope"]["schema"], 1)
        self.assertNotIn("execution_scope", record)
        self.assertEqual(
            image.read_image_result(json.loads(json.dumps(record)), elf, eav, source), record
        )
        self.assertIn("named ordinary/callback paths only", stdout.getvalue())
        self.assertIn("NMI/HardFault", stdout.getvalue())
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(capture, "ElfImage", return_value=elf),
        ):
            p = Path(folder) / "memory-layout.json"
            raw = (json.dumps(record, sort_keys=True) + "\n").encode()
            p.write_bytes(raw)
            checked = capture.read_memory_gate(
                p, Path("supplied-stage.elf"), eav, source, gate="experimental-full"
            )
            self.assertEqual(checked, dict(record=record, sha256=hashlib.sha256(raw).hexdigest()))

    def test_missing_changed_or_stronger_scope_rejects_through_real_enclosing_reader(self) -> None:
        elf, eav, stack, source = self.fixture()
        static, headroom = image.geometry(elf)
        record = image._qualified_image(stack, elf, static, headroom, eav, source)
        for fault in (
            "missing",
            "schema1",
            "scope",
            "source",
            "frontier",
            "claim",
            "extra",
            "bounds",
        ):
            bad: dict[str, object] = dict(deepcopy(record))
            if fault == "missing":
                fixture_fields(bad["stack"]).pop("execution_scope")
            elif fault == "schema1":
                fixture_fields(bad["stack"])["schema"] = 1
            elif fault == "scope":
                fixture_fields(fixture_fields(bad["stack"])["execution_scope"])["scope_sha256"] = (
                    "0" * 64
                )
            elif fault == "source":
                fixture_fields(fixture_fields(bad["stack"])["execution_scope"])[
                    "source_context_sha256"
                ] = "0" * 64
            elif fault == "frontier":
                fixture_items(
                    fixture_fields(fixture_fields(bad["stack"])["execution_scope"])[
                        "pending_exception_frontier"
                    ]
                ).pop()
            elif fault == "claim":
                fixture_fields(fixture_fields(bad["stack"])["execution_scope"])[
                    "exception_safe_completion"
                ] = True
            elif fault == "extra":
                bad["execution_scope"] = deepcopy(
                    fixture_fields(fixture_fields(bad["stack"])["execution_scope"])
                )
            else:
                fixture_fields(fixture_fields(bad["stack"])["stack_bounds"])["constructor"] = 32768
            with self.subTest(fault=fault), self.assertRaises(AssertionError):
                image.read_image_result(bad, elf, eav, source)
        with self.assertRaisesRegex(AssertionError, "explicit source"):
            image.read_image_result(record, elf, eav)
        with self.assertRaisesRegex(AssertionError, "identity"):
            image.read_image_result(
                record, elf, eav, replace(source, attempt=source.attempt + "-stale")
            )
        # No record, prefix label or valid receipt completes the unavailable
        # actual graph/resource producer. The fixture above supplies stages.
        from config_platform import current_image_policy, qualified_stack_result

        policy = current_image_policy(stack["policies"][0], elf, eav)
        for supplied in (None, record["stack"]["execution_scope"], dict(phase="completed_prefix")):
            with (
                self.subTest(supplied=supplied),
                self.assertRaisesRegex(AssertionError, "complete internal"),
            ):
                qualified_stack_result(
                    stack["stack_bounds"], headroom, policy, ordinary_result=supplied
                )

    def test_missing_source_rejects_before_child_and_real_nonzero_output_is_not_admitted(
        self,
    ) -> None:
        elf, eav, stack, source = self.fixture()
        static, headroom = image.geometry(elf)
        record = image._qualified_image(stack, elf, static, headroom, eav, source)
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder)
            frozen = destination / "pico2.elf"
            frozen.write_bytes(elf.data)
            with (
                patch.object(capture, "run_checked") as unlaunched_child,
                self.assertRaisesRegex(ValueError, "explicit source"),
            ):
                capture.run_memory_gate(frozen, destination, eav)
            unlaunched_child.assert_not_called()
            from config_build_source import write_source_context

            source = replace(source, attempt=destination.name)
            path = destination / "source-context.json"
            write_source_context(path, source)
            from config_platform import current_image_policy, _execution_receipt

            policy = current_image_policy(stack["policies"][0], elf, eav)
            record["stack"]["execution_scope"] = _execution_receipt(policy, source, elf).to_record()
            supplied = destination / "supplied.json"
            supplied.write_text(json.dumps(record))
            real_run = run_checked

            def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                self.assertEqual(Path(argv[argv.index("--source-context") + 1]), path.resolve())
                return real_run(
                    [
                        sys.executable,
                        "-B",
                        "-c",
                        "import pathlib,sys;pathlib.Path(sys.argv[1]).write_bytes(pathlib.Path(sys.argv[2]).read_bytes());sys.exit(7)",
                        argv[argv.index("--result-json") + 1],
                        str(supplied),
                    ],
                    check=bool(kwargs["check"]),
                    timeout=10,
                )

            with (
                patch.object(capture, "ElfImage", return_value=elf),
                patch.object(capture, "run_checked", side_effect=child),
                self.assertRaises(subprocess.CalledProcessError) as error,
            ):
                capture.run_memory_gate(frozen, destination, eav, source, path)
            self.assertEqual(error.exception.returncode, 7)
            self.assertEqual(json.loads((destination / "memory-layout.json").read_bytes()), record)

    def test_real_memory_CLI_subprocess_retains_explicit_source_and_pending_exception_qualification(
        self,
    ) -> None:
        # The actual CLI, strict readers and file transport run in a separate
        # process. Only ELF construction and unavailable analysis stages are
        # explicit synthetic fixtures; this is not linked-image readiness.
        elf, eav, stack, source = self.fixture()
        from config_build_source import write_source_context
        from config_platform import current_image_policy, _execution_receipt

        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder)
            frozen = destination / "pico2.elf"
            frozen.write_bytes(elf.data)
            source = replace(source, attempt=destination.name)
            sidecar = destination / "source-context.json"
            write_source_context(sidecar, source)
            stack["execution_scope"] = _execution_receipt(
                current_image_policy(stack["policies"][0], elf), source, elf
            ).to_record()
            stages = destination / "supplied-stage-record.json"
            stages.write_text(json.dumps(stack))
            result = destination / "memory-layout.json"
            code = (
                "import json,sys;from pathlib import Path;from unittest.mock import patch;"
                "sys.path.insert(0,sys.argv[1]);import check_memory_layout as image;"
                "from test_validation_policy import MemoryQualification;"
                "elf,_,_=MemoryQualification().fixture();stack=json.loads(Path(sys.argv[3]).read_bytes());"
                "\nwith patch.object(image,'ElfImage',return_value=elf),patch.object(image,'check_stack_budget',return_value=stack),patch.object(image,'check_flash_lockout'):"
                "\n image.main([sys.argv[2],'--gate','experimental-full','--result-json',sys.argv[5],'--source-context',sys.argv[4]])"
            )
            completed = run_checked(
                [
                    sys.executable,
                    "-B",
                    *(["-O"] if sys.flags.optimize else []),
                    "-c",
                    code,
                    str(Path(fixtures.__file__).parent),
                    str(frozen),
                    str(stages),
                    str(sidecar),
                    str(result),
                ],
                capture_output=True,
                timeout=10,
            )
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            stdout = completed.stdout
            self.assertIn("named ordinary/callback paths only", stdout)
            self.assertIn("NMI/HardFault", stdout)
            self.assertIn("reset requires fresh qualified epoch", stdout)
            with patch.object(capture, "ElfImage", return_value=elf):
                checked = capture.read_memory_gate(
                    result, frozen, eav, source, gate="experimental-full"
                )
            self.assertEqual(checked["record"]["stack"], stack)
            self.assertEqual(checked["sha256"], hashlib.sha256(result.read_bytes()).hexdigest())
            self.assertFalse(
                fixture_fields(fixture_fields(checked["record"]["stack"])["execution_scope"])[
                    "exception_safe_completion"
                ]
            )


if __name__ == "__main__":
    unittest.main()
