"""Check the linked ELF, including boundaries affected by INSERT linker scripts.

Run on an unstripped ARM build (CARGO_PROFILE_RELEASE_STRIP=none).
Uses only Python's standard library; never executes target code.
"""

import argparse
import hashlib
import json
from pathlib import Path
from elf_image import ElfImage
from validation import require
from check_flash_lockout import check as check_flash_lockout
from check_stack_budget import check as check_stack_budget
from config_platform import current_image_policy,read_stack_result

MIN_CORE0_HEADROOM = 65536


def check_headroom(headroom):
    require(headroom >= MIN_CORE0_HEADROOM, "less than 64 KiB core 0 stack headroom")


def geometry(elf):
    """Current linked RAM geometry, shared with the strict artifact reader."""
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
    check_headroom(ram_end - aligned_end)
    return aligned_end-ram_start,ram_end-aligned_end


def _qualified_image(stack,elf,static_ram,headroom,expected_eav=None):
    require(isinstance(stack,dict) and isinstance(stack.get('policies'),list) and len(stack['policies'])==1,
            'full image gate has no retained stack policy')
    policy=current_image_policy(stack['policies'][0],elf,expected_eav)
    checked=read_stack_result(stack,policy,headroom)
    return dict(schema=1,gate='full_memory_layout',status='CONDITIONAL_IMAGE_PASS',
        platform_policy='assumed_external',hardware_qualified=False,
        elf_sha256=hashlib.sha256(elf.data).hexdigest(),static_ram_bytes=static_ram,headroom=headroom,stack=checked)


def read_image_result(record,elf,expected_eav=None):
    """Validate a full-gate record, never an intermediate stack-only result."""
    require(isinstance(record,dict) and isinstance(record.get('stack'),dict),'missing full image gate result')
    static_ram,headroom=geometry(elf)
    wanted=_qualified_image(record['stack'],elf,static_ram,headroom,expected_eav)
    require(set(record)==set(wanted) and type(record.get('schema')) is int and
            type(record.get('static_ram_bytes')) is int and type(record.get('headroom')) is int and
            record.get('hardware_qualified') is False and record==wanted,
            'full image consumer lost conditional status or current identity/geometry')
    return wanted


def check(path):
    elf = ElfImage(path)
    sections,named,symbols=elf.sections,elf.named,elf.symbols
    ram_start,ram_end=0x20000000,0x20080000
    static_ram,headroom=geometry(elf)
    stack=check_stack_budget(elf,headroom)
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
    result=_qualified_image(stack,elf,static_ram,headroom)
    print(f"static RAM {static_ram} B; core 0 headroom {headroom} B; boundaries verified; "
          "CONDITIONAL_IMAGE_PASS, assumed_external, hardware_qualified=false")
    return result


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('elf');parser.add_argument('--result-json',type=Path)
    args=parser.parse_args(argv)
    if args.result_json is not None:require(not args.result_json.exists(),'image result path already exists')
    result=check(args.elf)
    # No record is created before the complete gate returns successfully.
    if args.result_json is not None:
        with args.result_json.open('x',encoding='utf-8',newline='\n') as stream:json.dump(result,stream,sort_keys=True);stream.write('\n')


if __name__ == "__main__":
    main()
