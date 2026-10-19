//! Read-only display observations, independent of scan/inspection state.
use crate::display::telemetry::FrameRate;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProbeDiagnostics {
    pub fps: Option<FrameRate>,
    /// Application poll work, in tenths of a percent, not whole-core utilization.
    pub work: [Option<u16>; 2],
    /// Static allocation within the linked main RAM arena, including core1 stack.
    pub ram: Option<MemoryUse>,
    /// Maximum depth seen at task-poll boundaries, not full stack high water.
    pub stacks: [Option<MemoryUse>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryUse {
    pub used: u32,
    pub total: u32,
}
impl MemoryUse {
    pub fn valid(self) -> bool { self.total > 0 && self.used <= self.total && self.total <= 512 * 1024 }
}
