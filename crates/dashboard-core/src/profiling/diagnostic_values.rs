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
