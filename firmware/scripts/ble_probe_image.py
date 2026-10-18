"""Independent practical BLE utility admission. Never admits Studio artifacts."""

import argparse
import hashlib
import json
import re
from pathlib import Path

from check_memory_layout import geometry
from check_program_image import inspect_program, read_uf2, verify_uf2
from check_stack_budget import frames, task_frames
from elf_image import ElfImage
from validation import require

GATE = "ble_probe_practical_image"
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


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_hashes(repo):
    paths = {repo / name for name in FILES if (repo / name).is_file()}
    for name in ROOTS:
        paths.update(
            path
            for path in (repo / name).rglob("*")
            if path.is_file() and not {"target", ".git", "__pycache__"}.intersection(path.parts)
        )
    require(len(paths) <= 20000, "source inventory bound")
    return {path.relative_to(repo).as_posix(): sha(path.read_bytes()) for path in sorted(paths)}


def check_sources(repo, expected):
    require(type(expected) is dict and bool(expected), "missing source snapshot")
    require(source_hashes(repo) == expected, "source changed since build snapshot")


def objects(elf):
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


def stack_budgets(bounds, headroom):
    require(set(bounds) == {"boot", "core0", "radio", "render"}, "stack path set")
    require(all(type(v) is int and v >= 0 for v in bounds.values()), "stack bounds")
    require(headroom >= 65536, "core0 headroom below 64 KiB")
    for name in ("boot", "core0", "radio"):
        require(bounds[name] + 8192 <= headroom, "core0 stack reserve: " + name)
    require(bounds["render"] + 4096 <= 32768, "core1 render reserve")


def stack_check(elf, headroom):
    section = elf.named.get(".stack_sizes")
    require(
        section is not None and section[1] == 1 and not section[2] & 2,
        "missing/noninformational stack metadata",
    )
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
    # 2026-10-19 optimized 280 image: all emitted frames sum to 18080 B;
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


def inspect_elf(elf):
    """CI layout stage only; full capture also requires sources and UF2 equality."""
    static, headroom = geometry(elf)
    _, program = inspect_program(elf)
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
    }


def check(elf_path, uf2_path, manifest_path, repo):
    raw = manifest_path.read_bytes()
    require(len(raw) <= 8 * 1024 * 1024, "source manifest bound")
    manifest = json.loads(raw)
    require(manifest.get("gate") == GATE and manifest.get("schema") == 1, "utility manifest schema")
    check_sources(repo, manifest["sources"])
    require(
        manifest["target"] == "thumbv8m.main-none-eabihf"
        and manifest["target_cpu"] == "cortex-m33",
        "utility target contract",
    )
    elf = ElfImage(elf_path)
    require(sha(elf.data) == manifest["elf_sha256"], "ELF identity differs")
    static, headroom = geometry(elf)
    mapping, program = inspect_program(elf)
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
        "elf_sha256": sha(elf.data),
        "static_ram_bytes": static,
        "headroom": headroom,
        "objects": obj,
        "stack": stack,
        "program": program,
        "uf2": uf2,
        "limitations": [
            "source snapshot is not compiler input closure",
            "named stack paths require source review",
            "physical clock, stack, display and RF unqualified",
        ],
    }


def main():
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
