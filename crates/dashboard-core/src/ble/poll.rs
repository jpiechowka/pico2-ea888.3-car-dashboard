//! Completion-based cadence without replaying missed polling ticks.

pub const MAX_CHANNELS: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cadence {
    pub period_ms: u64,
    pub deadline_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Clock,
    Timeout,
    Token,
    Exhausted,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    epoch: u64,
    index: usize,
    generation: u64,
    started_ms: u64,
    deadline_ms: u64,
}
impl Ticket {
    pub const fn index(self) -> usize { self.index }

    pub const fn started_ms(self) -> u64 { self.started_ms }

    pub const fn deadline_ms(self) -> u64 { self.deadline_ms }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Latency {
    pub count: u64,
    pub total_ms: u64,
    pub max_ms: u64,
}

pub struct Scheduler<const N: usize> {
    epoch: u64,
    cadences: [Cadence; N],
    due_ms: [u64; N],
    successes: [Option<u64>; N],
    gap_ms: u64,
    ready_ms: u64,
    last_ms: u64,
    generation: u64,
    cursor: usize,
    active: Option<Ticket>,
    stopped: bool,
    latency: Latency,
}
impl<const N: usize> Scheduler<N> {
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

    pub fn last_success_ms(
        &self,
        index: usize,
    ) -> Option<u64> {
        self.successes.get(index).copied().flatten()
    }

    pub const fn latency(&self) -> Latency { self.latency }

    /// Stop on disconnect or flash park. Recreate only for a fresh Session epoch.
    pub fn stop(&mut self) {
        self.active = None;
        self.stopped = true;
    }

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
mod tests {
    use super::*;

    #[test]
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
