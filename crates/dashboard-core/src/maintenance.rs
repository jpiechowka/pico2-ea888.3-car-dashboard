//! Generation-checked park/resume barrier. No lock, wait or hardware operation
//! lives here. The platform wraps each short state operation in one mutex.
use crate::health::{MAINTENANCE_MS, MaintenanceTicket};

/// Inclusive maximum milliseconds from ticket start to all Park acknowledgments.
pub const PREPARE_MS: u64 = 400;
/// Inclusive maximum synchronous-write phase milliseconds after admission.
pub const WRITE_MS: u64 = 500;

/// Copyable barrier identity derived from an affine supervisor maintenance ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// Supervisor-issued nonzero generation; replay never opens another window.
    generation: u64,
    /// Original monotonic ticket-start milliseconds, shared by all phase deadlines.
    started_ms: u64,
}
impl Request {
    /// Returns original ticket-start time in monotonic milliseconds.
    pub const fn started_ms(self) -> u64 { self.started_ms }
}

/// Generation plus phase; a Park acknowledgment cannot satisfy Resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Stop new work and drain owned I/O before acknowledging.
    Park(
        /// Immutable identity of the preparation window.
        Request,
    ),
    /// Restore owner operation, including any required transport requalification.
    Resume(
        /// Same identity and absolute deadline as the preceding preparation.
        Request,
    ),
}
/// Independently acknowledged producer; configured owners are frozen at construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// Acquisition/core1 application owner, responsible for draining its traffic.
    Acquisition,
    /// Input owner, responsible for completing encoder/I2C work.
    Input,
    /// Radio bus owner when linked, even if its runtime mode is disabled.
    Radio,
}
impl Owner {
    /// Returns this owner's unique bit in the required/acknowledged masks.
    const fn bit(self) -> u8 {
        match self {
            Self::Acquisition => 1,
            Self::Input => 2,
            Self::Radio => 4,
        }
    }
}
/// Barrier rejection; clock and write/resume deadline failures latch gate-wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Wrong/missing phase or request, nested begin, or repeated write admission.
    State,
    /// Ticket generation is no newer than one already used by this gate.
    Generation,
    /// Required phase acknowledgments have not all arrived.
    Pending,
    /// A required owner reported failure for the current phase.
    Owner,
    /// Initiator time moved backward; survives release and prevents future success.
    Clock,
    /// Preparation, write or absolute maintenance deadline was exceeded.
    Timeout,
}

/// Current request phase and its bounded acknowledgment/timing state.
struct Active {
    /// Immutable request paired with the active Park/Resume phase.
    command: Command,
    /// Required owners whose first response in this phase has been accepted.
    seen: u8,
    /// First phase error; preparation errors may be cleared solely to resume cleanup.
    error: Option<Error>,
    /// Monotonic write-admission milliseconds, absent before admission/after release.
    write_since: Option<u64>,
    // Only the initiator supplies time, avoiding cross-core sampled-time races.
    /// Most recent initiator time; owner responses deliberately carry no timestamps.
    last_ms: u64,
}
/// Short mutex-protected state operations for a single maintenance initiator.
/// This checks declarations/acknowledgments, not physical peripheral quiescence.
pub struct Gate {
    /// Greatest admitted supervisor generation, retained after completion.
    last_generation: u64,
    /// Active barrier or `None` when a new ticket may be admitted.
    active: Option<Active>,
    /// Permanent clock/write/resume failure, never cleared by release.
    fatal: Option<Error>,
    /// Required owner mask: acquisition/input, optionally radio.
    owners: u8,
}
impl Default for Gate {
    /// Returns an idle acquisition/input gate without a radio owner.
    fn default() -> Self { Self::new() }
}
impl Gate {
    /// Returns an idle gate requiring acquisition and input acknowledgments.
    pub const fn new() -> Self { Self::with_radio(false) }

    /// Freeze the required owners before any maintenance can begin. Even a
    /// configured-off radio task must acknowledge when its feature is linked.
    /// Returns an idle gate; `radio` adds the radio owner when true.
    pub const fn with_radio(radio: bool) -> Self {
        Self {
            last_generation: 0,
            active: None,
            fatal: None,
            owners: if radio { 7 } else { 3 },
        }
    }

    /// Opens Park for a fresh supervisor `ticket` and returns its copyable request.
    /// Does not consume the ticket; its original health owner retains it through
    /// cleanup. Returns latched failure, `State` for active work or `Generation`
    /// for replay. Successful admission permanently consumes this generation.
    pub fn begin(
        &mut self,
        ticket: &MaintenanceTicket,
    ) -> Result<Request, Error> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        if self.active.is_some() {
            return Err(Error::State);
        }
        if ticket.generation() <= self.last_generation {
            return Err(Error::Generation);
        }
        let request = Request {
            generation: ticket.generation(),
            started_ms: ticket.started_ms(),
        };
        self.last_generation = request.generation;
        self.active = Some(Active {
            command: Command::Park(request),
            seen: 0,
            error: None,
            write_since: None,
            last_ms: request.started_ms,
        });
        Ok(request)
    }

    /// Returns the current command even after failure, or `None` when inactive.
    pub fn command(&self) -> Option<Command> { self.active.as_ref().map(|active| active.command) }

    /// Only the first response for each owner and phase counts. A delayed park
    /// response cannot release a resume barrier, even for the same generation.
    /// Accepts `owner`'s `success` response to exact `command`; returns true when
    /// recorded, false for inactive/stale/duplicate/unrequired replies. A false
    /// success value latches phase failure and cannot be replaced by a retry.
    pub fn acknowledge(
        &mut self,
        command: Command,
        owner: Owner,
        success: bool,
    ) -> bool {
        let Some(active) = &mut self.active else {
            return false;
        };
        if command != active.command || active.seen & owner.bit() != 0 || self.owners & owner.bit() == 0 {
            return false;
        }
        active.seen |= owner.bit();
        if !success {
            active.error.get_or_insert(Error::Owner);
        }
        true
    }

    /// Checks phase `resume` (true for Resume, false for Park) at initiator `now`
    /// in monotonic milliseconds. Returns whether all owners acknowledged or a
    /// phase/owner/time error. Valid times advance the clock; preparation expiry
    /// permits release cleanup, while clock/resume expiry remains globally fatal.
    fn check(
        &mut self,
        now: u64,
        resume: bool,
    ) -> Result<bool, Error> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        let active = self.active.as_mut().ok_or(Error::State)?;
        if !resume && active.write_since.is_some() {
            return Err(Error::State);
        }
        let request = match (active.command, resume) {
            (Command::Park(request), false) | (Command::Resume(request), true) => request,
            _ => return Err(Error::State),
        };
        if let Some(error) = active.error {
            return Err(error);
        }
        if now < active.last_ms {
            self.fatal = Some(Error::Clock);
            return Err(Error::Clock);
        }
        active.last_ms = now;
        let limit = if resume { MAINTENANCE_MS } else { PREPARE_MS };
        let error = match now.checked_sub(request.started_ms) {
            None => Some(Error::Clock),
            Some(elapsed) if elapsed > limit => Some(Error::Timeout),
            _ => None,
        };
        if let Some(error) = error {
            if error == Error::Clock || resume {
                self.fatal = Some(error);
            }
            active.error = Some(error);
            return Err(error);
        }
        Ok(active.seen == self.owners)
    }

    /// Checks Park completion at monotonic `now` milliseconds. Returns true only
    /// when all owners succeeded within 400 ms; false while waiting, or an error.
    /// Once writing starts this phase check returns `State` rather than renewing it.
    pub fn ready(
        &mut self,
        now: u64,
    ) -> Result<bool, Error> {
        self.check(now, false)
    }

    /// Checks Resume at monotonic `now` milliseconds. Returns true only after all
    /// required acknowledgments within 1000 ms of original ticket start, false
    /// while waiting, or an error. Expired resumption permanently faults the gate.
    pub fn resumed(
        &mut self,
        now: u64,
    ) -> Result<bool, Error> {
        self.check(now, true)
    }

    /// Admits writing once at monotonic `now` milliseconds after successful Park.
    /// Returns success, `Pending` for missing acknowledgments, or phase/time error;
    /// starts a 500 ms write budget without extending the 1000 ms ticket budget.
    pub fn begin_write(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        if !self.ready(now)? {
            return Err(Error::Pending);
        }
        self.active.as_mut().ok_or(Error::State)?.write_since = Some(now);
        Ok(())
    }

    /// Rechecks exact `request` at monotonic `now` milliseconds before/after each
    /// synchronous operation. Returns success only within write and total budgets;
    /// clock/deadline failures latch permanently. This cannot interrupt blocking
    /// hardware work; the caller must retain ownership through its actual return.
    pub fn writing(
        &mut self,
        request: Request,
        now: u64,
    ) -> Result<(), Error> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        let active = self.active.as_mut().ok_or(Error::State)?;
        if active.command != Command::Park(request) {
            return Err(Error::State);
        }
        let since = active.write_since.ok_or(Error::State)?;
        if let Some(error) = active.error {
            return Err(error);
        }
        let error = if now < active.last_ms {
            Some(Error::Clock)
        } else if now - since > WRITE_MS || now - request.started_ms > MAINTENANCE_MS {
            Some(Error::Timeout)
        } else {
            None
        };
        active.last_ms = now;
        if let Some(error) = error {
            self.fatal = Some(error);
            active.error = Some(error);
            return Err(error);
        }
        Ok(())
    }

    /// Always allow cleanup of the matching preparation, including a denied
    /// or expired one. This never renews the monitor's absolute time budget.
    /// Changes exact Park `request` to Resume and returns success, or `State` for
    /// a mismatch. Clears phase responses/errors but preserves gate-wide faults.
    pub fn release(
        &mut self,
        request: Request,
    ) -> Result<(), Error> {
        let active = self.active.as_mut().ok_or(Error::State)?;
        if active.command != Command::Park(request) {
            return Err(Error::State);
        }
        *active = Active {
            command: Command::Resume(request),
            seen: 0,
            error: None,
            write_since: None,
            last_ms: active.last_ms,
        };
        Ok(())
    }

    /// Ends exact Resume `request` at monotonic `now` milliseconds after all owners
    /// succeeded. Returns success and clears active work, or an error retaining
    /// the barrier. The caller separately returns its affine ticket to the monitor.
    pub fn finish(
        &mut self,
        request: Request,
        now: u64,
    ) -> Result<(), Error> {
        if self.command() != Some(Command::Resume(request)) {
            return Err(Error::State);
        }
        if !self.resumed(now)? {
            return Err(Error::Pending);
        }
        self.active = None;
        Ok(())
    }
}
