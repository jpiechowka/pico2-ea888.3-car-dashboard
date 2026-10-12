//! Watchdog eligibility follows useful task progress, not timer wakeups alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub acquisition: u32,
    pub rendering: u32,
    pub input: u32,
    pub completed: u32,
    pub submitted: u32,
    pub display_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Clock,
    Supervisor,
    Acquisition,
    Rendering,
    Input,
    Display,
    Maintenance,
}

/// Includes cooperative drain, synchronous flash operation and resumption.
/// The separate hardware watchdog remains armed for two seconds throughout.
pub const MAINTENANCE_MS: u64 = 1000;

/// Affine permit, returned only to the single monitor that issued it.
/// Neither copying a permit nor reconstructing it from a generation is allowed.
#[derive(Debug)]
pub struct MaintenanceTicket {
    generation: u64,
    started_ms: u64,
}
impl MaintenanceTicket {
    pub const fn generation(&self) -> u64 { self.generation }

    pub const fn started_ms(&self) -> u64 { self.started_ms }
}

struct Maintenance {
    generation: u64,
    started_ms: u64,
}

pub struct ProgressMonitor {
    previous: Progress,
    last_check_ms: u64,
    acquisition_ms: u64,
    rendering_ms: u64,
    input_ms: u64,
    completed_ms: u64,
    fault: Option<Fault>,
    maintenance: Option<Maintenance>,
    next_maintenance: u64,
    resumed: Option<Progress>,
}

impl ProgressMonitor {
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

    pub const fn feeding_allowed(&self) -> bool { self.fault.is_none() && self.maintenance.is_none() }

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

    fn fail<T>(
        &mut self,
        fault: Fault,
    ) -> Result<T, Fault> {
        self.fault = Some(fault);
        Err(fault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const IDLE: Progress = Progress {
        acquisition: 0,
        rendering: 0,
        input: 0,
        completed: 0,
        submitted: 0,
        display_pending: false,
    };

    #[test]
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
