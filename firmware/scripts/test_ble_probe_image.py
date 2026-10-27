"""Standalone utility admission regression tests; no target execution."""

from copy import deepcopy
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from check_program_image import ProgramMap
from image_test_support import ImageFixture, section_fields, segment_fields
import unittest
from unittest.mock import patch

import ble_probe_image as image
import build_ble_probe as build


class ProbeAdmission(unittest.TestCase):
    def test_identity_tool_requires_explicit_binary_features_and_marker(self) -> None:
        args = build.build_command("stock", "ngm-identity")
        self.assertEqual(args[args.index("--bin") + 1], "ngm-identity")
        self.assertIn("ngm-identity", args[args.index("--features") + 1].split(","))
        args = build.build_command("280-70", "ngm-identity")
        self.assertEqual(
            set(args[args.index("--features") + 1].split(",")),
            {"ngm-identity", "cpu280-spi70-1v30"},
        )
        with self.assertRaises(AssertionError):
            build.build_command("stock", "unknown")
        for value, name in ((0, "ble-probe"), (1, "ngm-identity")):
            elf = ImageFixture(
                data=bytes([value]),
                section_entries=[(1, ".probe_tool", (0, 1, 2, 0x10000100, 0, 1, 0, 0, 1, 0))],
                symbol_entries=[("BLE_PROBE_TOOL_KIND", 0x10000100, 1, 0x11, 1)],
                segments=[(1, 0, 0x10000100, 0x10000100, 1, 1, 4, 1)],
            )
            self.assertEqual(image.linked_tool(elf, name), name)
            for wrong in ("unknown", "ble-probe" if value else "ngm-identity"):
                with self.assertRaises(AssertionError):
                    image.linked_tool(elf, wrong)
            elf.data = b"\x02"
            with self.assertRaises(AssertionError):
                image.linked_tool(elf)

    def test_manifest_requires_object_and_exact_integer_schema_before_source_access(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            malformed_manifests: tuple[object, ...] = (
                [],
                None,
                {"gate": image.GATE, "schema": True, "sources": {}},
            )
            for malformed in malformed_manifests:
                manifest_path.write_text(json.dumps(malformed), encoding="utf-8")
                with (
                    self.subTest(manifest=malformed),
                    patch.object(
                        image, "check_sources", side_effect=AssertionError("source access reached")
                    ) as sources,
                    self.assertRaisesRegex(AssertionError, "utility manifest schema"),
                ):
                    image.check(root / "missing.elf", root / "missing.uf2", manifest_path, root)
                sources.assert_not_called()

    def test_release_options_are_scoped_to_the_selected_binary(self) -> None:
        for profile in ("stock", "280-70"):
            args = build.build_command(profile)
            separator = args.index("--")
            cargo_args, rustc_args = args[:separator], args[separator + 1 :]
            self.assertEqual(cargo_args[:2], ["cargo", "rustc"])
            self.assertEqual(cargo_args[cargo_args.index("--bin") + 1], "ble-probe")
            self.assertEqual(
                cargo_args[cargo_args.index("--target") + 1], "thumbv8m.main-none-eabihf"
            )
            self.assertEqual(rustc_args, ["-C", "llvm-args=--inline-threshold=100000"])
            self.assertEqual("cpu280-spi70-1v30" in cargo_args, profile == "280-70")
            env = build.build_environment({}, "nightly-test", Path("attempt"))
            self.assertNotIn("inline-threshold", env["CARGO_ENCODED_RUSTFLAGS"])
        with self.assertRaises(AssertionError):
            build.build_command("unknown")

    def test_build_environment_is_owned_and_bounded(self) -> None:
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
            "BLE_PROBE_SOURCE_DIGEST": "untrusted inherited identity",
            "ble_probe_source_digest": "case-insensitive Windows override",
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
            "BLE_PROBE_SOURCE_DIGEST",
            "ble_probe_source_digest",
            "CARGO_TARGET_THUMBV8M_MAIN_NONE_EABIHF_LINKER",
        ):
            self.assertNotIn(key, env)

    def test_canonical_source_digest_is_order_independent_and_strict(self) -> None:
        source = {"z": "11" * 32, "a": "00" * 32}
        self.assertEqual(
            image.source_digest(source),
            "52a50a8f344d28cd19867191a8ca1f0d3e2fde3a7ac7476bada1dcfcc2c97b38",
        )
        self.assertEqual(
            image.source_digest(source), image.source_digest(dict(reversed(list(source.items()))))
        )
        for invalid in ({}, {"a": "not a digest"}, {"a": 12}, {1: "00" * 32}):
            with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                image.source_digest(invalid)

    @staticmethod
    def identity_elf(digest: bytes = bytes(range(32))) -> ImageFixture:
        section = (0, 1, 2, 0x10000100, 16, 32, 0, 0, 4, 0)
        return ImageFixture(
            data=bytes(16) + digest + b"\x00",
            section_entries=[
                (1, ".probe_identity", section),
                (2, ".probe_tool", (0, 1, 2, 0x10000120, 48, 1, 0, 0, 1, 0)),
            ],
            symbol_entries=[
                ("BLE_PROBE_SOURCE_DIGEST", 0x10000100, 32, 0x11, 1),
                ("BLE_PROBE_TOOL_KIND", 0x10000120, 1, 0x11, 2),
            ],
            segments=[(1, 16, 0x10000100, 0x10000100, 33, 33, 4, 4)],
        )

    def test_linked_identity_is_unique_read_only_file_backed_and_exact(self) -> None:
        elf = self.identity_elf()
        self.assertEqual(
            image.linked_identity(elf, bytes(range(32)).hex())["digest"], bytes(range(32)).hex()
        )
        for fault in (
            "wrong",
            "missing",
            "duplicate",
            "size",
            "index",
            "flags",
            "type",
            "load",
            "lma",
            "truncated",
        ):
            bad = deepcopy(elf)
            if fault == "wrong":
                bad.data = bytes(48)
            elif fault == "missing":
                bad.symbol_entries = []
            elif fault == "duplicate":
                bad.symbol_entries *= 2
            elif fault in ("size", "index"):
                name, address, size, info, index = bad.symbol_entries[0]
                bad.symbol_entries[0] = (
                    name,
                    address,
                    size + (fault == "size"),
                    info,
                    index + (fault == "index"),
                )
            elif fault in ("flags", "type"):
                index, name, section = bad.section_entries[0]
                row = list(section)
                row[2 if fault == "flags" else 1] = 3 if fault == "flags" else 8
                bad.section_entries[0] = (index, name, section_fields(row))
            elif fault == "load":
                bad.segments *= 2
            elif fault == "lma":
                row = list(bad.segments[0])
                row[3] = 0x101E6000
                bad.segments[0] = segment_fields(row)
            else:
                bad.data = bad.data[:47]
            with self.subTest(fault=fault), self.assertRaises(AssertionError):
                image.linked_identity(bad, bytes(range(32)).hex())
        zero = self.identity_elf(bytes(32))
        self.assertFalse(image.linked_identity(zero)["available"])
        with self.assertRaises(AssertionError):
            image.linked_identity(zero, "00" * 32)

    def test_retained_identity_remains_read_only_and_rejects_other_flags(self) -> None:
        for flags in (0x200002, 0, 0x200003, 0x200006, 0x200012):
            elf = self.identity_elf()
            index, name, section = elf.section_entries[0]
            row = list(section)
            row[2] = flags
            elf.section_entries[0] = (index, name, section_fields(row))
            with self.subTest(flags=flags):
                if flags == 0x200002:
                    self.assertTrue(image.linked_identity(elf)["available"])
                else:
                    with self.assertRaises(AssertionError):
                        image.linked_identity(elf)

    @staticmethod
    def reserved_elf() -> ImageFixture:
        return ImageFixture(
            symbol_entries=[
                (name, value, 0, 0x10, 0xFFF1)
                for name, value in (
                    ("__captures_start", 0x101E6000),
                    ("__captures_end", 0x101FE000),
                    ("__settings_start", 0x101FE000),
                    ("__settings_end", 0x10200000),
                )
            ],
            segments=[(1, 0, 0x20000000, 0x101E5F00, 256, 256, 6, 4)],
        )

    def test_all_physical_payload_erase_and_uf2_pages_preserve_captures(self) -> None:
        elf = self.reserved_elf()
        mapping: ProgramMap = {
            "payload_sha256": "unused synthetic digest",
            "fragments": [(0x101E5F00, bytes(256))],
            "erase_ranges": [(0x101E5000, 0x101E6000)],
            "pages": {0x101E5F00: bytes(256)},
            "verification_ranges": [(0x101E5F00, 0x101E6000)],
        }
        image.capture_reservation(elf, mapping)
        for fault in (
            "load",
            "later_load",
            "erase",
            "page",
            "fragment",
            "verification",
            "symbol",
            "duplicate_symbol",
        ):
            bad, badmap = deepcopy(elf), deepcopy(mapping)
            if fault in ("load", "later_load"):
                segment = (1, 0, 0x20000000, 0x101E6000, 1, 1, 6, 4)
                if fault == "load":
                    bad.segments = [segment]
                else:
                    bad.segments.append(segment)
            elif fault == "symbol":
                name, address, size, info, index = bad.symbol_entries[0]
                bad.symbol_entries[0] = (name, address + 4096, size, info, index)
            elif fault == "duplicate_symbol":
                bad.symbol_entries.append(bad.symbol_entries[0])
            elif fault == "erase":
                badmap["erase_ranges"] = [(0x101E5000, 0x101E7000)]
            elif fault == "page":
                badmap["pages"][0x101E6000] = bytes(256)
            elif fault == "fragment":
                badmap["fragments"].append((0x101E6000, b"x"))
            else:
                badmap["verification_ranges"] = [(0x101E5000, 0x101E7000)]
            with self.subTest(fault=fault), self.assertRaises(AssertionError):
                image.capture_reservation(bad, badmap)

    def test_full_gate_binds_manifest_digest_and_executes_shared_flash_gate(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "firmware/ble-probe/src/main.rs"
            source.parent.mkdir(parents=True)
            source.write_text("fn main() {}")
            sources = image.source_hashes(root)
            digest = image.source_digest(sources)
            elf = self.identity_elf(bytes.fromhex(digest))
            elf.symbol_entries.extend(self.reserved_elf().symbol_entries)
            mapping: ProgramMap = {
                "payload_sha256": "unused synthetic digest",
                "fragments": [(0x10000100, bytes.fromhex(digest))],
                "erase_ranges": [(0x10000000, 0x10001000)],
                "verification_ranges": [(0x10000100, 0x10000120)],
                "pages": {0x10000100: bytes(256)},
            }
            elf_path, uf2_path, manifest_path = (
                root / name for name in ("image.elf", "image.uf2", "manifest.json")
            )
            elf_path.write_bytes(elf.data)
            uf2_path.write_bytes(b"mocked canonical UF2")
            manifest = dict(
                gate=image.GATE,
                schema=2,
                tool="ble-probe",
                sources=sources,
                source_digest=digest,
                target="thumbv8m.main-none-eabihf",
                target_cpu="cortex-m33",
                elf_sha256=image.sha(elf.data),
                uf2_sha256=image.sha(uf2_path.read_bytes()),
            )

            def write_manifest(value: object) -> None:
                manifest_path.write_text(json.dumps(value))

            write_manifest(manifest)
            with (
                patch.object(image, "ElfImage", return_value=elf),
                patch.object(image, "geometry", return_value=(0, 65536)),
                patch.object(image, "inspect_program", return_value=(mapping, {})),
                patch.object(image, "objects", return_value={}),
                patch.object(image, "stack_check", return_value={}),
                patch.object(
                    image, "verify_uf2", return_value={"uf2_sha256": manifest["uf2_sha256"]}
                ),
                patch.object(image, "check_flash_lockout") as flash,
            ):
                result = image.check(elf_path, uf2_path, manifest_path, root)
                self.assertTrue(result["source_identity"]["snapshot_bound"])
                self.assertEqual(result["tool"], "ble-probe")
                flash.assert_called_once_with(elf)
                for wrong in (None, "ngm-identity", "unknown", True):
                    write_manifest(dict(manifest, tool=wrong))
                    with self.subTest(tool=wrong), self.assertRaises(AssertionError):
                        image.check(elf_path, uf2_path, manifest_path, root)
                for invalid in (None, "00" * 32, "garbage"):
                    write_manifest(dict(manifest, source_digest=invalid))
                    with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                        image.check(elf_path, uf2_path, manifest_path, root)
                write_manifest(manifest)
                with (
                    patch.object(
                        image,
                        "check_flash_lockout",
                        side_effect=AssertionError("unreviewed helper"),
                    ),
                    self.assertRaises(AssertionError),
                ):
                    image.check(elf_path, uf2_path, manifest_path, root)
                elf.data = bytes(len(elf.data))
                write_manifest(dict(manifest, elf_sha256=image.sha(elf.data)))
                with self.assertRaises(AssertionError):
                    image.check(elf_path, uf2_path, manifest_path, root)

    def test_elf_only_allows_unavailable_identity_but_still_gates_flash_and_reservations(
        self,
    ) -> None:
        elf = self.identity_elf(bytes(32))
        elf.symbol_entries.extend(self.reserved_elf().symbol_entries)
        mapping: ProgramMap = {
            "payload_sha256": "unused synthetic digest",
            "fragments": [(0x10000100, bytes(32))],
            "erase_ranges": [(0x10000000, 0x10001000)],
            "verification_ranges": [(0x10000100, 0x10000120)],
            "pages": {0x10000100: bytes(256)},
        }
        with (
            patch.object(image, "geometry", return_value=(0, 65536)),
            patch.object(image, "inspect_program", return_value=(mapping, {})),
            patch.object(image, "objects", return_value={}),
            patch.object(image, "stack_check", return_value={}),
            patch.object(image, "check_flash_lockout") as flash,
        ):
            result = image.inspect_elf(elf)
            self.assertFalse(result["source_identity"]["available"])
            self.assertFalse(result["source_bound"])
            flash.assert_called_once_with(elf)
            with (
                patch.object(
                    image, "check_flash_lockout", side_effect=AssertionError("unreviewed helper")
                ),
                self.assertRaises(AssertionError),
            ):
                image.inspect_elf(elf)

    def test_generated_clock_layouts_and_ambiguity(self) -> None:
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

    def test_changed_or_added_source_rejected(self) -> None:
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

    def test_stack_limits_do_not_accept_largest_frame_alone(self) -> None:
        values = dict(boot=4000, core0=12000, radio=24000, render=20000)
        image.stack_budgets(values, 65536)
        for name, value in (("boot", 60000), ("core0", 60000), ("radio", 60000), ("render", 30000)):
            bad = dict(values, **{name: value})
            with self.subTest(name=name), self.assertRaises(AssertionError):
                image.stack_budgets(bad, 65536)

    def test_exact_objects_and_nonoverlap(self) -> None:
        elf = ImageFixture(
            symbol_entries=[
                ("BLE_PROBE_FRAMEBUFFER_0", 0x20000000, 153600, 1, 1),
                ("BLE_PROBE_FRAMEBUFFER_1", 0x20025800, 153600, 1, 1),
                ("BLE_PROBE_CORE1_STACK", 0x2004B000, 32768, 1, 1),
            ]
        )
        image.objects(elf)
        for index, size in ((0, 153601), (1, 153599), (2, 16384)):
            bad = deepcopy(elf)
            name, address, _, info, section = bad.symbol_entries[index]
            bad.symbol_entries[index] = (name, address, size, info, section)
            with self.assertRaises(AssertionError):
                image.objects(bad)
        bad = deepcopy(elf)
        name, address, size, info, index = bad.symbol_entries[1]
        bad.symbol_entries[1] = (name, address - 4, size, info, index)
        with self.assertRaises(AssertionError):
            image.objects(bad)

    def test_named_paths_and_metadata_are_required(self) -> None:
        names = (
            "embassy_main_task",
            "probe_ui_task",
            "probe_display_task",
            "probe_inputs_task",
            "probe_encoder_task",
            "probe_supervisor_task",
            "probe_radio_task",
            "probe_storage_task",
            "probe_usb_device_task",
            "probe_usb_capture_task",
        )
        functions = {
            "TaskStorage4poll" + name: (0x10001000 + i * 4, 4, 1) for i, name in enumerate(names)
        }
        functions["probe_core1"] = (0x10002000, 4, 1)
        functions["Executor_run_probe_core1_closure"] = (0x10002004, 4, 1)
        sizes = {address: 128 for address, _, _ in functions.values()}
        elf = ImageFixture(
            named={".stack_sizes": (0, 1, 0, 0, 100, 16, 0, 0, 1, 0)},
            segments=[],
            functions=functions,
            section_data={".stack_sizes": b"metadata"},
        )
        with patch.object(image, "frames", return_value=sizes):
            result = image.stack_check(elf, 65536)
            self.assertEqual(result["bounds"]["radio"], 128 + 24576)
            for name in ("probe_storage_task", "probe_usb_device_task", "probe_usb_capture_task"):
                key = "TaskStorage4poll" + name
                saved = functions.pop(key)
                with self.subTest(missing=name), self.assertRaises(AssertionError):
                    image.stack_check(elf, 65536)
                functions[key] = saved
            del functions["TaskStorage4pollprobe_radio_task"]
            with self.assertRaises(AssertionError):
                image.stack_check(elf, 65536)
        elf.named[".stack_sizes"] = (0, 1, 2, 0, 100, 16, 0, 0, 1, 0)
        with self.assertRaises(AssertionError):
            image.stack_check(elf, 65536)

    def test_load_metadata_overlap_rejected(self) -> None:
        elf = ImageFixture(
            named={".stack_sizes": (0, 1, 0, 0, 100, 16, 0, 0, 1, 0)},
            segments=[(1, 80, 0, 0, 40, 40, 0, 0)],
        )
        with self.assertRaises(AssertionError):
            image.stack_check(elf, 65536)

    def test_identity_radio_ownership_preserves_all_frame_limits(self) -> None:
        names = (
            "embassy_main_task",
            "probe_ui_task",
            "probe_display_task",
            "probe_inputs_task",
            "probe_encoder_task",
            "probe_supervisor_task",
            "probe_radio_task",
            "probe_storage_task",
            "probe_usb_device_task",
            "probe_usb_capture_task",
        )
        functions = {
            "TaskStorage4poll" + name: (0x10001000 + i * 4, 4, 1) for i, name in enumerate(names)
        }
        functions["probe_core1"] = (0x10002000, 4, 1)
        sizes = {address: 128 for address, _, _ in functions.values()}
        additions = (
            ("14identity_radio12inspect_peer", 0x10003000, 2048),
            ("13identity_flow13run_connected", 0x10003004, 2048),
            ("14dashboard_core9discovery6render", 0x10003008, 6500),
        )
        for name, address, size in additions:
            functions[name] = (address, 4, 1)
            sizes[address] = size
        elf = ImageFixture(
            named={".stack_sizes": (0, 1, 0, 0, 100, 16, 0, 0, 1, 0)},
            functions=functions,
            section_data={".stack_sizes": b"metadata"},
        )
        with patch.object(image, "frames", return_value=sizes):
            result = image.stack_check(elf, 65536)
            self.assertEqual(result["nonradio_frame_total"], 11 * 128 + 6500)
            self.assertEqual(result["emitted_frame_total"], sum(sizes.values()))
            for name, frames, reason in (
                ("14dashboard_core3ble9canyonero", [4500], "non-radio nested"),
                ("5radio6helper", [8192, 512], "aggregate utility"),
                ("14identity_radio6helper", [8193], "frame over 8 KiB"),
            ):
                added = []
                for index, size in enumerate(frames):
                    address = 0x10004000 + index * 4
                    key = name + str(index)
                    functions[key] = (address, 4, 1)
                    sizes[address] = size
                    added.append((key, address))
                with self.subTest(reason=reason), self.assertRaisesRegex(AssertionError, reason):
                    image.stack_check(elf, 65536)
                for key, address in added:
                    del functions[key]
                    del sizes[address]


if __name__ == "__main__":
    unittest.main()
