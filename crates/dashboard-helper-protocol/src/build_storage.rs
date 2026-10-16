//! Bounded local storage summaries; they never confer artifact/flash authority.
use serde::{Deserialize, Serialize};

use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildAttemptState {
    Artifact,
    Failed,
    Incomplete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildAttemptSummary {
    pub attempt_id: AttemptId,
    pub state: BuildAttemptState,
    pub bytes: u64,
    pub protected: bool,
}
impl Wire for BuildAttemptSummary {
    const MAX_BYTES: usize = 1024;

    fn validate(&self) -> Result<(), Error> {
        if self.bytes > 16 * 1024 * 1024 * 1024 {
            Err(Error::Bounds)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildStorage {
    pub attempts: BoundedVec<BuildAttemptSummary, 21>,
    pub reason: Text<512>,
}
impl Wire for BuildStorage {
    const MAX_BYTES: usize = 32768;

    fn validate(&self) -> Result<(), Error> {
        for a in self.attempts.iter() {
            a.validate()?;
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
