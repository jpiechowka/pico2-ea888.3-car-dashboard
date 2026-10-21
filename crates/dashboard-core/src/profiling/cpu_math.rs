//! Portable arithmetic for configured DWT clocks; no hardware ownership.
/// Initialization evidence only; this does not calibrate the physical clock.
/// Returns true if `control` (DWT CTRL) has CYCCNTENA and lacks NOCYCCNT,
/// `demcr` enables trace, and raw `start`/`end` CYCCNT samples differ. Later
/// disable/reset by a debugger is not detected. See the
/// [Arm DWT register model](https://arm-software.github.io/CMSIS_6/v6.0.0/Core/structDWT__Type.html).
pub fn cycle_counter_available(
    control: u32,
    demcr: u32,
    start: u32,
    end: u32,
) -> bool {
    control & (1 << 25) == 0 && control & 1 != 0 && demcr & (1 << 24) != 0 && end != start
}

/// Returns the two-second cycle bound for configured `freq_hz`, saturated at
/// `u32::MAX`, or `None` for unknown/zero frequency. This is not clock measurement.
pub fn sanity_cycles(freq_hz: u32) -> Option<u32> {
    if freq_hz == 0 {
        return None;
    }
    Some((u64::from(freq_hz) * 2).min(u64::from(u32::MAX)) as u32)
}

/// Returns the wrapping cycle difference from `start` to `end` at configured
/// `freq_hz`, or `None` for zero frequency, `elapsed_us` >2 seconds or a delta
/// exceeding two seconds of cycles. `elapsed_us` is monotonic wall microseconds.
/// At supported <=300 MHz clocks the wall guard excludes a whole revolution;
/// arbitrary higher frequencies are not wrap-qualified. Counter availability
/// and absence of resets are caller preconditions, not checked here.
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

/// Returns instrumented work in tenths of percent, clamped to 0..=1000, from
/// `cycles` over `elapsed_us` microseconds at configured `freq_hz`. Returns `None`
/// for arithmetic overflow or <1 expected cycle (including zero frequency/time).
/// Includes interrupts counted in segments; it is not whole-core utilization.
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
pub struct CycleWindow(
    /// Checked cycle sum, or unavailable after any invalid/overflowing segment.
    Option<u64>,
);
impl Default for CycleWindow {
    /// Returns an empty valid cycle sum.
    fn default() -> Self { Self::new() }
}
impl CycleWindow {
    /// Returns a valid zero-cycle accumulator; no hardware observation is made.
    pub const fn new() -> Self { Self(Some(0)) }

    /// Adds optional `cycles` to the current sum. Missing input or addition
    /// overflow invalidates the whole window until [`Self::finish`] resets it.
    pub fn record(
        &mut self,
        cycles: Option<u64>,
    ) {
        self.0 = self.0.zip(cycles).and_then(|(total, next)| total.checked_add(next));
    }

    /// Returns work tenths-percent for `elapsed_us` microseconds at `freq_hz`,
    /// or `None` for an invalid sum/arithmetic. Always resets to a valid zero sum,
    /// including on failure; caller supplies the complete window's elapsed time.
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
/// Returns the clamped fraction of `cycles_used` over `elapsed_us` microseconds
/// at configured `freq_hz`; a denominator below one cycle returns zero. This
/// legacy arithmetic conflates unavailable timing with idle; use
/// [`checked_utilization`] where availability matters.
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
