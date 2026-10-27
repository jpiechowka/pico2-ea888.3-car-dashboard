MEMORY {
    /*
     * The RP2350 has either external or internal flash.
     *
     * Keep the existing 2 MiB contract (Pico 2 W has 4 MiB), reserving the
     * final 96 KiB for captures plus 8 KiB for settings. No ELF load contents.
     */
    FLASH : ORIGIN = 0x10000000, LENGTH = 1944K
    /*
     * RAM consists of 8 banks, SRAM0-SRAM7, with a striped mapping.
     * This is usually good for performance, as it distributes load on
     * those banks evenly.
     */
    RAM : ORIGIN = 0x20000000, LENGTH = 512K
    /*
     * RAM banks 8 and 9 use a direct mapping. They can be used to have
     * memory areas dedicated for some specific job, improving predictability
     * of access times.
     * Example: Separate stacks for core0 and core1.
     */
    SRAM8 : ORIGIN = 0x20080000, LENGTH = 4K
    SRAM9 : ORIGIN = 0x20081000, LENGTH = 4K
}

__settings_start = 0x101FE000;
__settings_end = 0x10200000;
__captures_start = 0x101E6000;
__captures_end = __settings_start;
ASSERT(ORIGIN(FLASH) + LENGTH(FLASH) == __captures_start,
       "program FLASH overlaps reserved captures");

SECTIONS {
    /* ### Boot ROM info
     *
     * Goes after .vector_table, to keep it in the first 4K of flash
     * where the Boot ROM (and picotool) can find it
     */
    .start_block : ALIGN(4)
    {
        __start_block_addr = .;
        KEEP(*(.start_block));
        KEEP(*(.boot_info));
    } > FLASH

} INSERT AFTER .vector_table;

/* Match the actual .text alignment, including stricter aligned functions.
 * Embassy upstream 1948c4146aacd005459989ea4d2d24f8a5e32188. */
_stext = ALIGN(ADDR(.start_block) + SIZEOF(.start_block), ALIGNOF(.text));

SECTIONS {
    .probe_identity : ALIGN(4) { KEEP(*(.probe_identity)); } > FLASH
    /* Executable policy is independent of source identity and optional features. */
    .probe_tool : ALIGN(1) { KEEP(*(.probe_tool)); } > FLASH
    /* ### Picotool 'Binary Info' Entries
     *
     * Picotool looks through this block (as we have pointers to it in our
     * header) to find interesting information.
     */
    .bi_entries : ALIGN(4)
    {
        /* We put this in the header */
        __bi_entries_start = .;
        /* Here are the entries */
        KEEP(*(.bi_entries));
        /* Keep this block a nice round size */
        . = ALIGN(4);
        /* We put this in the header */
        __bi_entries_end = .;
    } > FLASH
} INSERT AFTER .text;

SECTIONS {
    /* ### Boot ROM extra info
     *
     * Goes after everything in our program, so it can contain a signature.
     */
    .end_block : ALIGN(4)
    {
        __end_block_addr = .;
        KEEP(*(.end_block));
    } > FLASH

} INSERT AFTER .uninit;

/* cortex-m-rt 0.7.7 provides these from the current location counter after
 * .uninit. The FLASH insertion above changes that counter. Anchor the RAM
 * boundaries explicitly so heap diagnostics and future stack guards use RAM. */
__sheap = ALIGN(__euninit, 8);
_stack_end = __sheap;
ASSERT(__sheap >= ORIGIN(RAM) && __sheap <= ORIGIN(RAM) + LENGTH(RAM),
       "static RAM boundary is outside RAM");
ASSERT(_stack_start >= __sheap + 32K,
       "less than 32 KiB remains for the core 0 stack");

PROVIDE(start_to_end = __end_block_addr - __start_block_addr);
PROVIDE(end_to_start = __start_block_addr - __end_block_addr);

/* LLVM frame metadata is for host admission only and must never consume RAM
 * or flash LOAD bytes. The gate also verifies this section is non-allocatable. */
SECTIONS {
    .stack_sizes (INFO) : { KEEP(*(.stack_sizes)); }
} INSERT AFTER .end_block;
