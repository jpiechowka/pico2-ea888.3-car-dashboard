//! Accumulate short DWT segments while excluding cooperative suspension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleError {
    Paused,
    Running,
    TooLong,
    Exhausted,
}
pub struct CycleSegments {
    start: Option<u32>,
    total: u64,
    max_segment: u32,
}
impl CycleSegments {
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

    pub fn total(&self) -> u64 { self.total }

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
