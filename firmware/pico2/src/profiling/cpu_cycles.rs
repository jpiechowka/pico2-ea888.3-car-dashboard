use core::sync::atomic::{AtomicU32, Ordering};

static CPU_FREQ_HZ: AtomicU32 = AtomicU32::new(150_000_000);

const MAX_SANE_CYCLES: u32 = 200_000_000;

pub fn init(freq_hz: u32) {
    let clamped_freq = freq_hz.clamp(100_000_000, 500_000_000);
    CPU_FREQ_HZ.store(clamped_freq, Ordering::Relaxed);

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

    if elapsed > MAX_SANE_CYCLES { 0 } else { elapsed }
}

pub fn calc_util_percent(
    cycles_used: u32,
    frame_time_us: u32,
) -> u32 {
    if frame_time_us == 0 || cycles_used == 0 {
        return 0;
    }

    let freq = CPU_FREQ_HZ.load(Ordering::Relaxed) as u64;

    let cycles_expected = (freq * frame_time_us as u64) / 1_000_000;

    if cycles_expected == 0 {
        return 0;
    }

    let util = (cycles_used as u64 * 100) / cycles_expected;

    util.min(100) as u32
}
