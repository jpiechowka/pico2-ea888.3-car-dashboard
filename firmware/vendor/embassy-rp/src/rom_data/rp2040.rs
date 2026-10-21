//! RP2040 ROM lookup, memory/flash helpers and raw arithmetic entry points.
//! Addresses and ABI are chip-specific; none may be called on a host or RP235x.
//! <https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf> section 2.8.
//!
//! Raw arithmetic flushes subnormals and treats NaNs as infinities; scientific
//! functions have reduced accuracy near cancellation/poles. These wrappers do
//! not provide the nonfinite policy of the separate compiler intrinsics.

// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp2040-hal/src/rom_data.rs

/// A bootrom function table code.
pub type RomFnTableCode = [u8; 2];

/// ROM lookup ABI: first argument is a ROM table, second is a little-endian tag.
/// Returns a pointer-shaped `T`; unknown tags yield null and must not be called.
type RomTableLookupFn<T> = unsafe extern "C" fn(*const u16, u32) -> T;

/// The following addresses are described at `2.8.2. Bootrom Contents`
/// Pointer to the lookup table function supplied by the rom.
const ROM_TABLE_LOOKUP_PTR: *const u16 = 0x0000_0018 as _;

/// Pointer to helper functions lookup table.
const FUNC_TABLE: *const u16 = 0x0000_0014 as _;

/// Pointer to the public data lookup table.
const DATA_TABLE: *const u16 = 0x0000_0016 as _;

/// Address of the version number of the ROM.
const VERSION_NUMBER: *const u8 = 0x0000_0013 as _;

/// Returns pointer-shaped ROM entry `T` for `tag` from the 16-bit pointer at `table`.
/// Internal callers supply known tags and valid RP2040 ROM addresses; no null check.
fn rom_table_lookup<T>(
    table: *const u16,
    tag: RomFnTableCode,
) -> T {
    unsafe {
        let rom_table_lookup_ptr: *const u32 = rom_hword_as_ptr(ROM_TABLE_LOOKUP_PTR);
        let rom_table_lookup: RomTableLookupFn<T> = core::mem::transmute(rom_table_lookup_ptr);
        rom_table_lookup(rom_hword_as_ptr(table) as *const u16, u16::from_le_bytes(tag) as u32)
    }
}

/// To save space, the ROM likes to store memory pointers (which are 32-bit on
/// the Cortex-M0+) using only the bottom 16-bits. The assumption is that the
/// values they point at live in the first 64 KiB of ROM, and the ROM is mapped
/// to address `0x0000_0000` and so 16-bits are always sufficient.
///
/// Returns the zero-extended pointer stored at `rom_address`.
/// # Safety
/// `rom_address` must be aligned, readable RP2040 ROM containing a valid 16-bit address.
unsafe fn rom_hword_as_ptr(rom_address: *const u16) -> *const u32 {
    let ptr: u16 = *rom_address;
    ptr as *const u32
}

/// Defines a typed ROM wrapper and pointer accessor. `$lookup` must return the
/// same immutable function with exactly the declared ABI/signature on every call;
/// atomic caching permits redundant first lookups across cores and IRQs.
macro_rules! declare_rom_function {
    (
        $(#[$outer:meta])*
        fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty
        $lookup:block
    ) => {
        declare_rom_function!{
            __internal ,
            $(#[$outer])*
            fn $name( $($argname: $ty),* ) -> $ret
            $lookup
        }
    };

    (
        $(#[$outer:meta])*
        unsafe fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty
        $lookup:block
    ) => {
        declare_rom_function!{
            __internal unsafe ,
            $(#[$outer])*
            fn $name( $($argname: $ty),* ) -> $ret
            $lookup
        }
    };

    (
        __internal
        $( $maybe_unsafe:ident )? ,
        $(#[$outer:meta])*
        fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty
        $lookup:block
    ) => {
        #[doc = r"Additional access for the `"]
        #[doc = stringify!($name)]
        #[doc = r"` ROM function."]
        pub mod $name {
            /// Returns the resolved ROM function pointer; repeats lookup on each call.
            #[cfg(not(feature = "rom-func-cache"))]
            pub(crate) fn outer_call() -> $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret {
                let p: *const u32 = $lookup;
                unsafe {
                    let func : $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret
                        = core::mem::transmute(p);
                    func
                }
            }

            /// Returns the resolved ROM function pointer; ROM-version checks may panic.
            #[cfg(not(feature = "rom-func-cache"))]
            pub fn ptr() -> $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret {
                outer_call()
            }

            /// Lazily resolved function address; atomic accesses permit concurrent core/IRQ use.
            #[cfg(feature = "rom-func-cache")]
            static CACHE: core::sync::atomic::AtomicPtr<()> =
                core::sync::atomic::AtomicPtr::new(trampoline as *mut ());

            #[cfg(feature = "rom-func-cache")]
            $(#[$outer])*
            $( $maybe_unsafe )? extern "C" fn trampoline( $($argname: $ty),* ) -> $ret {
                let p: *const u32 = $lookup;
                CACHE.store(p as *mut (), core::sync::atomic::Ordering::Release);
                #[allow(unused_unsafe)]
                unsafe {
                    let function: $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret
                        = core::mem::transmute(p);
                    function($($argname),*)
                }
            }

            /// Returns the cached target or lazy resolver; calling retains the ROM function contract.
            #[cfg(feature = "rom-func-cache")]
            pub(crate) fn outer_call() -> $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret {
                let p = CACHE.load(core::sync::atomic::Ordering::Acquire);
                // Concurrent first calls may resolve twice, but all accesses are atomic and
                // lookup always returns the same immutable ROM entry.
                unsafe { core::mem::transmute(p) }
            }

            /// Returns the actual resolved ROM function address, never a flash-resident trampoline.
            /// Resolves lazily; ROM-version lookup may panic before any function call.
            #[cfg(feature = "rom-func-cache")]
            pub fn ptr() -> $( $maybe_unsafe )? extern "C" fn( $($argname: $ty),* ) -> $ret {
                let mut p = CACHE.load(core::sync::atomic::Ordering::Acquire);
                if p == trampoline as *mut () {
                    let resolved: *const u32 = $lookup;
                    p = resolved as *mut ();
                    CACHE.store(p, core::sync::atomic::Ordering::Release);
                }
                unsafe { core::mem::transmute(p) }
            }
        }

        $(#[$outer])*
        pub $( $maybe_unsafe )? extern "C" fn $name( $($argname: $ty),* ) -> $ret {
            $name::outer_call()($($argname),*)
        }
    };
}

/// Converts two-byte ROM function tags into documented typed wrappers.
macro_rules! rom_functions {
    () => {};

    (
        $(#[$outer:meta])*
        $c:literal fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty;

        $($rest:tt)*
    ) => {
        declare_rom_function! {
            $(#[$outer])*
            fn $name( $($argname: $ty),* ) -> $ret {
                $crate::rom_data::inner::rom_table_lookup($crate::rom_data::inner::FUNC_TABLE, *$c)
            }
        }

        rom_functions!($($rest)*);
    };

    (
        $(#[$outer:meta])*
        $c:literal unsafe fn $name:ident( $($argname:ident: $ty:ty),* ) -> $ret:ty;

        $($rest:tt)*
    ) => {
        declare_rom_function! {
            $(#[$outer])*
            unsafe fn $name( $($argname: $ty),* ) -> $ret {
                $crate::rom_data::inner::rom_table_lookup($crate::rom_data::inner::FUNC_TABLE, *$c)
            }
        }

        rom_functions!($($rest)*);
    };
}

rom_functions! {
    /// Returns the number of one bits in `value`, in 0..=32.
    b"P3" fn popcount32(value: u32) -> u32;

    /// Returns `value` with all 32 bit positions reversed.
    b"R3" fn reverse32(value: u32) -> u32;

    /// Returns leading-zero count of `value`; zero returns 32.
    b"L3" fn clz32(value: u32) -> u32;

    /// Returns trailing-zero count of `value`; zero returns 32.
    b"T3" fn ctz32(value: u32) -> u32;

    /// Resets the RP2040 and uses the watchdog facility to re-start in BOOTSEL mode:
    ///   * `gpio_activity_pin_mask` is provided to enable an 'activity light' via GPIO attached LED
    ///     for the USB Mass Storage Device:
    ///     * 0 No pins are used as per cold boot.
    ///     * Otherwise a single bit set indicating which GPIO pin should be set to output and
    ///       raised whenever there is mass storage activity from the host.
    ///  * `disable_interface_mask` may be used to control the exposed USB interfaces:
    ///    * 0 To enable both interfaces (as per cold boot).
    ///    * 1 To disable the USB Mass Storage Interface.
    ///    * 2 to Disable the USB PICOBOOT Interface.
    b"UB" fn reset_to_usb_boot(gpio_activity_pin_mask: u32, disable_interface_mask: u32) -> ();

    /// Sets n bytes start at ptr to the value c and returns ptr
    /// Writes `n` bytes of `c` at `ptr`; returns `ptr`.
    /// # Safety
    /// `ptr` must be writable/exclusively owned for `n` bytes, without address wrap.
    b"MS" unsafe fn memset(ptr: *mut u8, c: u8, n: u32) -> *mut u8;

    /// Sets n bytes start at ptr to the value c and returns ptr.
    ///
    /// Note this is a slightly more efficient variant of _memset that may only
    /// be used if ptr is word aligned.
    // Note the datasheet does not match the actual ROM for the code here, see
    // https://github.com/raspberrypi/pico-feedback/issues/217
    /// Writes `n` bytes of `c` at word-aligned `ptr`; returns `ptr`.
    /// # Safety
    /// `ptr` must be four-byte aligned and writable/exclusively owned for `n` bytes, without address wrap.
    b"S4" unsafe fn memset4(ptr: *mut u32, c: u8, n: u32) -> *mut u32;

    /// Copies n bytes starting at src to dest and returns dest. The results are undefined if the
    /// regions overlap.
    /// Copies `n` bytes from `src` to `dest`; returns `dest`.
    /// # Safety
    /// Spans must be valid, nonoverlapping and nonwrapping; `dest` must be exclusively
    /// writable and `src` remain readable/unmodified.
    b"MC" unsafe fn memcpy(dest: *mut u8, src: *const u8, n: u32) -> *mut u8;

    /// Copies n bytes starting at src to dest and returns dest. The results are undefined if the
    /// regions overlap.
    ///
    /// Note this is a slightly more efficient variant of _memcpy that may only be
    /// used if dest and src are word aligned.
    /// Copies `n` bytes from word-aligned `src` to word-aligned `dest`; returns `dest` as bytes.
    /// # Safety
    /// Both pointers must be four-byte aligned, valid for `n` bytes, nonoverlapping
    /// and nonwrapping; destination is exclusively writable and source immutable.
    b"C4" unsafe fn memcpy44(dest: *mut u32, src: *const u32, n: u32) -> *mut u8;

    /// Restore all QSPI pad controls to their default state, and connect the SSI to the QSPI pads.
    /// # Safety
    /// Run from RAM with QSPI exclusively owned and all cores/IRQs/DMA prevented from XIP access.
    b"IF" unsafe fn connect_internal_flash() -> ();

    /// First set up the SSI for serial-mode operations, then issue the fixed XIP exit sequence.
    ///
    /// Note that the bootrom code uses the IO forcing logic to drive the CS pin, which must be
    /// cleared before returning the SSI to XIP mode (e.g. by a call to _flash_flush_cache). This
    /// function configures the SSI with a fixed SCK clock divisor of /6.
    /// # Safety
    /// Run from RAM with all XIP users quiescent; restore cache/chip-select and XIP
    /// configuration before allowing them to resume.
    b"EX" unsafe fn flash_exit_xip() -> ();

    /// Erase a count bytes, starting at addr (offset from start of flash). Optionally, pass a
    /// block erase command e.g. D8h block erase, and the size of the block erased by this
    /// command — this function will use the larger block erase where possible, for much higher
    /// erase speed. addr must be aligned to a 4096-byte sector, and count must be a multiple of
    /// 4096 bytes.
    /// # Safety
    /// `addr` is a flash byte offset aligned to 4096; `count` is a multiple of 4096
    /// within the device. `block_size`/`block_cmd` must describe a supported erase
    /// opcode. Execute from RAM after exiting XIP, with other bus users excluded.
    /// Busy polling has no timeout or returned error.
    b"RE" unsafe fn flash_range_erase(addr: u32, count: usize, block_size: u32, block_cmd: u8) -> ();

    /// Program data to a range of flash addresses starting at `addr` (and
    /// offset from the start of flash) and `count` bytes in size. The value
    /// `addr` must be aligned to a 256-byte boundary, and `count` must be a
    /// multiple of 256.
    /// # Safety
    /// `data` must remain readable from RAM for `count` bytes, with valid erased flash
    /// range at `addr` (address/count multiples of 256). Exclude all XIP users and
    /// execute from RAM after exiting XIP; no timeout or failure result.
    b"RP" unsafe fn flash_range_program(addr: u32, data: *const u8, count: usize) -> ();

    /// Flush and enable the XIP cache. Also clears the IO forcing on QSPI CSn, so that the SSI can
    /// drive the flashchip select as normal.
    /// # Safety
    /// Run from RAM with exclusive QSPI/XIP ownership; this alone does not restore
    /// a device-specific XIP read mode.
    b"FC" unsafe fn flash_flush_cache() -> ();

    /// Configure the SSI to generate a standard 03h serial read command, with 24 address bits,
    /// upon each XIP access. This is a very slow XIP configuration, but is very widely supported.
    /// The debugger calls this function after performing a flash erase/programming operation, so
    /// that the freshly-programmed code and data is visible to the debug host, without having to
    /// know exactly what kind of flash device is connected.
    /// # Safety
    /// Run from RAM with exclusive QSPI/XIP ownership and a compatible flash device;
    /// restore cache/CS state first.
    b"CX" unsafe fn flash_enter_cmd_xip() -> ();

    /// This is the method that is entered by core 1 on reset to wait to be launched by core 0.
    /// There are few cases where you should call this method (resetting core 1 is much better).
    /// This method does not return and should only ever be called on core 1.
    /// # Safety
    /// Call only on core1 after abandoning its current execution/resources and
    /// coordinating the FIFO launch protocol with core0.
    b"WV" unsafe fn wait_for_vector() -> !;
}

// Various C intrinsics in the ROM
intrinsics! {
    /// Returns the number of one bits in 32-bit `x`; no 64-bit DI alias is exported.
    extern "C" fn __popcountsi2(x: u32) -> u32 {
        popcount32(x)
    }

    /// Returns leading-zero count of 32-bit `x`, including 32 for zero.
    extern "C" fn __clzsi2(x: u32) -> u32 {
        clz32(x)
    }

    /// Returns trailing-zero count of 32-bit `x`, including 32 for zero.
    extern "C" fn __ctzsi2(x: u32) -> u32 {
        ctz32(x)
    }

    // __rbit is only unofficial, but it show up in the ARM documentation,
    // so may as well hook it up.
    #[alias = __rbitl]
    /// Returns all 32 bits of `x` reversed.
    extern "C" fn __rbit(x: u32) -> u32 {
        reverse32(x)
    }

    /// Fills `n` bytes at `dest` with the low byte of `c`.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memset(dest: *mut u8, n: usize, c: i32) -> () {
        // Different argument order
        memset(dest, c as u8, n as u32);
    }

    #[alias = __aeabi_memset8]
    /// Fills `n` bytes at four-byte-aligned `dest` with the low byte of `c`.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memset4(dest: *mut u8, n: usize, c: i32) -> () {
        // Different argument order
        memset4(dest as *mut u32, c as u8, n as u32);
    }

    /// Fills `n` bytes at `dest` with zero.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memclr(dest: *mut u8, n: usize) -> () {
        memset(dest, 0, n as u32);
    }

    #[alias = __aeabi_memclr8]
    /// Fills `n` bytes at four-byte-aligned `dest` with zero.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memclr4(dest: *mut u8, n: usize) -> () {
        memset4(dest as *mut u32, 0, n as u32);
    }

    /// Copies `n` bytes from `src` to `dest`, without overlap.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memcpy(dest: *mut u8, src: *const u8, n: usize) -> () {
        memcpy(dest, src, n as u32);
    }

    #[alias = __aeabi_memcpy8]
    /// Copies `n` bytes from four-byte-aligned `src` to four-byte-aligned `dest`, without overlap.
    /// # Safety
    /// Pointers must be valid for `n` bytes with stated alignment and exclusive destination
    /// access; copy source must remain immutable. Spans must not wrap; `n` must fit u32.
    unsafe extern "aapcs" fn __aeabi_memcpy4(dest: *mut u8, src: *const u8, n: usize) -> () {
        memcpy44(dest as *mut u32, src as *const u32, n as u32);
    }
}

/// Returns the NUL-terminated UTF-8 string beginning at `s`, excluding its terminator.
/// # Safety
/// `s` must reference immutable static memory containing a reachable NUL, with
/// valid UTF-8 before it; scanning and pointer subtraction must remain in one allocation.
unsafe fn convert_str(s: *const u8) -> &'static str {
    let mut end = s;
    while *end != 0 {
        end = end.add(1);
    }
    let s = core::slice::from_raw_parts(s, end.offset_from(s) as usize);
    core::str::from_utf8_unchecked(s)
}

/// Returns the RP2040 ROM revision byte directly from its fixed ROM address.
pub fn rom_version_number() -> u8 { unsafe { *VERSION_NUMBER } }

/// Returns the ROM-owned copyright string; assumes its documented NUL/UTF-8 encoding.
pub fn copyright_string() -> &'static str {
    let s: *const u8 = rom_table_lookup(DATA_TABLE, *b"CR");
    unsafe { convert_str(s) }
}

/// Returns the ROM's 32-bit Git revision prefix, not a complete commit identity.
pub fn git_revision() -> u32 {
    let s: *const u32 = rom_table_lookup(DATA_TABLE, *b"GR");
    unsafe { *s }
}

/// Returns the start pointer of the contiguous ROM floating-point code/data region.
///
/// This and fplib_end along with the individual function pointers in
/// soft_float_table can be used to copy the floating point implementation into
/// RAM if desired.
pub fn fplib_start() -> *const u8 { rom_table_lookup(DATA_TABLE, *b"FS") }

/// Returns the single-precision function-pointer table; entries from byte 0x54 require ROM v2.
#[cfg_attr(feature = "rom-func-cache", inline(never))]
pub fn soft_float_table() -> *const usize { rom_table_lookup(DATA_TABLE, *b"SF") }

/// Returns the end pointer of the ROM floating-point code/data region.
pub fn fplib_end() -> *const u8 { rom_table_lookup(DATA_TABLE, *b"FE") }

/// Returns the double-precision function-pointer table; panics when ROM revision is below two.
#[cfg_attr(feature = "rom-func-cache", inline(never))]
pub fn soft_double_table() -> *const usize {
    if rom_version_number() < 2 {
        panic!(
            "Double precision operations require V2 bootrom (found: V{})",
            rom_version_number()
        );
    }
    rom_table_lookup(DATA_TABLE, *b"SD")
}

/// ROM functions using single-precision arithmetic (i.e. 'f32' in Rust terms)
pub mod float_funcs {

    /// Defines raw single-precision entries from byte offsets in the ROM pointer table.
    macro_rules! make_functions {
        (
            $(
                $(#[$outer:meta])*
                $offset:literal $name:ident (
                    $( $aname:ident : $aty:ty ),*
                ) -> $ret:ty;
            )*
        ) => {
            $(
                declare_rom_function! {
                    $(#[$outer])*
                    fn $name( $( $aname : $aty ),* ) -> $ret {
                        let table: *const usize = $crate::rom_data::soft_float_table();
                        unsafe {
                            // This is the entry in the table. Our offset is given as a
                            // byte offset, but we want the table index (each pointer in
                            // the table is 4 bytes long)
                            let entry: *const usize = table.offset($offset / 4);
                            // Read the pointer from the table
                            core::ptr::read(entry) as *const u32
                        }
                    }
                }
            )*
        }
    }

    make_functions! {
        /// Returns the ROM approximation to `a + b`; `a` and `b` follow the module's non-IEEE rules.
        0x00 fadd(a: f32, b: f32) -> f32;
        /// Returns the ROM approximation to `a - b`; `a` and `b` follow the module's non-IEEE rules.
        0x04 fsub(a: f32, b: f32) -> f32;
        /// Returns the ROM approximation to `a * b`; `a` and `b` follow the module's non-IEEE rules.
        0x08 fmul(a: f32, b: f32) -> f32;
        /// Returns the ROM approximation to `a / b`; `a` and `b` follow the module's non-IEEE rules.
        0x0c fdiv(a: f32, b: f32) -> f32;

        // 0x10 and 0x14 are deprecated

        /// Returns the ROM square root of `v`; negative input yields infinity (positive in ROM v1, negative in v2).
        0x18 fsqrt(v: f32) -> f32;
        /// Returns `v` rounded toward negative infinity and saturated to the `i32` range. `v` is the source value.
        0x1c float_to_int(v: f32) -> i32;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `i32` range. `n` is the output binary-point position; `v` is the source value.
        0x20 float_to_fix(v: f32, n: i32) -> i32;
        /// Returns `v` rounded toward negative infinity and saturated to the `u32` range. `v` is the source value.
        0x24 float_to_uint(v: f32) -> u32;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `u32` range. `n` is the output binary-point position; `v` is the source value.
        0x28 float_to_ufix(v: f32, n: i32) -> u32;
        /// Returns the nearest `f32` representation of integer `v`, with ties to even.
        0x2c int_to_float(v: i32) -> f32;
        /// Returns `f32` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even.
        0x30 fix_to_float(v: i32, n: i32) -> f32;
        /// Returns the nearest `f32` representation of integer `v`, with ties to even.
        0x34 uint_to_float(v: u32) -> f32;
        /// Returns `f32` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even.
        0x38 ufix_to_float(v: u32, n: i32) -> f32;
        /// Returns the ROM cos of `angle` in radians; documented accuracy requires `abs(angle) < 128`.
        0x3c fcos(angle: f32) -> f32;
        /// Returns the ROM sin of `angle` in radians; documented accuracy requires `abs(angle) < 128`.
        0x40 fsin(angle: f32) -> f32;
        /// Returns the ROM tan of `angle` in radians; documented accuracy requires `abs(angle) < 128`.
        0x44 ftan(angle: f32) -> f32;

        // 0x48 is deprecated

        /// Returns the ROM approximation to `exp(v)` for the dimensionless exponent `v`.
        0x4c fexp(v: f32) -> f32;
        /// Returns the ROM natural logarithm of `v`; nonpositive input yields negative infinity.
        0x50 fln(v: f32) -> f32;
    }

    /// Defines entries guarded by ROM revision >=2; older devices panic during lookup.
    macro_rules! make_functions_v2 {
        (
            $(
                $(#[$outer:meta])*
                $offset:literal $name:ident (
                    $( $aname:ident : $aty:ty ),*
                ) -> $ret:ty;
            )*
        ) => {
            $(
                declare_rom_function! {
                    $(#[$outer])*
                    fn $name( $( $aname : $aty ),* ) -> $ret {
                        if $crate::rom_data::rom_version_number() < 2 {
                            panic!(
                                "Floating point function requires V2 bootrom (found: V{})",
                                $crate::rom_data::rom_version_number()
                            );
                        }
                        let table: *const usize = $crate::rom_data::soft_float_table();
                        unsafe {
                            // This is the entry in the table. Our offset is given as a
                            // byte offset, but we want the table index (each pointer in
                            // the table is 4 bytes long)
                            let entry: *const usize = table.offset($offset / 4);
                            // Read the pointer from the table
                            core::ptr::read(entry) as *const u32
                        }
                    }
                }
            )*
        }
    }

    // These are only on BootROM v2 or higher
    make_functions_v2! {
        /// Returns -1, 0 or 1 for `a` less than, equal to or greater than `b`, under the module's ROM rules. Panics on ROM revisions below two.
        0x54 fcmp(a: f32, b: f32) -> i32;
        /// Returns the ROM angle in radians for Cartesian coordinates `y` and `x`, preserving their quadrant. Panics on ROM revisions below two.
        0x58 fatan2(y: f32, x: f32) -> f32;
        /// Returns the nearest `f32` representation of integer `v`, with ties to even. Panics on ROM revisions below two.
        0x5c int64_to_float(v: i64) -> f32;
        /// Returns `f32` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even. Panics on ROM revisions below two.
        0x60 fix64_to_float(v: i64, n: i32) -> f32;
        /// Returns the nearest `f32` representation of integer `v`, with ties to even. Panics on ROM revisions below two.
        0x64 uint64_to_float(v: u64) -> f32;
        /// Returns `f32` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even. Panics on ROM revisions below two.
        0x68 ufix64_to_float(v: u64, n: i32) -> f32;
        /// Returns `v` rounded toward negative infinity and saturated to the `i64` range. `v` is the source value. Panics on ROM revisions below two.
        0x6c float_to_int64(v: f32) -> i64;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `i64` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x70 float_to_fix64(v: f32, n: i32) -> i64;
        /// Returns `v` rounded toward negative infinity and saturated to the `u64` range. `v` is the source value. Panics on ROM revisions below two.
        0x74 float_to_uint64(v: f32) -> u64;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `u64` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x78 float_to_ufix64(v: f32, n: i32) -> u64;
        /// Returns `v` converted to `f64` under the module's ROM rounding and non-IEEE rules. Panics on ROM revisions below two.
        0x7c float_to_double(v: f32) -> f64;
    }
}

/// Functions using double-precision arithmetic (i.e. 'f64' in Rust terms)
pub mod double_funcs {

    /// Defines double entries; table lookup panics on ROM revisions below two.
    macro_rules! make_double_funcs {
        (
            $(
                $(#[$outer:meta])*
                $offset:literal $name:ident (
                    $( $aname:ident : $aty:ty ),*
                ) -> $ret:ty;
            )*
        ) => {
            $(
                declare_rom_function! {
                    $(#[$outer])*
                    fn $name( $( $aname : $aty ),* ) -> $ret {
                        let table: *const usize = $crate::rom_data::soft_double_table();
                        unsafe {
                            // This is the entry in the table. Our offset is given as a
                            // byte offset, but we want the table index (each pointer in
                            // the table is 4 bytes long)
                            let entry: *const usize = table.offset($offset / 4);
                            // Read the pointer from the table
                            core::ptr::read(entry) as *const u32
                        }
                    }
                }
            )*
        }
    }

    make_double_funcs! {
        /// Returns the ROM approximation to `a + b`; `a` and `b` follow the module's non-IEEE rules. Panics on ROM revisions below two.
        0x00 dadd(a: f64, b: f64) -> f64;
        /// Returns the ROM approximation to `a - b`; `a` and `b` follow the module's non-IEEE rules. Panics on ROM revisions below two.
        0x04 dsub(a: f64, b: f64) -> f64;
        /// Returns the ROM approximation to `a * b`; `a` and `b` follow the module's non-IEEE rules. Panics on ROM revisions below two.
        0x08 dmul(a: f64, b: f64) -> f64;
        /// Returns the ROM approximation to `a / b`; `a` and `b` follow the module's non-IEEE rules. Panics on ROM revisions below two.
        0x0c ddiv(a: f64, b: f64) -> f64;

        // 0x10 and 0x14 are deprecated

        /// Returns the ROM square root of `v`; negative input yields infinity (positive in ROM v1, negative in v2). Panics on ROM revisions below two.
        0x18 dsqrt(v: f64) -> f64;
        /// Returns `v` rounded toward negative infinity and saturated to the `i32` range. `v` is the source value. Panics on ROM revisions below two.
        0x1c double_to_int(v: f64) -> i32;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `i32` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x20 double_to_fix(v: f64, n: i32) -> i32;
        /// Returns `v` rounded toward negative infinity and saturated to the `u32` range. `v` is the source value. Panics on ROM revisions below two.
        0x24 double_to_uint(v: f64) -> u32;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `u32` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x28 double_to_ufix(v: f64, n: i32) -> u32;
        /// Returns the nearest `f64` representation of integer `v`, with ties to even.
        0x2c int_to_double(v: i32) -> f64;
        /// Returns `f64` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even.
        0x30 fix_to_double(v: i32, n: i32) -> f64;
        /// Returns the nearest `f64` representation of integer `v`, with ties to even.
        0x34 uint_to_double(v: u32) -> f64;
        /// Returns `f64` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even.
        0x38 ufix_to_double(v: u32, n: i32) -> f64;
        /// Returns the ROM cos of `angle` in radians; documented accuracy requires `abs(angle) < 1024`. Panics on ROM revisions below two.
        0x3c dcos(angle: f64) -> f64;
        /// Returns the ROM sin of `angle` in radians; documented accuracy requires `abs(angle) < 1024`. Panics on ROM revisions below two.
        0x40 dsin(angle: f64) -> f64;
        /// Returns the ROM tan of `angle` in radians; documented accuracy requires `abs(angle) < 1024`. Panics on ROM revisions below two.
        0x44 dtan(angle: f64) -> f64;

        // 0x48 is deprecated

        /// Returns the ROM approximation to `exp(v)` for the dimensionless exponent `v`. Panics on ROM revisions below two.
        0x4c dexp(v: f64) -> f64;
        /// Returns the ROM natural logarithm of `v`; nonpositive input yields negative infinity. Panics on ROM revisions below two.
        0x50 dln(v: f64) -> f64;

        // These are only on BootROM v2 or higher

        /// Returns -1, 0 or 1 for `a` less than, equal to or greater than `b`, under the module's ROM rules. Panics on ROM revisions below two.
        0x54 dcmp(a: f64, b: f64) -> i32;
        /// Returns the ROM angle in radians for Cartesian coordinates `y` and `x`, preserving their quadrant. Panics on ROM revisions below two.
        0x58 datan2(y: f64, x: f64) -> f64;
        /// Returns the nearest `f64` representation of integer `v`, with ties to even. Panics on ROM revisions below two.
        0x5c int64_to_double(v: i64) -> f64;
        /// Returns `f64` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even. Panics on ROM revisions below two.
        0x60 fix64_to_doubl(v: i64, n: i32) -> f64;
        /// Returns the nearest `f64` representation of integer `v`, with ties to even. Panics on ROM revisions below two.
        0x64 uint64_to_double(v: u64) -> f64;
        /// Returns `f64` nearest to the fixed-point value `v / 2^n`; `n` is its binary-point position, with ties to even. Panics on ROM revisions below two.
        0x68 ufix64_to_double(v: u64, n: i32) -> f64;
        /// Returns `v` rounded toward negative infinity and saturated to the `i64` range. `v` is the source value. Panics on ROM revisions below two.
        0x6c double_to_int64(v: f64) -> i64;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `i64` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x70 double_to_fix64(v: f64, n: i32) -> i64;
        /// Returns `v` rounded toward negative infinity and saturated to the `u64` range. `v` is the source value. Panics on ROM revisions below two.
        0x74 double_to_uint64(v: f64) -> u64;
        /// Returns `v * 2^n` rounded toward negative infinity and saturated to the `u64` range. `n` is the output binary-point position; `v` is the source value. Panics on ROM revisions below two.
        0x78 double_to_ufix64(v: f64, n: i32) -> u64;
        /// Returns `v` converted to `f32` under the module's ROM rounding and non-IEEE rules. Panics on ROM revisions below two.
        0x7c double_to_float(v: f64) -> f32;
    }
}

/// Native macro tests resolve a local function, never RP2040 ROM addresses.
#[cfg(all(test, feature = "rom-func-cache", not(target_arch = "arm")))]
mod cache_tests {
    extern crate std;
    use core::sync::atomic::{AtomicUsize, Ordering};

    /// Number of lazy resolver calls, including redundant resolutions.
    static LOOKUPS: AtomicUsize = AtomicUsize::new(0);

    /// Returns a recognizable transformation of `value` without hardware access.
    extern "C" fn fake_rom(value: u32) -> u32 { value ^ 0x1234_5678 }

    declare_rom_function! {
        /// Returns a test-ROM transformation of `value` through the real wrapper macro.
        fn cached_test(value: u32) -> u32 {
            super::LOOKUPS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            super::fake_rom as *const u32
        }
    }

    /// Pointer lookup must resolve the target once and remain callable across threads.
    #[test]
    fn resolved_pointer_and_calls_share_atomic_cache() {
        assert_eq!(cached_test(7), fake_rom(7));
        let first = cached_test::ptr();
        let second = cached_test::ptr();
        assert_eq!(first as usize, fake_rom as *const () as usize);
        assert_eq!(second as usize, first as usize);
        assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
        std::thread::scope(|scope| {
            for worker in 0..4 {
                scope.spawn(move || {
                    for value in 0..1000 {
                        assert_eq!(cached_test(value + worker), fake_rom(value + worker));
                        assert_eq!(cached_test::ptr()(value), fake_rom(value));
                    }
                });
            }
        });
        assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
    }
}

/// Compile-time checks of ABI return widths, without calling ROM.
#[cfg(test)]
mod abi_tests {
    /// The ROM entry returns a signed integer in r0/r1, not a float in r0.
    #[test]
    fn fixed64_conversion_signature_matches_rom() {
        let _: extern "C" fn(f32, i32) -> i64 = super::float_funcs::float_to_fix64;
    }
}
