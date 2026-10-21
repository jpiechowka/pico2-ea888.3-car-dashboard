//! Shared command and presentation contracts. Threshold evaluation stays in alarms.
use crate::alarms::Status;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Acknowledgment request bound to one source run and observed warning episode frontier.
pub struct WarningCommand {
    /// Nonzero requester-issued identity, retained unchanged across retries.
    pub request_id: u64,
    /// Nonzero warning producer run; prevents acknowledgment across source replacement.
    pub run_id: u64,
    /// Latest warning event observed when the completed page was captured.
    pub observed_event_serial: u64,
    /// Bitmask of requested widget/rule indices, at most 64 bits.
    pub rules: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
/// Producer's disjoint accepted/rejected partition of one request's rule mask.
pub struct CommandResult {
    /// Request identity echoed by the producer; zero denotes no result.
    pub request_id: u64,
    /// Requested rules accepted for acknowledgment.
    pub accepted: u64,
    /// Requested rules rejected because their source/episode/state was ineligible.
    pub rejected: u64,
}
impl CommandResult {
    /// No completed request and no affected rules.
    pub const EMPTY: Self = Self {
        request_id: 0,
        accepted: 0,
        rejected: 0,
    };

    /// Returns total rejection of `command`, preserving its identity and complete rule mask.
    pub const fn rejected(command: WarningCommand) -> Self {
        Self {
            request_id: command.request_id,
            accepted: 0,
            rejected: command.rules,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Local admission failures; no request identity is consumed on these errors.
pub enum RequestError {
    /// Another request still owns the sole pending slot.
    Busy,
    /// The monotonic request identity reached `u64::MAX` and cannot be reused.
    Exhausted,
    /// Source run zero is not an issued producer identity.
    InvalidRun,
    /// Completed presentation contained no warning rules to acknowledge.
    NoWarnings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// User feedback distinguishing request dispatch from producer acknowledgment.
pub enum WarningFeedback {
    /// A request exists but has no matching producer result yet.
    Pending,
    /// A new action was refused while an earlier request remains pending.
    Busy,
    /// Producer accepted all requested rules.
    Accepted,
    /// Producer accepted some requested rules and rejected the remainder.
    Partial,
    /// No rule was accepted, or the pending request's source run was replaced.
    Rejected,
    /// No request could be made for the currently shown warning state.
    Unavailable,
}
impl WarningFeedback {
    /// Returns the static popup label for this dispatch/result state.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "ACK PENDING",
            Self::Busy => "ACK BUSY / PENDING",
            Self::Accepted => "WARNINGS ACKED",
            Self::Partial => "PARTIAL ACK",
            Self::Rejected => "ACK REJECTED",
            Self::Unavailable => "NO SHOWN WARNINGS",
        }
    }

    /// Returns dispatch feedback for `result`: success is pending, busy remains
    /// busy, and all other admission errors become unavailable; this is not an acknowledgment.
    pub const fn requested(result: Result<WarningCommand, RequestError>) -> Self {
        match result {
            Ok(_) => Self::Pending,
            Err(RequestError::Busy) => Self::Busy,
            Err(_) => Self::Unavailable,
        }
    }
}
/// Caller retains exactly one request until its result arrives; retries reuse its bytes.
#[derive(Default)]
pub struct WarningRequests {
    /// Last issued request identity; never decreases or wraps.
    last_id: u64,
    /// Sole in-flight command, preserved byte-for-byte for producer retries.
    pending: Option<WarningCommand>,
}
impl WarningRequests {
    /// Returns an idle requester whose first issued identity will be one.
    pub const fn new() -> Self {
        Self {
            last_id: 0,
            pending: None,
        }
    }

    /// Returns a newly retained command for nonzero `run_id`, observed episode
    /// frontier `observed_event_serial` and rule bitmask `rules`. This low-level
    /// route allows an empty mask and does not verify presentation evidence.
    /// Busy, invalid run or identity exhaustion return errors without mutation.
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

    /// Returns a retained acknowledgment command for completed warning `page`.
    /// Rejects busy first, then an empty rule mask, then invalid run/exhaustion;
    /// caller must supply metadata from actual completed presentation.
    pub fn request_presented(
        &mut self,
        page: crate::status_led::PresentedWarning,
    ) -> Result<WarningCommand, RequestError> {
        if self.pending.is_some() {
            return Err(RequestError::Busy);
        }
        if page.rules == 0 {
            return Err(RequestError::NoWarnings);
        }
        self.request(page.run_id, page.event_serial, page.rules)
    }

    /// Caller first accepts the coherent snapshot into its current source owner.
    /// A replacement source cancels an old request without claiming producer success.
    /// Returns `None` while idle or awaiting an exact result in `snapshot`; returns
    /// feedback and clears pending state on a matching partition or run replacement.
    pub fn observe(
        &mut self,
        snapshot: &crate::acquisition::DashboardSnapshot,
    ) -> Option<WarningFeedback> {
        let command = self.pending?;
        if snapshot.warning_run_id() != Some(command.run_id) {
            self.pending = None;
            return Some(WarningFeedback::Rejected);
        }
        let result = snapshot.warning_command_result();
        if !self.complete(result) {
            return None;
        }
        Some(if result.accepted == 0 {
            WarningFeedback::Rejected
        } else if result.rejected == 0 {
            WarningFeedback::Accepted
        } else {
            WarningFeedback::Partial
        })
    }

    /// Returns the retained retry command, or `None` when no request is outstanding.
    pub const fn pending(&self) -> Option<WarningCommand> { self.pending }

    /// Returns `true` and clears pending state only if `result` echoes its identity
    /// and disjointly partitions exactly the requested mask. Otherwise leaves state
    /// unchanged. Source-run validation belongs to [`Self::observe`] or the caller.
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
/// Current rule qualification plus persistent latch/acknowledgment/snooze state.
pub struct WarningEligibility {
    /// Threshold result after data qualification, including unavailable data.
    pub status: Status,
    /// Episode retained until its configured clear/acknowledgment conditions hold.
    pub latched: bool,
    /// The retained episode has met its clear condition.
    pub cleared: bool,
    /// The current episode was explicitly acknowledged.
    pub acknowledged: bool,
    /// Its temporary attention-suppression interval is still active.
    pub snoozed: bool,
}
impl WarningEligibility {
    /// Returns whether active threshold state or a retained latch remains visible, including snooze.
    pub const fn visible(self) -> bool { self.latched || matches!(self.status, Status::Active) }

    /// Returns whether visible state requires attention now; snooze suppresses attention only.
    pub const fn attention(self) -> bool { self.visible() && !self.snoozed }

    /// Returns the highest-priority explanatory state label, or empty text for no retained state.
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
/// Keeps the cross-owner command payload at its four-u64 footprint.
const _: () = assert!(core::mem::size_of::<WarningCommand>() == 32);

#[cfg(test)]
/// Request identity exhaustion and sole-owner regressions.
mod tests {
    use super::*;
    #[test]
    /// Verifies the final identity can complete but is never wrapped or overwritten by a retry.
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
