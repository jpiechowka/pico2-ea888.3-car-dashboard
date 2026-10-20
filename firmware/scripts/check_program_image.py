"""Bounded physical program map and canonical pinned-producer UF2 equality.
This format gate alone does not admit a firmware artifact or authorize USB writes.
"""

import hashlib
from pathlib import Path
import struct
from typing import NotRequired, TypedDict
from elf_image import ElfImage, LinkedData, ProgramData, Symbol
from validation import require

FLASH_START = 0x10000000
SETTINGS_START = 0x101FE000
FAMILY = 0xE48BFF59
MAX_UF2_BYTES = 8160 * 512


class ProgramMap(TypedDict):
    fragments: list[tuple[int, bytes]]
    verification_ranges: list[tuple[int, int]]
    erase_ranges: list[tuple[int, int]]
    pages: dict[int, bytes]
    payload_sha256: str


class Uf2Record(TypedDict):
    schema_version: int
    scope: str
    family: int
    page_count: int
    payload_sha256: str
    uf2_sha256: str
    erase_ranges: list[tuple[int, int]]


class BootRecord(TypedDict):
    policy: str
    root_address: int
    root_sha256: str


class LinkedRecord(TypedDict):
    sha256: str
    bytes: int
    physical_address: int


class AssetRecord(TypedDict):
    sha256: str
    bytes: int
    storage: LinkedRecord


class StartupRecord(TypedDict):
    profile: LinkedRecord
    asset: AssetRecord


class ProgramRecord(TypedDict):
    schema_version: int
    scope: str
    elf_sha256: str
    payload_sha256: str
    payload_ranges: list[tuple[int, int]]
    verification_ranges: list[tuple[int, int]]
    erase_ranges: list[tuple[int, int]]
    expected_uf2_pages: int
    boot_metadata: BootRecord
    uf2: NotRequired[Uf2Record]


def program_map(elf: ProgramData) -> ProgramMap:
    fragments = []
    for kind, offset, virtual, physical, filesz, memsz, flags, alignment in elf.segments:
        if kind != 1 or not filesz:
            continue
        require(filesz <= memsz, "program file size exceeds memory")
        require(
            FLASH_START <= physical < physical + filesz <= SETTINGS_START,
            "program physical range outside flash",
        )
        require(offset + filesz <= len(elf.data), "truncated program bytes")
        fragments.append((physical, elf.data[offset : offset + filesz]))
    fragments.sort(key=lambda part: part[0])
    require(fragments, "empty program map")
    end = FLASH_START
    mutable_pages: dict[int, bytearray] = {}
    sectors: set[int] = set()
    erase: list[tuple[int, int]] = []
    verification: list[tuple[int, int]] = []
    payload_hash = hashlib.sha256(b"StudioPhysicalPayloadV1\0")
    for address, data in fragments:
        require(address >= end, "overlapping physical program payload")
        end = address + len(data)
        # Pinned picotool get_coalesced_ranges compares the exclusive end sector.
        if verification and verification[-1][1] // 4096 == address // 4096:
            verification[-1] = (verification[-1][0], end)
        else:
            verification.append((address, end))
        payload_hash.update(struct.pack("<II", address, len(data)))
        payload_hash.update(data)
        lo = address & ~4095
        hi = (end + 4095) & ~4095
        require(FLASH_START <= lo < hi <= SETTINGS_START, "erase envelope outside program flash")
        if erase and lo <= erase[-1][1]:
            erase[-1] = (erase[-1][0], max(hi, erase[-1][1]))
        else:
            erase.append((lo, hi))
        for page in range(address & ~255, (end + 255) & ~255, 256):
            target = mutable_pages.setdefault(page, bytearray(256))
            a = max(address, page)
            b = min(end, page + 256)
            target[a - page : b - page] = data[a - address : b - address]
            sectors.add(page & ~4095)
    last = max(mutable_pages)
    for sector in sectors:
        for page in range(sector, sector + 4096, 256):
            if page < last:
                mutable_pages.setdefault(page, bytearray(256))
    pages = {address: bytes(data) for address, data in sorted(mutable_pages.items())}
    require(0 < len(pages) <= 8160, "program page bound")
    return {
        "fragments": fragments,
        "verification_ranges": verification,
        "erase_ranges": erase,
        "pages": pages,
        "payload_sha256": payload_hash.hexdigest(),
    }


def verify_uf2(data: bytes | bytearray, mapping: ProgramMap) -> Uf2Record:
    require(0 < len(data) <= MAX_UF2_BYTES and len(data) % 512 == 0, "UF2 byte bound or framing")
    pages = mapping["pages"]
    count = len(data) // 512
    require(count == len(pages), "UF2 page count mismatch")
    for index, (address, expected) in enumerate(pages.items()):
        block = data[index * 512 : (index + 1) * 512]
        first, second, flags, target, size, number, total, family = struct.unpack_from("<8I", block)
        require(
            (first, second, struct.unpack_from("<I", block, 508)[0])
            == (0x0A324655, 0x9E5D5157, 0x0AB16F30),
            "UF2 magic",
        )
        require(
            flags == 0x2000 and family == FAMILY and size == 256, "UF2 flags family or payload size"
        )
        require(
            target == address and number == index and total == count,
            "UF2 canonical address or sequence",
        )
        require(block[32:288] == expected, "UF2 payload mismatch")
        require(block[288:508] == bytes(220), "UF2 unsupported extension bytes")
    return {
        "schema_version": 1,
        "scope": "program_map_and_uf2_equality_only",
        "family": FAMILY,
        "page_count": count,
        "payload_sha256": mapping["payload_sha256"],
        "uf2_sha256": hashlib.sha256(data).hexdigest(),
        "erase_ranges": mapping["erase_ranges"],
    }


def read_uf2(path: str | Path) -> bytes:
    with open(path, "rb") as stream:
        data = stream.read(MAX_UF2_BYTES + 1)
    require(len(data) <= MAX_UF2_BYTES, "UF2 byte bound")
    return data


def boot_metadata(elf: LinkedData, mapping: ProgramMap) -> BootRecord:
    """Initial supported slot0 root; additional device boot-state admission is separate."""
    sections = [section for _, name, section in elf.section_entries if name == ".start_block"]
    require(len(sections) == 1, "unique start block section required")
    section = sections[0]
    require(section[1] != 8 and section[5] >= 20, "start block is not file backed")
    candidates = []
    for kind, offset, virtual, physical, filesz, memsz, flags, alignment in elf.segments:
        if (
            kind == 1
            and virtual <= section[3]
            and section[3] + 20 <= virtual + filesz
            and offset + section[3] - virtual == section[4]
        ):
            candidates.append(physical + section[3] - virtual)
    require(len(candidates) == 1, "ambiguous start block mapping")
    root = candidates[0]
    require(
        root % 4 == 0 and FLASH_START <= root and root + 20 <= FLASH_START + 4096,
        "start block outside boot window",
    )
    pages = mapping["pages"]

    def read(address: int, length: int) -> bytes | None:
        output = bytearray()
        for cursor in range(address, address + length):
            page = pages.get(cursor & ~255)
            if page is None:
                return None
            output.append(page[cursor & 255])
        return bytes(output)

    expected = struct.pack("<5I", 0xFFFFDED3, 0x10210142, 0x000001FF, 0, 0xAB123579)
    require(read(root, 20) == expected, "unsupported boot root or linked metadata")
    for address in range(FLASH_START, root, 4):
        word = read(address, 4)
        require(word is not None, "boot search prefix coverage")
        require(word != expected[:4], "unsupported boot search prefix")

    def complete_envelope(address: int) -> bool:
        cursor = address + 4
        for _ in range(160):
            raw = read(cursor, 4)
            if raw is None:
                return False
            item = struct.unpack("<I", raw)[0]
            tag = item & 255
            if tag == 255:
                length = (item >> 8) & 65535
                return (
                    item >> 24 == 0
                    and length == (cursor - address - 4) // 4
                    and read(cursor + 8, 4) == expected[-4:]
                    and read(cursor + 4, 4) is not None
                )
            size = (item >> 8) & (65535 if tag & 128 else 255)
            if size == 0 or cursor + size * 4 > address + 628:
                return False
            if read(cursor, size * 4) is None:
                return False
            cursor += size * 4
        return False

    for address in range(root + 4, FLASH_START + 4096, 4):
        if read(address, 4) == expected[:4]:
            require(not complete_envelope(address), "additional boot-window block")
    return {
        "policy": "slot0_secure_self_link_v1",
        "root_address": root,
        "root_sha256": hashlib.sha256(expected).hexdigest(),
    }


def linked_bytes(
    elf: LinkedData,
    expected: bytes,
    section_name: str,
    object_name: str,
    start_name: str,
    end_name: str,
) -> LinkedRecord:
    sections = [
        (index, section) for index, name, section in elf.section_entries if name == section_name
    ]
    require(len(sections) == 1, "unique dashboard configuration section required")
    index, section = sections[0]
    require(
        section[1] == 1 and section[2] & 2 and not section[2] & 1,
        "dashboard configuration section flags",
    )
    require(section[5] == len(expected) > 0, "dashboard configuration length mismatch")

    def symbol(name: str) -> Symbol:
        found = [row for row in elf.symbol_entries if row[0] == name]
        require(len(found) == 1, "unique dashboard configuration symbol required")
        return found[0]

    obj = symbol(object_name)
    start = symbol(start_name)
    end = symbol(end_name)
    require(
        obj[1] == section[3] and obj[2] == section[5] and obj[3] & 15 == 1 and obj[4] == index,
        "dashboard configuration object mismatch",
    )
    require(
        start[1] == section[3] and end[1] == section[3] + section[5],
        "dashboard configuration linker boundary mismatch",
    )
    mappings = []
    for kind, offset, virtual, physical, filesz, memsz, flags, alignment in elf.segments:
        if (
            kind == 1
            and virtual <= section[3]
            and section[3] + section[5] <= virtual + filesz
            and offset + section[3] - virtual == section[4]
        ):
            mappings.append(physical + section[3] - virtual)
    require(
        len(mappings) == 1
        and FLASH_START <= mappings[0] < mappings[0] + len(expected) <= SETTINGS_START,
        "dashboard configuration load mapping",
    )
    actual = elf.data[section[4] : section[4] + section[5]]
    require(actual == expected, "linked configuration differs from accepted EAV")
    return {
        "sha256": hashlib.sha256(actual).hexdigest(),
        "bytes": len(actual),
        "physical_address": mappings[0],
    }


def linked_configuration(elf: LinkedData, expected: bytes) -> LinkedRecord:
    return linked_bytes(
        elf,
        expected,
        ".dashboard_config",
        "DASHBOARD_EMBEDDED_CONFIG",
        "__dashboard_config_start",
        "__dashboard_config_end",
    )


def linked_startup(elf: LinkedData, profile: bytes, asset: bytes) -> StartupRecord:
    require(len(profile) == 16 and len(asset) <= 524288, "startup linked input bound")
    profile_record = linked_bytes(
        elf,
        profile,
        ".startup_profile",
        "DASHBOARD_STARTUP_PROFILE",
        "__startup_profile_start",
        "__startup_profile_end",
    )
    storage = linked_bytes(
        elf,
        asset + b"\0",
        ".startup_asset",
        "DASHBOARD_STARTUP_ASSET",
        "__startup_asset_start",
        "__startup_asset_end",
    )
    return {
        "profile": profile_record,
        "asset": {
            "sha256": hashlib.sha256(asset).hexdigest(),
            "bytes": len(asset),
            "storage": storage,
        },
    }


def inspect_program(elf: LinkedData) -> tuple[ProgramMap, ProgramRecord]:
    mapping = program_map(elf)
    metadata = boot_metadata(elf, mapping)
    return mapping, {
        "schema_version": 1,
        "scope": "program_ranges_and_boot_metadata_only",
        "elf_sha256": hashlib.sha256(elf.data).hexdigest(),
        "payload_sha256": mapping["payload_sha256"],
        "payload_ranges": [
            (address, address + len(data)) for address, data in mapping["fragments"]
        ],
        "verification_ranges": mapping["verification_ranges"],
        "erase_ranges": mapping["erase_ranges"],
        "expected_uf2_pages": len(mapping["pages"]),
        "boot_metadata": metadata,
    }


def main() -> None:
    import argparse
    import json

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf")
    parser.add_argument("--uf2")
    args = parser.parse_args()
    mapping, record = inspect_program(ElfImage(args.elf))
    if args.uf2:
        record["uf2"] = verify_uf2(read_uf2(args.uf2), mapping)
    print(json.dumps(record, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
