//! Software observations, not physical edge timestamps. The render owner
//! consumes batches once; only successful FrameTrace completion measures them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub observed_us: u64,
    pub accepted_us: u64,
    pub events: u32,
}
impl Observation {
    pub fn append(
        target: &mut Option<Self>,
        observation: Self,
    ) -> bool {
        if observation.events == 0 || observation.observed_us > observation.accepted_us {
            return false;
        }
        if let Some(old) = target {
            old.merge(observation)
        } else {
            *target = Some(observation);
            true
        }
    }

    pub const fn new(
        observed_us: u64,
        accepted_us: u64,
    ) -> Option<Self> {
        if observed_us > accepted_us {
            return None;
        }
        Some(Self {
            observed_us,
            accepted_us,
            events: 1,
        })
    }

    pub fn merge(
        &mut self,
        newer: Self,
    ) -> bool {
        if self.events == 0
            || self.observed_us > self.accepted_us
            || newer.events == 0
            || newer.observed_us > newer.accepted_us
            || newer.accepted_us < self.accepted_us
        {
            return false;
        }
        let Some(events) = self.events.checked_add(newer.events) else {
            return false;
        };
        self.observed_us = self.observed_us.min(newer.observed_us);
        self.accepted_us = newer.accepted_us;
        self.events = events;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputTrace {
    pub first_id: u64,
    pub last_id: u64,
    pub observed_us: u64,
    pub accepted_us: u64,
    pub consumed_us: u64,
    pub events: u32,
}
impl InputTrace {
    pub fn valid_before(
        self,
        render_us: u64,
    ) -> bool {
        self.first_id != 0
            && self.first_id <= self.last_id
            && self.events > 0
            && self.observed_us <= self.accepted_us
            && self.accepted_us <= self.consumed_us
            && self.consumed_us <= render_us
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputCounters {
    pub ignored_events: u64,
    pub no_damage_events: u64,
    pub obscured_events: u64,
    pub invalid: u64,
    pub maintenance_gaps: u64,
    pub encoder_dropped: u64,
    pub encoder_discarded: u64,
}
impl InputCounters {
    pub const ZERO: Self = Self {
        ignored_events: 0,
        no_damage_events: 0,
        obscured_events: 0,
        invalid: 0,
        maintenance_gaps: 0,
        encoder_dropped: 0,
        encoder_discarded: 0,
    };
}
pub struct InputTracker {
    sequence: u64,
    last_us: u64,
    pending: Option<InputTrace>,
    counters: InputCounters,
}
impl Default for InputTracker {
    fn default() -> Self { Self::new() }
}
impl InputTracker {
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            last_us: 0,
            pending: None,
            counters: InputCounters::ZERO,
        }
    }

    pub fn consume(
        &mut self,
        observation: Observation,
        now_us: u64,
    ) -> bool {
        self.apply(observation, now_us, now_us)
    }

    /// Deferred effects (statistics reset acknowledgement) keep the original
    /// consumption time, but are attached only when that effect enters the UI.
    pub fn apply(
        &mut self,
        observation: Observation,
        consumed_us: u64,
        now_us: u64,
    ) -> bool {
        let result = (|| {
            if observation.events == 0
                || observation.observed_us > observation.accepted_us
                || observation.accepted_us > consumed_us
                || consumed_us > now_us
                || now_us < self.last_us
            {
                return None;
            }
            let sequence = self.sequence.checked_add(1)?;
            let mut pending = InputTrace {
                first_id: sequence,
                last_id: sequence,
                observed_us: observation.observed_us,
                accepted_us: observation.accepted_us,
                consumed_us,
                events: observation.events,
            };
            if let Some(old) = self.pending {
                pending.first_id = old.first_id;
                pending.observed_us = pending.observed_us.min(old.observed_us);
                pending.accepted_us = pending.accepted_us.max(old.accepted_us);
                pending.consumed_us = pending.consumed_us.max(old.consumed_us);
                pending.events = pending.events.checked_add(old.events)?;
            }
            Some(pending)
        })();
        if let Some(pending) = result {
            self.sequence = pending.last_id;
            self.last_us = now_us;
            self.pending = Some(pending);
            true
        } else {
            self.counters.invalid = self.counters.invalid.saturating_add(1);
            false
        }
    }

    pub const fn pending(&self) -> Option<InputTrace> { self.pending }

    pub fn submitted(&mut self) -> Option<InputTrace> { self.pending.take() }

    pub fn no_damage(&mut self) {
        if let Some(trace) = self.pending.take() {
            self.counters.no_damage_events = self.counters.no_damage_events.saturating_add(u64::from(trace.events));
        }
    }

    /// A critical overlay can hide the only acknowledgement of a deferred
    /// action. Conservatively exclude the entire batch on these frames, even
    /// when unrelated pixels change. Never infer causality from global damage.
    pub fn exclude_obscured(
        &mut self,
        obscured: bool,
    ) {
        if obscured && let Some(trace) = self.pending.take() {
            self.counters.obscured_events = self.counters.obscured_events.saturating_add(u64::from(trace.events));
        }
    }

    pub fn ignore(
        &mut self,
        events: u32,
    ) {
        self.counters.ignored_events = self.counters.ignored_events.saturating_add(u64::from(events));
    }

    pub fn maintenance_gap(&mut self) {
        self.counters.maintenance_gaps = self.counters.maintenance_gaps.saturating_add(1);
    }

    pub const fn counters(&self) -> InputCounters { self.counters }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_identity_and_batch_count_cannot_wrap() {
        let mut tracker = InputTracker::new();
        tracker.sequence = u64::MAX;
        assert!(!tracker.consume(Observation::new(0, 0).unwrap(), 0));
        assert!(tracker.pending().is_none());
        let mut observation = Observation {
            observed_us: 0,
            accepted_us: 1,
            events: u32::MAX,
        };
        assert!(!observation.merge(Observation::new(1, 2).unwrap()));
        assert_eq!(observation.events, u32::MAX);
    }
}
