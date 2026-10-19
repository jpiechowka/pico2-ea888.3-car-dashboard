use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);
static AVAILABLE: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Whether this core's initialization observed an enabled, advancing counter.
pub fn available(core: usize) -> bool { AVAILABLE.get(core).is_some_and(|value| value.load(Ordering::Acquire)) }

pub fn init(freq_hz: u32) {
    assert!(freq_hz > 0, "configured CPU clock must be nonzero");
    CPU_FREQ_HZ.store(freq_hz, Ordering::Relaxed);

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
    elapsed_us: u64,
) -> Option<u64> {
    dashboard_core::cpu_math::checked_segment(CPU_FREQ_HZ.load(Ordering::Relaxed), start, end, elapsed_us)
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
    cycles_used: u64,
    elapsed_us: u64,
) -> Option<u32> {
    dashboard_core::cpu_math::checked_utilization(CPU_FREQ_HZ.load(Ordering::Relaxed), cycles_used, elapsed_us)
}
