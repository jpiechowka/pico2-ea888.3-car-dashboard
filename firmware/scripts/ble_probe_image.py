"""Independent practical BLE utility admission. Never admits Studio artifacts."""

import argparse
from collections.abc import Mapping
import hashlib
import json
import re
from pathlib import Path
from typing import TypedDict

from check_memory_layout import geometry
from check_program_image import (
    ProgramMap,
    ProgramRecord,
    Uf2Record,
    inspect_program,
    read_uf2,
    verify_uf2,
)
from check_stack_budget import frames, task_frames
from check_flash_lockout import check as check_flash_lockout
from elf_image import ElfImage, InspectableImage
from validation import require
from script_values import integer_value, object_fields, string_value

GATE = "ble_probe_practical_image"
FLASH_START = 0x10000000
CAPTURES_START = 0x101E6000
CAPTURES_END = 0x101FE000
ROOTS = (
    "firmware/ble-probe",
    "firmware/pico2/src",
    "firmware/vendor",
    "crates",
    "firmware/.cargo",
    "firmware/scripts",
    "host/scripts",
)
FILES = (
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "firmware/Cargo.toml",
    "firmware/Cargo.lock",
    "firmware/rust-toolchain.toml",
    "firmware/pico2/Cargo.toml",
    "firmware/pico2/build.rs",
    "firmware/pico2/memory.x",
    "firmware/pico2/rp235x_riscv.x",
)


class IdentityRecord(TypedDict):
    digest: str
    available: bool
    snapshot_bound: bool
    physical_address: int
    bytes: int


class ReservationRecord(TypedDict):
    captures_start: int
    captures_end: int
    bytes: int
    settings_start: int
    policy: str


class ProbeStackRecord(TypedDict):
    method: str
    bounds: dict[str, int]
    core0_reserve: int
    core1_bytes: int
    core1_reserve: int
    emitted_frame_total: int
    nonradio_frame_total: int
    largest_emitted_frame: int


class ProbeLayoutRecord(TypedDict):
    schema: int
    gate: str
    status: str
    hardware_qualified: bool
    complete_instruction_proof: bool
    source_bound: bool
    uf2_checked: bool
    elf_sha256: str
    static_ram_bytes: int
    headroom: int
    objects: dict[str, int]
    stack: ProbeStackRecord
    program: ProgramRecord
    capture_reservation: ReservationRecord
    source_identity: IdentityRecord
    reviewed_flash_lockout: bool


class ProbeImageRecord(ProbeLayoutRecord):
    manifest_sha256: str
    uf2: Uf2Record
    limitations: list[str]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_hashes(repo: Path) -> dict[str, str]:
    paths = {repo / name for name in FILES if (repo / name).is_file()}
    for name in ROOTS:
        paths.update(
            path
            for path in (repo / name).rglob("*")
            if path.is_file() and not {"target", ".git", "__pycache__"}.intersection(path.parts)
        )
    require(len(paths) <= 20000, "source inventory bound")
    return {path.relative_to(repo).as_posix(): sha(path.read_bytes()) for path in sorted(paths)}


def check_sources(repo: Path, expected: object) -> None:
    require(type(expected) is dict and bool(expected), "missing source snapshot")
    require(source_hashes(repo) == expected, "source changed since build snapshot")


def _source_inventory(sources: object) -> dict[str, str]:
    fields = object_fields(sources, "source inventory bound")
    require(0 < len(fields) <= 20000, "source inventory bound")
    result: dict[str, str] = {}
    for path, member in fields.items():
        digest = string_value(member, "invalid source inventory entry")
        require(
            bool(path) and re.fullmatch(r"[0-9a-f]{64}", digest), "invalid source inventory entry"
        )
        result[path] = digest
    return result


def source_digest(sources: object) -> str:
    """Canonical source inventory identity, not a compiler-input closure proof."""
    sources = _source_inventory(sources)
    return sha(
        json.dumps(sources, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode(
            "utf-8"
        )
    )


def linked_identity(elf: InspectableImage, expected: str | None = None) -> IdentityRecord:
    sections = [
        (index, section)
        for index, name, section in elf.section_entries
        if name == ".probe_identity"
    ]
    require(len(sections) == 1, "missing or ambiguous probe identity section")
    index, section = sections[0]
    require(
        # SHF_GNU_RETAIN only prevents linker garbage collection. Rust #[used]
        # can emit it; allocation/read-only/non-executable requirements remain.
        section[1] == 1 and section[2] in (2, 0x200002) and section[5] == 32,
        "probe identity must be 32 read-only allocated bytes",
    )
    symbols = [row for row in elf.symbol_entries if row[0] == "BLE_PROBE_SOURCE_DIGEST"]
    require(len(symbols) == 1, "missing or ambiguous probe identity symbol")
    _, address, size, info, symbol_index = symbols[0]
    require(
        address == section[3] and size == 32 and info & 15 == 1 and symbol_index == index,
        "probe identity symbol mismatch",
    )
    require(section[4] + 32 <= len(elf.data), "truncated probe identity")
    mappings = [
        physical + address - virtual
        for kind, offset, virtual, physical, filesz, memsz, flags, alignment in elf.segments
        if kind == 1
        and virtual <= address
        and address + 32 <= virtual + filesz
        and offset + address - virtual == section[4]
        and filesz <= memsz
        and offset + filesz <= len(elf.data)
    ]
    require(
        len(mappings) == 1
        and mappings[0] == address
        and FLASH_START <= address < address + 32 <= CAPTURES_START,
        "probe identity needs a unique program flash LOAD mapping",
    )
    actual = elf.data[section[4] : section[4] + 32]
    if expected is not None:
        require(
            type(expected) is str
            and re.fullmatch(r"[0-9a-f]{64}", expected) is not None
            and expected != "00" * 32,
            "missing or invalid expected source identity",
        )
        require(actual.hex() == expected, "linked probe source identity differs from snapshot")
    return {
        "digest": actual.hex(),
        "available": actual != bytes(32),
        "snapshot_bound": expected is not None,
        "physical_address": address,
        "bytes": 32,
    }


def capture_reservation(elf: InspectableImage, mapping: ProgramMap) -> ReservationRecord:
    expected = {
        "__captures_start": CAPTURES_START,
        "__captures_end": CAPTURES_END,
        "__settings_start": CAPTURES_END,
        "__settings_end": 0x10200000,
    }
    for name, address in expected.items():
        symbols = [row for row in elf.symbol_entries if row[0] == name]
        require(
            len(symbols) == 1 and symbols[0][1] == address,
            "capture/settings boundary symbol " + name,
        )

    def below_capture(start: int, end: int) -> None:
        require(
            FLASH_START <= start < end <= CAPTURES_START,
            "program range overlaps captures or leaves flash",
        )

    for kind, offset, virtual, physical, filesz, memsz, flags, alignment in elf.segments:
        if kind != 1:
            continue
        if filesz:
            below_capture(physical, physical + filesz)
        elif memsz and FLASH_START <= physical < 0x10200000:
            below_capture(physical, physical + memsz)
    for address, payload in mapping["fragments"]:
        below_capture(address, address + len(payload))
    for ranges in (mapping["erase_ranges"], mapping["verification_ranges"]):
        for start, end in ranges:
            below_capture(start, end)
    for address, page in mapping["pages"].items():
        require(address % 256 == 0 and len(page) == 256, "invalid program UF2 page")
        below_capture(address, address + 256)
    return {
        "captures_start": CAPTURES_START,
        "captures_end": CAPTURES_END,
        "bytes": CAPTURES_END - CAPTURES_START,
        "settings_start": CAPTURES_END,
        "policy": "program_LOAD_erase_verification_and_UF2_exclude_captures",
    }


def objects(elf: InspectableImage) -> dict[str, int]:
    expected = {
        "BLE_PROBE_FRAMEBUFFER_0": 153600,
        "BLE_PROBE_FRAMEBUFFER_1": 153600,
        "BLE_PROBE_CORE1_STACK": 32768,
    }
    ranges = []
    for name, size in expected.items():
        matches = [entry for entry in elf.symbol_entries if entry[0] == name]
        require(len(matches) == 1, "missing or ambiguous utility object " + name)
        _, address, actual, info, _ = matches[0]
        require(actual == size and info & 15 == 1, "utility object size/type " + name)
        require(0x20000000 <= address < address + size <= 0x20080000, "object outside SRAM")
        ranges.append((address, address + size))
    ranges.sort()
    require(all(a[1] <= b[0] for a, b in zip(ranges, ranges[1:])), "utility objects overlap")
    return expected


def stack_budgets(bounds: Mapping[str, int], headroom: int) -> None:
    require(set(bounds) == {"boot", "core0", "radio", "render"}, "stack path set")
    require(all(type(v) is int and v >= 0 for v in bounds.values()), "stack bounds")
    require(headroom >= 65536, "core0 headroom below 64 KiB")
    for name in ("boot", "core0", "radio"):
        require(bounds[name] + 8192 <= headroom, "core0 stack reserve: " + name)
    require(bounds["render"] + 4096 <= 32768, "core1 render reserve")


def stack_check(elf: InspectableImage, headroom: int) -> ProbeStackRecord:
    section = elf.named.get(".stack_sizes")
    if section is None or section[1] != 1 or section[2] & 2:
        raise AssertionError("missing/noninformational stack metadata")
    for kind, offset, _, _, size, _, _, _ in elf.segments:
        if kind == 1 and size:
            require(
                section[4] + section[5] <= offset or offset + size <= section[4],
                "stack metadata overlaps LOAD",
            )
    sizes = frames(elf.section_bytes(".stack_sizes"))
    # Fixed child allowances require source/disassembly review when paths change.
    # They cover nested drawing/formatting and radio/GATT construction, not just
    # the root's frame. This is not an instruction graph or physical high-water proof.
    boot = task_frames(elf.functions, sizes, "embassy_main_task")
    control = max(
        task_frames(elf.functions, sizes, token)
        for token in (
            "probe_display_task",
            "probe_inputs_task",
            "probe_encoder_task",
            "probe_supervisor_task",
            "probe_storage_task",
            "probe_usb_device_task",
            "probe_usb_capture_task",
        )
    )
    radio = task_frames(elf.functions, sizes, "probe_radio_task")
    render_addresses = {
        address & ~1 for name, (address, _, _) in elf.functions.items() if "probe_core1" in name
    }
    entry_addresses = {
        address & ~1
        for name, (address, _, _) in elf.functions.items()
        if name == "probe_core1"
        or re.fullmatch(r"_RNvCs[A-Za-z0-9]+_9ble_probe11probe_core1", name)
    }
    require(len(entry_addresses) == 1, "missing or ambiguous core1 render entry")
    # Include the matching executor specialization, FnOnce shim and core startup
    # wrapper as resident callers; they are distinct from the direct entry.

    require(all(address in sizes for address in render_addresses), "core1 frame metadata")
    bounds = {
        "boot": boot + 16384,
        "core0": control + 16384,
        "radio": radio + 24576,
        "render": sum(sizes[address] for address in render_addresses)
        + task_frames(elf.functions, sizes, "probe_ui_task")
        + 16384,
    }
    # Historical pre-capture 280 image: all emitted frames sum to 18080 B;
    # non-radio frames total 5884 B, largest radio::run frame is 4992 B.
    # Aggregate ceilings catch nested-child growth even when task shims do not.
    # The non-radio partition follows source ownership, not an inferred call graph.
    radio_addresses = {
        address & ~1
        for name, (address, _, _) in elf.functions.items()
        if any(token in name for token in ("5radio", "cyw43", "trouble_host", "bt_hci"))
    }
    nonradio_total = sum(size for address, size in sizes.items() if address not in radio_addresses)
    require(max(sizes.values()) <= 8192, "unreviewed utility frame over 8 KiB")
    require(sum(sizes.values()) <= 20480, "aggregate utility frame regression")
    require(nonradio_total <= 12288, "non-radio nested frame regression")
    stack_budgets(bounds, headroom)
    return {
        "method": "conservative_named_paths",
        "bounds": bounds,
        "core0_reserve": 8192,
        "core1_bytes": 32768,
        "core1_reserve": 4096,
        "emitted_frame_total": sum(sizes.values()),
        "nonradio_frame_total": nonradio_total,
        "largest_emitted_frame": max(sizes.values()),
    }


def inspect_elf(elf: InspectableImage) -> ProbeLayoutRecord:
    """CI layout stage only; full capture also requires sources and UF2 equality."""
    static, headroom = geometry(elf)
    mapping, program = inspect_program(elf)
    reservation = capture_reservation(elf, mapping)
    identity = linked_identity(elf)
    check_flash_lockout(elf)
    return {
        "schema": 1,
        "gate": "ble_probe_elf_layout_only",
        "status": "ELF_LAYOUT_CHECKS_PASS",
        "hardware_qualified": False,
        "complete_instruction_proof": False,
        "source_bound": False,
        "uf2_checked": False,
        "elf_sha256": sha(elf.data),
        "static_ram_bytes": static,
        "headroom": headroom,
        "objects": objects(elf),
        "stack": stack_check(elf, headroom),
        "program": program,
        "capture_reservation": reservation,
        "source_identity": identity,
        "reviewed_flash_lockout": True,
    }


def check(elf_path: Path, uf2_path: Path, manifest_path: Path, repo: Path) -> ProbeImageRecord:
    with manifest_path.open("rb") as stream:
        raw = stream.read(8 * 1024 * 1024 + 1)
    require(len(raw) <= 8 * 1024 * 1024, "source manifest bound")
    manifest = object_fields(json.loads(raw), "utility manifest schema")
    require(
        manifest.get("gate") == GATE
        and integer_value(manifest.get("schema"), "utility manifest schema") == 1,
        "utility manifest schema",
    )
    require(
        {"sources", "source_digest", "target", "target_cpu", "elf_sha256", "uf2_sha256"}
        <= manifest.keys(),
        "utility manifest required fields",
    )
    check_sources(repo, manifest["sources"])
    digest = source_digest(manifest["sources"])
    require(manifest.get("source_digest") == digest, "source manifest digest mismatch")
    require(
        manifest["target"] == "thumbv8m.main-none-eabihf"
        and manifest["target_cpu"] == "cortex-m33",
        "utility target contract",
    )
    elf = ElfImage(elf_path)
    require(sha(elf.data) == manifest["elf_sha256"], "ELF identity differs")
    static, headroom = geometry(elf)
    mapping, program = inspect_program(elf)
    reservation = capture_reservation(elf, mapping)
    identity = linked_identity(elf, digest)
    check_flash_lockout(elf)
    obj = objects(elf)
    stack = stack_check(elf, headroom)
    uf2 = verify_uf2(read_uf2(uf2_path), mapping)
    require(uf2["uf2_sha256"] == manifest["uf2_sha256"], "UF2 identity differs")
    return {
        "schema": 1,
        "gate": GATE,
        "status": "STATIC_IMAGE_CHECKS_PASS",
        "hardware_qualified": False,
        "complete_instruction_proof": False,
        "manifest_sha256": sha(raw),
        "source_bound": True,
        "uf2_checked": True,
        "elf_sha256": sha(elf.data),
        "static_ram_bytes": static,
        "headroom": headroom,
        "objects": obj,
        "stack": stack,
        "program": program,
        "uf2": uf2,
        "capture_reservation": reservation,
        "source_identity": identity,
        "reviewed_flash_lockout": True,
        "limitations": [
            "source snapshot is not compiler input closure",
            "named stack paths require source review",
            "physical clock, stack, display and RF unqualified",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("uf2", type=Path, nargs="?")
    parser.add_argument("manifest", type=Path, nargs="?")
    parser.add_argument(
        "--elf-only", action="store_true", help="CI layout stage, no artifact admission"
    )
    parser.add_argument("--result", type=Path, required=True)
    args = parser.parse_args()
    if args.elf_only:
        require(args.uf2 is None and args.manifest is None, "ELF-only stage takes only an ELF")
        result = inspect_elf(ElfImage(args.elf))
    else:
        require(
            args.uf2 is not None and args.manifest is not None,
            "full admission needs UF2 and manifest",
        )
        result = check(args.elf, args.uf2, args.manifest, Path(__file__).resolve().parents[2])
    with args.result.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
    print("BLE utility " + result["status"] + "; hardware_qualified=false")


if __name__ == "__main__":
    main()
