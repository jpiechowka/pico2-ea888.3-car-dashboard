//! Accumulate short DWT segments while excluding cooperative suspension.
/// Rejected segment operation; all variants leave accumulation/state unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleError {
    /// Pause requested without an active segment.
    Paused,
    /// Resume requested while a segment is already active.
    Running,
    /// Wrapping cycle difference exceeds the configured plausibility bound.
    TooLong,
    /// Accepted segment would overflow the accumulated `u64` count.
    Exhausted,
}
/// Checked cycle-segment accumulator; caller must independently bound wall time
/// to exclude a complete counter wrap and establish counter availability.
pub struct CycleSegments {
    /// Raw wrapping counter at segment start, or `None` during excluded suspension.
    start: Option<u32>,
    /// Sum of successfully paused segment cycles; current running segment excluded.
    total: u64,
    /// Inclusive maximum permitted cycle delta for any one segment.
    max_segment: u32,
}
impl CycleSegments {
    /// Returns a running accumulator at raw CYCCNT `start` with inclusive
    /// `max_segment` cycle bound; no frequency/elapsed-time validation is performed.
    pub const fn new(
        start: u32,
        max_segment: u32,
    ) -> Self {
        Self {
            start: Some(start),
            total: 0,
            max_segment,
        }
    }

    /// Returns completed segment cycles, excluding the currently running segment.
    pub fn total(&self) -> u64 { self.total }

    /// Ends the running segment at raw `now` CYCCNT, accepting one wrapping
    /// subtraction. Returns success or paused/too-long/overflow error without
    /// mutation. Caller excludes full counter revolutions using a wall-time bound.
    pub fn pause(
        &mut self,
        now: u32,
    ) -> Result<(), CycleError> {
        let start = self.start.ok_or(CycleError::Paused)?;
        let elapsed = now.wrapping_sub(start);
        if elapsed > self.max_segment {
            return Err(CycleError::TooLong);
        }
        self.total = self
            .total
            .checked_add(u64::from(elapsed))
            .ok_or(CycleError::Exhausted)?;
        self.start = None;
        Ok(())
    }

    /// Starts a fresh segment at raw `now` CYCCNT, excluding suspension cycles.
    /// Returns success or `Running` if the previous segment was not paused.
    pub fn resume(
        &mut self,
        now: u32,
    ) -> Result<(), CycleError> {
        if self.start.is_some() {
            return Err(CycleError::Running);
        }
        self.start = Some(now);
        Ok(())
    }
}
