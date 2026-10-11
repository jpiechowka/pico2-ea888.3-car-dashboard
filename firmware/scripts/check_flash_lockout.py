"""Gate exact reviewed optimized RP2350 pause, JEDEC and write instructions.

Changing compiler/HAL may require a new disassembly review. Do not regenerate
these digests automatically. See docs/hardware/flash-persistence.md.
"""
import hashlib
import struct
import sys
from elf_image import ElfImage, require


def check(elf):
    data = elf.named[".data"]
    require(data[1] == 1 and data[2] & 7 == 7, "RAM pause section must be allocated writable executable PROGBITS")
    require(elf.symbols["__sdata"] == data[3] and elf.symbols["__edata"] == data[3] + data[5], "startup must copy all pause code")
    copies = [segment for segment in elf.segments
              if segment[0] == 1 and segment[2] <= data[3] and data[3] + data[5] <= segment[2] + segment[4]
              and segment[1] + data[3] - segment[2] == data[4]]
    require(len(copies) == 1, "RAM code has no unique initialized LOAD mapping")
    segment = copies[0]
    lma = segment[3] + data[3] - segment[2]
    require(elf.symbols["__sidata"] == lma and 0x10000000 <= lma < lma + data[5] <= 0x101FE000, "startup RAM copy source mismatch")
    helpers = [name for name in elf.functions if "multicore12sio_fifo_ram" in name]
    require(len(helpers) == 1, "missing or ambiguous pause helper")
    helper = helpers[0]
    reviewed = {
        "SIO_IRQ_FIFO": (10, "deb277f063c302877b6bf7e748344c9ab01f406e86c67469c3bf53decc60ba39"),
        helper: (168, "8448334fffeaab20dc3c599cb17ce58ae694dff3e3b5d5f74fcf1df6c461c4fc"),
    }
    for suffix, size, digest in (
        ("14jedec_id_inner", 1184, "a45a5800403e13de2cafd0c6d27e232397784cf812cb81edd53327e0ee69a24e"),
        ("17write_flash_inner", 328, "767c70efd1162ef93b11f51b87f9f54eb8b7797de4aaf1fa7ab05c630468f5f2"),
    ):
        # Exclude the flash-resident entry thunk: it only jumps into RAM before
        # XIP is changed. Every instruction of the actual inner must be copied.
        matches = [n for n in elf.functions if n.endswith(suffix) and not n.startswith("__")]
        require(len(matches) == 1, "missing or ambiguous core0 flash helper")
        reviewed[matches[0]] = (size, digest)
    for name, (expected_size, digest) in reviewed.items():
        value, size, index = elf.functions[name]
        address = value & ~1
        require(value & 1 and 0x20000000 <= address < address + size <= 0x20080000, "flash helper extent not in SRAM")
        require(elf.sections[index] == elf.named[".data"], "flash helper must be initialized SRAM")
        require(size == expected_size, "flash helper changed size; review disassembly")
        require(hashlib.sha256(elf.function_bytes(name)).hexdigest() == digest, "flash helper instructions changed; review disassembly")
    # The final relative branch in the reviewed 10-byte wrapper reaches +12.
    require(elf.functions[helper][0] == elf.functions["SIO_IRQ_FIFO"][0] + 12, "pause branch target moved")
    vector = struct.unpack_from("<I", elf.section_bytes(".vector_table"), 4 * (16 + 25))[0]
    require(vector == elf.functions["SIO_IRQ_FIFO"][0], "secure FIFO IRQ25 vector bypasses the RAM wrapper")
    require(elf.symbols["__settings_start"] == 0x101FE000 and elf.symbols["__settings_end"] == 0x10200000)
    print("Verified reviewed pause/JEDEC/write bytes, startup SRAM copy, branch and IRQ25 vector")


if __name__ == "__main__":
    check(ElfImage(sys.argv[1]))
