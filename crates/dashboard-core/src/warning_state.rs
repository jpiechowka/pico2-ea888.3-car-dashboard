//! Shared command and presentation contracts. Threshold evaluation stays in alarms.
use crate::alarms::Status;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningCommand {
    pub request_id: u64,
    pub run_id: u64,
    pub observed_event_serial: u64,
    pub rules: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CommandResult {
    pub request_id: u64,
    pub accepted: u64,
    pub rejected: u64,
}
impl CommandResult {
    pub const EMPTY: Self = Self {
        request_id: 0,
        accepted: 0,
        rejected: 0,
    };

    pub const fn rejected(command: WarningCommand) -> Self {
        Self {
            request_id: command.request_id,
            accepted: 0,
            rejected: command.rules,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    Busy,
    Exhausted,
    InvalidRun,
}
/// Caller retains exactly one request until its result arrives; retries reuse its bytes.
#[derive(Default)]
pub struct WarningRequests {
    last_id: u64,
    pending: Option<WarningCommand>,
}
impl WarningRequests {
    pub const fn new() -> Self {
        Self {
            last_id: 0,
            pending: None,
        }
    }

    pub fn request(
        &mut self,
        run_id: u64,
        observed_event_serial: u64,
        rules: u64,
    ) -> Result<WarningCommand, RequestError> {
        if self.pending.is_some() {
            return Err(RequestError::Busy);
        }
        if run_id == 0 {
            return Err(RequestError::InvalidRun);
        }
        let request_id = self.last_id.checked_add(1).ok_or(RequestError::Exhausted)?;
        let command = WarningCommand {
            request_id,
            run_id,
            observed_event_serial,
            rules,
        };
        self.last_id = request_id;
        self.pending = Some(command);
        Ok(command)
    }

    pub const fn pending(&self) -> Option<WarningCommand> { self.pending }

    pub fn complete(
        &mut self,
        result: CommandResult,
    ) -> bool {
        if self.pending.is_some_and(|c| {
            c.request_id == result.request_id
                && result.accepted & result.rejected == 0
                && result.accepted | result.rejected == c.rules
        }) {
            self.pending = None;
            true
        } else {
            false
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningEligibility {
    pub status: Status,
    pub latched: bool,
    pub cleared: bool,
    pub acknowledged: bool,
    pub snoozed: bool,
}
impl WarningEligibility {
    pub const fn visible(self) -> bool { self.latched || matches!(self.status, Status::Active) }

    pub const fn attention(self) -> bool { self.visible() && !self.snoozed }

    pub const fn label(self) -> &'static str {
        if self.latched && matches!(self.status, Status::Unavailable) {
            "LATCHED / DATA STALE"
        } else if self.latched && self.cleared {
            "CLEARED / ACK"
        } else if matches!(self.status, Status::Active) && self.acknowledged {
            "ACTIVE / ACKNOWLEDGED"
        } else if matches!(self.status, Status::Active) {
            "ACTIVE"
        } else if self.latched {
            "LATCHED"
        } else if self.acknowledged {
            "ACKNOWLEDGED"
        } else {
            ""
        }
    }
}
const _: () = assert!(core::mem::size_of::<WarningCommand>() == 32);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_exhaustion_never_wraps_or_overwrites_pending() {
        let mut requests = WarningRequests {
            last_id: u64::MAX - 1,
            pending: None,
        };
        let command = requests.request(1, 1, 1).unwrap();
        assert_eq!(command.request_id, u64::MAX);
        assert_eq!(requests.request(1, 1, 1), Err(RequestError::Busy));
        assert!(requests.complete(CommandResult {
            request_id: u64::MAX,
            accepted: 1,
            rejected: 0
        }));
        assert_eq!(requests.request(1, 1, 1), Err(RequestError::Exhausted));
        assert_eq!(requests.pending(), None);
    }
}
