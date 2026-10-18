"""Independent small ELF and UF2 fixtures; no compilation or image analysis."""

import struct, unittest
from types import SimpleNamespace
from elf_image import ElfImage
from check_program_image import (
    program_map,
    verify_uf2,
    boot_metadata,
    linked_configuration,
    linked_startup,
    FLASH_START as B,
)


def elf_fixture():
    data = bytearray(256)
    data[:7] = b"\x7fELF\x01\x01\x01"
    struct.pack_into("<HHIIIIIHHHHHH", data, 16, 2, 40, 1, B, 52, 84, 0, 52, 32, 1, 40, 2, 1)
    struct.pack_into("<8I", data, 52, 1, 200, B, B, 1, 1, 5, 1)
    struct.pack_into("<10I", data, 124, 0, 3, 0, 0, 180, 1, 0, 0, 1, 0)
    data[200] = 0x42
    return bytes(data)


def sparse():
    return SimpleNamespace(
        data=bytes([0xA1, 0xB2, 0xC3]),
        segments=[(1, i, 0, B + a, 1, 1, 5, 1) for i, a in enumerate([0x120, 0x320, 0x2050])],
    )


def independent_uf2():
    result = bytearray()
    addresses = [B + i * 256 for i in range(16)] + [B + 0x2000]
    for index, address in enumerate(addresses):
        block = bytearray(512)
        struct.pack_into(
            "<8I", block, 0, 0x0A324655, 0x9E5D5157, 0x2000, address, 256, index, 17, 0xE48BFF59
        )
        struct.pack_into("<I", block, 508, 0x0AB16F30)
        for absolute, value in [(B + 0x120, 0xA1), (B + 0x320, 0xB2), (B + 0x2050, 0xC3)]:
            if address <= absolute < address + 256:
                block[32 + absolute - address] = value
        result.extend(block)
    return bytes(result)


class ProgramImage(unittest.TestCase):
    def test_verification_ranges_follow_pinned_exclusive_end_coalescing(self):
        self.assertEqual(
            program_map(sparse())["verification_ranges"],
            [(B + 0x120, B + 0x321), (B + 0x2050, B + 0x2051)],
        )
        elf = SimpleNamespace(
            data=bytes(8194),
            segments=[
                (1, 0, B, B, 4096, 4096, 5, 1),
                (1, 4096, B + 4096, B + 4096, 4096, 4096, 5, 1),
                (1, 8192, B + 0x2100, B + 0x2100, 1, 1, 5, 1),
                (1, 8193, B + 0x4000, B + 0x4000, 1, 1, 5, 1),
            ],
        )
        self.assertEqual(
            program_map(elf)["verification_ranges"], [(B, B + 0x2101), (B + 0x4000, B + 0x4001)]
        )

    def test_linked_startup_empty_and_nonempty_asset(self):
        for asset in [b"", b"converted asset"]:
            profile = bytes(range(16))
            storage = asset + b"\0"
            data = profile + storage
            sections = []
            symbols = []
            for index, name, obj, offset, size in [
                (1, "startup_profile", "DASHBOARD_STARTUP_PROFILE", 0, 16),
                (2, "startup_asset", "DASHBOARD_STARTUP_ASSET", 16, len(storage)),
            ]:
                sections.append(
                    (index, "." + name, (0, 1, 2, B + offset, offset, size, 0, 0, 4, 0))
                )
                symbols.extend(
                    [
                        (obj, B + offset, size, 0x11, index),
                        ("__" + name + "_start", B + offset, 0, 0x10, index),
                        ("__" + name + "_end", B + offset + size, 0, 0x10, index),
                    ]
                )
            elf = SimpleNamespace(
                data=data,
                section_entries=sections,
                symbol_entries=symbols,
                segments=[(1, 0, B, B, len(data), len(data), 5, 1)],
            )
            result = linked_startup(elf, profile, asset)
            self.assertEqual(result["asset"]["bytes"], len(asset))
            elf.data = data[:-1] + b"x"
            with self.assertRaises(AssertionError):
                linked_startup(elf, profile, asset)

    def test_linked_configuration_object_identity(self):
        data = b"accepted eav"
        section = (0, 1, 2, B, 0, len(data), 0, 0, 4, 0)
        obj = ("DASHBOARD_EMBEDDED_CONFIG", B, len(data), 0x11, 1)
        symbols = [
            obj,
            ("__dashboard_config_start", B, 0, 0x10, 1),
            ("__dashboard_config_end", B + len(data), 0, 0x10, 1),
        ]
        elf = SimpleNamespace(
            data=data,
            section_entries=[(1, ".dashboard_config", section)],
            symbol_entries=symbols.copy(),
            segments=[(1, 0, B, B, len(data), len(data), 5, 1)],
        )
        self.assertEqual(linked_configuration(elf, data)["bytes"], len(data))
        with self.assertRaises(AssertionError):
            linked_configuration(elf, b"wrong bytes!")
        elf.symbol_entries.append(obj)
        with self.assertRaises(AssertionError):
            linked_configuration(elf, data)
        elf.symbol_entries = symbols
        elf.segments.append(elf.segments[0])
        with self.assertRaises(AssertionError):
            linked_configuration(elf, data)

    def test_boot_window_literals_and_roots(self):
        root = struct.pack("<5I", 0xFFFFDED3, 0x10210142, 0x000001FF, 0, 0xAB123579)

        def fixture(extra_offset=None, extra=b""):
            data = bytearray(0x4000)
            data[0x100:0x114] = root
            if extra_offset is not None:
                data[extra_offset : extra_offset + len(extra)] = extra
            elf = SimpleNamespace(
                data=bytes(data),
                segments=[(1, 0, B, B, len(data), len(data), 5, 1)],
                section_entries=[(1, ".start_block", (0, 1, 2, B + 0x100, 0x100, 20, 0, 0, 4, 0))],
            )
            return elf, program_map(elf)

        elf, mapping = fixture()
        self.assertEqual(boot_metadata(elf, mapping)["root_address"], B + 0x100)
        # Literal complete block outside discovery window remains ordinary payload.
        for offset, extra in [(0x3000, root), (0x800, root[:4] + bytes(4))]:
            elf, mapping = fixture(offset, extra)
            boot_metadata(elf, mapping)
        for offset, extra in [
            (0x80, root[:4]),
            (0x800, root),
            (0xFFC, root),
            (0x10C, struct.pack("<I", 0x3000 - 0x100)),
        ]:
            elf, mapping = fixture(offset, extra)
            with self.subTest(offset=offset), self.assertRaises(AssertionError):
                boot_metadata(elf, mapping)

    def test_bounded_elf_control_and_header_mutations(self):
        good = elf_fixture()
        self.assertEqual(program_map(ElfImage(good))["pages"][B][0], 0x42)
        for length in [0, 6, 51, 60, 130, 200]:
            with self.subTest(length=length), self.assertRaises((AssertionError, ValueError)):
                ElfImage(good[:length])
        for offset, value, fmt in [
            (18, 3, "H"),
            (42, 31, "H"),
            (44, 1025, "H"),
            (46, 39, "H"),
            (48, 4097, "H"),
            (52 + 16, 2, "I"),
        ]:
            bad = bytearray(good)
            struct.pack_into("<" + fmt, bad, offset, value)
            with self.subTest(offset=offset), self.assertRaises((AssertionError, ValueError)):
                ElfImage(bad)

    def test_sparse_page_oracle(self):
        mapping = program_map(sparse())
        result = verify_uf2(independent_uf2(), mapping)
        self.assertEqual(result["page_count"], 17)
        self.assertEqual(mapping["erase_ranges"], [(B, B + 4096), (B + 8192, B + 12288)])

    def test_uf2_mutations(self):
        good = independent_uf2()
        mapping = program_map(sparse())
        for offset in [0, 4, 8, 12, 16, 20, 24, 28, 32, 288, 508, 512 + 20]:
            bad = bytearray(good)
            bad[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(AssertionError):
                verify_uf2(bad, mapping)
        for bad in [b"", good[:-1], good + bytes(512), good[512:]]:
            with self.assertRaises(AssertionError):
                verify_uf2(bad, mapping)

    def test_physical_ranges_and_shared_page(self):
        for address in [B - 1, 0x20000000, 0x101FE000]:
            elf = SimpleNamespace(data=b"x", segments=[(1, 0, 0, address, 1, 1, 5, 1)])
            with self.assertRaises(AssertionError):
                program_map(elf)
        elf = SimpleNamespace(data=b"xy", segments=[(1, 0, 0, 0x101FDFFF, 1, 1, 5, 1)])
        self.assertEqual(program_map(elf)["erase_ranges"][-1][1], 0x101FE000)
        elf.segments = [(1, 0, 0, B + 1, 1, 1, 5, 1), (1, 1, 0, B + 3, 1, 1, 5, 1)]
        self.assertEqual(program_map(elf)["pages"][B][1:4], b"x\0y")
        elf.segments[1] = (1, 1, 0, B + 1, 1, 1, 5, 1)
        with self.assertRaises(AssertionError):
            program_map(elf)
        elf.segments = []
        with self.assertRaises(AssertionError):
            program_map(elf)


if __name__ == "__main__":
    unittest.main()
