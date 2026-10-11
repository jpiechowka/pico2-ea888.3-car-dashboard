"""Negative artifact checks against an actual optimized build, never target execution."""
import copy
import struct
import sys
from check_flash_lockout import check
from elf_image import ElfImage

elf = ElfImage(sys.argv[1])
check(elf)
for fault in ("address", "size", "instruction", "vector", "relative_branch", "copy_start", "copy_end", "copy_source", "load"):
    bad = copy.deepcopy(elf)
    name = "SIO_IRQ_FIFO"
    value, size, index = bad.functions[name]
    if fault.startswith("copy_"):
        symbol = {"copy_start": "__sdata", "copy_end": "__edata", "copy_source": "__sidata"}[fault]
        bad.symbols[symbol] += 4
    elif fault == "load":
        bad.segments = []
    elif fault == "address":
        bad.functions[name] = (0x10000101, size, index)
    elif fault == "size":
        bad.functions[name] = (value, size + 2, index)
    elif fault == "relative_branch":
        helper = next(n for n in bad.functions if "multicore12sio_fifo_ram" in n)
        v, s, i = bad.functions[helper]
        bad.functions[helper] = (v + 4, s, i)
    else:
        data = bytearray(bad.data)
        if fault == "instruction":
            section = bad.sections[index]
            data[section[4] + (value & ~1) - section[3]] ^= 1
        else:
            vector = bad.named[".vector_table"]
            struct.pack_into("<I", data, vector[4] + 4 * (16 + 25), 0x10000101)
        bad.data = bytes(data)
    try:
        check(bad)
    except AssertionError:
        print(f"Rejected modified artifact: {fault}")
    else:
        raise AssertionError(f"missed {fault}")

for suffix in ("14jedec_id_inner", "17write_flash_inner"):
    for fault in ("missing", "address", "size", "instruction"):
        bad = copy.deepcopy(elf)
        name = next(n for n in bad.functions if n.endswith(suffix) and not n.startswith("__"))
        value, size, index = bad.functions[name]
        if fault == "missing":
            del bad.functions[name]
        elif fault == "address":
            bad.functions[name] = (0x10000101, size, index)
        elif fault == "size":
            bad.functions[name] = (value, size + 2, index)
        else:
            data = bytearray(bad.data)
            section = bad.sections[index]
            data[section[4] + (value & ~1) - section[3]] ^= 1
            bad.data = bytes(data)
        try:
            check(bad)
        except AssertionError:
            print(f"Rejected modified core0 artifact: {suffix} {fault}")
        else:
            raise AssertionError(f"missed {suffix} {fault}")
