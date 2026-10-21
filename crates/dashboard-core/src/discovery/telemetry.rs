//! Read-only display observations, independent of scan/inspection state.
use crate::display::telemetry::FrameRate;

/// Current hardware observations rendered beside frozen capture evidence; defaults are unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProbeDiagnostics {
    /// Successful software-transfer rates, not panel scanout or synthetic render speed.
    pub fps: Option<FrameRate>,
    /// Application poll work, in tenths of a percent, not whole-core utilization.
    pub work: [Option<u16>; 2],
    /// Static allocation within the linked main RAM arena, including core1 stack.
    pub ram: Option<MemoryUse>,
    /// Maximum depth seen at task-poll boundaries, not full stack high water.
    pub stacks: [Option<MemoryUse>; 2],
}

/// Byte occupancy within a bounded measured/linked memory region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryUse {
    /// Occupied/sampled depth in bytes; meaningful only when `valid` accepts the pair.
    pub used: u32,
    /// Region capacity in bytes; valid range is 1..=524288.
    pub total: u32,
}
impl MemoryUse {
    /// Returns whether `self` has nonzero capacity at most 512 KiB and usage no greater
    /// than capacity. This checks representation only, not the measurement's provenance.
    pub fn valid(self) -> bool { self.total > 0 && self.used <= self.total && self.total <= 512 * 1024 }
}
