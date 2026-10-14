//! Portable arithmetic for configured DWT clocks; no hardware ownership.
pub fn sanity_cycles(freq_hz: u32) -> Option<u32> {
    if freq_hz == 0 {
        return None;
    }
    Some((u64::from(freq_hz) * 2).min(u64::from(u32::MAX)) as u32)
}

/// CPU use in tenths of percent, bounded to 0..=1000.
pub fn utilization(
    freq_hz: u32,
    cycles_used: u32,
    elapsed_us: u32,
) -> u32 {
    let expected = u64::from(freq_hz) * u64::from(elapsed_us) / 1_000_000;
    if expected == 0 {
        return 0;
    }
    (u64::from(cycles_used) * 1000 / expected).min(1000) as u32
}
