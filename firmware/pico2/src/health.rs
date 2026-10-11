//! Watchdog eligibility follows useful task progress, not timer wakeups alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub acquisition: u32,
    pub rendering: u32,
    pub completed: u32,
    pub display_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Clock,
    Supervisor,
    Acquisition,
    Rendering,
    Display,
}

pub struct ProgressMonitor {
    previous: Progress,
    last_check_ms: u64,
    acquisition_ms: u64,
    rendering_ms: u64,
    completed_ms: u64,
    fault: Option<Fault>,
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
            completed_ms: now_ms,
            fault: None,
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
        if now_ms - self.last_check_ms > 250 {
            return self.fail(Fault::Supervisor);
        }
        if current.acquisition != self.previous.acquisition {
            self.acquisition_ms = now_ms;
        }
        if current.rendering != self.previous.rendering {
            self.rendering_ms = now_ms;
        }
        if current.completed != self.previous.completed || (current.display_pending && !self.previous.display_pending) {
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
        if current.display_pending && now_ms - self.completed_ms >= 250 {
            return self.fail(Fault::Display);
        }
        Ok(())
    }

    fn fail(
        &mut self,
        fault: Fault,
    ) -> Result<(), Fault> {
        self.fault = Some(fault);
        Err(fault)
    }
}
