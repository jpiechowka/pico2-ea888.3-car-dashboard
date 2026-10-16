MEMORY {
    /*
     * The RP2350 has either external or internal flash.
     *
     * Keep the existing 2 MiB contract (Pico 2 has 4 MiB), reserving the
     * final two 4 KiB sectors for settings. They have no ELF load contents.
     */
    FLASH : ORIGIN = 0x10000000, LENGTH = 2040K
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
    SRAM4 : ORIGIN = 0x20080000, LENGTH = 4K
    SRAM5 : ORIGIN = 0x20081000, LENGTH = 4K
}

__settings_start = 0x101FE000;
__settings_end = 0x10200000;
ASSERT(ORIGIN(FLASH) + LENGTH(FLASH) == __settings_start,
       "program FLASH overlaps reserved settings");

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

/* Move .text after boot info. cortex-m-rt 0.7.7 checks section alignment;
 * the boot info size alone can leave _stext at a four-byte-only address. */
_stext = ALIGN(ADDR(.start_block) + SIZEOF(.start_block), 16);

SECTIONS {
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



/* The sole runtime configuration array is retained for independent ELF admission. */
SECTIONS {
    .dashboard_config : ALIGN(4)
    {
        __dashboard_config_start = .;
        KEEP(*(.dashboard_config));
        __dashboard_config_end = .;
    } > FLASH
} INSERT AFTER .rodata;
ASSERT(__dashboard_config_end > __dashboard_config_start, "missing embedded dashboard configuration");
ASSERT(__dashboard_config_end <= ORIGIN(FLASH) + LENGTH(FLASH), "dashboard configuration outside program flash");

SECTIONS {
    .startup_profile : ALIGN(4) {
        __startup_profile_start = .;
        KEEP(*(.startup_profile));
        __startup_profile_end = .;
    } > FLASH
    .startup_asset : ALIGN(4) {
        __startup_asset_start = .;
        KEEP(*(.startup_asset));
        __startup_asset_end = .;
    } > FLASH
} INSERT AFTER .dashboard_config;
ASSERT(__startup_profile_end - __startup_profile_start == 16, "invalid linked startup profile size");
ASSERT(__startup_asset_end > __startup_asset_start, "missing linked startup asset sentinel");
ASSERT(__startup_asset_end <= ORIGIN(FLASH) + LENGTH(FLASH), "startup asset outside program flash");
