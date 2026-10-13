//! Fixed canonical buckets shared by every widget of a channel.
use super::extrema::valid_sample;

pub const BUCKETS: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HistoryData {
    values: [f32; BUCKETS],
    valid: u64,
    newest: u64,
    seconds: u16,
}
impl HistoryData {
    pub(crate) const EMPTY: Self = Self {
        values: [0.0; BUCKETS],
        valid: 0,
        newest: 0,
        seconds: 120,
    };

    pub(crate) fn view(&self) -> HistoryView<'_> { HistoryView { data: self } }
}

/// Immutable chronological view. Empty buckets have explicit absence; bounds
/// scan only valid points. Early uptime has no invented negative timestamps.
#[derive(Clone, Copy)]
pub struct HistoryView<'a> {
    data: &'a HistoryData,
}
impl<'a> HistoryView<'a> {
    pub fn duration_seconds(self) -> u16 { self.data.seconds }

    pub fn newest_ordinal(self) -> u64 { self.data.newest }

    pub fn validity(self) -> u64 { self.data.valid }

    pub fn count(self) -> usize { self.data.valid.count_ones() as usize }

    pub fn bounds(self) -> Option<(f32, f32)> {
        self.iter().filter_map(|(_, v)| v).fold(None, |bounds, v| {
            Some(bounds.map_or((v, v), |(lo, hi): (f32, f32)| (lo.min(v), hi.max(v))))
        })
    }

    /// Exactly sixty screen slots, oldest to newest. Before time zero is blank.
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

    pub fn iter(self) -> impl Iterator<Item = (u64, Option<f32>)> + 'a {
        (0..BUCKETS).filter_map(move |i| self.point(i))
    }
}

pub struct History {
    data: HistoryData,
    last_ms: Option<u64>,
    observed_ms: Option<u64>,
    revision: Option<u32>,
}
impl History {
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

    pub fn view(&self) -> HistoryView<'_> { self.data.view() }

    pub(crate) fn data(&self) -> HistoryData { self.data }

    pub(crate) const fn revision(&self) -> Option<u32> { self.revision }

    #[cfg(test)]
    pub(crate) fn force_revision(
        &mut self,
        revision: u32,
    ) {
        self.revision = Some(revision);
    }

    fn changed(&mut self) { self.revision = self.revision.and_then(|r| r.checked_add(1)); }

    pub fn clear(&mut self) {
        self.data.valid = 0;
        self.observed_ms = None;
        self.changed();
    }

    /// Duration changes affect history alone, never independent extrema/averages.
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
    pub fn advance(
        &mut self,
        now_ms: u64,
    ) -> usize {
        let duration = u64::from(self.data.seconds) * 1000;
        let ordinal = (now_ms / duration) * 60 + (now_ms % duration) * 60 / duration;
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
        self.advance(received_ms);
        self.observed_ms = Some(received_ms);
        let slot = (self.data.newest % 60) as usize;
        let bit = 1 << slot;
        if self.data.valid & bit == 0 || self.data.values[slot].to_bits() != value.to_bits() {
            self.data.values[slot] = value;
            self.data.valid |= bit;
            self.changed();
        }
    }

    /// A sub-bucket fault deliberately sacrifices at most one preceding point
    /// so recovery in this bucket cannot draw a continuous line across an outage.
    pub fn invalidate(
        &mut self,
        now_ms: u64,
    ) {
        self.advance(now_ms);
        let slot = self.data.newest % 60;
        let mut mask = 1 << slot;
        if self.data.newest > 0 {
            mask |= 1 << ((slot + 59) % 60);
        }
        if self.data.valid & mask != 0 {
            self.data.valid &= !mask;
            self.changed();
        }
    }
}
