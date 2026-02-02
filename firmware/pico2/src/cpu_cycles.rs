//! CPU cycle counter utilities using Cortex-M33 DWT.
//!
//! Provides hardware-accurate CPU utilization measurement via the
//! Data Watchpoint and Trace (DWT) cycle counter (CYCCNT).
//!
//! # Overflow Handling
//!
//! The CYCCNT is a 32-bit counter that wraps at different intervals:
//! - 150 MHz: wraps every ~28.6 seconds (2^32 / 150M)
//! - 250 MHz: wraps every ~17.2 seconds (overclock profile)
//! - 280 MHz: wraps every ~15.3 seconds (spi-70mhz profile)
//!
//! This module uses `wrapping_sub` to correctly handle counter wrap
//! for elapsed time measurements within a single frame (~milliseconds).
//! For very long operations (>10s), the elapsed time will be incorrect.

use core::sync::atomic::{AtomicU32, Ordering};

/// CPU frequency in Hz (set at init based on feature flags).
/// Default to 150 MHz (stock RP2350).
static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);

/// Maximum valid cycle count for sanity checking.
/// If elapsed cycles exceed this, likely a wrap or error occurred.
/// Set to ~500ms worth of cycles at max freq (375 MHz * 0.5 = 187.5M).
const MAX_SANE_CYCLES: u32 = 200_000_000;

/// Initialize DWT cycle counter.
///
/// Must be called after embassy_rp::init() to enable cycle counting.
/// Safe to call multiple times (idempotent).
///
/// # Arguments
/// * `freq_hz` - CPU frequency in Hz (e.g., 150_000_000, 250_000_000, 375_000_000)
pub fn init(freq_hz: u32) {
    // Validate frequency is reasonable (100 MHz - 500 MHz)
    let clamped_freq = freq_hz.clamp(100_000_000, 500_000_000);
    CPU_FREQ_HZ.store(clamped_freq, Ordering::Relaxed);

    // Enable DWT cycle counter via raw register access
    // DEMCR.TRCENA (bit 24) must be set first, then DWT.CTRL.CYCCNTENA (bit 0)
    #[cfg(target_arch = "arm")]
    unsafe {
        use core::ptr::{read_volatile, write_volatile};

        // DCB DEMCR register (0xE000EDFC) - enable trace
        const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
        let demcr_val = read_volatile(DEMCR);
        write_volatile(DEMCR, demcr_val | (1 << 24)); // TRCENA bit

        // DWT CTRL register (0xE0001000) - enable cycle counter
        const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
        let ctrl_val = read_volatile(DWT_CTRL);
        write_volatile(DWT_CTRL, ctrl_val | 1); // CYCCNTENA bit
    }
}

/// Read current cycle count (32-bit, wraps).
#[inline]
pub fn read() -> u32 {
    #[cfg(target_arch = "arm")]
    unsafe {
        // DWT CYCCNT register (0xE0001004)
        const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
        core::ptr::read_volatile(DWT_CYCCNT)
    }
    #[cfg(not(target_arch = "arm"))]
    {
        0 // Placeholder for tests
    }
}

/// Calculate elapsed cycles with wrap handling and sanity check.
///
/// Uses wrapping subtraction to handle 32-bit counter overflow.
/// Returns 0 if elapsed cycles exceed sanity threshold (likely a measurement error).
#[inline]
pub fn elapsed(
    start: u32,
    end: u32,
) -> u32 {
    let elapsed = end.wrapping_sub(start);

    // Sanity check: if elapsed exceeds ~500ms worth of cycles, something is wrong
    if elapsed > MAX_SANE_CYCLES {
        0 // Return 0 to indicate measurement error
    } else {
        elapsed
    }
}

/// Get configured CPU frequency in Hz.
#[inline]
#[allow(dead_code)]
pub fn freq_hz() -> u32 { CPU_FREQ_HZ.load(Ordering::Relaxed) }

/// Calculate CPU utilization percentage from cycle counts.
///
/// Uses 64-bit arithmetic internally to avoid overflow.
///
/// # Arguments
/// * `cycles_used` - Number of CPU cycles consumed this frame
/// * `frame_time_us` - Total frame time in microseconds
///
/// # Returns
/// CPU utilization as percentage (0-100), clamped.
///
/// # Overflow Safety
/// - Uses u64 for intermediate calculations
/// - Handles 0 frame time gracefully
/// - Handles 0 cycles gracefully
/// - Clamps result to 0-100 range
pub fn calc_util_percent(
    cycles_used: u32,
    frame_time_us: u32,
) -> u32 {
    // Guard against division by zero
    if frame_time_us == 0 || cycles_used == 0 {
        return 0;
    }

    let freq = CPU_FREQ_HZ.load(Ordering::Relaxed) as u64;

    // cycles_expected = freq_hz * frame_time_us / 1_000_000
    // Use u64 to avoid overflow: max is ~375M * 1M = 375T (fits in u64)
    let cycles_expected = (freq * frame_time_us as u64) / 1_000_000;

    if cycles_expected == 0 {
        return 0;
    }

    // util = (cycles_used / cycles_expected) * 100
    let util = (cycles_used as u64 * 100) / cycles_expected;

    // Clamp to 0-100 (can exceed 100 if measurement includes interrupt time)
    util.min(100) as u32
}

// =============================================================================
// Unit Tests (run on host with: cargo test --lib --target <host-triple>)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elapsed_normal() {
        assert_eq!(elapsed(100, 200), 100);
        assert_eq!(elapsed(0, 1000), 1000);
    }

    #[test]
    fn test_elapsed_wrap() {
        // Wrap from near max to near zero
        assert_eq!(elapsed(u32::MAX - 100, 100), 201);
    }

    #[test]
    fn test_elapsed_sanity_check() {
        // Huge elapsed value should return 0
        assert_eq!(elapsed(0, MAX_SANE_CYCLES + 1), 0);
    }

    #[test]
    fn test_util_zero_inputs() {
        assert_eq!(calc_util_percent(0, 1000), 0);
        assert_eq!(calc_util_percent(1000, 0), 0);
    }

    #[test]
    fn test_util_calculation() {
        // At 250 MHz, 1ms = 250,000 cycles
        CPU_FREQ_HZ.store(250_000_000, Ordering::Relaxed);

        // 125,000 cycles in 1000us (1ms) = 50% utilization
        // cycles_expected = 250M * 1000 / 1M = 250,000
        // util = 125,000 * 100 / 250,000 = 50
        assert_eq!(calc_util_percent(125_000, 1000), 50);

        // 250,000 cycles in 1000us = 100% utilization
        assert_eq!(calc_util_percent(250_000, 1000), 100);
    }
}
