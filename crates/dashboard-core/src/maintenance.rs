//! Generation-checked park/resume barrier. No lock, wait or hardware operation
//! lives here. The platform wraps each short state operation in one mutex.
use crate::health::{MAINTENANCE_MS, MaintenanceTicket};

pub const PREPARE_MS: u64 = 400;
pub const WRITE_MS: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    generation: u64,
    started_ms: u64,
}
impl Request {
    pub const fn started_ms(self) -> u64 { self.started_ms }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Park(Request),
    Resume(Request),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Acquisition,
    Input,
    Radio,
}
impl Owner {
    const fn bit(self) -> u8 {
        match self {
            Self::Acquisition => 1,
            Self::Input => 2,
            Self::Radio => 4,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    State,
    Generation,
    Pending,
    Owner,
    Clock,
    Timeout,
}

struct Active {
    command: Command,
    seen: u8,
    error: Option<Error>,
    write_since: Option<u64>,
    // Only the initiator supplies time, avoiding cross-core sampled-time races.
    last_ms: u64,
}
pub struct Gate {
    last_generation: u64,
    active: Option<Active>,
    fatal: Option<Error>,
    owners: u8,
}
impl Default for Gate {
    fn default() -> Self { Self::new() }
}
impl Gate {
    pub const fn new() -> Self { Self::with_radio(false) }

    /// Freeze the required owners before any maintenance can begin. Even a
    /// configured-off radio task must acknowledge when its feature is linked.
    pub const fn with_radio(radio: bool) -> Self {
        Self {
            last_generation: 0,
            active: None,
            fatal: None,
            owners: if radio { 7 } else { 3 },
        }
    }

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

    pub fn command(&self) -> Option<Command> { self.active.as_ref().map(|active| active.command) }

    /// Only the first response for each owner and phase counts. A delayed park
    /// response cannot release a resume barrier, even for the same generation.
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

    pub fn ready(
        &mut self,
        now: u64,
    ) -> Result<bool, Error> {
        self.check(now, false)
    }

    pub fn resumed(
        &mut self,
        now: u64,
    ) -> Result<bool, Error> {
        self.check(now, true)
    }

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
