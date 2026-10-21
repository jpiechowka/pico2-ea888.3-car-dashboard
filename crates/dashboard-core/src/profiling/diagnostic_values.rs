//! Bounded presentation and validity shared by both diagnostic firmware routes.
use core::fmt::Write;

use heapless::String;

/// Inclusive maximum age of a work/rate observation, in microseconds.
pub const FRESH_US: u64 = 2_000_000;

/// Returns whether monotonic `now_us` is at/after observation `at_us` and no more
/// than two seconds later; both are microseconds in the same timebase.
pub fn fresh(
    at_us: u64,
    now_us: u64,
) -> bool {
    now_us.checked_sub(at_us).is_some_and(|age| age <= FRESH_US)
}

/// Copy under one cross-core lock. Stack is a lifetime observed maximum;
/// work refers only to the timestamped window and must expire independently.
#[derive(Clone, Copy)]
pub struct WorkObservation {
    /// End/publication timestamp of the instrumented window, or unavailable.
    pub at_us: Option<u64>,
    /// Work in tenths-percent, valid only in 0..=1000 with a fresh timestamp.
    pub work_tenths: Option<u32>,
    /// Lifetime maximum observed stack bytes; independently available, not expiring.
    pub stack_bytes: Option<u32>,
}
impl WorkObservation {
    /// No timestamp, work fraction or stack evidence; distinct from measured zero.
    pub const EMPTY: Self = Self {
        at_us: None,
        work_tenths: None,
        stack_bytes: None,
    };

    /// Returns work tenths-percent at monotonic `now_us` microseconds, or `None`
    /// for absent/expired/future timestamps, missing work or values above 1000.
    pub fn work(
        self,
        now_us: u64,
    ) -> Option<u32> {
        fresh(self.at_us?, now_us)
            .then_some(self.work_tenths?)
            .filter(|&v| v <= 1000)
    }
}

/// One completed UI iteration. Publish this value only after waits/maintenance
/// decisions so a discarded render cannot replace just the cycle numerator.
#[derive(Clone, Copy)]
pub struct FrameWorkObservation {
    /// Shared timestamp and instrumented fraction for this completed iteration.
    work: WorkObservation,
    /// Cycle count if representable in `u32`; availability also depends on work.
    cycles: Option<u32>,
}
impl FrameWorkObservation {
    /// Unavailable frame observation, used until a complete iteration publishes.
    pub const EMPTY: Self = Self {
        work: WorkObservation::EMPTY,
        cycles: None,
    };

    /// Returns coherent frame metadata at `at_us` (monotonic microseconds), using
    /// configured `cpu_hz`, optional active `cycles` and total `elapsed_us`.
    /// Invalid arithmetic leaves work unavailable; a count above `u32::MAX` leaves
    /// cycles unavailable even if work is valid. Does not measure any hardware.
    pub fn new(
        at_us: u64,
        cpu_hz: u32,
        cycles: Option<u64>,
        elapsed_us: u64,
    ) -> Self {
        Self {
            work: WorkObservation {
                at_us: Some(at_us),
                work_tenths: cycles.and_then(|value| crate::cpu_math::checked_utilization(cpu_hz, value, elapsed_us)),
                stack_bytes: None,
            },
            cycles: cycles.and_then(|value| u32::try_from(value).ok()),
        }
    }

    /// Returns fresh instrumented work tenths-percent at `now_us` microseconds,
    /// or `None` when unavailable, expired or future-dated.
    pub fn work(
        self,
        now_us: u64,
    ) -> Option<u32> {
        self.work.work(now_us)
    }

    /// Returns the recorded cycle count only while work is valid/fresh at
    /// `now_us` microseconds; `None` also covers counts too wide for `u32`.
    pub fn cycles(
        self,
        now_us: u64,
    ) -> Option<u32> {
        self.work(now_us).and(self.cycles)
    }
}

/// RP2350 SSP divider metadata, not a physical measurement. CPSDVSR must be
/// even in 2..=254; SCR encodes an additional divisor in 1..=256.
/// Returns integer hertz from configured `peripheral_hz`, raw prescaler `cpsdvsr`
/// and serial-rate field `scr`, flooring division; `None` for invalid prescaler
/// or a zero resulting rate. See the SPI register definitions in the
/// [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf).
pub fn spi_clock_hz(
    peripheral_hz: u32,
    cpsdvsr: u8,
    scr: u8,
) -> Option<u32> {
    if cpsdvsr < 2 || !cpsdvsr.is_multiple_of(2) {
        return None;
    }
    let hz = peripheral_hz / (u32::from(cpsdvsr) * (u32::from(scr) + 1));
    (hz != 0).then_some(hz)
}

/// Rounded KiB for a checked pair; rounding cannot reverse used <= capacity.
/// Returns `(used, total)` in KiB rounded up independently from byte inputs
/// `used` and `total`, or `None` unless 0 <= used <= total <=512 KiB and total >0.
pub fn memory_kib(
    used: u32,
    total: u32,
) -> Option<(u32, u32)> {
    (total > 0 && used <= total && total <= 512 * 1024).then_some((used.div_ceil(1024), total.div_ceil(1024)))
}

/// Includes percent suffix for numeric values; missing is always explicit.
/// Returns bounded text for tenths-percent `value`: `0.0%` through `100.0%`,
/// or `--` for absent/out-of-range input. No heap allocation occurs.
pub fn work_text(value: Option<u32>) -> String<8> {
    let mut out = String::new();
    if let Some(v) = value.filter(|&v| v <= 1000) {
        let _ = write!(out, "{}.{:01}%", v / 10, v % 10);
    } else {
        let _ = out.push_str("--");
    }
    out
}

/// Bounded conversion avoids the general float formatter on small core stacks.
/// Returns one-decimal text for finite nonnegative `value` below 1000, rounded
/// to nearest tenth; finite values >=1000 become `>999`, missing/negative/nonfinite
/// values become `--`. Rounding a value just below 1000 can produce `1000.0`.
pub fn rate_text(value: Option<f32>) -> String<8> {
    let mut out = String::new();
    match value {
        Some(v) if v.is_finite() && (0.0..1000.0).contains(&v) => {
            let tenths = (v * 10.0 + 0.5) as u32;
            let _ = write!(out, "{}.{:01}", tenths / 10, tenths % 10);
        }
        Some(v) if v.is_finite() && v >= 1000.0 => {
            let _ = out.push_str(">999");
        }
        _ => {
            let _ = out.push_str("--");
        }
    }
    out
}
