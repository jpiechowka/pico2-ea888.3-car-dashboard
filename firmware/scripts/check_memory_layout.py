"""Check the linked ELF, including boundaries affected by INSERT linker scripts.

Run on an unstripped ARM build (CARGO_PROFILE_RELEASE_STRIP=none).
Uses only Python's standard library; never executes target code.
"""

import pathlib
import struct
import sys


def check(path):
    data = pathlib.Path(path).read_bytes()
    assert data[:7] == b"\x7fELF\x01\x01\x01", "expected ELF32 little endian"
    assert struct.unpack_from("<H", data, 18)[0] == 40, "expected ARM ELF"
    shoff = struct.unpack_from("<I", data, 32)[0]
    entsize, count, names_index = struct.unpack_from("<HHH", data, 46)
    assert entsize == 40 and count > 0 and names_index < count
    sections = [struct.unpack_from("<10I", data, shoff + i * entsize) for i in range(count)]

    def table(section):
        return data[section[4]:section[4] + section[5]]

    def string(blob, offset):
        return blob[offset:blob.index(b"\0", offset)].decode("ascii")

    names = table(sections[names_index])
    named = {string(names, s[0]): s for s in sections}
    symbols = {}
    for section in sections:
        if section[1] != 2:
            continue
        assert section[9] == 16, "unexpected symbol size"
        strings = table(sections[section[6]])
        for offset in range(section[4], section[4] + section[5], 16):
            name, value, _, _, _, _ = struct.unpack_from("<IIIBBH", data, offset)
            symbols[string(strings, name)] = value

    ram_start, ram_end = 0x20000000, 0x20080000
    ram_sections = [s for s in sections if s[2] & 2 and ram_start <= s[3] < ram_end]
    assert ram_sections, "missing allocated RAM sections"
    static_end = max(s[3] + s[5] for s in ram_sections)
    aligned_end = (static_end + 7) & ~7
    for name in (".data", ".bss", ".uninit"):
        section = named[name]
        assert ram_start <= section[3] <= section[3] + section[5] <= ram_end, name
    for name in ("__euninit", "__sheap", "_stack_end"):
        assert name in symbols, f"{name} missing; build with CARGO_PROFILE_RELEASE_STRIP=none"
        expected = static_end if name == "__euninit" else aligned_end
        assert symbols[name] == expected, f"{name}=0x{symbols[name]:08x}, expected=0x{expected:08x}"
    assert symbols.get("_stack_start") == ram_end, "unexpected core 0 stack top"
    assert ram_end - aligned_end >= 32768, "less than 32 KiB core 0 stack headroom"
    assert 0x10000000 <= named[".end_block"][3] < 0x10200000, "boot info outside flash"
    print(f"static RAM {aligned_end - ram_start} B; core 0 headroom {ram_end - aligned_end} B; boundaries verified")


if __name__ == "__main__":
    check(sys.argv[1])
