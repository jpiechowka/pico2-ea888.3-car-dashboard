//! Software observations, not physical edge timestamps. The render owner
//! consumes batches once; only successful FrameTrace completion measures them.
/// Ordered software input observations; physical edge/debounce latency is not directly measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    /// Earliest software observation time in monotonic microseconds.
    pub observed_us: u64,
    /// Latest acceptance/debounce time in monotonic microseconds, at least observed_us.
    pub accepted_us: u64,
    /// Nonzero number of coalesced observations, not necessarily detent magnitude.
    pub events: u32,
}
impl Observation {
    /// Appends valid `observation` to optional `target` using chronological merge.
    /// Returns true on assignment/merge; malformed/order/overflow rejection leaves target unchanged.
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

    /// Returns one event with software `observed_us` and acceptance `accepted_us`
    /// in monotonic microseconds, or None if observation follows acceptance.
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

    /// Merges `newer` with nondecreasing acceptance time, keeping the earliest
    /// observation and checked event sum. Returns false without mutation for invalid
    /// metadata, reversed acceptance or count overflow; observation times may overlap.
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

/// Render-consumed input batch, attachable once to a frame and meaningful only after valid completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputTrace {
    /// First nonzero consumption identity in this coalesced batch.
    pub first_id: u64,
    /// Latest consumption identity, at least first_id; never wrapped.
    pub last_id: u64,
    /// Earliest software observation in monotonic microseconds.
    pub observed_us: u64,
    /// Latest source acceptance in monotonic microseconds.
    pub accepted_us: u64,
    /// Latest original render-owner consumption in monotonic microseconds.
    pub consumed_us: u64,
    /// Checked total source observations; can exceed the count of consumption identities.
    pub events: u32,
}
impl InputTrace {
    /// Returns whether identities/count and observed<=accepted<=consumed<=`render_us`
    /// hold, with all times in monotonic microseconds; does not prove physical causality.
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

/// Saturating diagnostic counts; excluded inputs never become completed-frame latency samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputCounters {
    /// Source events deliberately ignored by control policy.
    pub ignored_events: u64,
    /// Consumed events whose resolved scene caused no panel damage.
    pub no_damage_events: u64,
    /// Consumed events hidden by the resolved overlay policy.
    pub obscured_events: u64,
    /// Rejected trace applications due to metadata, chronology or counter exhaustion.
    pub invalid: u64,
    /// Explicit maintenance gaps; these do not clear pending trace automatically.
    pub maintenance_gaps: u64,
    /// Encoder publisher drop count supplied by external diagnostic aggregation.
    pub encoder_dropped: u64,
    /// Encoder maintenance/page-discard count supplied by external aggregation.
    pub encoder_discarded: u64,
}
impl InputCounters {
    /// Const all-zero counters, including externally populated encoder counts.
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
/// Render-owner trace accumulator; issuance is checked and pending metadata transfers only once.
pub struct InputTracker {
    /// Last issued nonzero consumption identity; MAX prevents further admission.
    sequence: u64,
    /// Last accepted application time in monotonic microseconds, including deferred effects.
    last_us: u64,
    /// Coalesced effects awaiting submission, damage exclusion or obscuration exclusion.
    pending: Option<InputTrace>,
    /// Saturating local diagnostic counters.
    counters: InputCounters,
}
impl Default for InputTracker {
    /// Returns an empty tracker with zero identity/time/counters, as new.
    fn default() -> Self { Self::new() }
}
impl InputTracker {
    /// Returns empty trace state; the first valid application receives identity1.
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            last_us: 0,
            pending: None,
            counters: InputCounters::ZERO,
        }
    }

    /// Applies `observation` consumed immediately at monotonic `now_us` microseconds.
    /// Returns admission status; invalid metadata/time/count leaves pending state unchanged
    /// and increments invalid, as apply with equal consumption/application times.
    pub fn consume(
        &mut self,
        observation: Observation,
        now_us: u64,
    ) -> bool {
        self.apply(observation, now_us, now_us)
    }

    /// Deferred effects (statistics reset acknowledgement) keep the original
    /// consumption time, but are attached only when that effect enters the UI.
    /// `observation` supplies source times/count; `consumed_us` is original consumption
    /// and `now_us` effect application, both monotonic microseconds. Returns true after
    /// checked coalescing/identity issuance; chronology or overflow rejection only increments invalid.
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

    /// Returns a copy of the pending batch without consuming its one-submission ownership.
    pub const fn pending(&self) -> Option<InputTrace> { self.pending }

    /// Returns and removes the pending batch for one submitted frame, or None.
    pub fn submitted(&mut self) -> Option<InputTrace> { self.pending.take() }

    /// Removes pending input and counts its events as no-damage; does not publish a latency sample.
    pub fn no_damage(&mut self) {
        if let Some(trace) = self.pending.take() {
            self.counters.no_damage_events = self.counters.no_damage_events.saturating_add(u64::from(trace.events));
        }
    }

    /// A visible critical overlay can hide an action's changed pixels.
    /// Callers must use the resolved page/feedback policy, not alarm presence.
    /// Conservatively exclude the entire batch on these frames, even
    /// when unrelated pixels change. Never infer causality from global damage.
    /// `obscured` true removes/counts the pending batch; false leaves it unchanged.
    pub fn exclude_obscured(
        &mut self,
        obscured: bool,
    ) {
        if obscured && let Some(trace) = self.pending.take() {
            self.counters.obscured_events = self.counters.obscured_events.saturating_add(u64::from(trace.events));
        }
    }

    /// Saturating-adds `events` to deliberate ignores without changing pending trace.
    pub fn ignore(
        &mut self,
        events: u32,
    ) {
        self.counters.ignored_events = self.counters.ignored_events.saturating_add(u64::from(events));
    }

    /// Counts one maintenance gap without clearing pending effects or reusing identities.
    pub fn maintenance_gap(&mut self) {
        self.counters.maintenance_gaps = self.counters.maintenance_gaps.saturating_add(1);
    }

    /// Returns local diagnostic counters; encoder fields remain zero until externally combined.
    pub const fn counters(&self) -> InputCounters { self.counters }
}

#[cfg(test)]
/// Checked identity and coalesced-count exhaustion regressions.
mod tests {
    use super::*;
    #[test]
    /// Verifies exhausted identities and event sums reject admission without wrapping or losing prior state.
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
