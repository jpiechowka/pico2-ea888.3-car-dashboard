//! Watchdog eligibility follows useful task progress, not timer wakeups alone.
/// Retained reset-stage encoding and conservative watchdog/ROM reboot evidence.
pub mod reset;
/// Useful owner progress counters sampled coherently by the watchdog supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Acquisition work completion count; inequality is progress, including counter wrap.
    pub acquisition: u32,
    /// Completed render-owner work count, including deliberate unchanged-scene decisions.
    pub rendering: u32,
    /// Completed input-owner work count, including bounded absent-device retries.
    pub input: u32,
    /// Successful display-transfer completion count.
    pub completed: u32,
    /// Display transfer submission count, used to start each pending-transfer window.
    pub submitted: u32,
    /// At least one display transfer remains outstanding and needs a completion deadline.
    pub display_pending: bool,
}

/// Latched supervisor failure; later task progress cannot restore feed eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Monotonic observation time regressed.
    Clock,
    /// Supervisor check interval exceeded 250 ms outside maintenance.
    Supervisor,
    /// Acquisition work did not advance for at least 500 ms.
    Acquisition,
    /// Rendering work did not advance for at least 500 ms.
    Rendering,
    /// Input work did not advance for at least 500 ms.
    Input,
    /// Pending transfer stalled for 250 ms or display changed during maintenance.
    Display,
    /// Maintenance deadline, permit identity or required resumed-owner progress failed.
    Maintenance,
}

/// Includes cooperative drain, synchronous flash operation and resumption.
/// The separate hardware watchdog remains armed for two seconds throughout.
pub const MAINTENANCE_MS: u64 = 1000;

/// Affine permit, returned only to the single monitor that issued it.
/// Neither copying a permit nor reconstructing it from a generation is allowed.
#[derive(Debug)]
pub struct MaintenanceTicket {
    /// Nonreused permit identity local to the issuing monitor.
    generation: u64,
    /// Boot-relative milliseconds when the bounded maintenance window began.
    started_ms: u64,
}
impl MaintenanceTicket {
    /// Returns `self`'s immutable maintenance identity for external coordination.
    pub const fn generation(&self) -> u64 { self.generation }

    /// Returns `self`'s start time in boot-relative milliseconds.
    pub const fn started_ms(&self) -> u64 { self.started_ms }
}

/// Monitor-owned counterpart of its outstanding affine maintenance permit.
struct Maintenance {
    /// Expected returned permit identity.
    generation: u64,
    /// Original window start in boot-relative milliseconds, never refreshed by observation.
    started_ms: u64,
}

/// Single-owner watchdog eligibility model with bounded maintenance exemption and sticky failure.
pub struct ProgressMonitor {
    /// Previous ordinary progress snapshot; frozen while maintenance is active.
    previous: Progress,
    /// Most recent checked boot-relative time in milliseconds.
    last_check_ms: u64,
    /// Milliseconds when acquisition count last changed or a completed window rebased it.
    acquisition_ms: u64,
    /// Milliseconds when rendering count last changed or a completed window rebased it.
    rendering_ms: u64,
    /// Milliseconds when input count last changed or a completed window rebased it.
    input_ms: u64,
    /// Milliseconds of last submission/completion/new pending transition or maintenance rebase.
    completed_ms: u64,
    /// First observed failure, permanently preventing watchdog feeding.
    fault: Option<Fault>,
    /// Outstanding bounded maintenance window, suppressing feed eligibility.
    maintenance: Option<Maintenance>,
    /// Next checked permit generation, beginning at one and never wrapping.
    next_maintenance: u64,
    /// Snapshot at last successful maintenance end; all useful owners must advance before another.
    resumed: Option<Progress>,
}

impl ProgressMonitor {
    /// Returns a healthy monitor anchored at boot-relative `now_ms` milliseconds with
    /// coherent `initial` counters. No hardware watchdog is fed or configured here.
    pub const fn new(
        now_ms: u64,
        initial: Progress,
    ) -> Self {
        Self {
            previous: initial,
            last_check_ms: now_ms,
            acquisition_ms: now_ms,
            rendering_ms: now_ms,
            input_ms: now_ms,
            completed_ms: now_ms,
            fault: None,
            maintenance: None,
            next_maintenance: 1,
            resumed: None,
        }
    }

    /// Checks coherent `current` against `self` at boot-relative `now_ms` milliseconds,
    /// returning unit while progress remains valid. Outside maintenance, supervisor gaps
    /// over 250 ms, owner inactivity at least 500 ms and pending display inactivity at
    /// least 250 ms latch faults. During maintenance, display must remain idle/unchanged
    /// and total elapsed must not exceed 1000 ms; successful checks still prohibit feeds.
    /// Reversed time or prior fault fails permanently. Counter inequality recognizes wrap;
    /// callers must not permit an entire unseen wrap between snapshots.
    pub fn observe(
        &mut self,
        now_ms: u64,
        current: Progress,
    ) -> Result<(), Fault> {
        if let Some(fault) = self.fault {
            return Err(fault);
        }
        if now_ms < self.last_check_ms {
            return self.fail(Fault::Clock);
        }
        if let Some(window) = &self.maintenance {
            if now_ms - window.started_ms > MAINTENANCE_MS {
                return self.fail(Fault::Maintenance);
            }
            if current.display_pending
                || current.submitted != self.previous.submitted
                || current.completed != self.previous.completed
            {
                return self.fail(Fault::Display);
            }
            // Observe time without refreshing progress or permitting a feed.
            self.last_check_ms = now_ms;
            return Ok(());
        }
        if now_ms - self.last_check_ms > 250 {
            return self.fail(Fault::Supervisor);
        }
        if current.acquisition != self.previous.acquisition {
            self.acquisition_ms = now_ms;
        }
        if current.rendering != self.previous.rendering {
            self.rendering_ms = now_ms;
        }
        if current.input != self.previous.input {
            self.input_ms = now_ms;
        }
        if current.completed != self.previous.completed
            || current.submitted != self.previous.submitted
            || (current.display_pending && !self.previous.display_pending)
        {
            self.completed_ms = now_ms;
        }
        self.previous = current;
        self.last_check_ms = now_ms;
        if now_ms - self.acquisition_ms >= 500 {
            return self.fail(Fault::Acquisition);
        }
        if now_ms - self.rendering_ms >= 500 {
            return self.fail(Fault::Rendering);
        }
        if now_ms - self.input_ms >= 500 {
            return self.fail(Fault::Input);
        }
        if current.display_pending && now_ms - self.completed_ms >= 250 {
            return self.fail(Fault::Display);
        }
        Ok(())
    }

    /// Returns whether `self` has neither fault nor active maintenance; caller must first
    /// observe current progress. This query does not itself validate elapsed time or feed hardware.
    pub const fn feeding_allowed(&self) -> bool { self.fault.is_none() && self.maintenance.is_none() }

    /// Returns an affine maintenance permit after checking `current` at boot-relative
    /// `now_ms` against `self`. Requires idle display, no other window, and acquisition/
    /// rendering/input progress since the last window. Identity exhaustion or violated
    /// preconditions latch a fault. Successful admission suppresses feeds until checked end.
    pub fn begin_maintenance(
        &mut self,
        now_ms: u64,
        current: Progress,
    ) -> Result<MaintenanceTicket, Fault> {
        self.observe(now_ms, current)?;
        if self.maintenance.is_some() {
            return self.fail(Fault::Maintenance);
        }
        if current.display_pending {
            return self.fail(Fault::Display);
        }
        // Repeated windows cannot manufacture task liveness. All owners must
        // have run again since the last completed window, even on save failure.
        if self.resumed.is_some_and(|previous| {
            previous.acquisition == current.acquisition
                || previous.rendering == current.rendering
                || previous.input == current.input
        }) {
            return self.fail(Fault::Maintenance);
        }
        let Some(next) = self.next_maintenance.checked_add(1) else {
            return self.fail(Fault::Maintenance);
        };
        let generation = self.next_maintenance;
        self.next_maintenance = next;
        self.maintenance = Some(Maintenance {
            generation,
            started_ms: now_ms,
        });
        Ok(MaintenanceTicket {
            generation,
            started_ms: now_ms,
        })
    }

    /// Consumes exact issuing-monitor `ticket` after checking `current` at boot-relative
    /// `now_ms`. Returns unit after closing `self`'s window and rebasing liveness clocks
    /// once; acquisition sample timestamps are untouched. Wrong/replayed permit or
    /// progress/deadline failures latch a fault and never restore feed eligibility.
    pub fn end_maintenance(
        &mut self,
        ticket: MaintenanceTicket,
        now_ms: u64,
        current: Progress,
    ) -> Result<(), Fault> {
        self.observe(now_ms, current)?;
        if !self
            .maintenance
            .as_ref()
            .is_some_and(|window| window.generation == ticket.generation && window.started_ms == ticket.started_ms)
        {
            return self.fail(Fault::Maintenance);
        }
        self.maintenance = None;
        self.resumed = Some(current);
        self.previous = current;
        // One liveness rebase, never a change to sensor acquisition timestamps.
        self.acquisition_ms = now_ms;
        self.rendering_ms = now_ms;
        self.input_ms = now_ms;
        self.completed_ms = now_ms;
        Ok(())
    }

    /// Latches `fault` in `self` and returns `Err(fault)` for any caller output type.
    /// Does not release maintenance ownership or alter hardware watchdog state.
    fn fail<T>(
        &mut self,
        fault: Fault,
    ) -> Result<T, Fault> {
        self.fault = Some(fault);
        Err(fault)
    }
}

#[cfg(test)]
/// Private permit-exhaustion and replay regressions.
mod tests {
    use super::*;
    /// Zeroed owner counters with no outstanding display transfer for permit tests.
    const IDLE: Progress = Progress {
        acquisition: 0,
        rendering: 0,
        input: 0,
        completed: 0,
        submitted: 0,
        display_pending: false,
    };

    #[test]
    /// Returns unit after checking identity exhaustion, mismatched and replayed permits
    /// permanently prohibit feeds; panics if maintenance can recreate healthy state.
    fn maintenance_generation_exhaustion_and_replayed_permits_fail_closed() {
        let mut monitor = ProgressMonitor::new(0, IDLE);
        monitor.next_maintenance = u64::MAX;
        assert!(monitor.begin_maintenance(0, IDLE).is_err());
        assert!(!monitor.feeding_allowed());
        let mut monitor = ProgressMonitor::new(0, IDLE);
        let ticket = monitor.begin_maintenance(0, IDLE).unwrap();
        let forged = MaintenanceTicket {
            generation: ticket.generation + 1,
            started_ms: 0,
        };
        assert_eq!(monitor.end_maintenance(forged, 1, IDLE), Err(Fault::Maintenance));
        assert_eq!(monitor.end_maintenance(ticket, 2, IDLE), Err(Fault::Maintenance));
        let mut monitor = ProgressMonitor::new(0, IDLE);
        let ticket = monitor.begin_maintenance(0, IDLE).unwrap();
        let replay = MaintenanceTicket {
            generation: ticket.generation,
            started_ms: 0,
        };
        monitor.end_maintenance(ticket, 1, IDLE).unwrap();
        assert_eq!(monitor.end_maintenance(replay, 2, IDLE), Err(Fault::Maintenance));
    }
}
