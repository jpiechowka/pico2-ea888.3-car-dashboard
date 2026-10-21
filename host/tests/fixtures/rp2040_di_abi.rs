//! Link-only ARMv6-M fixture for retained HAL integer intrinsic symbol widths.
//!
//! Link this object before the RP2040 HAL, core and compiler-builtins archives,
//! in that order. The SI call selects the HAL implementation; all three DI calls
//! must resolve from compiler-builtins. Inspect the link map and disassembly;
//! this image has no vector table and must never be flashed or executed.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

unsafe extern "C" {
    /// Returns the number of set bits in the 32-bit `value`.
    fn __popcountsi2(value: u32) -> u32;
    /// Returns the number of set bits in the full 64-bit `value`.
    fn __popcountdi2(value: u64) -> i32;
    /// Returns leading zero bits in nonzero 64-bit `value`; zero is ABI-undefined.
    fn __clzdi2(value: u64) -> i32;
    /// Returns trailing zero bits in nonzero 64-bit `value`; zero is ABI-undefined.
    fn __ctzdi2(value: u64) -> i32;
}

/// Returns a bounded sum of SI/DI bit counts for nonzero `value`.
///
/// This exported linker root retains actual calls with both ARM argument registers.
/// It is a link fixture, not a board entry point.
///
/// # Safety
///
/// `value` must be nonzero, as required by the CLZ/CTZ runtime ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(value: u64) -> i32 {
    unsafe { __popcountsi2(value as u32) as i32 + __popcountdi2(value) + __clzdi2(value) + __ctzdi2(value) }
}

/// Discards panic metadata `_info` and never returns; only satisfies no-std linking.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! { loop {} }
