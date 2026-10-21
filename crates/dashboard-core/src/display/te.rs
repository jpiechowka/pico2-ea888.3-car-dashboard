//! Bounded software observations of the panel's TE signal. These timestamps
//! include interrupt/executor latency and do not count missed physical edges.
/// Number of most recent accepted observations retained without allocation.
pub const TE_CAPACITY: usize = 64;

/// Single-owner ring; maintenance gaps invalidate intervals but retain timestamps.
pub struct TeObservations {
    /// Monotonic microsecond observations in circular storage order.
    timestamps: [u64; TE_CAPACITY],
    /// Next write slot, wrapping modulo capacity.
    next: usize,
    /// Initialized slots, zero through capacity.
    len: usize,
    /// Saturating count of accepted observations replacing the oldest entry.
    overwritten: u64,
    /// Saturating count of nonincreasing timestamps rejected after gap filtering.
    rejected: u32,
    /// Saturating count of explicit observation gaps.
    gaps: u32,
    /// Saturating count of first post-gap observations discarded as potentially stale.
    discarded_after_gap: u32,
    /// Whether the next result must be discarded without recording its timestamp.
    skip_next: bool,
    /// Accepted post-gap observations, saturating at two for interval availability.
    consecutive: u8,
}

impl Default for TeObservations {
    /// Returns an empty ring with no gap or rejection counters.
    fn default() -> Self { Self::new() }
}

impl TeObservations {
    /// Returns an empty ring; the second accepted observation enables an interval.
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

    /// Records software edge time `observed_us` in monotonic microseconds.
    /// Returns true on acceptance, overwriting the oldest slot if full; false
    /// for the first result after a gap or a timestamp <= the last retained one.
    /// Counters saturate, and rejected timestamps do not advance interval history.
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

    /// Returns the saturating lifetime count of explicitly marked gaps.
    pub const fn gaps(&self) -> u32 { self.gaps }

    /// Returns how many first post-gap results were deliberately discarded.
    pub const fn discarded_after_gap(&self) -> u32 { self.discarded_after_gap }

    /// Returns retained observation count, at most 64.
    pub const fn len(&self) -> usize { self.len }

    /// Returns true before any timestamp has been accepted.
    pub const fn is_empty(&self) -> bool { self.len == 0 }

    /// Returns overwritten ring entries, not missed physical TE edges.
    pub const fn overwritten(&self) -> u64 { self.overwritten }

    /// Returns the saturating count of nonincreasing timestamps rejected.
    pub const fn rejected(&self) -> u32 { self.rejected }

    /// Returns microsecond timestamp at oldest-relative `index`, or `None` when
    /// out of range; retained entries can span explicitly marked gaps.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<u64> {
        if index >= self.len {
            return None;
        }
        Some(self.timestamps[(self.next + TE_CAPACITY - self.len + index) % TE_CAPACITY])
    }

    /// Returns the newest retained microsecond timestamp, even during a gap,
    /// or `None` if no observation was ever accepted.
    pub fn latest(&self) -> Option<u64> { self.len.checked_sub(1).and_then(|index| self.get(index)) }

    /// Returns the latest positive observed interval in microseconds, or `None`
    /// until two accepted post-gap observations exist. This is not physical FPS.
    pub fn latest_interval_us(&self) -> Option<u64> {
        if self.consecutive < 2 {
            return None;
        }
        let previous = self.get(self.len.checked_sub(2)?)?;
        self.latest()?.checked_sub(previous)
    }

    /// Returns age of the retained latest timestamp at `now_us` in microseconds;
    /// `None` denotes no observation or clock reversal. Gaps do not erase this age.
    pub fn age_us(
        &self,
        now_us: u64,
    ) -> Option<u64> {
        now_us.checked_sub(self.latest()?)
    }
}
