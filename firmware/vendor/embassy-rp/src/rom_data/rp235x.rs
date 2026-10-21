//! RP235x ROM entry points, selected by architecture/security mask and ROM revision.
//!
//! These are raw calls: they do not acquire SDK ROM locks, quiesce the other core,
//! disable interrupts, validate Rust pointer lifetimes, or restore application state.
//! Call only on matching silicon after normal ROM initialization. Lookup failure is
//! not checked before function-pointer conversion. Resolve `ptr()` before disabling
//! XIP; the wrapper/lookup itself can reside in flash. See the
//! [SDK ABI and locking wrappers](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_bootrom/include/pico/bootrom.h).
//!
//! From [Section 5.4](https://rptl.io/rp2350-datasheet#section_bootrom) of the
//! RP2350 datasheet:
//!
//! > Whilst some ROM space is dedicated to the implementation of the boot
//! > sequence and USB/UART boot interfaces, the bootrom also contains public
//! > functions that provide useful RP2350 functionality that may be useful for
//! > any code or runtime running on the device

// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp235x-hal/src/rom_data.rs

/// A bootrom function table code.
pub type RomFnTableCode = [u8; 2];

/// ROM ABI: `code` is a little-endian two-byte tag, `mask` selects entry kind; returns its address or zero.
type RomTableLookupFn = unsafe extern "C" fn(code: u32, mask: u32) -> usize;

/// Pointer to the value lookup function supplied by the ROM.
///
/// This address is described at `5.5.1. Locating the API Functions`
#[cfg(all(target_arch = "arm", target_os = "none"))]
const ROM_TABLE_LOOKUP_A2: *const u16 = 0x0000_0016 as _;

/// Pointer to the value lookup function supplied by the ROM.
///
/// This address is described at `5.5.1. Locating the API Functions`
#[cfg(all(target_arch = "arm", target_os = "none"))]
const ROM_TABLE_LOOKUP_A1: *const u32 = 0x0000_0018 as _;

/// Pointer to the data lookup function supplied by the ROM.
///
/// On Arm, the same function is used to look up code and data.
#[cfg(all(target_arch = "arm", target_os = "none"))]
const ROM_DATA_LOOKUP_A2: *const u16 = ROM_TABLE_LOOKUP_A2;

/// Pointer to the data lookup function supplied by the ROM.
///
/// On Arm, the same function is used to look up code and data.
#[cfg(all(target_arch = "arm", target_os = "none"))]
const ROM_DATA_LOOKUP_A1: *const u32 = ROM_TABLE_LOOKUP_A1;

/// Pointer to the value lookup function supplied by the ROM.
///
/// This address is described at `5.5.1. Locating the API Functions`
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
const ROM_TABLE_LOOKUP_A2: *const u16 = 0x0000_7DFA as _;

/// Pointer to the value lookup function supplied by the ROM.
///
/// This address is described at `5.5.1. Locating the API Functions`
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
const ROM_TABLE_LOOKUP_A1: *const u32 = 0x0000_7DF8 as _;

/// Pointer to the data lookup function supplied by the ROM.
///
/// On RISC-V, a different function is used to look up data.
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
const ROM_DATA_LOOKUP_A2: *const u16 = 0x0000_7DF8 as _;

/// Pointer to the data lookup function supplied by the ROM.
///
/// On RISC-V, a different function is used to look up data.
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
const ROM_DATA_LOOKUP_A1: *const u32 = 0x0000_7DF4 as _;

/// Address of the version number of the ROM.
const VERSION_NUMBER: *const u8 = 0x0000_0013 as _;

/// ROM table architecture, privilege and data-entry selection bits.
#[allow(unused)]
mod rt_flags {
    /// Near RISC-V function entry.
    pub const FUNC_RISCV: u32 = 0x0001;
    /// Far RISC-V function entry (combined selector bits).
    pub const FUNC_RISCV_FAR: u32 = 0x0003;
    /// Secure Arm function entry.
    pub const FUNC_ARM_SEC: u32 = 0x0004;
    // reserved for 32-bit pointer: 0x0008
    /// Non-secure Arm function entry.
    pub const FUNC_ARM_NONSEC: u32 = 0x0010;
    // reserved for 32-bit pointer: 0x0020
    /// ROM data-pointer entry.
    pub const DATA: u32 = 0x0040;
    // reserved for 32-bit pointer: 0x0080
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    /// Secure Arm on bare-metal Arm builds; RISC-V selector otherwise.
    pub const FUNC_ARM_SEC_RISCV: u32 = FUNC_ARM_SEC;
    #[cfg(not(all(target_arch = "arm", target_os = "none")))]
    /// Secure Arm on bare-metal Arm builds; RISC-V selector otherwise.
    pub const FUNC_ARM_SEC_RISCV: u32 = FUNC_RISCV;
}

/// Returns the ROM function address for little-endian `tag` and architecture/security `mask`, or zero if absent. Does
/// not validate callability; accesses fixed silicon ROM addresses.
pub fn rom_table_lookup(
    tag: RomFnTableCode,
    mask: u32,
) -> usize {
    let tag = u16::from_le_bytes(tag) as u32;
    unsafe {
        let lookup_func = if rom_version_number() == 1 {
            ROM_TABLE_LOOKUP_A1.read() as usize
        } else {
            ROM_TABLE_LOOKUP_A2.read() as usize
        };
        let lookup_func: RomTableLookupFn = core::mem::transmute(lookup_func);
        lookup_func(tag, mask)
    }
}

/// Returns the ROM data address for little-endian `tag` and entry-kind `mask`, or zero if absent. Uses the
/// revision-specific lookup ABI; does not dereference the result.
pub fn rom_data_lookup(
    tag: RomFnTableCode,
    mask: u32,
) -> usize {
    let tag = u16::from_le_bytes(tag) as u32;
    unsafe {
        let lookup_func = if rom_version_number() == 1 {
            ROM_DATA_LOOKUP_A1.read() as usize
        } else {
            ROM_DATA_LOOKUP_A2.read() as usize
        };
        let lookup_func: RomTableLookupFn = core::mem::transmute(lookup_func);
        lookup_func(tag, mask)
    }
}

/// Generates raw ROM wrappers and pointer access; optional AtomicU16 caching publishes only immutable ROM addresses.
macro_rules! declare_rom_function {
    (
        $(#[$outer:meta])*
        fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty
        $lookup:block
    ) => {
        #[doc = r"Additional access for the `"]
        #[doc = stringify!($name)]
        #[doc = r"` ROM function."]
        pub mod $name {
            /// Returns the resolved ROM target with this function's ABI and preconditions.
            /// Resolves before returning even with caching, so callers can retain the
            /// pointer across XIP maintenance. Missing entries are not checked.
            #[cfg(not(feature = "rom-func-cache"))]
            pub fn ptr() -> extern "C" fn( $($argname: $ty),* ) -> $ret {
                let p: usize = $lookup;
                unsafe {
                    let func : extern "C" fn( $($argname: $ty),* ) -> $ret = core::mem::transmute(p);
                    func
                }
            }

            /// Returns the resolved ROM target with this function's ABI and preconditions.
            /// Resolves before returning even with caching, so callers can retain the
            /// pointer across XIP maintenance. Missing entries are not checked.
            #[cfg(feature = "rom-func-cache")]
            pub fn ptr() -> extern "C" fn( $($argname: $ty),* ) -> $ret {
                use core::sync::atomic::{AtomicU16, Ordering};

                // All pointers in the ROM fit in 16 bits, so we don't need a
                // full width word to store the cached value.
                /// Zero means unresolved; nonzero values are immutable sub-64KiB ROM addresses.
                static CACHED_PTR: AtomicU16 = AtomicU16::new(0);
                // This is safe because the lookup will always resolve
                // to the same value.  So even if an interrupt or another
                // core starts at the same time, it just repeats some
                // work and eventually writes back the correct value.
                let p: usize = match CACHED_PTR.load(Ordering::Relaxed) {
                    0 => {
                        let raw: usize = $lookup;
                        CACHED_PTR.store(raw as u16, Ordering::Relaxed);
                        raw
                    },
                    val => val as usize,
                };
                unsafe {
                    let func : extern "C" fn( $($argname: $ty),* ) -> $ret = core::mem::transmute(p);
                    func
                }
            }
        }

        $(#[$outer])*
        pub extern "C" fn $name( $($argname: $ty),* ) -> $ret {
            $name::ptr()($($argname),*)
        }
    };

    (
        $(#[$outer:meta])*
        unsafe fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty
        $lookup:block
    ) => {
        #[doc = r"Additional access for the `"]
        #[doc = stringify!($name)]
        #[doc = r"` ROM function."]
        pub mod $name {
            /// Returns the resolved ROM target with this function's ABI and preconditions.
            /// Resolves before returning even with caching, so callers can retain the
            /// pointer across XIP maintenance. Missing entries are not checked.
            #[cfg(not(feature = "rom-func-cache"))]
            pub fn ptr() -> unsafe extern "C" fn( $($argname: $ty),* ) -> $ret {
                let p: usize = $lookup;
                unsafe {
                    let func : unsafe extern "C" fn( $($argname: $ty),* ) -> $ret = core::mem::transmute(p);
                    func
                }
            }

            /// Returns the resolved ROM target with this function's ABI and preconditions.
            /// Resolves before returning even with caching, so callers can retain the
            /// pointer across XIP maintenance. Missing entries are not checked.
            #[cfg(feature = "rom-func-cache")]
            pub fn ptr() -> unsafe extern "C" fn( $($argname: $ty),* ) -> $ret {
                use core::sync::atomic::{AtomicU16, Ordering};

                // All pointers in the ROM fit in 16 bits, so we don't need a
                // full width word to store the cached value.
                /// Zero means unresolved; nonzero values are immutable sub-64KiB ROM addresses.
                static CACHED_PTR: AtomicU16 = AtomicU16::new(0);
                // This is safe because the lookup will always resolve
                // to the same value.  So even if an interrupt or another
                // core starts at the same time, it just repeats some
                // work and eventually writes back the correct value.
                let p: usize = match CACHED_PTR.load(Ordering::Relaxed) {
                    0 => {
                        let raw: usize = $lookup;
                        CACHED_PTR.store(raw as u16, Ordering::Relaxed);
                        raw
                    },
                    val => val as usize,
                };
                unsafe {
                    let func : unsafe extern "C" fn( $($argname: $ty),* ) -> $ret = core::mem::transmute(p);
                    func
                }
            }
        }

        $(#[$outer])*
        pub unsafe extern "C" fn $name( $($argname: $ty),* ) -> $ret {
            $name::ptr()($($argname),*)
        }
    };
}

// **************** 5.5.7 Low-level Flash Commands ****************

declare_rom_function! {
    /// Restore all QSPI pad controls to their default state, and connect the
    /// QMI peripheral to the QSPI pads.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Resets QSPI pad/mux state; affects both flash access and any application pad configuration.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them.
    unsafe fn connect_internal_flash() -> () {
        crate::rom_data::rom_table_lookup(*b"IF", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Initialise the QMI for serial operations (direct mode)
    ///
    /// Also initialise a basic XIP mode, where the QMI will perform 03h serial
    /// read commands at low speed (CLKDIV=12) in response to XIP reads.
    ///
    /// Then, issue a sequence to the QSPI device on chip select 0, designed to
    /// return it from continuous read mode ("XIP mode") and/or QPI mode to a
    /// state where it will accept serial commands. This is necessary after
    /// system reset to restore the QSPI device to a known state, because
    /// resetting RP2350 does not reset attached QSPI devices. It is also
    /// necessary when user code, having already performed some
    /// continuous-read-mode or QPI-mode accesses, wishes to return the QSPI
    /// device to a state where it will accept the serial erase and programming
    /// commands issued by the bootrom’s flash access functions.
    ///
    /// If a GPIO for the secondary chip select is configured via FLASH_DEVINFO,
    /// then the XIP exit sequence is also issued to chip select 1.
    ///
    /// The QSPI device should be accessible for XIP reads after calling this
    /// function; the name flash_exit_xip refers to returning the QSPI device
    /// from its XIP state to a serial command state.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Transitions configured flash devices to serial commands, installing slow XIP reads; does not preserve a custom QMI mode.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them.
    unsafe fn flash_exit_xip() -> () {
        crate::rom_data::rom_table_lookup(*b"EX", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Erase count bytes, starting at addr (offset from start of flash).
    ///
    /// Optionally, pass a block erase command e.g. D8h block erase, and the
    /// size of the block erased by this command — this function will use the
    /// larger block erase where possible, for much higher erase speed. addr
    /// must be aligned to a 4096-byte sector, and count must be a multiple of
    /// 4096 bytes.
    ///
    /// This is a low-level flash API, and no validation of the arguments is
    /// performed. See flash_op() for a higher-level API which checks alignment,
    /// flash bounds and partition permissions, and can transparently apply a
    /// runtime-to-storage address translation.
    ///
    /// The QSPI device must be in a serial command state before calling this
    /// API, which can be achieved by calling connect_internal_flash() followed
    /// by flash_exit_xip(). After the erase, the flash cache should be flushed
    /// via flash_flush_cache() to ensure the modified flash data is visible to
    /// cached XIP accesses.
    ///
    /// Finally, the original XIP mode should be restored by copying the saved
    /// XIP setup function from bootram into SRAM, and executing it: the bootrom
    /// provides a default function which restores the flash mode/clkdiv
    /// discovered during flash scanning, and user programs can override this
    /// with their own XIP setup function.
    ///
    /// For the duration of the erase operation, QMI is in direct mode (Section
    /// 12.14.5) and attempting to access XIP from DMA, the debugger or the
    /// other core will return a bus fault. XIP becomes accessible again once
    /// the function returns.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Erases `count` bytes at flash offset `addr`; `block_size` bytes and `block_cmd` optionally select larger erases.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. `addr`/`count` must fit the physical chip and be 4096-byte aligned/multiple; block parameters must match that chip. No bounds/status result is provided.
    unsafe fn flash_range_erase(addr: u32, count: usize, block_size: u32, block_cmd: u8) -> () {
        crate::rom_data::rom_table_lookup(*b"RE", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Program data to a range of flash storage addresses starting at addr
    /// (offset from the start of flash) and count bytes in size.
    ///
    /// `addr` must be aligned to a 256-byte boundary, and count must be a
    /// multiple of 256.
    ///
    /// This is a low-level flash API, and no validation of the arguments is
    /// performed. See flash_op() for a higher-level API which checks alignment,
    /// flash bounds and partition permissions, and can transparently apply a
    /// runtime-to-storage address translation.
    ///
    /// The QSPI device must be in a serial command state before calling this
    /// API — see notes on flash_range_erase().
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Programs `count` bytes from `data` at flash offset `addr`; no status or byte count is returned.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. `addr` and `count` must fit flash and be 256-byte aligned/multiple. `data` must be readable SRAM for the entire range, and target bits must be erased as needed.
    unsafe fn flash_range_program(addr: u32, data: *const u8, count: usize) -> () {
        crate::rom_data::rom_table_lookup(*b"RP", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Flush the entire XIP cache, by issuing an invalidate by set/way
    /// maintenance operation to every cache line (Section 4.4.1).
    ///
    /// This ensures that flash program/erase operations are visible to
    /// subsequent cached XIP reads.
    ///
    /// Note that this unpins pinned cache lines, which may interfere with
    /// cache-as-SRAM use of the XIP cache.
    ///
    /// No other operations are performed.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Invalidates every XIP line, including pinned cache-as-RAM contents.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. No live allocation may rely on pinned cache contents surviving.
    unsafe fn flash_flush_cache() -> () {
        crate::rom_data::rom_table_lookup(*b"FC", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Configure the QMI to generate a standard 03h serial read command, with
    /// 24 address bits, upon each XIP access.
    ///
    /// This is a slow XIP configuration, but is widely supported. CLKDIV is set
    /// to 12. The debugger may call this function to ensure that flash is
    /// readable following a program/erase operation.
    ///
    /// Note that the same setup is performed by flash_exit_xip(), and the
    /// RP2350 flash program/erase functions do not leave XIP in an inaccessible
    /// state, so calls to this function are largely redundant. It is provided
    /// for compatibility with RP2040.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Installs command-03h, 24-bit-address, divider12 XIP reads on the configured flash interface.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them.
    unsafe fn flash_enter_cmd_xip() -> () {
        crate::rom_data::rom_table_lookup(*b"CX", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Configure QMI for one of a small menu of XIP read modes supported by the
    /// bootrom. This mode is configured for both memory windows (both chip
    /// selects), and the clock divisor is also applied to direct mode.
    ///
    /// The available modes are:
    ///
    /// * 0: `03h` serial read: serial address, serial data, no wait cycles
    /// * 1: `0Bh` serial read: serial address, serial data, 8 wait cycles
    /// * 2: `BBh` dual-IO read: dual address, dual data, 4 wait cycles
    ///   (including MODE bits, which are driven to 0)
    /// * 3: `EBh` quad-IO read: quad address, quad data, 6 wait cycles
    ///   (including MODE bits, which are driven to 0)
    ///
    /// The XIP write command/format are not configured by this function. When
    /// booting from flash, the bootrom tries each of these modes in turn, from
    /// 3 down to 0. The first mode that is found to work is remembered, and a
    /// default XIP setup function is written into bootram that calls this
    /// function (flash_select_xip_read_mode) with the parameters discovered
    /// during flash scanning. This can be called at any time to restore the
    /// flash parameters discovered during flash boot.
    ///
    /// All XIP modes configured by the bootrom have an 8-bit serial command
    /// prefix, so that the flash can remain in a serial command state, meaning
    /// XIP accesses can be mixed more freely with program/erase serial
    /// operations. This has a performance penalty, so users can perform their
    /// own flash setup after flash boot using continuous read mode or QPI mode
    /// to avoid or alleviate the command prefix cost.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Selects `bootrom_xip_mode` 0..=3 from the modes above, using `clkdiv` as the QMI clock divisor for both windows and direct mode.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. Both devices and the system clock must support the chosen mode/divisor.
    unsafe fn flash_select_xip_read_mode(bootrom_xip_mode: u8, clkdiv: u8) -> () {
        crate::rom_data::rom_table_lookup(*b"XM", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Restore the QMI address translation registers, ATRANS0 through ATRANS7,
    /// to their reset state. This makes the runtime- to-storage address map an
    /// identity map, i.e. the mapped and unmapped address are equal, and the
    /// entire space is fully mapped.
    ///
    /// See [Section 12.14.4](https://rptl.io/rp2350-datasheet#section_bootrom) of the RP2350
    /// datasheet.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Restores identity QMI translation for every window.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. No live reference or execution address may depend on the old mapping.
    unsafe fn flash_reset_address_trans() -> () {
        crate::rom_data::rom_table_lookup(*b"RA", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

// **************** High-level Flash Commands ****************

declare_rom_function! {
    /// Applies the address translation currently configured by QMI address
    /// translation registers, ATRANS0 through ATRANS7.
    ///
    /// See [Section 12.14.4](https://rptl.io/rp2350-datasheet#section_bootrom) of the RP2350
    /// datasheet.
    ///
    /// Translating an address outside of the XIP runtime address window, or
    /// beyond the bounds of an ATRANSx_SIZE field, returns
    /// BOOTROM_ERROR_INVALID_ADDRESS, which is not a valid flash storage
    /// address. Otherwise, return the storage address which QMI would access
    /// when presented with the runtime address addr. This is effectively a
    /// virtual-to-physical address translation for QMI.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Returns the storage address mapped from XIP runtime `addr`, or a negative ROM address error.
    ///
    /// # Safety
    /// Serialize changes to QMI translation and resident flash state for the lookup.
    unsafe fn flash_runtime_to_storage_addr(addr: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"FA", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [flash_runtime_to_storage_addr()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Returns the storage address mapped from XIP runtime `addr`, or a negative ROM address error. This entry uses Non-secure access policy.
    ///
    /// # Safety
    /// Serialize changes to QMI translation and resident flash state for the lookup.
    unsafe fn flash_runtime_to_storage_addr_ns(addr: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"FA", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

declare_rom_function! {
    /// Perform a flash read, erase, or program operation.
    ///
    /// Erase operations must be sector-aligned (4096 bytes) and sector-
    /// multiple-sized, and program operations must be page-aligned (256 bytes)
    /// and page-multiple-sized; misaligned erase and program operations will
    /// return BOOTROM_ERROR_BAD_ALIGNMENT. The operation — erase, read, program
    /// — is selected by the CFLASH_OP_BITS bitfield of the flags argument.
    ///
    /// See datasheet section 5.5.8.2 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Uses `flags` to select operation, effective security and address space; accesses `size_bytes` from `addr` using `buffer`. Returns zero or a negative ROM error; translation holes can fail after partial effects.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. Serialize ROM flash-operation state; provide readable program or writable read storage for the full size (unused for erase), valid alignment, and appropriate security permission. This raw call supplies no SDK lock.
    unsafe fn flash_op(flags: u32, addr: u32, size_bytes: u32, buffer: *mut u8) -> i32 {
        crate::rom_data::rom_table_lookup(*b"FO", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [flash_op()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Uses `flags` to select operation, effective security and address space; accesses `size_bytes` from `addr` using `buffer`. Returns zero or a negative ROM error; translation holes can fail after partial effects. This entry uses Non-secure access policy.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. Serialize ROM flash-operation state; provide readable program or writable read storage for the full size (unused for erase), valid alignment, and appropriate security permission. This raw call supplies no SDK lock.
    unsafe fn flash_op_ns(flags: u32, addr: u32, size_bytes: u32, buffer: *mut u8) -> i32 {
        crate::rom_data::rom_table_lookup(*b"FO", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

// **************** Security Related Functions ****************

declare_rom_function! {
    /// Allow or disallow the specific NS API (note all NS APIs default to
    /// disabled).
    ///
    /// See datasheet section 5.5.9.1 for more details.
    ///
    /// Supported architectures: ARM-S
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Sets permission for `ns_api_num` using `allowed` 0/1; returns zero or a negative ROM argument error. Changes persist until ROM state reset.
    ///
    /// # Safety
    /// Call in Secure Arm state and serialize the permission policy with affected Non-secure clients.
    unsafe fn set_ns_api_permission(ns_api_num: u32, allowed: u8) -> i32 {
        crate::rom_data::rom_table_lookup(*b"SP", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC)
    }
}

declare_rom_function! {
    /// Utility method that can be used by secure ARM code to validate a buffer
    /// passed to it from Non-secure code.
    ///
    /// See datasheet section 5.5.9.2 for more details.
    ///
    /// # Safety
    /// `ok` must point to writable storage for the access result. `write` must
    /// be the RCP boolean 0xa500a500 (write access) or 0x00c300c3 (read access);
    /// other values can cause the ROM to halt. The returned pointer may only be
    /// used when `*ok` is the RCP true value.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Checks `size` bytes at `addr` for access selected by RCP boolean `write`, storing RCP success in `ok`. Returns the validated pointer, usable only after success.
    ///
    /// # Safety
    /// `ok` must be aligned writable u32 storage; `write` must be 0xa500a500 or 0x00c300c3 or ROM may halt. Retain normal Rust lifetime/aliasing checks and prevent permissions changing before use.
    unsafe fn validate_ns_buffer(addr: *const u8, size: u32, write: u32, ok: *mut u32) -> *mut u8 {
        crate::rom_data::rom_table_lookup(*b"VB", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

// **************** Miscellaneous Functions ****************

declare_rom_function! {
    /// Resets the RP2350 and uses the watchdog facility to restart.
    ///
    /// See datasheet section 5.5.10.1 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Requests a watchdog reboot with `flags`, delay `delay_ms` in milliseconds and boot-mode parameters `p0`/`p1`. Returns a ROM status unless no-return-on-success is requested; errors are negative. May overwrite watchdog scratch/boot diagnostics; does not coordinate application shutdown.
    fn reboot(flags: u32, delay_ms: u32, p0: u32, p1: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"RB", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [reboot()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Requests a watchdog reboot with `flags`, delay `delay_ms` in milliseconds and boot-mode parameters `p0`/`p1`. Returns a ROM status unless no-return-on-success is requested; errors are negative. May overwrite watchdog scratch/boot diagnostics; does not coordinate application shutdown. This entry uses Non-secure access policy.
    fn reboot_ns(flags: u32, delay_ms: u32, p0: u32, p1: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"RB", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

declare_rom_function! {
    /// Resets internal bootrom state.
    ///
    /// See datasheet section 5.5.10.2 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Resets state selected by `flags`: bit 0 current core, bit 1 other core, bit 2 global locks/permissions/callbacks.
    ///
    /// # Safety
    /// No affected core, interrupt or caller may be using ROM state being reset; global reset releases ROM locks and removes security policy.
    unsafe fn bootrom_state_reset(flags: u32) -> () {
        crate::rom_data::rom_table_lookup(*b"SR", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Set a boot ROM callback.
    ///
    /// The only supported callback_number is 0 which sets the callback used for
    /// the secure_call API.
    ///
    /// See datasheet section 5.5.10.3 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Sets callback slot `callback_number` (only 0) to `callback_fn`; null clears and a negative pointer value queries. Returns the prior pointer as i32 or a negative ROM error.
    ///
    /// # Safety
    /// A positive pointer must remain executable with the ROM callback C ABI and valid captured/global state for every later invocation; serialize replacement and calls.
    unsafe fn set_rom_callback(callback_number: i32, callback_fn: *const ()) -> i32 {
        crate::rom_data::rom_table_lookup(*b"RC", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

// **************** System Information Functions ****************

declare_rom_function! {
    /// Fills a buffer with various system information.
    ///
    /// Note that this API is also used to return information over the PICOBOOT
    /// interface.
    ///
    /// See datasheet section 5.5.11.1 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Fills `out_buffer` with fields selected by `flags`, within `out_buffer_word_size` u32 words. Returns written word count or negative ROM error; check the first supported-flags word before interpreting fields.
    ///
    /// # Safety
    /// `out_buffer` must be aligned, exclusively writable for the declared capacity and valid throughout the call.
    unsafe fn get_sys_info(out_buffer: *mut u32, out_buffer_word_size: usize, flags: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GS", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [get_sys_info()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Fills `out_buffer` with fields selected by `flags`, within `out_buffer_word_size` u32 words. Returns written word count or negative ROM error; check the first supported-flags word before interpreting fields. This entry uses Non-secure access policy.
    ///
    /// # Safety
    /// `out_buffer` must be aligned, exclusively writable for the declared capacity and valid throughout the call.
    unsafe fn get_sys_info_ns(out_buffer: *mut u32, out_buffer_word_size: usize, flags: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GS", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

declare_rom_function! {
    /// Fills a buffer with information from the partition table.
    ///
    /// Note that this API is also used to return information over the PICOBOOT
    /// interface.
    ///
    /// See datasheet section 5.5.11.2 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Fills `out_buffer` (capacity `out_buffer_word_size` words) using field/partition selector `flags_and_partition`. Returns written words or negative ROM error for missing/changed table or insufficient space; first word reports supported flags.
    ///
    /// # Safety
    /// Provide aligned exclusive output storage for the capacity; serialize partition-table/SHA state and flash mutation. No SDK ROM lock is acquired.
    unsafe fn get_partition_table_info(out_buffer: *mut u32, out_buffer_word_size: usize, flags_and_partition: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GP", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [get_partition_table_info()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Fills `out_buffer` (capacity `out_buffer_word_size` words) using field/partition selector `flags_and_partition`. Returns written words or negative ROM error for missing/changed table or insufficient space; first word reports supported flags. This entry uses Non-secure access policy.
    ///
    /// # Safety
    /// Provide aligned exclusive output storage for the capacity; serialize partition-table/SHA state and flash mutation. No SDK ROM lock is acquired.
    unsafe fn get_partition_table_info_ns(out_buffer: *mut u32, out_buffer_word_size: usize, flags_and_partition: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GP", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

declare_rom_function! {
    /// Loads the current partition table from flash, if present.
    ///
    /// See datasheet section 5.5.11.3 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Loads flash partition metadata using `workarea_base` scratch of `workarea_size` bytes; `force_reload` replaces already-resident metadata. Returns zero or negative ROM validation/resource error.
    ///
    /// # Safety
    /// Provide exclusive word-aligned scratch (SDK documents 3264 bytes), serialize ROM SHA/partition state and keep source flash readable/stable.
    unsafe fn load_partition_table(workarea_base: *mut u8, workarea_size: usize, force_reload: bool) -> i32 {
        crate::rom_data::rom_table_lookup(*b"LP", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Writes data from a buffer into OTP, or reads data from OTP into a buffer.
    ///
    /// See datasheet section 5.5.11.4 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Transfers `buf_len` bytes via `buf`; `row_and_flags` low16 bits select row0..4095, bit 16 writes, bit 17 enables ECC. Returns zero or negative ROM error, possibly after partial irreversible programming.
    ///
    /// # Safety
    /// Provide exclusive readable/writeable buffer as direction requires, aligned and sized in 2-byte ECC or 4-byte raw units. Serialize OTP/ROM use; programming/security changes must preserve all active code/data access assumptions.
    unsafe fn otp_access(buf: *mut u8, buf_len: usize, row_and_flags: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"OA", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Non-secure version of [otp_access()]
    ///
    /// Supported architectures: ARM-NS
    #[cfg(all(target_arch = "arm", target_os = "none"))]
    ///
    /// Transfers `buf_len` bytes via `buf`; `row_and_flags` low16 bits select row0..4095, bit 16 writes, bit 17 enables ECC. Returns zero or negative ROM error, possibly after partial irreversible programming. This entry uses Non-secure access policy.
    ///
    /// # Safety
    /// Provide exclusive readable/writeable buffer as direction requires, aligned and sized in 2-byte ECC or 4-byte raw units. Serialize OTP/ROM use; programming/security changes must preserve all active code/data access assumptions.
    unsafe fn otp_access_ns(buf: *mut u8, buf_len: usize, row_and_flags: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"OA", crate::rom_data::inner::rt_flags::FUNC_ARM_NONSEC)
    }
}

// **************** Boot Related Functions ****************

declare_rom_function! {
    /// Determines which of the partitions has the "better" IMAGE_DEF. In the
    /// case of executable images, this is the one that would be booted.
    ///
    /// See datasheet section 5.5.12.1 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Selects an A/B image for `partition_a_num`, using `workarea_base` with `workarea_size` scratch bytes. Uses `flash_update_boot_window_base` to favor the matching updated flash region; zero selects ordinary version policy. Returns chosen partition index or negative ROM error.
    ///
    /// # Safety
    /// Provide exclusive word-aligned scratch (SDK documents 3264 bytes), stable flash and serialized SHA/partition state. This raw selection can disturb pending try-before-you-buy downgrade state.
    unsafe fn pick_ab_parition(workarea_base: *mut u8, workarea_size: usize, partition_a_num: u32, flash_update_boot_window_base: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"AB", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Searches a memory region for a launchable image, and executes it if
    /// possible.
    ///
    /// See datasheet section 5.5.12.2 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Searches `region_size` bytes at signed `region_base` using `workarea_base` scratch of `workarea_size` bytes. Negative base also marks a flash-update region. Returns a negative error on failure; successful image transfer does not return.
    ///
    /// # Safety
    /// Provide exclusive word-aligned scratch (SDK documents 3264 bytes), word-aligned region/size and a valid executable image. Quiesce other cores/IRQs/DMA and relinquish state/resources required by the new image.
    unsafe fn chain_image(workarea_base: *mut u8, workarea_size: usize, region_base: i32, region_size: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"CI", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Perform an "explicit" buy of an executable launched via an IMAGE_DEF
    /// which was "explicit buy" flagged.
    ///
    /// See datasheet section 5.5.12.3 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Commits the running trial image using `buffer` scratch of `buffer_size` bytes. Returns ROM status, may erase flash/program OTP, and may reboot while advancing rollback version; errors do not imply rollback.
    ///
    /// # Safety
    /// All code, data and return paths needed during QMI reconfiguration must be in SRAM/ROM; exclude competing cores, IRQs, DMA and debugger XIP accesses, and restore the required XIP state before releasing them. Provide exclusive word-aligned SRAM scratch of at least4096 bytes and serialize boot/OTP state.
    unsafe fn explicit_buy(buffer: *mut u8, buffer_size: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"EB", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Selects the resident partition accepted for a UF2 family.
    ///
    /// See datasheet section 5.5.12.4 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Chooses a UF2 destination for `family_id`, using `workarea_base` with `workarea_size` scratch bytes and filling `partition_out`. Returns a partition index or negative ROM error; requires a loaded partition table.
    ///
    /// # Safety
    /// Provide exclusive word-aligned scratch (SDK documents 3264 bytes), aligned writable resident-partition storage at `partition_out`, and serialized SHA/partition state. The output is a whole resident_partition_t (two u32 words/eight bytes), not a single u32.
    unsafe fn get_uf2_target_partition(workarea_base: *mut u8, workarea_size: usize, family_id: u32, partition_out: *mut u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GU", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

declare_rom_function! {
    /// Returns: The index of the B partition of partition A if a partition
    /// table is present and loaded, and there is a partition A with a B
    /// partition; otherwise returns BOOTROM_ERROR_NOT_FOUND.
    ///
    /// See datasheet section 5.5.12.5 for more details.
    ///
    /// Supported architectures: ARM-S, RISC-V
    ///
    /// Returns the partner index for A partition `partition_a`, or negative NOT_FOUND when table/partner is absent.
    ///
    /// # Safety
    /// Serialize resident partition-table replacement for the lookup.
    unsafe fn get_b_partition(partition_a: u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"GB", crate::rom_data::inner::rt_flags::FUNC_ARM_SEC_RISCV)
    }
}

// **************** Non-secure-specific Functions ****************

// NB: The "secure_call" function should be here, but it doesn't have a fixed
// function signature as it is designed to let you bounce into any secure
// function from non-secure mode.

// **************** RISC-V Functions ****************

declare_rom_function! {
    /// Set stack for RISC-V bootrom functions to use.
    ///
    /// See datasheet section 5.5.14.1 for more details.
    ///
    /// Supported architectures: RISC-V
    #[cfg(not(all(target_arch = "arm", target_os = "none")))]
    ///
    /// Installs the current core's ROM emulation stack from `base_size`, returning ROM status and replacing that descriptor with the prior stack.
    ///
    /// # Safety
    /// `base_size` must reference two writable u32 words (base pointer, byte size); the pointed-to word-aligned stack must remain exclusively available, with size a multiple of 4, while installed. Do not replace during ROM emulation.
    unsafe fn set_bootrom_stack(base_size: *mut u32) -> i32 {
        crate::rom_data::rom_table_lookup(*b"SS", crate::rom_data::inner::rt_flags::FUNC_RISCV)
    }
}

/// Returns the immutable silicon ROM revision byte at address 0x13; only callable on matching hardware.
pub fn rom_version_number() -> u8 { unsafe { *VERSION_NUMBER } }

/// Returns the high eight hexadecimal digits of the ROM source Git revision, reading the GR data entry.
pub fn git_revision() -> u32 {
    let ptr = rom_data_lookup(*b"GR", rt_flags::DATA) as *const u32;
    unsafe { ptr.read() }
}

/// Returns a borrowed pointer to mutable ROM-owned resident partition metadata; no Rust aliasing/lifetime guarantee is
/// provided.
///
/// The resident partition table is the subset of the full partition table that
/// is kept in memory, and used for flash permissions.
pub fn partition_table_pointer() -> *const u32 {
    let ptr = rom_data_lookup(*b"PT", rt_flags::DATA) as *const *const u32;
    unsafe { ptr.read() }
}

/// Determine if we are in secure mode
///
/// Returns `true` if we are in secure mode and `false` if we are in non-secure
/// mode.
#[cfg(all(target_arch = "arm", target_os = "none"))]
pub fn is_secure_mode() -> bool {
    // Look at the start of ROM, which is always readable
    #[allow(clippy::zero_ptr)]
    let rom_base: *mut u32 = 0x0000_0000 as *mut u32;
    // Use the 'tt' instruction to check the permissions for that address
    let tt = cortex_m::asm::tt(rom_base);
    // Is the secure bit set? => secure mode
    (tt & (1 << 22)) != 0
}

/// Determine if we are in secure mode
///
/// Always returns `false` on RISC-V as it is impossible to determine if
/// you are in Machine Mode or User Mode by design.
#[cfg(not(all(target_arch = "arm", target_os = "none")))]
pub fn is_secure_mode() -> bool { false }

// These and the reset_to_usb_boot function are found from https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_bootrom/bootrom.c#L35-L51
// The following has just been translated to rust from the original c++
/// BOOTSEL parameter flag meaning the activity pin index was supplied.
const BOOTSEL_FLAG_GPIO_PIN_SPECIFIED: u32 = 0x20;
/// Reboot-mode selector for the USB BOOTSEL loader.
const REBOOT2_FLAG_REBOOT_TYPE_BOOTSEL: u32 = 0x2;
/// Spin in ROM after a successful reboot request instead of returning before reset.
const REBOOT2_FLAG_NO_RETURN_ON_SUCCESS: u32 = 0x100;

/// Resets the RP235x and uses the watchdog facility to re-start in BOOTSEL mode:
///   * gpio_activity_pin_mask is provided to enable an 'activity light' via GPIO attached LED for the USB Mass Storage
///     Device:
///     * 0 No pins are used as per cold boot.
///     * Otherwise a single bit set indicating which GPIO pin should be set to output and raised whenever there is mass
///       storage activity from the host.
///  * disable_interface_mask may be used to control the exposed USB interfaces:
///    * 0 To enable both interfaces (as per cold boot).
///    * 1 To disable the USB Mass Storage Interface.
///    * 2 to Disable the USB PICOBOOT Interface.
/// Uses the lowest set bit of `usb_activity_gpio_pin_mask` (zero disables activity),
/// and `disable_interface_mask` bits 0/1 to disable MSC/PICOBOOT. Requests a 10 ms
/// watchdog reboot and ignores failure status; normally never returns after success.
/// Multi-bit activity masks are silently reduced; selected GPIO must be valid/free.
pub fn reset_to_usb_boot(
    mut usb_activity_gpio_pin_mask: u32,
    disable_interface_mask: u32,
) {
    let mut flags = disable_interface_mask;

    if usb_activity_gpio_pin_mask != 0 {
        flags = flags | BOOTSEL_FLAG_GPIO_PIN_SPECIFIED;
        usb_activity_gpio_pin_mask = usb_activity_gpio_pin_mask.trailing_zeros()
    }

    reboot(
        REBOOT2_FLAG_REBOOT_TYPE_BOOTSEL | REBOOT2_FLAG_NO_RETURN_ON_SUCCESS,
        10,
        flags,
        usb_activity_gpio_pin_mask,
    );
}

/// Compile-only ROM ABI checks; none of these tests calls silicon ROM.
#[cfg(test)]
mod abi_tests {
    /// Verifies A/B selection forwards all four SDK/ROM arguments, including update base.
    #[test]
    fn pick_ab_has_four_argument_rom_abi() {
        let _entry: unsafe extern "C" fn(*mut u8, usize, u32, u32) -> i32 = super::pick_ab_parition;
    }
}
