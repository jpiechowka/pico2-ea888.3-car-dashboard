//! Single-owner coalescing and finite retry policy. Tickets are immutable, move
//! only, and must be returned to the policy instance that issued them.
use super::Settings;

/// Minimum milliseconds since the latest desired-setting change before saving.
const DEBOUNCE_MS: u64 = 2_000;
/// Minimum milliseconds between save-attempt starts, including failed attempts.
const MIN_INTERVAL_MS: u64 = 30_000;
/// Maximum attempts for one desired-state episode; a new desired value resets the count.
const MAX_ATTEMPTS: u8 = 3;

/// Persistence feedback derived from policy state; uncertainty takes precedence over dirtiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Desired settings equal the verified baseline with no uncertain outcome.
    Clean,
    /// Desired settings differ from baseline and no attempt is active.
    Dirty,
    /// One immutable save ticket is outstanding.
    Saving,
    /// Last attempt failed verification; a write may nevertheless have occurred.
    Failed,
    /// Persistence is disabled or a terminal clock/identity fault prevents further attempts.
    Disabled,
}
impl Status {
    /// Returns `self`'s compact UI label; `has_record` distinguishes SAVED from DEFAULT
    /// only for Clean. Other states do not claim any particular flash contents.
    pub const fn label(
        self,
        has_record: bool,
    ) -> &'static str {
        match self {
            Self::Clean if has_record => "SAVED",
            Self::Clean => "DEFAULT",
            Self::Dirty => "UNSAVED",
            Self::Saving => "SAVING",
            Self::Failed => "FAILED",
            Self::Disabled => "VOLATILE",
        }
    }
}
/// Policy coordination failure, independent of the storage adapter's write error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// Time moved backwards; persistence is permanently disabled for this instance.
    Clock,
    /// Ticket identity exhausted; no old ID is reused.
    Exhausted,
    /// Completion ticket does not match this policy's outstanding identity.
    StaleTicket,
}

/// Move-only immutable desired-state snapshot; return to the issuing policy exactly once.
#[derive(Debug)]
pub struct SaveTicket {
    /// Checked nonreused identity within its issuing policy.
    id: u64,
    /// Exact settings to write, unaffected by later edits to desired state.
    settings: Settings,
}
impl SaveTicket {
    /// Returns `self`'s immutable settings snapshot for the journal transaction.
    pub const fn settings(&self) -> Settings { self.settings }
}

/// Single-owner bounded coalescing/retry state; does not perform flash operations itself.
pub struct Policy {
    /// Latest requested runtime persistence state.
    desired: Settings,
    /// Last verified successful settings or initial caller-supplied baseline.
    saved: Settings,
    /// Monotonic milliseconds of the latest desired-value change.
    changed_at: u64,
    /// Most recently admitted caller time in milliseconds.
    last_now: u64,
    /// Milliseconds when the most recent attempt began, including failed attempts.
    last_attempt: Option<u64>,
    /// Last allocated immutable ticket identity, checked before incrementing.
    next_id: u64,
    /// Outstanding ticket identity; prevents overlapping saves.
    in_flight: Option<u64>,
    /// Attempts since desired state last changed, at most three.
    attempts: u8,
    /// Failed verification left actual persisted state unknown, even after reverting desired values.
    uncertain: bool,
    /// Caller permits persistence; false still allows tracking desired changes.
    enabled: bool,
    /// Latched clock/exhaustion fault blocking future persistence operations.
    fault: Option<PolicyError>,
}
impl Policy {
    /// Returns an enabled policy with caller-trusted baseline `initial` at monotonic
    /// `now` milliseconds. Starts Clean with no pending save; does not inspect flash.
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

    /// Returns volatile-only policy for baseline `initial` at monotonic `now` milliseconds.
    /// Desired edits remain trackable but no save ticket can be issued.
    pub const fn disabled(
        initial: Settings,
        now: u64,
    ) -> Self {
        Self {
            enabled: false,
            ..Self::new(initial, now)
        }
    }

    /// Advances `self` to monotonic `now` milliseconds, returning unit or the latched
    /// fault. Time reversal latches Clock permanently; it does not clear an outstanding ticket.
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

    /// Replaces `self`'s requested `desired` settings at monotonic `now` milliseconds,
    /// returning unit after clock admission. A changed value restarts debounce and the
    /// attempt count without changing saved baseline, uncertainty or an in-flight ticket.
    /// Clock/fault rejection leaves desired state unchanged.
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

    /// Returns `self`'s feedback with priority Disabled, Saving, Failed, Dirty, Clean.
    /// Equal desired/saved values cannot clear a previously uncertain write outcome.
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

    /// Returns one immutable save ticket for `self` at monotonic `now` milliseconds,
    /// or `None` when disabled, already saving, clean, retry-exhausted, inside two-second
    /// debounce or inside the 30-second attempt interval. Admitted attempts consume one
    /// of three retries and a checked identity before I/O; terminal clock/ID errors propagate.
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
    /// Consumes issuing-policy `ticket` at monotonic `now`, using `success` only for a
    /// fully verified journal result. Returns unit after releasing the fence and updating
    /// baseline/uncertainty. A stale ticket is rejected before clock mutation; a clock
    /// fault leaves the fence retained and disables subsequent saves.
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
/// Native-only private-state regression for nonreused policy tickets.
mod tests {
    use super::*;
    #[test]
    /// Returns unit after checking stale completion cannot acknowledge a newer attempt
    /// and exhausted identities disable persistence; panics if either fence is bypassed.
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
