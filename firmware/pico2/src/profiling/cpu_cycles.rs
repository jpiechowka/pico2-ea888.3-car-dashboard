use core::sync::atomic::{AtomicU32, Ordering};

static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);

/// Maximum sane cycle count for a single elapsed measurement.
/// Derived from CPU frequency: allows up to ~2 seconds of elapsed time.
/// At 300 MHz this is 600M cycles; at 150 MHz it's 300M cycles.
/// Using AtomicU32 so it adapts when init() is called with overclock frequencies.
static MAX_SANE_CYCLES: AtomicU32 = AtomicU32::new(300_000_000);

pub fn init(freq_hz: u32) {
    let clamped_freq = freq_hz.clamp(100_000_000, 500_000_000);
    CPU_FREQ_HZ.store(clamped_freq, Ordering::Relaxed);

    // Allow up to 2 seconds worth of cycles as the sanity bound.
    // This ensures overclock frequencies (250-300 MHz) don't get
    // their legitimate measurements clamped to zero.
    let max_cycles = (clamped_freq as u64 * 2).min(u32::MAX as u64) as u32;
    MAX_SANE_CYCLES.store(max_cycles, Ordering::Relaxed);

    unsafe {
        use core::ptr::{read_volatile, write_volatile};

        const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
        let demcr_val = read_volatile(DEMCR);
        write_volatile(DEMCR, demcr_val | (1 << 24));

        const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
        let ctrl_val = read_volatile(DWT_CTRL);
        write_volatile(DWT_CTRL, ctrl_val | 1);
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

/// Calculate CPU utilization as a percentage.
///
/// `cycles_used`: DWT cycles consumed by work during the measurement window.
/// `elapsed_us`: duration of the measurement window in microseconds
///               (e.g. per-frame time on Core 0, or 1_000_000 for Core 1's 1-second window).
pub fn calc_util_percent(
    cycles_used: u32,
    elapsed_us: u32,
) -> u32 {
    if elapsed_us == 0 || cycles_used == 0 {
        return 0;
    }

    let freq = CPU_FREQ_HZ.load(Ordering::Relaxed) as u64;

    let cycles_expected = (freq * elapsed_us as u64) / 1_000_000;

    if cycles_expected == 0 {
        return 0;
    }

    let util = (cycles_used as u64 * 100) / cycles_expected;

    util.min(100) as u32
}
