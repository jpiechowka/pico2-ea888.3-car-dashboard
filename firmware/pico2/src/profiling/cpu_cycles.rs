use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);
static AVAILABLE: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Whether this core's initialization observed an enabled, advancing counter.
pub fn available(core: usize) -> bool { AVAILABLE.get(core).is_some_and(|value| value.load(Ordering::Acquire)) }

/// Maximum sane cycle count for a single elapsed measurement.
/// Derived from CPU frequency: allows up to ~2 seconds of elapsed time.
/// At 300 MHz this is 600M cycles; at 150 MHz it's 300M cycles.
/// Using AtomicU32 so it adapts when init() is called with overclock frequencies.
static MAX_SANE_CYCLES: AtomicU32 = AtomicU32::new(300_000_000);

pub fn init(freq_hz: u32) {
    let max_cycles = dashboard_core::cpu_math::sanity_cycles(freq_hz).expect("configured CPU clock must be nonzero");
    CPU_FREQ_HZ.store(freq_hz, Ordering::Relaxed);
    MAX_SANE_CYCLES.store(max_cycles, Ordering::Relaxed);

    unsafe {
        use core::ptr::{read_volatile, write_volatile};

        const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
        let demcr_val = read_volatile(DEMCR);
        write_volatile(DEMCR, demcr_val | (1 << 24));

        // Use the CoreSight software-lock key as supported by cortex-m's DWT
        // helper. The bounded progress check below does not reset the counter;
        // target-bench retains its independent controls and timing checks.
        const DWT_LAR: *mut u32 = 0xE000_1FB0 as *mut u32;
        write_volatile(DWT_LAR, 0xC5AC_CE55);

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
pub fn read() -> u32 {
    unsafe {
        const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
        core::ptr::read_volatile(DWT_CYCCNT)
    }
}

#[inline]
pub fn elapsed(
    start: u32,
    end: u32,
) -> u32 {
    let elapsed = end.wrapping_sub(start);
    let max = MAX_SANE_CYCLES.load(Ordering::Relaxed);

    if elapsed > max { 0 } else { elapsed }
}

/// Calculate CPU utilization in tenths-of-percent (0–1000 → 0.0%–100.0%).
///
/// Returns 10× the percentage so callers can display one decimal place
/// without floating-point math (e.g. 163 → "16.3%").
///
/// `cycles_used`: DWT cycles consumed by work during the measurement window.
/// `elapsed_us`: duration of the measurement window in microseconds
///               (e.g. per-frame time on Core 0, or 1_000_000 for Core 1's 1-second window).
pub fn calc_util_percent(
    cycles_used: u32,
    elapsed_us: u32,
) -> u32 {
    dashboard_core::cpu_math::utilization(CPU_FREQ_HZ.load(Ordering::Relaxed), cycles_used, elapsed_us)
}
