"""Gate exact reviewed optimized RP2350 pause, JEDEC and write instructions.

Changing compiler/HAL may require a new disassembly review. Do not regenerate
these digests automatically. See docs/hardware/flash-persistence.md.
"""
import hashlib
import struct
import sys
from elf_image import ElfImage
from validation import require


# Complete current-body tuples independently reviewed at e6ca58f. Order is
# wrapper, FIFO, JEDEC, write; each delta is the wrapper's exact tail target.
# Tuple membership is code identity, not a claim about caller/source provenance.
REVIEWED_TUPLES = (
    (((10, 'deb277f063c302877b6bf7e748344c9ab01f406e86c67469c3bf53decc60ba39'),
      (168, '8448334fffeaab20dc3c599cb17ce58ae694dff3e3b5d5f74fcf1df6c461c4fc'),
      (1184, 'a45a5800403e13de2cafd0c6d27e232397784cf812cb81edd53327e0ee69a24e'),
      (328, '767c70efd1162ef93b11f51b87f9f54eb8b7797de4aaf1fa7ab05c630468f5f2')), 12),
    (((10, 'deb277f063c302877b6bf7e748344c9ab01f406e86c67469c3bf53decc60ba39'),
      (142, '285fe5bb86161cff8b946361acaa40fb767cc0e53bb287ad70312f556accbf0a'),
      (1128, 'a01fc3e709be03acf07ef2fa79420b3b95bc630821bbab3637017ee3bf5e91e3'),
      (328, '767c70efd1162ef93b11f51b87f9f54eb8b7797de4aaf1fa7ab05c630468f5f2')), 12),
    (((10, '8bff0aa1caea106d62cfbc88439aaceb4f99087e2c717ae7a05fdb10846f4393'),
      (144, 'bbf829702e89f2c13a7d39e0413617dd54429c474226e019dbb3fe4d59f0e8ce'),
      (418, '73bcf3be90b64e83c7f799b38a54596ea4c7c2343f69972170e99e7506b5974e'),
      (332, '282497a7f4ebfd76a157aebfb89e8d185427e7da3c6fc50fe571417be3726154')), 10),
    (((10, 'deb277f063c302877b6bf7e748344c9ab01f406e86c67469c3bf53decc60ba39'),
      (128, '7c43ff68a0b7534569f27d4239c4060cb286c2ad58ccd89b112230a741f6e787'),
      (372, 'c1b97f0e26290b74f1b1242210e2fcfd7f1f5a668a58aac9039d77c663e4c80e'),
      (284, 'e00b89c23781166351889251a58c734911fe52a0082c7b8a3b303c0dc851b4e2')), 12),
)


def check(elf):
    data = elf.named[".data"]
    require(data[1] == 1 and data[2] & 7 == 7, "RAM pause section must be allocated writable executable PROGBITS")
    require(0x20000000 <= data[3] < data[3] + data[5] <= 0x20080000,
            'complete initialized pause section must fit SRAM')
    require(elf.symbols["__sdata"] == data[3] and elf.symbols["__edata"] == data[3] + data[5], "startup must copy all pause code")
    copies = [segment for segment in elf.segments
              if segment[0] == 1 and segment[2] <= data[3] and data[3] + data[5] <= segment[2] + segment[4]
              and segment[1] + data[3] - segment[2] == data[4]]
    require(len(copies) == 1, "RAM code has no unique initialized LOAD mapping")
    segment = copies[0]
    require(segment[4] <= segment[5] and segment[1] + segment[4] <= len(elf.data),
            'initialized LOAD is truncated or exceeds its memory extent')
    lma = segment[3] + data[3] - segment[2]
    require(elf.symbols["__sidata"] == lma and 0x10000000 <= lma < lma + data[5] <= 0x101FE000, "startup RAM copy source mismatch")
    helpers = [name for name in elf.functions if "multicore12sio_fifo_ram" in name]
    require(len(helpers) == 1, "missing or ambiguous pause helper")
    helper = helpers[0]
    require('SIO_IRQ_FIFO' in elf.functions, 'missing secure FIFO wrapper')
    reviewed = ['SIO_IRQ_FIFO', helper]
    for suffix in ('14jedec_id_inner', '17write_flash_inner'):
        # Exclude the flash-resident entry thunk: it only jumps into RAM before
        # XIP is changed. Every instruction of the actual inner must be copied.
        matches = [n for n in elf.functions if n.endswith(suffix) and not n.startswith("__")]
        require(len(matches) == 1, "missing or ambiguous core0 flash helper")
        reviewed.append(matches[0])
    actual = []
    for name in reviewed:
        value, size, index = elf.functions[name]
        address = value & ~1
        require(value & 1 and 0x20000000 <= address < address + size <= 0x20080000, "flash helper extent not in SRAM")
        require(elf.sections[index] == elf.named[".data"], "flash helper must be initialized SRAM")
        actual.append((size, hashlib.sha256(elf.function_bytes(name)).hexdigest()))
    choices = [delta for bodies, delta in REVIEWED_TUPLES if tuple(actual) == bodies]
    require(len(choices) == 1, 'unreviewed coherent flash helper tuple; review complete bodies')
    require(elf.functions[helper][0] == elf.functions['SIO_IRQ_FIFO'][0] + choices[0], 'pause branch target moved')
    vector = struct.unpack_from("<I", elf.section_bytes(".vector_table"), 4 * (16 + 25))[0]
    require(vector == elf.functions["SIO_IRQ_FIFO"][0], "secure FIFO IRQ25 vector bypasses the RAM wrapper")
    require(elf.symbols["__settings_start"] == 0x101FE000 and elf.symbols["__settings_end"] == 0x10200000)
    print("Verified reviewed pause/JEDEC/write bytes, startup SRAM copy, branch and IRQ25 vector")


if __name__ == "__main__":
    check(ElfImage(sys.argv[1]))
