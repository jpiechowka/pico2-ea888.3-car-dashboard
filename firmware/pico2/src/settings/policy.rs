//! Single-owner coalescing and finite retry policy. Tickets are immutable, move
//! only, and must be returned to the policy instance that issued them.
use super::Settings;

const DEBOUNCE_MS: u64 = 2_000;
const MIN_INTERVAL_MS: u64 = 30_000;
const MAX_ATTEMPTS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Clean,
    Dirty,
    Saving,
    Failed,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    Clock,
    Exhausted,
    StaleTicket,
}

#[derive(Debug)]
pub struct SaveTicket {
    id: u64,
    settings: Settings,
}
impl SaveTicket {
    pub const fn settings(&self) -> Settings { self.settings }
}

pub struct Policy {
    desired: Settings,
    saved: Settings,
    changed_at: u64,
    last_now: u64,
    last_attempt: Option<u64>,
    next_id: u64,
    in_flight: Option<u64>,
    attempts: u8,
    uncertain: bool,
    enabled: bool,
    fault: Option<PolicyError>,
}
impl Policy {
    pub const fn new(
        initial: Settings,
        now: u64,
    ) -> Self {
        Self {
            desired: initial,
            saved: initial,
            changed_at: now,
            last_now: now,
            last_attempt: None,
            next_id: 0,
            in_flight: None,
            attempts: 0,
            uncertain: false,
            enabled: true,
            fault: None,
        }
    }

    pub const fn disabled(
        initial: Settings,
        now: u64,
    ) -> Self {
        Self {
            enabled: false,
            ..Self::new(initial, now)
        }
    }

    fn clock(
        &mut self,
        now: u64,
    ) -> Result<(), PolicyError> {
        if let Some(fault) = self.fault {
            return Err(fault);
        }
        if now < self.last_now {
            self.fault = Some(PolicyError::Clock);
            return Err(PolicyError::Clock);
        }
        self.last_now = now;
        Ok(())
    }

    pub fn set(
        &mut self,
        desired: Settings,
        now: u64,
    ) -> Result<(), PolicyError> {
        self.clock(now)?;
        if desired != self.desired {
            self.desired = desired;
            self.changed_at = now;
            self.attempts = 0;
        }
        Ok(())
    }

    pub fn status(&self) -> Status {
        if !self.enabled || self.fault.is_some() {
            Status::Disabled
        } else if self.in_flight.is_some() {
            Status::Saving
        } else if self.uncertain {
            Status::Failed
        } else if self.desired != self.saved {
            Status::Dirty
        } else {
            Status::Clean
        }
    }

    pub fn begin(
        &mut self,
        now: u64,
    ) -> Result<Option<SaveTicket>, PolicyError> {
        self.clock(now)?;
        if !self.enabled
            || self.in_flight.is_some()
            || (self.desired == self.saved && !self.uncertain)
            || self.attempts >= MAX_ATTEMPTS
            || now - self.changed_at < DEBOUNCE_MS
            || self.last_attempt.is_some_and(|last| now - last < MIN_INTERVAL_MS)
        {
            return Ok(None);
        }
        let Some(id) = self.next_id.checked_add(1) else {
            self.fault = Some(PolicyError::Exhausted);
            return Err(PolicyError::Exhausted);
        };
        self.next_id = id;
        self.in_flight = Some(id);
        self.last_attempt = Some(now);
        self.attempts += 1;
        Ok(Some(SaveTicket {
            id,
            settings: self.desired,
        }))
    }

    /// Failed readback does not prove nothing was written. Keep uncertainty
    /// until a verified transaction, including when UI reverts to old settings.
    pub fn finish(
        &mut self,
        ticket: SaveTicket,
        success: bool,
        now: u64,
    ) -> Result<(), PolicyError> {
        if self.in_flight != Some(ticket.id) {
            return Err(PolicyError::StaleTicket);
        }
        self.clock(now)?;
        self.in_flight = None;
        if success {
            self.saved = ticket.settings;
            self.uncertain = false;
        } else {
            self.uncertain = true;
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;
    #[test]
    fn exhausted_and_replayed_ticket_cannot_start_or_acknowledge_another_save() {
        let mut policy = Policy::new(Settings::default(), 0);
        policy
            .set(Settings::new(50, 100, false, crate::render::FpsMode::Off).unwrap(), 0)
            .unwrap();
        let ticket = policy.begin(2000).unwrap().unwrap();
        let stale = SaveTicket {
            id: ticket.id,
            settings: ticket.settings,
        };
        policy.finish(ticket, false, 2001).unwrap();
        let current = policy.begin(32000).unwrap().unwrap();
        assert_eq!(policy.finish(stale, true, 32000), Err(PolicyError::StaleTicket));
        assert_eq!(policy.status(), Status::Saving);
        policy.finish(current, false, 32001).unwrap();
        policy.next_id = u64::MAX;
        assert_eq!(policy.begin(62000).err(), Some(PolicyError::Exhausted));
        assert_eq!(policy.status(), Status::Disabled);
    }
}
