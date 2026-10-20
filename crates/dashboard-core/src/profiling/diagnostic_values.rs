//! Bounded presentation and validity shared by both diagnostic firmware routes.
use core::fmt::Write;

use heapless::String;

pub const FRESH_US: u64 = 2_000_000;

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
    pub at_us: Option<u64>,
    pub work_tenths: Option<u32>,
    pub stack_bytes: Option<u32>,
}
impl WorkObservation {
    pub const EMPTY: Self = Self {
        at_us: None,
        work_tenths: None,
        stack_bytes: None,
    };

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
    work: WorkObservation,
    cycles: Option<u32>,
}
impl FrameWorkObservation {
    pub const EMPTY: Self = Self {
        work: WorkObservation::EMPTY,
        cycles: None,
    };

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

    pub fn work(
        self,
        now_us: u64,
    ) -> Option<u32> {
        self.work.work(now_us)
    }

    pub fn cycles(
        self,
        now_us: u64,
    ) -> Option<u32> {
        self.work(now_us).and(self.cycles)
    }
}

/// RP2350 SSP divider metadata, not a physical measurement. CPSDVSR must be
/// even in 2..=254; SCR encodes an additional divisor in 1..=256.
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
pub fn memory_kib(
    used: u32,
    total: u32,
) -> Option<(u32, u32)> {
    (total > 0 && used <= total && total <= 512 * 1024).then_some((used.div_ceil(1024), total.div_ceil(1024)))
}

/// Includes percent suffix for numeric values; missing is always explicit.
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
