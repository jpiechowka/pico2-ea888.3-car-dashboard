//! Fixed canonical buckets shared by every widget of a channel.
use super::extrema::valid_sample;

/// Fixed chronological graph capacity, independent of configured history duration.
pub const BUCKETS: usize = 60;

/// Copyable snapshot storage; validity bits distinguish absent points from zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HistoryData {
    /// Canonical values in ordinal-modulo-60 storage order; invalid entries ignored.
    values: [f32; BUCKETS],
    /// Initialized point bits for circular slots 0..59; upper four bits remain zero.
    valid: u64,
    /// Newest absolute bucket ordinal on the monotonic uptime axis.
    newest: u64,
    /// Total visible duration in seconds, restricted to 10..=900.
    seconds: u16,
}
impl HistoryData {
    /// Empty 120-second history anchored at ordinal zero.
    pub(crate) const EMPTY: Self = Self {
        values: [0.0; BUCKETS],
        valid: 0,
        newest: 0,
        seconds: 120,
    };

    /// Returns a chronological immutable view borrowing this snapshot storage.
    pub(crate) fn view(&self) -> HistoryView<'_> { HistoryView { data: self } }
}

/// Immutable chronological view. Empty buckets have explicit absence; bounds
/// scan only valid points. Early uptime has no invented negative timestamps.
#[derive(Clone, Copy)]
pub struct HistoryView<'a> {
    /// Immutable circular data; this view does not own a sampling clock.
    data: &'a HistoryData,
}
impl<'a> HistoryView<'a> {
    /// Returns configured total visible duration in seconds.
    pub fn duration_seconds(self) -> u16 { self.data.seconds }

    /// Returns newest absolute bucket ordinal; zero can precede the first sample.
    pub fn newest_ordinal(self) -> u64 { self.data.newest }

    /// Returns validity bits in circular storage order, not screen-slot order.
    pub fn validity(self) -> u64 { self.data.valid }

    /// Returns present point count, excluding explicit gaps and pre-uptime slots.
    pub fn count(self) -> usize { self.data.valid.count_ones() as usize }

    /// Returns canonical minimum/maximum over present points, or `None` if empty.
    pub fn bounds(self) -> Option<(f32, f32)> {
        self.iter().filter_map(|(_, v)| v).fold(None, |bounds, v| {
            Some(bounds.map_or((v, v), |(lo, hi): (f32, f32)| (lo.min(v), hi.max(v))))
        })
    }

    /// Exactly sixty screen slots, oldest to newest. Before time zero is blank.
    /// Returns `(bucket_start_ms, optional_value)` at screen `index`, or `None`
    /// for index >=60 or before uptime zero. Inner `None` is an explicit gap;
    /// timestamps are the first integer millisecond belonging to each bucket.
    pub fn point(
        self,
        index: usize,
    ) -> Option<(u64, Option<f32>)> {
        if index >= BUCKETS {
            return None;
        }
        let ordinal = self.data.newest.checked_sub((BUCKETS - 1 - index) as u64)?;
        let slot = (ordinal % BUCKETS as u64) as usize;
        let value = (self.data.valid & (1 << slot) != 0).then_some(self.data.values[slot]);
        // First integer timestamp belonging to this bucket, without ordinal*D.
        let duration = u64::from(self.data.seconds) * 1000;
        let time = (ordinal / 60) * duration + ((ordinal % 60) * duration).div_ceil(60);
        Some((time, value))
    }

    /// Returns oldest-to-newest timestamp/value pairs, skipping pre-uptime slots
    /// but retaining absent-value gaps; at most sixty entries.
    pub fn iter(self) -> impl Iterator<Item = (u64, Option<f32>)> + 'a {
        (0..BUCKETS).filter_map(move |i| self.point(i))
    }
}

/// Single-owner canonical bucket accumulator; late events may update retained
/// buckets without reversing the presentation clock. Caller owns sample ordering.
pub struct History {
    /// Copyable graph contents and configured duration.
    data: HistoryData,
    /// Last presentation/forward-event time in milliseconds, absent initially.
    last_ms: Option<u64>,
    /// Most recently admitted sample timestamp for exact-duplicate suppression.
    observed_ms: Option<u64>,
    /// Checked graph revision; `None` permanently disables identity-based caching.
    revision: Option<u32>,
}
impl History {
    /// Returns an empty history covering `seconds`; panics outside 10..=900.
    /// Starts revision zero; no measurement is synthesized.
    pub const fn new(seconds: u16) -> Self {
        assert!(seconds >= 10 && seconds <= 900);
        let mut data = HistoryData::EMPTY;
        data.seconds = seconds;
        Self {
            data,
            last_ms: None,
            observed_ms: None,
            revision: Some(0),
        }
    }

    /// Returns a chronological immutable view of current buckets and gaps.
    pub fn view(&self) -> HistoryView<'_> { self.data.view() }

    /// Returns a coherent copy for publication without sampling-owner state.
    pub(crate) fn data(&self) -> HistoryData { self.data }

    /// Returns the current cache revision, or sticky `None` after counter exhaustion.
    pub(crate) const fn revision(&self) -> Option<u32> { self.revision }

    #[cfg(test)]
    /// Forces `revision` for exhaustion fixtures; bypasses normal monotonic issuance.
    pub(crate) fn force_revision(
        &mut self,
        revision: u32,
    ) {
        self.revision = Some(revision);
    }

    /// Advances the cache revision, retaining permanent absence after overflow.
    fn changed(&mut self) { self.revision = self.revision.and_then(|r| r.checked_add(1)); }

    /// Clears all point validity and duplicate history, advancing revision even
    /// when already empty; retains duration and presentation clock/ordinal.
    pub fn clear(&mut self) {
        self.data.valid = 0;
        self.observed_ms = None;
        self.changed();
    }

    /// Duration changes affect history alone, never independent extrema/averages.
    /// Sets `seconds` in 10..=900, clearing/reanchoring graph time only when
    /// changed; panics outside that range. Exhausted cache identity stays absent.
    pub fn set_duration(
        &mut self,
        seconds: u16,
    ) {
        assert!((10..=900).contains(&seconds));
        if self.data.seconds != seconds {
            self.clear();
            self.data.seconds = seconds;
            self.last_ms = None;
            self.data.newest = 0;
        }
    }

    /// Returns the number of slots visited: bounded by sixty even after a day.
    /// Advances to monotonic `now_ms`, invalidating newly elapsed buckets; a
    /// backward clock clears history and reanchors it. No missing sample is filled.
    pub fn advance(
        &mut self,
        now_ms: u64,
    ) -> usize {
        let ordinal = self.ordinal(now_ms);
        let backwards = self.last_ms.is_some_and(|last| now_ms < last);
        let elapsed = ordinal.saturating_sub(self.data.newest);
        let visited = elapsed.min(60) as usize;
        if backwards {
            self.clear();
        } else if elapsed >= 60 {
            self.data.valid = 0;
        } else {
            for offset in 1..=visited {
                let slot = ((self.data.newest + offset as u64) % 60) as usize;
                self.data.valid &= !(1 << slot);
            }
        }
        if ordinal != self.data.newest {
            self.changed();
        }
        self.data.newest = ordinal;
        self.last_ms = Some(now_ms);
        visited
    }

    /// Returns the bucket ordinal containing `at_ms` milliseconds, using quotient
    /// decomposition to avoid overflowing a direct timestamp-times-sixty product.
    fn ordinal(
        &self,
        at_ms: u64,
    ) -> u64 {
        let duration = u64::from(self.data.seconds) * 1000;
        (at_ms / duration) * 60 + (at_ms % duration) * 60 / duration
    }

    /// Returns the retained bucket for event time `at_ms` milliseconds, advancing
    /// only for current/future events. Returns `None` for an older event outside
    /// the sixty-bucket window; late events never move the clock backward.
    fn event_ordinal(
        &mut self,
        at_ms: u64,
    ) -> Option<u64> {
        if self.last_ms.is_none_or(|last| at_ms >= last) {
            self.advance(at_ms);
            return Some(self.data.newest);
        }
        let ordinal = self.ordinal(at_ms);
        self.data
            .newest
            .checked_sub(ordinal)
            .filter(|age| *age < BUCKETS as u64)
            .map(|_| ordinal)
    }

    /// Stores canonical `value` at `received_ms` milliseconds in its retained
    /// bucket, replacing that bucket's earlier value. Invalid input creates a gap;
    /// an exact duplicate timestamp is ignored and cannot refill a faulted bucket.
    /// Caller must enforce sample ordering; events older than the window are ignored.
    pub fn observe(
        &mut self,
        value: f32,
        received_ms: u64,
    ) {
        if !valid_sample(value) {
            self.invalidate(received_ms);
            return;
        }
        // A duplicate never repopulates a faulted bucket.
        if self.observed_ms == Some(received_ms) {
            return;
        }
        let Some(ordinal) = self.event_ordinal(received_ms) else {
            return;
        };
        self.observed_ms = Some(received_ms);
        let slot = (ordinal % BUCKETS as u64) as usize;
        let bit = 1 << slot;
        if self.data.valid & bit == 0 || self.data.values[slot].to_bits() != value.to_bits() {
            self.data.values[slot] = value;
            self.data.valid |= bit;
            self.changed();
        }
    }

    /// A sub-bucket fault deliberately sacrifices at most one preceding point
    /// so recovery in this bucket cannot draw a continuous line across an outage.
    /// Marks the event bucket at `now_ms` milliseconds and its retained predecessor
    /// absent. Too-old events do nothing; future events first advance the window.
    pub fn invalidate(
        &mut self,
        now_ms: u64,
    ) {
        let Some(ordinal) = self.event_ordinal(now_ms) else {
            return;
        };
        let slot = ordinal % BUCKETS as u64;
        let mut mask = 1 << slot;
        if ordinal > 0 && self.data.newest - ordinal < (BUCKETS - 1) as u64 {
            mask |= 1 << ((slot + 59) % 60);
        }
        if self.data.valid & mask != 0 {
            self.data.valid &= !mask;
            self.changed();
        }
    }
}
