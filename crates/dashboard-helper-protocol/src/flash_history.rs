//! Historical observations only; never grant device readiness or retry authority.
use serde::{Deserialize, Serialize};

use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashPhase {
    Unpublished,
    IntentArmed,
    LoadCompleted,
    VerificationConfirmed,
    RebootArmed,
    RebootCompleted,
    Interrupted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashAttemptSummary {
    pub job_id: JobId,
    pub original_session: Option<SessionId>,
    pub artifact_id: Option<ArtifactId>,
    pub device_id: Option<DeviceId>,
    pub phase: FlashPhase,
    pub write_outcome: WriteOutcome,
    pub recovery_write_outcome: WriteOutcome,
    pub exit: Option<RawExit>,
    pub historical_observation: bool,
    pub automatic_resume: bool,
    pub running_firmware_observed: bool,
    pub hardware_qualified: bool,
}
impl Wire for FlashAttemptSummary {
    const MAX_BYTES: usize = 4096;

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashHistory {
    pub schema_version: u8,
    pub attempts: BoundedVec<FlashAttemptSummary, 16>,
}
impl Wire for FlashHistory {
    const MAX_BYTES: usize = 65536;

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
