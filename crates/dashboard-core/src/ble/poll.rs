//! Completion-based cadence without replaying missed polling ticks.

/// Maximum independently scheduled channels; bounds each admission scan.
pub const MAX_CHANNELS: usize = 8;
/// Completion-relative per-channel timing in monotonic milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cadence {
    /// Positive delay from actual completion to this channel's next due time.
    pub period_ms: u64,
    /// Positive transaction timeout, no greater than session MAX_DEADLINE_MS.
    pub deadline_ms: u64,
}
/// Admission/scheduling failures; most temporal failures permanently stop this scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid channel count, zero period/deadline/epoch or excessive deadline.
    Configuration,
    /// Monotonic observer time moved backwards; scheduler is stopped.
    Clock,
    /// Active transaction reached or passed its absolute deadline; scheduler is stopped.
    Timeout,
    /// Completion ticket does not exactly match the active epoch/generation/channel/timing.
    Token,
    /// Checked identity, time or latency arithmetic overflowed; active scheduler is stopped.
    Exhausted,
    /// Scheduler was explicitly stopped or failed earlier.
    Stopped,
}
/// Immutable single-flight admission token, bound to a fresh session epoch and exact timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    /// Nonzero session epoch; prevents old owners completing newly created schedules.
    epoch: u64,
    /// Zero-based channel index within the scheduler's fixed array.
    index: usize,
    /// Checked monotonically increasing transaction identity within this epoch.
    generation: u64,
    /// Admission time in boot-relative milliseconds.
    started_ms: u64,
    /// Exclusive completion deadline in boot-relative milliseconds.
    deadline_ms: u64,
}
impl Ticket {
    /// Returns `self`'s zero-based channel index.
    pub const fn index(self) -> usize { self.index }

    /// Returns `self`'s actual admission time in boot-relative milliseconds.
    pub const fn started_ms(self) -> u64 { self.started_ms }

    /// Returns `self`'s exclusive absolute completion deadline in boot-relative milliseconds.
    pub const fn deadline_ms(self) -> u64 { self.deadline_ms }
}
/// Aggregate latency of all accepted completions, including unsuccessful replies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Latency {
    /// Number of accepted completions; overflow stops the scheduler instead of wrapping.
    pub count: u64,
    /// Sum of admission-to-completion durations in milliseconds, checked for overflow.
    pub total_ms: u64,
    /// Maximum accepted admission-to-completion duration in milliseconds.
    pub max_ms: u64,
}

/// Fixed-capacity round-robin due-channel scheduler with at most one active transaction.
pub struct Scheduler<const N: usize> {
    /// Nonzero session identity shared by all tickets this instance issues.
    epoch: u64,
    /// Validated period/deadline pairs for 1..=8 channels.
    cadences: [Cadence; N],
    /// Next absolute due time per channel in milliseconds, rebased on actual completion.
    due_ms: [u64; N],
    /// Last successful actual receipt time per channel; failures do not advance it.
    successes: [Option<u64>; N],
    /// Required global quiet interval after every accepted completion, in milliseconds.
    gap_ms: u64,
    /// Earliest next admission time after the intertransaction gap, in milliseconds.
    ready_ms: u64,
    /// Latest observed monotonic milliseconds for reversal detection.
    last_ms: u64,
    /// Last allocated transaction generation; checked increments never wrap.
    generation: u64,
    /// Next channel from which the bounded round-robin due scan begins.
    cursor: usize,
    /// Outstanding transaction ticket, absent when idle or stopped.
    active: Option<Ticket>,
    /// Terminal latch requiring creation under a fresh session epoch before reuse.
    stopped: bool,
    /// Checked aggregate accepted completion latency.
    latency: Latency,
}
impl<const N: usize> Scheduler<N> {
    /// Returns a scheduler for nonzero `epoch` and validated `cadences`, with all channels
    /// initially due at boot-relative `now_ms`. `gap_ms` is the post-completion global gap.
    /// Invalid channel count/period/deadline or epoch returns Configuration; future
    /// time arithmetic involving the unrestricted gap is checked when a completion occurs.
    pub fn new(
        epoch: u64,
        cadences: [Cadence; N],
        gap_ms: u64,
        now_ms: u64,
    ) -> Result<Self, Error> {
        validate(&cadences)?;
        if epoch == 0 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            epoch,
            cadences,
            due_ms: [now_ms; N],
            successes: [None; N],
            gap_ms,
            ready_ms: now_ms,
            last_ms: now_ms,
            generation: 0,
            cursor: 0,
            active: None,
            stopped: false,
            latency: Latency::default(),
        })
    }

    /// At most N slots are inspected and one transaction is admitted per call.
    /// Advances `self` at boot-relative `now_ms`, returning the next due ticket, or
    /// `Ok(None)` while active, inside the global gap, or with no due channel.
    /// Reversed time, expired active work or identity/deadline overflow stops the
    /// scheduler and returns its error. Missed periods never create catch-up bursts.
    pub fn next(
        &mut self,
        now_ms: u64,
    ) -> Result<Option<Ticket>, Error> {
        self.tick(now_ms)?;
        if self.active.is_some() || now_ms < self.ready_ms {
            return Ok(None);
        }
        for offset in 0..N {
            let index = (self.cursor + offset) % N;
            if now_ms < self.due_ms[index] {
                continue;
            }
            let Some(generation) = self.generation.checked_add(1) else {
                return self.fail(Error::Exhausted);
            };
            let Some(deadline_ms) = now_ms.checked_add(self.cadences[index].deadline_ms) else {
                return self.fail(Error::Exhausted);
            };
            self.generation = generation;
            let ticket = Ticket {
                epoch: self.epoch,
                index,
                generation,
                started_ms: now_ms,
                deadline_ms,
            };
            self.active = Some(ticket);
            self.cursor = (index + 1) % N;
            return Ok(Some(ticket));
        }
        Ok(None)
    }

    /// Record the actual prompt completion, not a later publication/display time.
    /// Failure changes scheduling but never the last successful receipt stamp.
    /// Completes exact `ticket` in `self` at boot-relative `completed_ms`; `successful`
    /// controls only the last-success stamp. Returns unit after rebasing due/gap times,
    /// aggregating latency and clearing active work. Wrong ticket returns Token without
    /// mutation; deadline equality, reversal or arithmetic exhaustion stops the scheduler.
    pub fn complete(
        &mut self,
        ticket: Ticket,
        completed_ms: u64,
        successful: bool,
    ) -> Result<(), Error> {
        if self.active != Some(ticket) {
            return Err(Error::Token);
        }
        self.tick(completed_ms)?;
        let elapsed = completed_ms - ticket.started_ms;
        let Some(due_ms) = completed_ms.checked_add(self.cadences[ticket.index].period_ms) else {
            return self.fail(Error::Exhausted);
        };
        let Some(ready_ms) = completed_ms.checked_add(self.gap_ms) else {
            return self.fail(Error::Exhausted);
        };
        let Some(count) = self.latency.count.checked_add(1) else {
            return self.fail(Error::Exhausted);
        };
        let Some(total_ms) = self.latency.total_ms.checked_add(elapsed) else {
            return self.fail(Error::Exhausted);
        };
        self.due_ms[ticket.index] = due_ms;
        self.ready_ms = ready_ms;
        self.latency = Latency {
            count,
            total_ms,
            max_ms: self.latency.max_ms.max(elapsed),
        };
        if successful {
            self.successes[ticket.index] = Some(completed_ms);
        }
        self.active = None;
        Ok(())
    }

    /// Observes boot-relative `now_ms` in `self`, returning unit while live and before
    /// an active deadline. Reversal or deadline equality stops and clears active work;
    /// an already stopped instance returns Stopped. Idle elapsed time creates no tickets.
    pub fn tick(
        &mut self,
        now_ms: u64,
    ) -> Result<(), Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        if now_ms < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now_ms;
        if self.active.is_some_and(|ticket| now_ms >= ticket.deadline_ms) {
            return self.fail(Error::Timeout);
        }
        Ok(())
    }

    /// Returns `self`'s last successful completion time for channel `index`, in
    /// boot-relative milliseconds; absent/out-of-range indices return `None`.
    pub fn last_success_ms(
        &self,
        index: usize,
    ) -> Option<u64> {
        self.successes.get(index).copied().flatten()
    }

    /// Returns `self`'s aggregate latency snapshot, including unsuccessful accepted completions.
    pub const fn latency(&self) -> Latency { self.latency }

    /// Stop on disconnect or flash park. Recreate only for a fresh Session epoch.
    /// Clears `self`'s active ticket and permanently stops admission, returning unit;
    /// previously collected success/latency evidence is retained for diagnostics.
    pub fn stop(&mut self) {
        self.active = None;
        self.stopped = true;
    }

    /// Stops `self` and returns the supplied `error` for any required output type, retaining
    /// prior diagnostic history but discarding active work.
    fn fail<T>(
        &mut self,
        error: Error,
    ) -> Result<T, Error> {
        self.stop();
        Err(error)
    }
}

/// Estimated serialized link occupancy. Above 1000 means the selected periods
/// cannot all be met at this observed round-trip cost; this is not ECU timing.
/// Returns the sum of per-channel rounded-up permille costs for validated `cadences`,
/// observed `round_trip_ms` and post-completion `gap_ms`. Invalid cadence returns
/// Configuration; checked add/multiply/sum overflow returns Exhausted without mutation.
pub fn utilization_permille(
    cadences: &[Cadence],
    round_trip_ms: u64,
    gap_ms: u64,
) -> Result<u64, Error> {
    validate(cadences)?;
    let cost = round_trip_ms
        .checked_add(gap_ms)
        .and_then(|ms| ms.checked_mul(1000))
        .ok_or(Error::Exhausted)?;
    cadences.iter().try_fold(0_u64, |total, cadence| {
        total
            .checked_add(cost.div_ceil(cadence.period_ms))
            .ok_or(Error::Exhausted)
    })
}

/// Returns unit if `cadences` has 1..=8 channels, positive periods/deadlines and deadlines
/// no greater than session MAX_DEADLINE_MS; otherwise Configuration. No mutation occurs.
fn validate(cadences: &[Cadence]) -> Result<(), Error> {
    if cadences.is_empty()
        || cadences.len() > MAX_CHANNELS
        || cadences.iter().any(|cadence| {
            cadence.period_ms == 0 || cadence.deadline_ms == 0 || cadence.deadline_ms > super::session::MAX_DEADLINE_MS
        })
    {
        return Err(Error::Configuration);
    }
    Ok(())
}

#[cfg(test)]
/// Cadence, fairness, identity and temporal-failure regressions.
mod tests {
    use super::*;

    #[test]
    /// Returns unit after checking completion-relative scheduling, no stall catch-up and
    /// unsuccessful-completion latency without success-stamp advancement; panics on drift.
    fn completion_time_drives_cadence_and_stalls_do_not_replay_ticks() {
        let mut scheduler = Scheduler::new(
            1,
            [Cadence {
                period_ms: 100,
                deadline_ms: 50,
            }],
            5,
            0,
        )
        .unwrap();
        let ticket = scheduler.next(0).unwrap().unwrap();
        assert_eq!(scheduler.next(10).unwrap(), None);
        scheduler.complete(ticket, 40, true).unwrap();
        assert_eq!(scheduler.last_success_ms(0), Some(40));
        assert_eq!(scheduler.next(139).unwrap(), None);
        let next = scheduler.next(10_000).unwrap().unwrap();
        assert_eq!(next.started_ms(), 10_000);
        scheduler.complete(next, 10_020, false).unwrap();
        assert_eq!(scheduler.last_success_ms(0), Some(40));
        assert_eq!(scheduler.next(10_021).unwrap(), None);
        assert_eq!(
            scheduler.latency(),
            Latency {
                count: 2,
                total_ms: 60,
                max_ms: 40
            }
        );
    }

    #[test]
    /// Returns unit after checking due slow channels receive round-robin turns despite
    /// an always-due fast channel; panics on starvation/order drift.
    fn fair_due_channels_include_slow_values_under_fast_channel_load() {
        let mut scheduler = Scheduler::new(
            1,
            [
                Cadence {
                    period_ms: 1,
                    deadline_ms: 100,
                },
                Cadence {
                    period_ms: 1000,
                    deadline_ms: 100,
                },
                Cadence {
                    period_ms: 1000,
                    deadline_ms: 100,
                },
            ],
            0,
            0,
        )
        .unwrap();
        for (now, index) in [(0, 0), (50, 1), (100, 2), (150, 0)] {
            let ticket = scheduler.next(now).unwrap().unwrap();
            assert_eq!(ticket.index(), index);
            scheduler.complete(ticket, now + 40, true).unwrap();
        }
    }

    #[test]
    /// Returns unit after checking deadline equality, time reversal and explicit stop
    /// reject completion without publishing success; panics if admission resumes.
    fn late_completion_clock_regression_and_stop_do_not_publish() {
        for kind in 0..3 {
            let mut scheduler = Scheduler::new(
                1,
                [Cadence {
                    period_ms: 100,
                    deadline_ms: 50,
                }],
                0,
                10,
            )
            .unwrap();
            let ticket = scheduler.next(10).unwrap().unwrap();
            match kind {
                0 => assert_eq!(scheduler.complete(ticket, 60, true), Err(Error::Timeout)),
                1 => assert_eq!(scheduler.complete(ticket, 9, true), Err(Error::Clock)),
                _ => {
                    scheduler.stop();
                    assert!(scheduler.complete(ticket, 20, true).is_err());
                }
            }
            assert_eq!(scheduler.last_success_ms(0), None);
            assert!(scheduler.next(100).is_err());
        }
    }

    #[test]
    /// Returns unit after checking modeled occupancy, empty/zero-period rejection and
    /// arithmetic overflow; panics if invalid timing is silently admitted.
    fn latency_budget_exposes_unsustainable_fast_channel_load() {
        let channels = [Cadence {
            period_ms: 100,
            deadline_ms: 500,
        }; 8];
        assert_eq!(utilization_permille(&channels, 40, 5), Ok(3600));
        assert_eq!(utilization_permille(&channels, 10, 0), Ok(800));
        assert!(Scheduler::<0>::new(1, [], 0, 0).is_err());
        assert!(
            Scheduler::new(
                1,
                [Cadence {
                    period_ms: 0,
                    deadline_ms: 1
                }],
                0,
                0
            )
            .is_err()
        );
        assert!(utilization_permille(&channels, u64::MAX, 1).is_err());
    }

    #[test]
    /// Returns unit after checking an old owner's ticket cannot complete a fresh-epoch
    /// scheduler even with equal channel/timing; panics if epoch identity is ignored.
    fn recreated_owner_must_reject_previous_epoch_ticket() {
        let cadences = [Cadence {
            period_ms: 100,
            deadline_ms: 50,
        }];
        let mut first = Scheduler::new(1, cadences, 0, 0).unwrap();
        let old = first.next(0).unwrap().unwrap();
        first.stop();
        let mut second = Scheduler::new(2, cadences, 0, 0).unwrap();
        second.next(0).unwrap().unwrap();
        assert_eq!(second.complete(old, 10, true), Err(Error::Token));
    }
}
