//! Bounded software observations of the panel's TE signal. These timestamps
//! include interrupt/executor latency and do not count missed physical edges.
pub const TE_CAPACITY: usize = 64;

pub struct TeObservations {
    timestamps: [u64; TE_CAPACITY],
    next: usize,
    len: usize,
    overwritten: u64,
    rejected: u32,
    gaps: u32,
    discarded_after_gap: u32,
    skip_next: bool,
    consecutive: u8,
}

impl Default for TeObservations {
    fn default() -> Self { Self::new() }
}

impl TeObservations {
    pub const fn new() -> Self {
        Self {
            timestamps: [0; TE_CAPACITY],
            next: 0,
            len: 0,
            overwritten: 0,
            rejected: 0,
            gaps: 0,
            discarded_after_gap: 0,
            skip_next: false,
            consecutive: 0,
        }
    }

    pub fn record(
        &mut self,
        observed_us: u64,
    ) -> bool {
        if self.skip_next {
            self.skip_next = false;
            self.discarded_after_gap = self.discarded_after_gap.saturating_add(1);
            return false;
        }
        if self.latest().is_some_and(|last| observed_us <= last) {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.timestamps[self.next] = observed_us;
        self.consecutive = self.consecutive.saturating_add(1).min(2);
        self.next = (self.next + 1) % TE_CAPACITY;
        if self.len == TE_CAPACITY {
            self.overwritten = self.overwritten.saturating_add(1);
        } else {
            self.len += 1;
        }
        true
    }

    /// IRQs may have accumulated an old edge during XIP-off maintenance. Keep
    /// the persistent edge future alive, discard its first result afterwards,
    /// and never form a period using observations on opposite sides of a gap.
    pub fn mark_gap(&mut self) {
        self.gaps = self.gaps.saturating_add(1);
        self.skip_next = true;
        self.consecutive = 0;
    }

    pub const fn gaps(&self) -> u32 { self.gaps }

    pub const fn discarded_after_gap(&self) -> u32 { self.discarded_after_gap }

    pub const fn len(&self) -> usize { self.len }

    pub const fn is_empty(&self) -> bool { self.len == 0 }

    pub const fn overwritten(&self) -> u64 { self.overwritten }

    pub const fn rejected(&self) -> u32 { self.rejected }

    /// Oldest retained observation is index zero. Out-of-range indices fail.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<u64> {
        if index >= self.len {
            return None;
        }
        Some(self.timestamps[(self.next + TE_CAPACITY - self.len + index) % TE_CAPACITY])
    }

    pub fn latest(&self) -> Option<u64> { self.len.checked_sub(1).and_then(|index| self.get(index)) }

    pub fn latest_interval_us(&self) -> Option<u64> {
        if self.consecutive < 2 {
            return None;
        }
        let previous = self.get(self.len.checked_sub(2)?)?;
        self.latest()?.checked_sub(previous)
    }

    pub fn age_us(
        &self,
        now_us: u64,
    ) -> Option<u64> {
        now_us.checked_sub(self.latest()?)
    }
}
