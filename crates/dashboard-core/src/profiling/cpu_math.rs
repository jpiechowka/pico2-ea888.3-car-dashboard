//! Portable arithmetic for configured DWT clocks; no hardware ownership.
/// Initialization evidence only; this does not calibrate the physical clock.
pub fn cycle_counter_available(
    control: u32,
    demcr: u32,
    start: u32,
    end: u32,
) -> bool {
    control & (1 << 25) == 0 && control & 1 != 0 && demcr & (1 << 24) != 0 && end != start
}

pub fn sanity_cycles(freq_hz: u32) -> Option<u32> {
    if freq_hz == 0 {
        return None;
    }
    Some((u64::from(freq_hz) * 2).min(u64::from(u32::MAX)) as u32)
}

/// The wall-time guard prevents an entire CYCCNT revolution being mistaken
/// for a short valid segment. Counter availability is checked by the caller.
pub fn checked_segment(
    freq_hz: u32,
    start: u32,
    end: u32,
    elapsed_us: u64,
) -> Option<u64> {
    let maximum = sanity_cycles(freq_hz)?;
    let cycles = end.wrapping_sub(start);
    (elapsed_us <= 2_000_000 && cycles <= maximum).then_some(u64::from(cycles))
}

pub fn checked_utilization(
    freq_hz: u32,
    cycles: u64,
    elapsed_us: u64,
) -> Option<u32> {
    let expected = u64::from(freq_hz).checked_mul(elapsed_us)? / 1_000_000;
    if expected == 0 {
        return None;
    }
    Some((cycles.checked_mul(1000)? / expected).min(1000) as u32)
}

/// A rejected segment invalidates the complete window, never becoming idle.
pub struct CycleWindow(Option<u64>);
impl Default for CycleWindow {
    fn default() -> Self { Self::new() }
}
impl CycleWindow {
    pub const fn new() -> Self { Self(Some(0)) }

    pub fn record(
        &mut self,
        cycles: Option<u64>,
    ) {
        self.0 = self.0.zip(cycles).and_then(|(total, next)| total.checked_add(next));
    }

    pub fn finish(
        &mut self,
        freq_hz: u32,
        elapsed_us: u64,
    ) -> Option<u32> {
        let cycles = self.0.take();
        self.0 = Some(0);
        checked_utilization(freq_hz, cycles?, elapsed_us)
    }
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
