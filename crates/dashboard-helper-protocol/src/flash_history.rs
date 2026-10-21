//! Historical observations only; never grant device readiness or retry authority.
use serde::{Deserialize, Serialize};

use crate::*;
/// Last durably published flash phase, not a command to resume or proof of current device state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashPhase {
    /// Attempt directory exists but no committed generation was recovered.
    Unpublished,
    /// Intent was persisted before invoking a device write; actual outcome may be unknown.
    IntentArmed,
    /// Load command completed; bytes are not yet verified.
    LoadCompleted,
    /// Verification succeeded for the recorded image/ranges at that time.
    VerificationConfirmed,
    /// Reboot intent was persisted before invoking the reboot command.
    RebootArmed,
    /// Reboot command completed; running application behavior was not observed.
    RebootCompleted,
    /// Operation stopped without reaching its planned final phase.
    Interrupted,
}
/// Read-only projection of one bounded flash journal; historical consent is never reused as authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashAttemptSummary {
    /// Original flash job identity, also used to locate the journal directory.
    pub job_id: JobId,
    /// Original helper session; required once a published phase exists.
    pub original_session: Option<SessionId>,
    /// Artifact bound to the historical intent; required after publication.
    pub artifact_id: Option<ArtifactId>,
    /// Device bound to the historical intent; required after publication.
    pub device_id: Option<DeviceId>,
    /// Last published phase; incomplete temporary generations do not advance it.
    pub phase: FlashPhase,
    /// Original recorded write outcome, retained without optimistic reconstruction.
    pub write_outcome: WriteOutcome,
    /// Conservative recovery projection: unpublished is not-started, verified stays verified, otherwise unknown.
    pub recovery_write_outcome: WriteOutcome,
    /// Recorded subprocess exit bits and signed interpretation, if available.
    pub exit: Option<RawExit>,
    /// Must be true: this record describes earlier evidence, not current authority.
    pub historical_observation: bool,
    /// Must be false: interrupted writes require a new explicit operation.
    pub automatic_resume: bool,
    /// Must be false: reboot-command success does not observe the running application.
    pub running_firmware_observed: bool,
    /// Must be false: transfer/verification cannot certify hardware margins or behavior.
    pub hardware_qualified: bool,
}
impl Wire for FlashAttemptSummary {
    /// Per-attempt JSON cap in bytes.
    const MAX_BYTES: usize = 4096;

    /// Return success for historical-only flags, coherent exit bits, conservative recovery and required identities.
    /// Published phases require session/artifact/device identities; byte integrity is checked by the journal reader.
    fn validate(&self) -> Result<(), Error> {
        if !self.historical_observation
            || self.automatic_resume
            || self.running_firmware_observed
            || self.hardware_qualified
        {
            return Err(Error::Schema);
        }
        if let Some(exit) = self.exit {
            exit.validate()?;
        }
        let expected = if self.phase == FlashPhase::Unpublished {
            WriteOutcome::NotStarted
        } else if self.write_outcome == WriteOutcome::Verified {
            WriteOutcome::Verified
        } else {
            WriteOutcome::Unknown
        };
        if self.recovery_write_outcome != expected {
            return Err(Error::Schema);
        }
        if self.phase != FlashPhase::Unpublished
            && (self.original_session.is_none() || self.artifact_id.is_none() || self.device_id.is_none())
        {
            return Err(Error::Schema);
        }
        crate::serialized_bound(self, Self::MAX_BYTES)
    }
}
/// At most sixteen historical flash attempts recovered from bounded local journals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashHistory {
    /// History response format revision; currently exactly 1.
    pub schema_version: u8,
    /// Read-only attempts; sequence is supplied by directory traversal, not a timestamp ordering guarantee.
    pub attempts: BoundedVec<FlashAttemptSummary, 16>,
}
impl Wire for FlashHistory {
    /// Complete history JSON cap in bytes, including all attempt envelopes.
    const MAX_BYTES: usize = 65536;

    /// Return success for revision 1, every valid historical attempt and the complete serialized byte cap.
    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err(Error::Schema);
        }
        for attempt in self.attempts.iter() {
            attempt.validate()?;
        }
        crate::serialized_bound(self, Self::MAX_BYTES)
    }
}
