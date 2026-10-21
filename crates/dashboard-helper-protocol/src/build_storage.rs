//! Bounded local storage summaries; they never confer artifact/flash authority.
use serde::{Deserialize, Serialize};

use crate::*;
/// Observed attempt-directory category; none of these states grants flash authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildAttemptState {
    /// An artifact record was recognized in the attempt directory.
    Artifact,
    /// A failed-attempt marker was recognized.
    Failed,
    /// Active/incomplete reservation marker; cleanup and new-build admission preserve it for review.
    Incomplete,
}
/// Bounded storage accounting for one local build attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildAttemptSummary {
    /// Attempt directory identity admitted by the helper's storage traversal.
    pub attempt_id: AttemptId,
    /// Observed durable record category; not a live job status.
    pub state: BuildAttemptState,
    /// Recorded accounted/reserved bytes, at most 16 GiB; not current free disk space or a live rescan.
    pub bytes: u64,
    /// Cleanup must preserve this attempt because an owner protects it or the reservation is incomplete.
    pub protected: bool,
}
impl Wire for BuildAttemptSummary {
    /// Per-attempt JSON cap in bytes.
    const MAX_BYTES: usize = 1024;

    /// Return success within the 16 GiB accounting bound; return `Bounds` above it.
    fn validate(&self) -> Result<(), Error> {
        if self.bytes > 16 * 1024 * 1024 * 1024 {
            Err(Error::Bounds)
        } else {
            Ok(())
        }
    }
}
/// Storage directory report; `reason` communicates bounded traversal/cleanup availability.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildStorage {
    /// At most twenty-one attempt summaries; no live artifact authority is implied.
    pub attempts: BoundedVec<BuildAttemptSummary, 21>,
    /// Bounded storage policy or availability explanation; empty text is permitted by the wire type.
    pub reason: Text<512>,
}
impl Wire for BuildStorage {
    /// Complete storage-report JSON cap in bytes.
    const MAX_BYTES: usize = 32768;

    /// Return success when every attempt is bounded and the serialized envelope fits.
    fn validate(&self) -> Result<(), Error> {
        for a in self.attempts.iter() {
            a.validate()?;
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
