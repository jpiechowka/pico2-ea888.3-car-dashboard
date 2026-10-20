"""Source completeness and emitted-artifact contracts; no target execution."""

import json
import hashlib
import zipfile
from collections.abc import Sequence
from config_build_source import SourceContext
from image_test_support import (
    QualifiedImageFixture,
    fixture_fields as fixture_fields,
    fixture_items as fixture_items,
)
from script_values import array_items, integer_value, string_value
from pathlib import Path
import unittest
import tempfile
import os
import subprocess
import sys
import io
from contextlib import ExitStack, redirect_stdout, redirect_stderr
from unittest.mock import patch
import capture_target_bench as capture
from owned_process import run_checked


def fixture_json(raw: str | bytes) -> dict[str, object]:
    value: object = json.loads(raw)
    return fixture_fields(value)


def stack_policy(record: dict[str, object]) -> dict[str, object]:
    return fixture_fields(fixture_items(fixture_fields(record["stack"])["policies"])[0])


class CaptureTests(unittest.TestCase):
    def test_cargo_identity_rejects_duplicate_keys_and_non_boolean_success(self) -> None:
        executable = (capture.FIRMWARE / "target/alternative/pico2").resolve()
        artifact = dict(
            reason="compiler-artifact",
            manifest_path=str(capture.FIRMWARE / "pico2/Cargo.toml"),
            target=dict(name="pico2", kind=["bin"]),
            profile=dict(test=False),
            executable=str(executable),
        )
        for finish in (
            '{"reason":"build-finished","success":1}',
            '{"reason":"build-finished","success":false,"success":true}',
        ):
            with self.subTest(finish=finish), self.assertRaises(ValueError):
                capture.emitted_elf(json.dumps(artifact) + "\n" + finish)

    def test_cargo_identity_rejects_malformed_nested_fields(self) -> None:
        malformed: list[object] = [None, [], True, 2, "invalid"]
        for value in malformed:
            row = dict(
                reason="compiler-artifact",
                manifest_path=str(capture.FIRMWARE / "pico2/Cargo.toml"),
                target=value,
                profile=dict(test=False),
            )
            with self.subTest(value=value), self.assertRaises(ValueError):
                capture.firmware_package(json.dumps(row))

    def test_embedded_config_rejects_duplicate_environment_identity(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            build = Path(folder).resolve()
            data = b"configuration"
            (build / "dashboard.eav").write_bytes(data)
            row = dict(
                reason="build-script-executed",
                out_dir=str(build),
                env=[
                    ["DASHBOARD_CONFIG_SHA256", "different"],
                    ["DASHBOARD_CONFIG_SHA256", hashlib.sha256(data).hexdigest()],
                ],
            )
            with self.assertRaises(ValueError):
                capture.embedded_config(json.dumps(row), build)

    def test_diagnostic_sink_failure_stops_descendants(self) -> None:
        import time

        sys.path.insert(0, str(capture.REPO / "host/scripts"))
        from owned_process import OwnedProcess

        with tempfile.TemporaryDirectory(prefix="capture-tree-") as folder:
            root = Path(folder)
            heartbeat, done = root / "heartbeat", root / "done"
            grandchild = (
                "import pathlib,time; p=pathlib.Path(" + repr(str(heartbeat)) + "); "
                "\nwhile True: p.write_text(str(time.time_ns())); time.sleep(.02)"
            )
            compiler = (
                "import subprocess,sys,pathlib,time; subprocess.Popen([sys.executable,'-c',"
                + repr(grandchild)
                + "]); p=pathlib.Path("
                + repr(str(heartbeat))
                + "); "
                "\nwhile not p.exists(): time.sleep(.01)\n"
                'print(\'{"reason":"compiler-message","message":{"rendered":"failure"}}\',flush=True); time.sleep(30)'
            )
            wrapper = (
                "import sys,pathlib,os,time; sys.path.insert(0,"
                + repr(str(capture.FIRMWARE / "scripts"))
                + "); "
                "import capture_target_bench as m\n"
                "def fail(*args): raise OSError('injected sink failure')\n"
                "m.CargoDiagnostics.emit=fail\n"
                "try: m.run_build([sys.executable,'-c',"
                + repr(compiler)
                + "],os.environ.copy(),pathlib.Path("
                + repr(folder)
                + "))\n"
                "except OSError: pathlib.Path(" + repr(str(done)) + ").write_text('done')\n"
                "time.sleep(30)"
            )
            with (root / "log").open("w+b") as log:
                with OwnedProcess([sys.executable, "-c", wrapper], stdout=log, stderr=log):
                    deadline = time.monotonic() + 5
                    while not done.exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(done.exists(), "sink failure never completed")
                    time.sleep(0.1)
                    stopped = heartbeat.read_bytes()
                    time.sleep(0.15)
                    self.assertEqual(
                        heartbeat.read_bytes(), stopped, "compiler descendant survived sink failure"
                    )

    def test_rust_capture_options_consumer_accepts_ble_and_explicit_legacy_envelopes(self) -> None:
        for feature, legacy in [("ble-obd", False), ("longan-rx12", True), ("", False)]:
            _, options = capture.fixed_json_tool(["--features", feature], os.environ.copy())
            self.assertEqual(options["schema_version"], 2)
            self.assertEqual(options["legacy_longan_backend"], legacy)
            self.assertEqual(options["legacy_unavailable_backend"], feature == "")
            if feature:
                self.assertIn(feature, array_items(options["features"], "features"))
            else:
                self.assertNotIn("ble-obd", array_items(options["features"], "features"))
                self.assertNotIn("longan-rx12", array_items(options["features"], "features"))

    def test_native_scratch_is_fixed_empty_and_path_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            attempt = Path(folder).resolve()
            repo = attempt / "source"
            firmware = repo / "firmware"
            firmware.mkdir(parents=True)
            scratch = attempt / "b"
            scratch.mkdir()
            destination = firmware / "target/bench-captures/native"
            with patch.object(capture, "REPO", repo), patch.object(capture, "FIRMWARE", firmware):
                self.assertEqual(
                    capture.build_directories(destination, True),
                    tuple(scratch / n for n in ("f", "h", "n")),
                )
                with self.assertRaisesRegex(ValueError, "fixed frozen"):
                    capture.build_directories(destination.with_name("other"), True)
                (scratch / "unexpected").write_bytes(b"not reusable")
                with self.assertRaisesRegex(ValueError, "empty owned"):
                    capture.build_directories(destination, True)
            self.assertEqual(
                capture.build_directories(destination, False), (destination / "build", None, None)
            )

    @unittest.skipUnless(os.name == "nt", "Windows path budget")
    def test_native_scratch_rejects_too_long_repository_before_cargo(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            attempt = Path(folder).resolve() / ("a" * 90)
            repo = attempt / "source"
            firmware = repo / "firmware"
            firmware.mkdir(parents=True)
            (attempt / "b").mkdir()
            with patch.object(capture, "REPO", repo), patch.object(capture, "FIRMWARE", firmware):
                with self.assertRaisesRegex(ValueError, "shorter path"):
                    capture.build_directories(firmware / "target/bench-captures/native", True)

    @unittest.skipUnless(os.name == "nt", "Windows source path boundary")
    def test_verbatim_import_inputs_match_ordinary_names_and_hashes(self) -> None:
        script = str(Path(capture.__file__).resolve())
        extended = script if script.startswith("\\\\?\\") else "\\\\?\\" + script
        code = (
            "import runpy,sys,json,hashlib; m=runpy.run_path(sys.argv[1]); "
            'print(json.dumps({k:hashlib.sha256(v).hexdigest() for k,v in m["inputs"]().items()},sort_keys=True))'
        )

        def snapshot(path: str | Path) -> str:
            return run_checked(
                [sys.executable, "-I", "-c", code, str(path)],
                capture_output=True,
                text=True,
                check=True,
            ).stdout

        self.assertEqual(snapshot(extended), snapshot(script))

    @unittest.skipUnless(os.name == "nt", "Windows external-tool path boundary")
    def test_extended_script_path_keeps_identity_without_exporting_prefix(self) -> None:
        script = str(Path(capture.__file__).resolve())
        extended = script if script.startswith("\\\\?\\") else "\\\\?\\" + script
        command = [
            sys.executable,
            "-I",
            "-c",
            'import runpy,sys; print(runpy.run_path(sys.argv[1])["FIRMWARE"])',
            extended,
        ]
        result = run_checked(command, capture_output=True, text=True, check=True)
        directory = Path(result.stdout.strip())
        self.assertFalse(str(directory).startswith("\\\\?\\"))
        self.assertTrue(directory.samefile(Path(script).parents[1]))

    @unittest.skipUnless(os.name == "nt", "Windows external-tool path boundary")
    def test_tool_directory_rejects_changed_identity(self) -> None:
        directory = str(Path(capture.__file__).resolve().parent)
        extended = directory if directory.startswith("\\\\?\\") else "\\\\?\\" + directory
        with patch.object(Path, "samefile", return_value=False):
            with self.assertRaisesRegex(ValueError, "identity changed"):
                capture.external_tool_path(extended)

    @unittest.skipUnless(os.name == "nt", "Windows 8.3 path identity")
    def test_short_name_import_uses_the_same_canonical_inventory(self) -> None:
        if sys.platform != "win32":
            self.skipTest("Windows 8.3 path identity")
        import ctypes

        script = Path(capture.__file__).resolve()
        short_name = ctypes.windll.kernel32.GetShortPathNameW
        short_name.argtypes = (ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint32)
        short_name.restype = ctypes.c_uint32
        size = short_name(str(script), None, 0)
        if not size:
            self.skipTest("volume has no usable 8.3 alias")
        buffer = ctypes.create_unicode_buffer(size)
        self.assertTrue(short_name(str(script), buffer, size))
        alias = Path(buffer.value)
        if str(alias).lower() == str(script).lower():
            self.skipTest("volume does not generate distinct 8.3 aliases")
        self.assertTrue(alias.samefile(script))
        self.assertEqual(capture.external_tool_path(alias), script)
        code = (
            "import runpy,sys,json,hashlib; m=runpy.run_path(sys.argv[1]); "
            'print(json.dumps({k:hashlib.sha256(v).hexdigest() for k,v in m["inputs"]().items()},sort_keys=True))'
        )

        def snapshot(path: str | Path) -> str:
            return run_checked(
                [sys.executable, "-I", "-c", code, str(path)],
                capture_output=True,
                text=True,
                check=True,
            ).stdout

        self.assertEqual(snapshot(alias), snapshot(script))

    def test_tool_directory_requires_existing_path(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder).resolve()
            self.assertTrue(capture.external_tool_path(directory).samefile(directory))
            with self.assertRaises(FileNotFoundError):
                capture.external_tool_path(directory / "missing")

    def test_custom_environment_presence_is_rejected_even_when_empty(self) -> None:
        for key in ["DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV"]:
            with self.assertRaises(ValueError):
                capture.check_clock_environment({key: ""})

    def test_every_ambient_compiler_override_is_rejected_by_presence(self) -> None:
        from benchmark_environment import ENV_PREFIXES

        for key in [
            *ENV_PREFIXES,
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_PROFILE_RELEASE_OPT_LEVEL",
        ]:
            for value in ["", "override"]:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    capture.check_clock_environment({key: value})
        capture.check_clock_environment(
            {"PATH": "ordinary PATH", "DASHBOARD_CONFIG": "selected.eav"}
        )

    def test_failed_build_keeps_raw_stdout_before_raising(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            command = [
                sys.executable,
                "-c",
                'import sys; print("{\\"reason\\":\\"build-finished\\",\\"success\\":false}"); sys.exit(7)',
            ]
            with self.assertRaises(subprocess.CalledProcessError) as error:
                capture.run_build(command, os.environ.copy(), destination)
            self.assertEqual(error.exception.returncode, 7)
            self.assertEqual(
                json.loads((destination / "cargo-messages.jsonl").read_text()),
                {"reason": "build-finished", "success": False},
            )

    def test_failed_build_streams_safe_rendered_diagnostics_and_retains_them(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            stream = io.StringIO()
            record = dict(
                reason="compiler-message",
                message=dict(rendered="error: fixture missing linker\n\x1b[31munsafe\x00\r\n"),
            )
            command = [
                sys.executable,
                "-c",
                "import sys; print(" + repr(json.dumps(record)) + "); sys.exit(7)",
            ]
            with redirect_stderr(stream), self.assertRaises(subprocess.CalledProcessError):
                capture.run_build(command, os.environ.copy(), destination)
            self.assertIn("error: fixture missing linker", stream.getvalue())
            self.assertNotIn("\x1b", stream.getvalue())
            self.assertNotIn("\x00", stream.getvalue())
            self.assertEqual((destination / "cargo-diagnostics.log").read_text(), stream.getvalue())

    def test_diagnostics_are_bounded_utf8_and_do_not_forward_artifact_json(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            stream = io.StringIO()
            diagnostic = capture.CargoDiagnostics(destination)
            with redirect_stderr(stream):
                diagnostic.record(
                    json.dumps(
                        dict(
                            reason="compiler-artifact", message=dict(rendered="must not be logged")
                        )
                    )
                )
                diagnostic.record(b"not JSON")
                diagnostic.record(
                    json.dumps(dict(reason="compiler-message", message=dict(rendered="é" * 300000)))
                )
                diagnostic.record(
                    json.dumps(dict(reason="compiler-message", message=dict(rendered="after cap")))
                )
            diagnostic.file.close()
            raw = (destination / "cargo-diagnostics.log").read_bytes()
            self.assertLessEqual(len(raw), 256 * 1024)
            self.assertTrue(raw.decode().endswith("[Cargo diagnostics truncated]\n"))
            self.assertEqual(raw.decode(), stream.getvalue())
            self.assertNotIn("must not be logged", stream.getvalue())
            self.assertNotIn("after cap", stream.getvalue())
            self.assertTrue(all(len(line) <= 2048 for line in stream.getvalue().splitlines()))

    def test_diagnostic_sink_failure_joins_the_owned_child(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            record = json.dumps(
                dict(reason="compiler-message", message=dict(rendered="error: sink fixture"))
            )
            command = [
                sys.executable,
                "-c",
                "import time; print(" + repr(record) + ",flush=True); time.sleep(30)",
            ]
            with patch.object(
                capture.CargoDiagnostics, "emit", side_effect=OSError("fixture full disk")
            ):
                with self.assertRaisesRegex(OSError, "fixture full disk"):
                    capture.run_build(command, os.environ.copy(), Path(folder).resolve())
            self.assertIn(
                "compiler-message", (Path(folder).resolve() / "cargo-messages.jsonl").read_text()
            )

    def test_diagnostic_is_published_before_compiler_exit(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            stream = io.StringIO()
            record = json.dumps(
                dict(reason="compiler-message", message=dict(rendered="error: live fixture"))
            )
            script = (
                "import pathlib,time,sys; print(" + repr(record) + ",flush=True); "
                "p=pathlib.Path(" + repr(str(destination / "cargo-diagnostics.log")) + "); "
                "deadline=time.monotonic()+3\n"
                "while p.stat().st_size==0 and time.monotonic()<deadline: time.sleep(.01)\n"
                "sys.exit(0 if p.stat().st_size else 9)"
            )
            with redirect_stderr(stream):
                result = capture.run_build(
                    [sys.executable, "-c", script], os.environ.copy(), destination
                )
            self.assertEqual(result.returncode, 0)
            self.assertIn("error: live fixture", stream.getvalue())
            self.assertEqual(json.loads(result.stdout)["reason"], "compiler-message")

    def test_generated_clock_is_bound_to_firmware_package_and_exact_attempt(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            build = Path(folder).resolve() / "build"
            out = build / "out"
            out.mkdir(parents=True)
            source = b"""// Generated from dashboard-build-profiles; do not edit.
pub const REQUESTED_CPU_HZ: u32 = 150000000;
pub const ACTUAL_CPU_HZ: u32 = 150000000;
pub const REQUESTED_SPI_HZ: u32 = 62500000;
pub const ACTUAL_SPI_HZ: u32 = 37500000;
pub const VOLTAGE_MV: u32 = 1100;
pub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::V1_10;
pub const PLL: PllConfig = PllConfig { refdiv: 1, fbdiv: 125, post_div1: 5, post_div2: 2 };
pub const CPSDVSR: u8 = 2;
pub const SCR: u8 = 1;
pub const LEGACY_ROUNDING: bool = false;
"""
            clock = dict(
                requested_cpu_hz=150000000,
                actual_cpu_hz=150000000,
                requested_spi_hz=62500000,
                actual_spi_hz=37500000,
                voltage="v1_10",
                pll=dict(refdiv=1, fbdiv=125, postdiv1=5, postdiv2=2),
                cpsdvsr=2,
                scr=1,
                legacy_rounding=False,
                spi_neighbors=dict(
                    below_or_equal=dict(request_hz=37500000, actual_hz=37500000, cpsdvsr=2, scr=1),
                    above=dict(request_hz=75000000, actual_hz=75000000, cpsdvsr=2, scr=0),
                ),
            )
            options = dict(
                normalized=dict(requested=dict(clock={"named": "stock"}), clock=clock),
                features=["demo"],
                optimization="3",
                voltage_mv=1100,
            )
            metadata = dict(
                schema_version=1,
                selection={"named": "stock"},
                clock=clock,
                optimization="3",
                features=["demo"],
            )
            (out / "clock_profile.rs").write_bytes(source)
            encoded = json.dumps(metadata).encode()
            (out / "clock_profile.json").write_bytes(encoded)
            record = dict(
                reason="build-script-executed",
                package_id="firmware-id",
                out_dir=str(out),
                env=[
                    ["DASHBOARD_CLOCK_SOURCE_SHA256", hashlib.sha256(source).hexdigest()],
                    [
                        "DASHBOARD_CLOCK_METADATA_SHA256",
                        hashlib.sha256(encoded).hexdigest(),
                    ],
                ],
            )
            line = json.dumps(record)
            evidence = capture.generated_clock(line, build, "firmware-id", options)
            self.assertEqual(evidence["source"], source)
            wrong_metadata = encoded.replace(b'"legacy_rounding": false', b'"legacy_rounding": 0')
            (out / "clock_profile.json").write_bytes(wrong_metadata)
            wrong_record = dict(
                record,
                env=[
                    ["DASHBOARD_CLOCK_SOURCE_SHA256", hashlib.sha256(source).hexdigest()],
                    ["DASHBOARD_CLOCK_METADATA_SHA256", hashlib.sha256(wrong_metadata).hexdigest()],
                ],
            )
            with self.assertRaisesRegex(ValueError, "clock/options"):
                capture.generated_clock(json.dumps(wrong_record), build, "firmware-id", options)
            (out / "clock_profile.json").write_bytes(encoded)
            for stream, package, attempt in [
                ("", "firmware-id", options),
                (line + "\n" + line, "firmware-id", options),
                (line, "other", options),
                (line, "firmware-id", dict(options, optimization="s")),
            ]:
                with self.assertRaises(ValueError):
                    capture.generated_clock(stream, build, package, attempt)
            with self.assertRaises(ValueError):
                capture.generated_clock(line, build / "outside", "firmware-id", options)
            (out / "clock_profile.rs").write_bytes(b"mismatched")
            with self.assertRaises(ValueError):
                capture.generated_clock(line, build, "firmware-id", options)
            wrong = source.replace(b"fbdiv: 125", b"fbdiv: 124")
            (out / "clock_profile.rs").write_bytes(wrong)
            environment = record["env"]
            if not isinstance(environment, list):
                self.fail("fixture environment is not a list")
            fixture_items(environment[0])[1] = hashlib.sha256(wrong).hexdigest()
            with self.assertRaises(ValueError):
                capture.generated_clock(json.dumps(record), build, "firmware-id", options)

    def test_new_clock_and_protocol_producers_are_capture_inputs(self) -> None:
        data = capture.inputs()
        for path in [
            "Cargo.lock",
            "firmware/Cargo.lock",
            "firmware/pico2/build.rs",
            "firmware/pico2/src/clock_profile.rs",
            "crates/dashboard-build-profiles/src/resolve.rs",
            "crates/dashboard-helper-protocol/src/options.rs",
            "crates/dashboard-config-tools/src/clocks.rs",
        ]:
            self.assertEqual(data[path], (capture.REPO / path).read_bytes())

    def test_only_matching_build_snapshot_is_archived(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            build = Path(folder).resolve() / "build"
            out = build / "out"
            out.mkdir(parents=True)
            data = b"validated configuration snapshot"
            (out / "dashboard.eav").write_bytes(data)
            record = dict(
                reason="build-script-executed",
                out_dir=str(out),
                env=[["DASHBOARD_CONFIG_SHA256", hashlib.sha256(data).hexdigest()]],
            )
            line = json.dumps(record)
            self.assertEqual(capture.embedded_config(line, build), data)
            for output in [
                "",
                line + "\n" + line,
                json.dumps(dict(record, env=[["DASHBOARD_CONFIG_SHA256", "bad"]])),
            ]:
                with self.subTest(output=output), self.assertRaises(ValueError):
                    capture.embedded_config(output, build)
            with self.assertRaises(ValueError):
                capture.embedded_config(line, build / "different")
            (out / "dashboard.eav").write_bytes(b"replaced after build")
            with self.assertRaises(ValueError):
                capture.embedded_config(line, build)

    def test_external_configuration_is_bounded_frozen_and_captured_by_content(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            selected = root / "external.eav"
            selected.write_bytes(b"first snapshot")
            environment = {"DASHBOARD_CONFIG": str(selected)}
            before = capture.selected_config(environment)
            self.assertEqual(before[1], b"first snapshot")
            out = root / "capture"
            out.mkdir()
            frozen, metadata = capture.freeze_config(before, out, environment)
            self.assertEqual(Path(frozen["DASHBOARD_CONFIG"]).read_bytes(), b"first snapshot")
            self.assertEqual(metadata["sha256"], hashlib.sha256(b"first snapshot").hexdigest())
            selected.write_bytes(b"changed snapshot")
            self.assertNotEqual(before, capture.selected_config(environment))
            self.assertEqual(Path(frozen["DASHBOARD_CONFIG"]).read_bytes(), b"first snapshot")
            self.assertEqual(capture.selected_config({}), (None, None))
            default_env, default = capture.freeze_config((None, None), out, environment)
            self.assertNotIn("DASHBOARD_CONFIG", default_env)
            self.assertEqual(default["selection"], "default")
            for value in ["", str(root), str(root / "missing"), "bad\npath"]:
                with self.subTest(value=value), self.assertRaises((ValueError, OSError)):
                    capture.selected_config({"DASHBOARD_CONFIG": value})
            selected.write_bytes(bytes(16385))
            with self.assertRaises(ValueError):
                capture.selected_config(environment)

    def test_migration_compares_canonical_bytes_and_keeps_original_identity(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            original, upgraded = b"original v1", b"canonical v2"
            (root / "selected.eav").write_bytes(original)
            record: capture.ConfigRecord = dict(
                selection="explicit", sha256=hashlib.sha256(original).hexdigest()
            )

            def compile_stub(*args: object, **kwargs: object) -> None:
                (root / "normalized.eav").write_bytes(upgraded)

            with patch.object(capture, "run_checked", side_effect=compile_stub):
                capture.verify_selected_config(root, {}, upgraded, record)
                self.assertEqual(record["sha256"], hashlib.sha256(original).hexdigest())
                self.assertEqual(record["normalized_sha256"], hashlib.sha256(upgraded).hexdigest())
                with self.assertRaises(ValueError):
                    capture.verify_selected_config(root, {}, original, record)
                (root / "selected.eav").write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    capture.verify_selected_config(root, {}, upgraded, record)
            capture.verify_selected_config(root, {}, upgraded, dict(selection="default"))

    def test_native_canonicalizer_reuses_host_build_and_preserves_failure_checks(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            original, canonical = b"old input", b"canonical output"
            (root / "selected.eav").write_bytes(original)
            environment = {"RUSTC": "selected-host-compiler"}
            target = root / "b/h"

            def record() -> capture.ConfigRecord:
                return dict(selection="explicit", sha256=hashlib.sha256(original).hexdigest())

            def child(command: Sequence[str], **kwargs: object) -> None:
                self.assertNotIn("--release", command)
                self.assertIn("--offline", command)
                self.assertEqual(Path(command[command.index("--target-dir") + 1]), target)
                self.assertIs(kwargs["env"], environment)
                self.assertEqual(kwargs["cwd"], capture.REPO)
                self.assertTrue(kwargs["check"])
                (root / "normalized.eav").write_bytes(canonical)

            with patch.object(capture, "run_checked", side_effect=child):
                evidence = record()
                capture.verify_selected_config(
                    root, environment, canonical, evidence, target, reuse_host=True
                )
                self.assertEqual(
                    evidence["normalized_sha256"], hashlib.sha256(canonical).hexdigest()
                )
                with self.assertRaises(ValueError):
                    capture.verify_selected_config(
                        root, environment, b"wrong linked bytes", record(), target, reuse_host=True
                    )
            evidence = record()
            with patch.object(
                capture, "run_checked", side_effect=subprocess.CalledProcessError(7, ["cargo"])
            ):
                with self.assertRaises(subprocess.CalledProcessError):
                    capture.verify_selected_config(
                        root, environment, canonical, evidence, target, reuse_host=True
                    )
            self.assertNotIn("normalized_sha256", evidence)
            # Legacy CLI retains its existing release canonicalizer.
            with patch.object(capture, "run_checked") as run:
                capture.verify_selected_config(root, environment, canonical, record())
                self.assertIn("--release", run.call_args.args[0])

    def test_archive_contains_portable_crates_and_workspace_inputs(self) -> None:
        before = capture.inputs()
        keys = [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "firmware/scripts/Build-Dashboard.ps1",
            "crates/dashboard-core/Cargo.toml",
            "crates/dashboard-core/src/lib.rs",
            "crates/dashboard-core/src/display/pipeline.rs",
            "crates/dashboard-config/src/cache_identity.rs",
            "crates/dashboard-core/src/cache_diagnostics.rs",
            "firmware/pico2/src/cache_reporting.rs",
        ]
        for key in keys:
            self.assertTrue(key in before, f"missing shared input: {key}")
            self.assertEqual(before[key], (capture.REPO / key).read_bytes())
        original = Path.read_bytes
        with patch.object(
            Path,
            "read_bytes",
            lambda p: original(p) + (b"changed" if p.name == "pipeline.rs" else b""),
        ):
            self.assertNotEqual(before, capture.inputs(), "portable source escaped archive")

    def test_archive_binds_unselected_firmware_workspace_member(self) -> None:
        before = capture.inputs()
        member = capture.FIRMWARE / "ble-probe"
        for path in member.rglob("*"):
            if not path.is_file() or any(
                part in ("target", ".git", "__pycache__") for part in path.relative_to(member).parts
            ):
                continue
            key = path.relative_to(capture.REPO).as_posix()
            self.assertTrue(key in before, "Cargo resolves the unselected workspace member: " + key)
            self.assertEqual(before[key], path.read_bytes())
        original = Path.read_bytes
        manifest = member / "Cargo.toml"
        with patch.object(
            Path,
            "read_bytes",
            lambda path: (
                original(path)
                + (b"\n# dependency resolution changed\n" if path == manifest else b"")
            ),
        ):
            self.assertNotEqual(before, capture.inputs())

    def test_archive_contains_crate_root_compile_inputs(self) -> None:
        data = capture.inputs()
        for name in ("README.md", "link-rp.x.in", "build.rs", "Cargo.toml"):
            key = "firmware/vendor/embassy-rp/" + name
            self.assertEqual(data[key], (capture.REPO / key).read_bytes())
        for name in ("memory.x", "rp235x_riscv.x", "build.rs"):
            self.assertIn("firmware/pico2/" + name, data)

    def test_archive_includes_complete_radio_vendor_trees_and_binary_changes(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            names = (
                "Cargo.toml",
                "firmware/pico2/Cargo.toml",
                "firmware/ble-probe/Cargo.toml",
                "firmware/ble-probe/build.rs",
                "firmware/ble-probe/src/main.rs",
                "firmware/vendor/embassy-rp/Cargo.toml",
                "firmware/vendor/cyw43/src/lib.rs",
                "firmware/vendor/trouble-host/Cargo.toml",
                "firmware/vendor/cyw43-firmware/nested/radio.bin",
                "firmware/vendor/cyw43-provenance.json",
                "firmware/vendor/trouble-host-provenance.json",
                "firmware/scripts/capture.py",
                "host/scripts/benchmark_environment.py",
                "host/scripts/owned_process.py",
            )
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"original")
            with (
                patch.object(capture, "REPO", root),
                patch.object(capture, "FIRMWARE", root / "firmware"),
                patch.object(capture, "__file__", str(root / "firmware/scripts/capture.py")),
                patch.object(capture, "shared_inputs", return_value={root / "Cargo.toml"}),
            ):
                before = capture.inputs()
                for name in names:
                    self.assertEqual(before.get(name), b"original", name)
                binary = "firmware/vendor/cyw43-firmware/nested/radio.bin"
                (root / binary).write_bytes(b"changed radio")
                self.assertNotEqual(before, capture.inputs())
                self.assertEqual(capture.inputs()[binary], b"changed radio")

    def test_source_inventory_notices_root_compile_input_changes(self) -> None:
        original = Path.read_bytes
        before = capture.inputs()
        with patch.object(
            Path,
            "read_bytes",
            lambda p: original(p) + (b"changed" if p.name == "README.md" else b""),
        ):
            self.assertTrue(before != capture.inputs(), "changed README escaped source inventory")

    def test_complete_advanced_runtime_and_seeded_decoder_are_capture_inputs(self) -> None:
        data = capture.inputs()
        for crate, names in [
            (
                "dashboard-core",
                (
                    "acquisition.rs",
                    "presentation.rs",
                    "peak_hold.rs",
                    "warning_state.rs",
                    "status_led.rs",
                    "configured_scene.rs",
                ),
            ),
            ("dashboard-config", ("advanced.rs", "decode_in_place.rs", "slot.rs", "legacy_v3.rs")),
        ]:
            for name in names:
                key = f"crates/{crate}/src/{name}"
                self.assertEqual(data[key], (capture.REPO / key).read_bytes())

    def test_only_cargo_emitted_firmware_executable_is_selected(self) -> None:
        executable = (capture.FIRMWARE / "target/alternative/pico2").resolve()
        artifact = dict(
            reason="compiler-artifact",
            manifest_path=str(capture.FIRMWARE / "pico2/Cargo.toml"),
            target=dict(name="pico2", kind=["bin"]),
            profile=dict(test=False),
            executable=str(executable),
        )
        unrelated = dict(artifact, manifest_path=str(capture.FIRMWARE / "other/Cargo.toml"))

        def stream(*records: object) -> str:
            return "\n".join(json.dumps(record) for record in records)

        finish = dict(reason="build-finished", success=True)
        self.assertEqual(capture.emitted_elf(stream(unrelated, artifact, finish)), executable)
        for records in (
            (finish,),
            (artifact,),
            (artifact, dict(finish, success=False)),
            (artifact, artifact, finish),
            (artifact, finish, finish),
            (dict(artifact, executable=None), finish),
            (dict(artifact, executable="relative/pico2"), finish),
            (dict(artifact, profile=dict(test=True)), finish),
        ):
            with self.subTest(records=records), self.assertRaises(ValueError):
                capture.emitted_elf(stream(*records))
        with self.assertRaises(ValueError):
            capture.emitted_elf("invalid JSON")


class CaptureMemoryQualification(unittest.TestCase):
    def setUp(self) -> None:
        self.stdout = io.StringIO()
        self.enterContext(redirect_stdout(self.stdout))

    def fixture(self) -> tuple[QualifiedImageFixture, bytes, dict[str, object]]:
        from test_validation_policy import MemoryQualification
        import check_memory_layout

        elf, eav, stack = MemoryQualification.owned_fixture(MemoryQualification())
        static, headroom = check_memory_layout.geometry(elf)
        return (
            elf,
            eav,
            dict(
                check_memory_layout._qualified_image(
                    stack, elf, static, headroom, source_context=elf.fixture_source_context
                )
            ),
        )

    @staticmethod
    def bound_context(
        destination: Path, elf: QualifiedImageFixture, record: dict[str, object]
    ) -> tuple[SourceContext, Path, dict[str, object]]:
        # Rebind supplied stage facts to the actual attempt's explicit source.
        from dataclasses import replace
        from config_build_source import write_source_context
        from config_platform import current_image_policy, _execution_receipt

        value = replace(elf.fixture_source_context, attempt=destination.name)
        path = destination / "source-context.json"
        write_source_context(path, value)
        record = fixture_json(json.dumps(record))
        policy = current_image_policy(stack_policy(record), elf)
        fixture_fields(record["stack"])["execution_scope"] = _execution_receipt(
            policy, value, elf
        ).to_record()
        return value, path, record

    def test_actual_child_result_is_retained_and_rechecked_against_current_input(self) -> None:
        elf, eav, record = self.fixture()
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            frozen = destination / "pico2.elf"
            frozen.write_bytes(elf.data)
            source, path, record = self.bound_context(destination, elf, record)

            def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                self.assertEqual(Path(argv[argv.index("--source-context") + 1]), path.resolve())
                self.assertTrue(kwargs["check"])
                Path(argv[argv.index("--result-json") + 1]).write_text(json.dumps(record))
                return subprocess.CompletedProcess(argv, 0)

            with (
                patch.object(capture, "ElfImage", return_value=elf),
                patch.object(capture, "run_checked", side_effect=child) as run,
            ):
                evidence = capture.run_memory_gate(
                    frozen, destination, eav, source, path, gate="experimental-full"
                )
                self.assertEqual(evidence["record"], record)
                self.assertLessEqual((destination / "memory-layout.json").stat().st_size, 65536)
                self.assertEqual(
                    capture.read_memory_gate(
                        destination / "memory-layout.json",
                        frozen,
                        eav,
                        source,
                        gate="experimental-full",
                    ),
                    evidence,
                )
                self.assertEqual(run.call_count, 1)
                with self.assertRaises(AssertionError):
                    capture.read_memory_gate(
                        destination / "memory-layout.json",
                        frozen,
                        eav + b"changed",
                        source,
                        gate="experimental-full",
                    )
                with self.assertRaises(ValueError):
                    capture.run_memory_gate(
                        frozen, destination, eav, source, path, gate="experimental-full"
                    )

    def test_nonzero_missing_and_intermediate_child_output_never_admit(self) -> None:
        elf, eav, record = self.fixture()
        for kind in (
            "nonzero",
            "missing",
            "intermediate",
            "lost",
            "stale",
            "owned_lost",
            "owned_changed",
            "oversized",
        ):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as folder:
                destination = Path(folder).resolve()
                frozen = destination / "pico2.elf"
                frozen.write_bytes(elf.data)
                source, path, record = self.bound_context(destination, elf, record)

                def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                    value = fixture_json(
                        json.dumps(record["stack"] if kind == "intermediate" else record)
                    )
                    if kind == "lost":
                        value["status"] = "IMAGE_PASS"
                    if kind == "stale":
                        value["elf_sha256"] = "0" * 64
                    if kind == "owned_lost":
                        fixture_items(stack_policy(value)["owned_dependencies"]).pop()
                    if kind == "owned_changed":
                        dependency = fixture_fields(
                            fixture_items(stack_policy(value)["owned_dependencies"])[1]
                        )
                        dependency["receiver_start"] = (
                            integer_value(dependency["receiver_start"], "receiver") + 4
                        )
                    if kind != "missing":
                        Path(argv[argv.index("--result-json") + 1]).write_text(
                            json.dumps(value) + (" " * 65537 if kind == "oversized" else "")
                        )
                    if kind == "nonzero":
                        raise subprocess.CalledProcessError(7, argv)
                    return subprocess.CompletedProcess(argv, 0)

                with (
                    patch.object(capture, "ElfImage", return_value=elf),
                    patch.object(capture, "run_checked", side_effect=child),
                    self.assertRaises(
                        (
                            AssertionError,
                            ValueError,
                            FileNotFoundError,
                            subprocess.CalledProcessError,
                        )
                    ),
                ):
                    capture.run_memory_gate(
                        frozen, destination, eav, source, path, gate="experimental-full"
                    )

    def test_real_nonzero_child_preserves_file_without_admitting_it(self) -> None:
        elf, eav, record = self.fixture()
        real_run = run_checked
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            frozen = destination / "pico2.elf"
            frozen.write_bytes(elf.data)
            source, path, record = self.bound_context(destination, elf, record)

            def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                return real_run(
                    [
                        sys.executable,
                        "-c",
                        "import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2]);sys.exit(7)",
                        argv[argv.index("--result-json") + 1],
                        json.dumps(record),
                    ],
                    check=bool(kwargs["check"]),
                    timeout=10,
                )

            with (
                patch.object(capture, "ElfImage", return_value=elf),
                patch.object(capture, "run_checked", side_effect=child),
                self.assertRaises(subprocess.CalledProcessError) as error,
            ):
                capture.run_memory_gate(
                    frozen, destination, eav, source, path, gate="experimental-full"
                )
            self.assertEqual(error.exception.returncode, 7)
            self.assertEqual(json.loads((destination / "memory-layout.json").read_text()), record)

    def test_actual_manifest_path_retains_qualifier_and_rejects_final_drift(self) -> None:
        elf, eav, record = self.fixture()
        from config_build_source import SOURCE_PATHS, load_source_context

        before_sources = {name: (capture.REPO / name).read_bytes() for name in SOURCE_PATHS}
        for drift in (
            False,
            True,
            "source_bytes",
            "source_members",
            "options",
            "captured_sources",
            "program_gate",
        ):
            with self.subTest(drift=drift), tempfile.TemporaryDirectory() as folder:
                firmware = Path(folder).resolve()
                destination = firmware / "target/bench-captures/fixture"
                emitted = firmware / "short/firmware.elf"
                calls: list[str] = []
                input_calls: list[str] = []

                def snapshot() -> dict[str, bytes]:
                    input_calls.append("inputs")
                    if drift == "captured_sources" and len(input_calls) > 1:
                        changed = dict(before_sources)
                        key = "firmware/pico2/src/main.rs"
                        changed[key] += b"\n// later source drift"
                        return changed
                    return before_sources

                def build(*args: object) -> subprocess.CompletedProcess[str]:
                    emitted.parent.mkdir()
                    emitted.write_bytes(elf.data)
                    return subprocess.CompletedProcess([], 0, "messages")

                def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                    if "scripts/check_memory_layout.py" in argv:
                        self.assertIn("--source-context", argv)
                        context_path = Path(argv[argv.index("--source-context") + 1])
                        self.assertEqual(context_path, destination / "source-context.json")
                        self.assertNotEqual(
                            context_path, Path(argv[argv.index("--result-json") + 1])
                        )
                        parsed = load_source_context(context_path, elf)
                        self.assertEqual(dict(parsed.sources), before_sources)
                        from config_platform import current_image_policy, _execution_receipt

                        policy = current_image_policy(stack_policy(record), elf)
                        fixture_fields(record["stack"])["execution_scope"] = _execution_receipt(
                            policy, parsed, elf
                        ).to_record()
                        Path(argv[argv.index("--result-json") + 1]).write_text(json.dumps(record))
                    return subprocess.CompletedProcess(argv, 0)

                def clock(*args: object) -> dict[str, bytes]:
                    calls.append("clock")
                    if drift and drift != "captured_sources" and len(calls) == 2:
                        if drift is True:
                            changed = dict(record, hardware_qualified=True)
                            (destination / "memory-layout.json").write_text(json.dumps(changed))
                        elif drift == "options":
                            path = destination / "build-options.json"
                            path.write_bytes(path.read_bytes() + b" ")
                        elif drift == "source_bytes":
                            path = destination / "source-context.json"
                            path.write_bytes(path.read_bytes() + b" ")
                        else:
                            path = destination / "source-context.json"
                            changed = fixture_json(path.read_bytes())
                            source = fixture_fields(fixture_items(changed["sources"])[0])
                            source["utf8"] = (
                                string_value(source["utf8"], "source")
                                + "\n// late changed source bytes"
                            )
                            raw = string_value(source["utf8"], "source").encode()
                            source["length"] = len(raw)
                            source["sha256"] = hashlib.sha256(raw).hexdigest()
                            path.write_text(json.dumps(changed))
                    return dict(source=b"clock source", metadata=b"clock metadata")

                options = dict(
                    benchmark=True,
                    features=["demo"],
                    optimization="3",
                    custom_clock=dict(cpu_hz="150000000", spi_hz="62500000", vsel_mv="1100"),
                )

                def verify_selection(
                    dest: Path,
                    env: dict[str, str],
                    embedded: bytes,
                    rec: capture.ConfigRecord,
                    target: Path,
                    **kw: object,
                ) -> None:
                    self.assertEqual(env, {"RUSTC": "rustc"})
                    self.assertEqual(target, firmware / "host")
                    self.assertEqual(kw, {"reuse_host": True})

                with ExitStack() as patches:
                    for context_patch in (
                        patch.object(capture, "FIRMWARE", firmware),
                        patch.object(
                            capture,
                            "build_directories",
                            return_value=(
                                emitted.parent,
                                firmware / "host",
                                firmware / "normalizer",
                            ),
                        ),
                        patch.object(capture, "ElfImage", return_value=elf),
                        patch.object(capture, "check_clock_environment"),
                        patch.object(capture, "compiler", return_value=("rustc", "pinned")),
                        patch.object(capture, "configuration", return_value={}),
                        patch.object(capture, "inputs", side_effect=snapshot),
                        patch.object(
                            capture,
                            "linked_configuration",
                            return_value={"test_only_linked_config": True},
                        ),
                        patch.object(
                            capture,
                            "inspect_program",
                            side_effect=AssertionError("program rejected")
                            if drift == "program_gate"
                            else None,
                            return_value=({}, {"test_only_program_gate": True}),
                        ),
                        patch.object(capture, "fixed_json_tool", return_value=(["tool"], options)),
                        patch.object(capture, "selected_config", return_value=("default", None)),
                        patch.object(capture, "freeze_config", return_value=({}, {})),
                        patch.object(
                            capture,
                            "verify_selected_config",
                            side_effect=verify_selection,
                        ),
                        patch.object(capture, "run_build", side_effect=build),
                        patch.object(capture, "emitted_elf", return_value=emitted),
                        patch.object(capture, "generated_clock", side_effect=clock),
                        patch.object(capture, "firmware_package", return_value="id"),
                        patch.object(
                            capture,
                            "embedded_config",
                            side_effect=lambda output, path: (
                                eav
                                if path == emitted.parent
                                else self.fail("embedded config used old build path")
                            ),
                        ),
                        patch.object(capture, "run_checked", side_effect=child),
                        patch.object(
                            sys,
                            "argv",
                            [
                                "capture",
                                "fixture",
                                "--native-scratch",
                                "--image-gate",
                                "experimental-full",
                            ],
                        ),
                        patch.dict(os.environ, {}, clear=True),
                    ):
                        patches.enter_context(context_patch)
                    if drift:
                        with self.assertRaises((AssertionError, ValueError)):
                            capture.main()
                        self.assertFalse((destination / "manifest.json").exists())
                    else:
                        capture.main()
                        manifest = fixture_json((destination / "manifest.json").read_text())
                        self.assertEqual(manifest["schema"], 2)
                        self.assertEqual(
                            manifest["program_image"],
                            {
                                "test_only_program_gate": True,
                                "linked_configuration": {"test_only_linked_config": True},
                            },
                        )
                        self.assertEqual(manifest["memory_layout"], record)
                        self.assertEqual(
                            fixture_fields(
                                fixture_fields(fixture_fields(manifest["memory_layout"])["stack"])[
                                    "execution_scope"
                                ]
                            )["schema"],
                            1,
                        )
                        self.assertNotIn("execution_scope", manifest)
                        self.assertNotIn(
                            "execution_scope", fixture_fields(manifest["memory_layout"])
                        )
                        source_path = destination / "source-context.json"
                        self.assertTrue(
                            source_path.is_file(),
                            "capture omitted its admitted pre-gate source context",
                        )
                        context = load_source_context(
                            source_path,
                            elf,
                            "fixture",
                            (destination / "build-options.json").read_bytes(),
                        )
                        self.assertEqual(
                            {name: raw for name, raw in context.sources}, before_sources
                        )
                        self.assertEqual(
                            fixture_fields(manifest["source_context"])["binding"], context.binding()
                        )
                        self.assertEqual(
                            fixture_fields(manifest["source_context"])["sha256"],
                            hashlib.sha256(source_path.read_bytes()).hexdigest(),
                        )
                        with zipfile.ZipFile(destination / "source.zip") as archive:
                            self.assertEqual(
                                archive.read("build-config/source-context.json"),
                                source_path.read_bytes(),
                            )
                        self.assertEqual(
                            len(
                                fixture_items(
                                    stack_policy(fixture_fields(manifest["memory_layout"]))[
                                        "owned_dependencies"
                                    ]
                                )
                            ),
                            10,
                        )
                        self.assertEqual(
                            manifest["memory_layout_json_sha256"],
                            hashlib.sha256(
                                (destination / "memory-layout.json").read_bytes()
                            ).hexdigest(),
                        )

    def test_explicit_source_sidecar_route_rejects_missing_changed_and_unproved_semantics(
        self,
    ) -> None:
        from config_build_source import SOURCE_PATHS, make_source_context, write_source_context

        elf, eav, record = self.fixture()
        for fault in (
            None,
            "missing_pair",
            "missing_context",
            "missing_both",
            "attempt",
            "changed_bytes",
            "semantics",
        ):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as folder:
                destination = Path(folder).resolve()
                frozen = destination / "pico2.elf"
                frozen.write_bytes(elf.data)
                before = {name: (capture.REPO / name).read_bytes() for name in SOURCE_PATHS}
                if fault == "semantics":
                    before[SOURCE_PATHS[1]] += b" pub fn leak() { CONFIGURATION.take(); }"
                value = make_source_context(
                    elf.data, "other" if fault == "attempt" else destination.name, b"{}", before
                )
                path = destination / "source-context.json"
                write_source_context(path, value)
                from config_platform import current_image_policy, _execution_receipt

                fixture_fields(record["stack"])["execution_scope"] = _execution_receipt(
                    current_image_policy(stack_policy(record), elf), value, elf
                ).to_record()
                if fault == "changed_bytes":
                    path.write_bytes(path.read_bytes() + b"not JSON")

                def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                    self.assertEqual(Path(argv[argv.index("--source-context") + 1]), path)
                    Path(argv[argv.index("--result-json") + 1]).write_text(json.dumps(record))
                    return subprocess.CompletedProcess(argv, 0)

                with (
                    patch.object(capture, "ElfImage", return_value=elf),
                    patch.object(capture, "run_checked", side_effect=child) as run,
                ):
                    if fault is None:
                        self.assertEqual(
                            capture.run_memory_gate(
                                frozen, destination, eav, value, path, gate="experimental-full"
                            )["record"],
                            record,
                        )
                        self.assertEqual(run.call_count, 1)
                    else:
                        with self.assertRaises((AssertionError, ValueError)):
                            capture.run_memory_gate(
                                frozen,
                                destination,
                                eav,
                                None if fault in ("missing_context", "missing_both") else value,
                                None if fault in ("missing_pair", "missing_both") else path,
                                gate="experimental-full",
                            )
                        run.assert_not_called()
                        self.assertFalse((destination / "memory-layout.json").exists())

    def test_real_python_child_reads_explicit_source_bytes_before_supplied_image_output(
        self,
    ) -> None:
        from config_build_source import SOURCE_PATHS, make_source_context, write_source_context

        elf, eav, record = self.fixture()
        real_run = run_checked
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder).resolve()
            frozen = destination / "pico2.elf"
            frozen.write_bytes(elf.data)
            before = {name: (capture.REPO / name).read_bytes() for name in SOURCE_PATHS}
            value = make_source_context(elf.data, destination.name, b"{}", before)
            source = destination / "source-context.json"
            write_source_context(source, value)
            from config_platform import current_image_policy, _execution_receipt

            fixture_fields(record["stack"])["execution_scope"] = _execution_receipt(
                current_image_policy(stack_policy(record), elf), value, elf
            ).to_record()
            code = (
                "import sys;from pathlib import Path;from types import SimpleNamespace;"
                "sys.path.insert(0,sys.argv[1]);from config_build_source import load_source_context,inspect_source_context;"
                "elf=SimpleNamespace(data=Path(sys.argv[2]).read_bytes());"
                "context=load_source_context(sys.argv[3],elf,sys.argv[4],b'{}');"
                "inspect_source_context(context,elf);Path(sys.argv[5]).write_text(sys.argv[6])"
            )

            def child(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
                return real_run(
                    [
                        sys.executable,
                        "-B",
                        "-c",
                        code,
                        str(capture.REPO / "firmware/scripts"),
                        str(frozen),
                        argv[argv.index("--source-context") + 1],
                        destination.name,
                        argv[argv.index("--result-json") + 1],
                        json.dumps(record),
                    ],
                    check=bool(kwargs["check"]),
                    timeout=10,
                )

            with (
                patch.object(capture, "ElfImage", return_value=elf),
                patch.object(capture, "run_checked", side_effect=child),
            ):
                self.assertEqual(
                    capture.run_memory_gate(
                        frozen, destination, eav, value, source, gate="experimental-full"
                    )["record"],
                    record,
                )
            # The child verifies actual sidecar bytes, but its image stages are
            # supplied fixtures. This is not a linked-firmware/image proof.


if __name__ == "__main__":
    unittest.main()
