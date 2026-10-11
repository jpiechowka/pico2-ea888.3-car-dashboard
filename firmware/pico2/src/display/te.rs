//! Bounded software observations of the panel's TE signal. These timestamps
//! include interrupt/executor latency and do not count missed physical edges.
pub const TE_CAPACITY: usize = 64;

pub struct TeObservations {
    timestamps: [u64; TE_CAPACITY],
    next: usize,
    len: usize,
    overwritten: u64,
    rejected: u32,
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
        }
    }

    pub fn record(
        &mut self,
        observed_us: u64,
    ) -> bool {
        if self.latest().is_some_and(|last| observed_us <= last) {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.timestamps[self.next] = observed_us;
        self.next = (self.next + 1) % TE_CAPACITY;
        if self.len == TE_CAPACITY {
            self.overwritten = self.overwritten.saturating_add(1);
        } else {
            self.len += 1;
        }
        true
    }

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
