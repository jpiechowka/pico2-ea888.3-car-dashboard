"""Check the linked ELF, including boundaries affected by INSERT linker scripts.

Run on an unstripped ARM build (CARGO_PROFILE_RELEASE_STRIP=none).
Uses only Python's standard library; never executes target code.
"""

import sys
from elf_image import ElfImage, require
from check_flash_lockout import check as check_flash_lockout


def check(path):
    elf = ElfImage(path)
    sections, named, symbols = elf.sections, elf.named, elf.symbols

    ram_start, ram_end = 0x20000000, 0x20080000
    ram_sections = [s for s in sections if s[2] & 2 and ram_start <= s[3] < ram_end]
    require(ram_sections, "missing allocated RAM sections")
    static_end = max(s[3] + s[5] for s in ram_sections)
    aligned_end = (static_end + 7) & ~7
    for name in (".data", ".bss", ".uninit"):
        section = named[name]
        require(ram_start <= section[3] <= section[3] + section[5] <= ram_end, name)
    for name in ("__euninit", "__sheap", "_stack_end"):
        require(name in symbols, f"{name} missing; build with CARGO_PROFILE_RELEASE_STRIP=none")
        expected = static_end if name == "__euninit" else aligned_end
        require(symbols[name] == expected, f"{name}=0x{symbols[name]:08x}, expected=0x{expected:08x}")
    require(symbols.get("_stack_start") == ram_end, "unexpected core 0 stack top")
    require(ram_end - aligned_end >= 32768, "less than 32 KiB core 0 stack headroom")
    flash_start, journal_start = 0x10000000, 0x101FE000
    require(flash_start <= named[".end_block"][3] < journal_start, "boot info outside program flash")
    # Physical LOAD addresses include the flash copy of RAM .data/ram_func.
    # Checking only virtual .text addresses can miss journal overlap.
    for kind, _, _, physical, size, _, _, _ in elf.segments:
        if kind == 1 and size and flash_start <= physical < 0x20000000:
            require(physical + size <= journal_start, "flash LOAD overlaps reserved settings")
    handlers = {name: addr for name, addr in symbols.items()
                if name == "SIO_IRQ_FIFO" or "__cortex_m_rt_SIO_IRQ_FIFO" in name or "sio_fifo_ram" in name}
    require("SIO_IRQ_FIFO" in handlers and any("sio_fifo_ram" in n for n in handlers), "missing lockout trampoline or body")
    for name, addr in handlers.items():
        require(ram_start <= (addr & ~1) < ram_end, f"lockout handler executes outside RAM: {name} at {addr:#x}")
    check_flash_lockout(elf)
    print(f"static RAM {aligned_end - ram_start} B; core 0 headroom {ram_end - aligned_end} B; boundaries verified")


if __name__ == "__main__":
    check(sys.argv[1])
