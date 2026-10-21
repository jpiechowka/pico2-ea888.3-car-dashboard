//! Per-core DWT observations with a shared configured frequency, not clock calibration.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Configured system clock in hertz, shared by both cores' interval arithmetic.
static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);
/// Boot-time enable/progress result for cores 0 and 1; not continuous monitoring.
static AVAILABLE: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Whether this core's initialization observed an enabled, advancing counter.
/// Returns `false` for `core` outside 0..=1 or before successful initialization.
pub fn available(core: usize) -> bool { AVAILABLE.get(core).is_some_and(|value| value.load(Ordering::Acquire)) }

/// Enables this core's DWT cycle counter, stores configured `freq_hz`, and checks
/// progress across 64 NOPs without resetting CYCCNT. Panics when `freq_hz` is zero.
/// Call on each core; a later debugger change is not detected by this boot check.
pub fn init(freq_hz: u32) {
    assert!(freq_hz > 0, "configured CPU clock must be nonzero");
    CPU_FREQ_HZ.store(freq_hz, Ordering::Relaxed);

    unsafe {
        use core::ptr::{read_volatile, write_volatile};

        /// Core-local debug exception/monitor control, including trace enable.
        const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
        let demcr_val = read_volatile(DEMCR);
        write_volatile(DEMCR, demcr_val | (1 << 24));

        // Use the CoreSight software-lock key as supported by cortex-m's DWT
        // helper. The bounded progress check below does not reset the counter;
        // target-bench retains its independent controls and timing checks.
        /// CoreSight DWT software-lock access register.
        const DWT_LAR: *mut u32 = 0xE000_1FB0 as *mut u32;
        write_volatile(DWT_LAR, 0xC5AC_CE55);

        /// DWT feature/enable control register; NOCYCCNT must remain clear.
        const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
        let ctrl_val = read_volatile(DWT_CTRL);
        write_volatile(DWT_CTRL, ctrl_val | 1);
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        let start = read();
        for _ in 0..64 {
            cortex_m::asm::nop();
        }
        let end = read();
        let valid = dashboard_core::cpu_math::cycle_counter_available(
            read_volatile(DWT_CTRL),
            read_volatile(DEMCR),
            start,
            end,
        );
        let core = embassy_rp::pac::SIO.cpuid().read() as usize;
        if let Some(value) = AVAILABLE.get(core) {
            value.store(valid, Ordering::Release);
        }
    }
}

#[inline]
/// Returns this core's wrapping raw CYCCNT sample; callers must separately check
/// availability and elapsed-time bounds before interpreting a difference.
pub fn read() -> u32 {
    unsafe {
        /// Core-local 32-bit cycle counter, wrapping modulo 2^32.
        const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
        core::ptr::read_volatile(DWT_CYCCNT)
    }
}

#[inline]
/// Returns accepted cycles between raw `start` and `end` samples over
/// `elapsed_us` microseconds, or `None` when shared frequency/time/wrap checks fail.
pub fn elapsed(
    start: u32,
    end: u32,
    elapsed_us: u64,
) -> Option<u64> {
    dashboard_core::cpu_math::checked_segment(CPU_FREQ_HZ.load(Ordering::Relaxed), start, end, elapsed_us)
}

/// Calculate instrumented work in tenths-of-percent (0–1000 → 0.0%–100.0%).
///
/// Returns 10× the percentage so callers can display one decimal place
/// without floating-point math (e.g. 163 → "16.3%").
///
/// `cycles_used`: DWT cycles consumed by work during the measurement window.
/// `elapsed_us`: duration of the measurement window in microseconds
///               (Core 1 publishes after at least one second).
/// Returns `None` for invalid/unavailable arithmetic instead of a valid zero;
/// this instrumented fraction includes IRQ cycles and is not whole-core utilization.
pub fn calc_util_percent(
    cycles_used: u64,
    elapsed_us: u64,
) -> Option<u32> {
    dashboard_core::cpu_math::checked_utilization(CPU_FREQ_HZ.load(Ordering::Relaxed), cycles_used, elapsed_us)
}
