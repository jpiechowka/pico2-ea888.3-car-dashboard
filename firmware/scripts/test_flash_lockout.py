"""Actual ELF flash mutations. Additional positive ELFs enable cross-body tests.

Optional --rejected OLD_ELF preserves an explicit historical unsafe negative.
Never executes target code.
"""

import copy
import struct
import sys
from unittest.mock import patch
from check_flash_lockout import check
import check_memory_layout
from elf_image import ElfImage


def names(elf):
    return [
        "SIO_IRQ_FIFO",
        next(n for n in elf.functions if "multicore12sio_fifo_ram" in n),
        *[
            next(n for n in elf.functions if n.endswith(s) and not n.startswith("__"))
            for s in ("14jedec_id_inner", "17write_flash_inner")
        ],
    ]


def rejected(elf, label):
    try:
        check(elf)
    except AssertionError:
        print("Rejected modified artifact:", label)
    else:
        raise AssertionError("missed " + label)


def flip(elf, address):
    data = bytearray(elf.data)
    section = elf.named[".data"]
    data[section[4] + address - section[3]] ^= 1
    elf.data = bytes(data)


def body_fixtures(elf):
    for name in names(elf):
        for fault in (
            "missing",
            "ambiguous",
            "flash",
            "arm",
            "size",
            "instruction",
            "last_byte",
            "extent",
        ):
            bad = copy.deepcopy(elf)
            value, size, index = bad.functions[name]
            if fault == "missing":
                del bad.functions[name]
            elif fault == "ambiguous":
                if name == "SIO_IRQ_FIFO":
                    continue
                bad.functions["extra_" + name] = (value, size, index)
            elif fault == "flash":
                bad.functions[name] = (0x10000101, size, index)
            elif fault == "arm":
                bad.functions[name] = (value & ~1, size, index)
            elif fault == "size":
                bad.functions[name] = (value, size + 2, index)
            elif fault == "extent":
                bad.functions[name] = (value, bad.named[".data"][5] + 1, index)
            else:
                flip(bad, (value & ~1) + (size - 1 if fault == "last_byte" else 0))
            rejected(bad, name + " " + fault)
    _, fifo, jedec, write = names(elf)
    if elf.functions[fifo][1] == 128:
        for name, offsets in [
            (fifo, (112, 116, 120, 124)),
            (jedec, (352, 356, 360, 364, 368)),
            (write, (272, 276, 280)),
        ]:
            for offset in offsets:
                bad = copy.deepcopy(elf)
                flip(bad, (bad.functions[name][0] & ~1) + offset)
                rejected(bad, name + " literal " + str(offset))


def layout_fixtures(elf):
    for fault in (
        "vector",
        "tail",
        "moved_fifo",
        "copy_start",
        "copy_end",
        "copy_source",
        "missing_load",
        "ambiguous_load",
        "short_load",
        "truncated_load_backing",
        "load_memsize",
        "load_source",
        "section_type",
        "section_flags",
        "wrong_section",
        "settings_start",
        "settings_end",
    ):
        bad = copy.deepcopy(elf)
        if fault.startswith("copy_"):
            bad.symbols[
                {"copy_start": "__sdata", "copy_end": "__edata", "copy_source": "__sidata"}[fault]
            ] += 4
        elif fault.startswith("settings_"):
            bad.symbols["__" + fault] += 4
        elif fault == "missing_load":
            bad.segments = []
        elif fault in (
            "ambiguous_load",
            "short_load",
            "truncated_load_backing",
            "load_memsize",
            "load_source",
        ):
            section = bad.named[".data"]
            segment = next(
                s for s in bad.segments if s[0] == 1 and s[1] + section[3] - s[2] == section[4]
            )
            if fault == "ambiguous_load":
                bad.segments.append(segment)
            elif fault in ("short_load", "load_memsize", "load_source"):
                changed = list(segment)
                if fault == "short_load":
                    changed[4] = section[5] - 1
                elif fault == "load_memsize":
                    changed[5] = changed[4] - 1
                else:
                    changed[3] += 4
                bad.segments[bad.segments.index(segment)] = tuple(changed)
            else:
                # Protected bodies/vector survive; the complete startup copy
                # section lacks initialized file backing.
                end = max((bad.functions[n][0] & ~1) + bad.functions[n][1] for n in names(bad))
                bad.data = bad.data[: section[4] + end - section[3]]
        elif fault == "wrong_section":
            name = names(bad)[2]
            value, size, _ = bad.functions[name]
            bad.functions[name] = (value, size, bad.sections.index(bad.named[".vector_table"]))
        elif fault.startswith("section_"):
            section = bad.named[".data"]
            changed = list(section)
            changed[1 if fault == "section_type" else 2] = 8 if fault == "section_type" else 3
            bad.sections[bad.sections.index(section)] = tuple(changed)
            bad.named[".data"] = tuple(changed)
        elif fault == "moved_fifo":
            helper = names(bad)[1]
            value, size, index = bad.functions[helper]
            section = bad.sections[index]
            data = bytearray(bad.data)
            moved = (section[3] + section[5] - size) & ~1
            offset = section[4] + moved - section[3]
            data[offset : offset + size] = bad.function_bytes(helper)
            bad.data = bytes(data)
            bad.functions[helper] = (moved | 1, size, index)
        elif fault == "tail":
            value, size, _ = bad.functions["SIO_IRQ_FIFO"]
            flip(bad, (value & ~1) + size - 2)
        else:
            data = bytearray(bad.data)
            vector = bad.named[".vector_table"]
            struct.pack_into("<I", data, vector[4] + 4 * (16 + 25), 0x10000101)
            bad.data = bytes(data)
        rejected(bad, fault)


def mixed_bodies(recipient, donor, selected):
    bad = copy.deepcopy(recipient)
    current, other = names(recipient), names(donor)
    bodies = [
        donor.function_bytes(other[i]) if i == selected else recipient.function_bytes(current[i])
        for i in range(4)
    ]
    # Repack the prefix: donors stay whole, nonoverlapping and SRAM/file backed.
    section = bad.named[".data"]
    data = bytearray(bad.data)
    address = bad.functions[current[0]][0] & ~1
    delta = (bad.functions[current[1]][0] & ~1) - address
    for i, body in enumerate(bodies):
        if i == 1:
            address = section[3] + delta
        offset = section[4] + address - section[3]
        data[offset : offset + len(body)] = body
        bad.functions[current[i]] = (address | 1, len(body), bad.functions[current[i]][2])
        address += len(body)
    bad.data = bytes(data)
    if bodies == [recipient.function_bytes(n) for n in current]:
        check(bad)  # Shared identical bytes have no distinguishable provenance.
    else:
        rejected(bad, "mixed whole body " + str(selected))


def main(args):
    rejected_paths = []
    if "--rejected" in args:
        split = args.index("--rejected")
        rejected_paths, args = args[split + 1 :], args[:split]
    if not args:
        raise ValueError("at least one actual positive ELF is required")
    elfs = [ElfImage(path) for path in args]
    for elf in elfs:
        check(elf)
        bad_stack = copy.deepcopy(elf)
        bad_stack.symbols["_stack_start"] = 0
        with patch.object(check_memory_layout, "ElfImage", return_value=bad_stack):
            try:
                check_memory_layout.check("in-memory corrupted ELF")
            except AssertionError:
                print("Rejected modified artifact: core0 stack top")
            else:
                raise AssertionError("missed core0 stack top")
        body_fixtures(elf)
        layout_fixtures(elf)
    for i, elf in enumerate(elfs):
        for j, donor in enumerate(elfs):
            if i != j:
                for body in range(4):
                    mixed_bodies(elf, donor, body)
    for path in rejected_paths:
        rejected(ElfImage(path), "historical unsafe full tuple " + path)
    print("Flash artifact fixtures passed for", len(elfs), "actual positive ELFs")


if __name__ == "__main__":
    main(sys.argv[1:])
