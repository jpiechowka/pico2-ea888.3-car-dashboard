"""Small read-only ELF32/ARM inspector shared by firmware artifact gates."""

from pathlib import Path
import struct
from typing import TypeAlias, cast
from validation import require


# Wire tuples preserve the ELF32 field order used by existing admission checks.
Section: TypeAlias = tuple[int, int, int, int, int, int, int, int, int, int]
Segment: TypeAlias = tuple[int, int, int, int, int, int, int, int]
Symbol: TypeAlias = tuple[str, int, int, int, int]


class ElfImage:
    def __init__(self, path: str | Path | bytes | bytearray) -> None:
        if isinstance(path, (bytes, bytearray)):
            require(len(path) <= 32 * 1024 * 1024, "ELF byte bound")
            data = bytes(path)
        else:
            with Path(path).open("rb") as stream:
                data = stream.read(32 * 1024 * 1024 + 1)
            require(len(data) <= 32 * 1024 * 1024, "ELF byte bound")
        self.data = data
        require(len(data) >= 52, "truncated ELF header")
        require(data[:7] == b"\x7fELF\x01\x01\x01", "expected ELF32 little endian")
        require(struct.unpack_from("<H", data, 18)[0] == 40, "expected ARM ELF")
        require(
            struct.unpack_from("<HHI", data, 16) == (2, 40, 1),
            "expected executable ARM ELF version1",
        )
        require(struct.unpack_from("<H", data, 40)[0] == 52, "unexpected ELF header size")

        def extent(offset: int, count: int, size: int) -> None:
            require(
                0 <= offset <= len(data) and count <= (len(data) - offset) // size,
                "truncated ELF extent",
            )

        shoff = struct.unpack_from("<I", data, 32)[0]
        entsize, count, names_index = struct.unpack_from("<HHH", data, 46)
        require(entsize == 40 and count > 0 and names_index < count)
        require(count <= 4096, "ELF section bound")
        extent(shoff, count, 40)
        self.sections = [
            cast(Section, struct.unpack_from("<10I", data, shoff + i * entsize))
            for i in range(count)
        ]

        def table(section: Section) -> bytes:
            require(section[4] + section[5] <= len(data), "truncated ELF table")
            return data[section[4] : section[4] + section[5]]

        def string(blob: bytes, offset: int) -> str:
            require(0 <= offset < len(blob), "ELF string offset")
            end = blob.find(b"\0", offset, min(len(blob), offset + 4097))
            require(end >= 0, "ELF string terminator or name bound")
            return blob[offset:end].decode("ascii")

        for section in self.sections:
            if section[1] != 8:
                extent(section[4], section[5], 1)
        require(self.sections[names_index][1] == 3, "ELF section-name string table")
        names = table(self.sections[names_index])
        self.section_entries = [
            (index, string(names, section[0]), section)
            for index, section in enumerate(self.sections)
        ]
        self.named = {name: section for _, name, section in self.section_entries}
        self.symbols: dict[str, int] = {}
        # Typed consumers bind every candidate, including conflicting same-name
        # entries. Keep the existing value lookup/function interfaces intact.
        self.symbol_entries: list[Symbol] = []
        self.functions: dict[str, tuple[int, int, int]] = {}
        # Mapping symbols deliberately repeat their short names. A dictionary
        # loses code/data transitions needed by the pinned decoder adapter.
        self.mapping_symbols: list[tuple[str, int, int]] = []
        symbol_count = 0
        name_bytes = 0
        for section in self.sections:
            if section[1] != 2:
                continue
            require(section[9] == 16 and section[5] % 16 == 0, "unexpected symbol size")
            symbol_count += section[5] // 16
            require(symbol_count <= 262144, "ELF symbol bound")
            require(
                section[6] < len(self.sections) and self.sections[section[6]][1] == 3,
                "ELF symbol string table",
            )
            strings = table(self.sections[section[6]])
            for offset in range(section[4], section[4] + section[5], 16):
                name, value, size, info, _, index = struct.unpack_from("<IIIBBH", data, offset)
                name = string(strings, name)
                name_bytes += len(name)
                require(name_bytes <= 16 * 1024 * 1024, "ELF aggregate name bound")
                require(
                    index < len(self.sections) or index in (0xFFF1, 0xFFF2),
                    "ELF symbol section index",
                )
                self.symbol_entries.append((name, value, size, info, index))
                self.symbols[name] = value
                if name in ("$t", "$d", "$a") or name.startswith(("$t.", "$d.", "$a.")):
                    require(
                        info == 0 and size == 0,
                        "mapping symbol must be local NOTYPE with zero size",
                    )
                    self.mapping_symbols.append((name, value, index))
                if info & 15 == 2 and size:
                    self.functions[name] = (value, size, index)
        phoff = struct.unpack_from("<I", data, 28)[0]
        phentsize, phcount = struct.unpack_from("<HH", data, 42)
        require(phentsize == 32 and phcount > 0)
        require(phcount <= 1024, "ELF program-header bound")
        extent(phoff, phcount, 32)
        self.segments = [
            cast(Segment, struct.unpack_from("<8I", data, phoff + i * phentsize))
            for i in range(phcount)
        ]

        for kind, offset, virtual, physical, filesz, memsz, flags, alignment in self.segments:
            if filesz:
                extent(offset, filesz, 1)
            if kind == 1:
                require(filesz <= memsz, "ELF load file size exceeds memory size")
                require(
                    physical + filesz < 2**32 and virtual + memsz < 2**32,
                    "ELF load address overflow",
                )
                require(
                    alignment in (0, 1) or alignment & (alignment - 1) == 0, "ELF load alignment"
                )
                if alignment > 1:
                    require(offset % alignment == virtual % alignment, "ELF load congruence")

    def function_bytes(self, name: str) -> bytes:
        value, size, index = self.functions[name]
        section = self.sections[index]
        address = value & ~1
        require(
            section[1] != 8 and section[3] <= address < address + size <= section[3] + section[5]
        )
        offset = section[4] + address - section[3]
        result = self.data[offset : offset + size]
        require(len(result) == size)
        return result

    def section_bytes(self, name: str) -> bytes:
        section = self.named[name]
        return self.data[section[4] : section[4] + section[5]]
