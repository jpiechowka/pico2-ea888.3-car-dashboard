"""Standalone utility admission regression tests; no target execution."""

from copy import deepcopy
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import ble_probe_image as image
import build_ble_probe as build


class ProbeAdmission(unittest.TestCase):
    def test_build_environment_is_owned_and_bounded(self):
        original = {
            "PATH": "kept",
            "DEFMT_LOG": "trace",
            "RUSTFLAGS": "generic",
            "RUSTC": "unexpected",
            "CARGO_BUILD_JOBS": "99",
            "CARGO_BUILD_TARGET": "wrong",
            "CARGO_TARGET_THUMBV8M_MAIN_NONE_EABIHF_LINKER": "unexpected",
            "CARGO_PROFILE_RELEASE_STRIP": "symbols",
            "DASHBOARD_CPU_HZ": "290000000",
        }
        before = dict(original)
        env = build.build_environment(original, "nightly-test", Path("attempt"))
        self.assertEqual(original, before)
        self.assertEqual(env["CARGO_BUILD_JOBS"], "2")
        self.assertEqual(env["CARGO_PROFILE_RELEASE_STRIP"], "none")
        self.assertIn("target-cpu=cortex-m33", env["CARGO_ENCODED_RUSTFLAGS"])
        self.assertEqual(env["PATH"], "kept")
        self.assertEqual(env["DEFMT_LOG"], "info,trouble_host=warn,cyw43=warn,cyw43_pio=warn")
        for key in (
            "RUSTFLAGS",
            "RUSTC",
            "CARGO_BUILD_TARGET",
            "DASHBOARD_CPU_HZ",
            "CARGO_TARGET_THUMBV8M_MAIN_NONE_EABIHF_LINKER",
        ):
            self.assertNotIn(key, env)

    def test_generated_clock_layouts_and_ambiguity(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            flat = root / "dashboard-pico2-hash/out/clock_profile.json"
            nested = root / "dashboard-pico2/hash/out/clock_profile.json"
            flat.parent.mkdir(parents=True)
            flat.write_text("flat")
            self.assertEqual(build.find_clock_metadata(root), flat)
            flat.unlink()
            nested.parent.mkdir(parents=True)
            nested.write_text("nested")
            self.assertEqual(build.find_clock_metadata(root), nested)
            flat.write_text("flat")
            with self.assertRaises(AssertionError):
                build.find_clock_metadata(root)
            flat.unlink()
            nested.unlink()
            with self.assertRaises(AssertionError):
                build.find_clock_metadata(root)

    def test_changed_or_added_source_rejected(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "firmware/ble-probe/src").mkdir(parents=True)
            source = root / "firmware/ble-probe/src/main.rs"
            source.write_text("first")
            before = image.source_hashes(root)
            image.check_sources(root, before)
            source.write_text("changed")
            with self.assertRaises(AssertionError):
                image.check_sources(root, before)
            source.write_text("first")
            source.with_name("new.rs").write_text("new")
            with self.assertRaises(AssertionError):
                image.check_sources(root, before)

    def test_stack_limits_do_not_accept_largest_frame_alone(self):
        values = dict(boot=4000, core0=12000, radio=24000, render=20000)
        image.stack_budgets(values, 65536)
        for name, value in (("boot", 60000), ("core0", 60000), ("radio", 60000), ("render", 30000)):
            bad = dict(values, **{name: value})
            with self.subTest(name=name), self.assertRaises(AssertionError):
                image.stack_budgets(bad, 65536)

    def test_exact_objects_and_nonoverlap(self):
        elf = SimpleNamespace(
            symbol_entries=[
                ("BLE_PROBE_FRAMEBUFFER_0", 0x20000000, 153600, 1, 1),
                ("BLE_PROBE_FRAMEBUFFER_1", 0x20025800, 153600, 1, 1),
                ("BLE_PROBE_CORE1_STACK", 0x2004B000, 32768, 1, 1),
            ]
        )
        image.objects(elf)
        for index, size in ((0, 153601), (1, 153599), (2, 16384)):
            bad = deepcopy(elf)
            row = list(bad.symbol_entries[index])
            row[2] = size
            bad.symbol_entries[index] = tuple(row)
            with self.assertRaises(AssertionError):
                image.objects(bad)
        bad = deepcopy(elf)
        row = list(bad.symbol_entries[1])
        row[1] -= 4
        bad.symbol_entries[1] = tuple(row)
        with self.assertRaises(AssertionError):
            image.objects(bad)

    def test_named_paths_and_metadata_are_required(self):
        names = (
            "embassy_main_task",
            "probe_ui_task",
            "probe_display_task",
            "probe_inputs_task",
            "probe_encoder_task",
            "probe_supervisor_task",
            "probe_radio_task",
        )
        functions = {
            "TaskStorage4poll" + name: (0x10001000 + i * 4, 4, 1) for i, name in enumerate(names)
        }
        functions["probe_core1"] = (0x10002000, 4, 1)
        functions["Executor_run_probe_core1_closure"] = (0x10002004, 4, 1)
        sizes = {address: 128 for address, _, _ in functions.values()}
        elf = SimpleNamespace(
            named={".stack_sizes": (0, 1, 0, 0, 100, 16)},
            segments=[],
            functions=functions,
            section_bytes=lambda _: b"metadata",
        )
        with patch.object(image, "frames", return_value=sizes):
            result = image.stack_check(elf, 65536)
            self.assertEqual(result["bounds"]["radio"], 128 + 24576)
            del functions["TaskStorage4pollprobe_radio_task"]
            with self.assertRaises(AssertionError):
                image.stack_check(elf, 65536)
        elf.named[".stack_sizes"] = (0, 1, 2, 0, 100, 16)
        with self.assertRaises(AssertionError):
            image.stack_check(elf, 65536)

    def test_load_metadata_overlap_rejected(self):
        elf = SimpleNamespace(
            named={".stack_sizes": (0, 1, 0, 0, 100, 16)}, segments=[(1, 80, 0, 0, 40, 40, 0, 0)]
        )
        with self.assertRaises(AssertionError):
            image.stack_check(elf, 65536)


if __name__ == "__main__":
    unittest.main()
