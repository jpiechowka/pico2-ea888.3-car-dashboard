//! Storage observations do not confer deletion or firmware authority.
use dashboard_helper_protocol::{AttemptId, BuildAttemptState, BuildStorage, Wire};
use serde::Deserialize;

#[derive(Clone)]
/// Validated storage snapshot or the helper's bounded conflict/unavailability result.
pub enum BuildStorageView {
    /// Complete validated report with unique attempt IDs.
    Ready(
        /// Validated metadata snapshot; eligibility still needs server rechecking.
        BuildStorage,
    ),
    /// An active helper operation prevents a current storage snapshot.
    Busy,
    /// Storage identity/content needs manual review and must remain intact.
    Quarantined,
    /// Inspection failed without establishing storage contents.
    Unavailable,
}
impl BuildStorageView {
    /// Returns a validated storage view from JSON `bytes`, enforcing the protocol
    /// byte bound and unique attempt IDs. Recognizes only the three exact conflict
    /// tags; malformed, oversized, invalid or unknown responses return diagnostics.
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > BuildStorage::MAX_BYTES {
            return Err("Build storage response exceeds its size limit");
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        /// Exact one-field conflict envelope; additional fields are rejected.
        struct Conflict {
            /// Server conflict tag, matched against the explicit supported list.
            error: String,
        }
        if let Ok(report) = serde_json::from_slice::<BuildStorage>(bytes) {
            report.validate().map_err(|_| "Invalid build storage metadata")?;
            for (index, attempt) in report.attempts.iter().enumerate() {
                if report.attempts[..index]
                    .iter()
                    .any(|other| other.attempt_id == attempt.attempt_id)
                {
                    return Err("Duplicate build storage identity");
                }
            }
            return Ok(Self::Ready(report));
        }
        let error: Conflict = serde_json::from_slice(bytes).map_err(|_| "Invalid build storage response")?;
        match error.error.as_str() {
            "BuildStorageBusy" => Ok(Self::Busy),
            "BuildStorageQuarantined" => Ok(Self::Quarantined),
            "BuildStorageUnavailable" => Ok(Self::Unavailable),
            _ => Err("Unrecognized build storage response"),
        }
    }

    /// Returns the report's reason or a static actionable explanation for this state.
    pub fn explanation(&self) -> &str {
        match self {
            Self::Ready(report) => report.reason.as_str(),
            Self::Busy => "A helper operation is using storage. Refresh after its stages finish.",
            Self::Quarantined => {
                "Unrecognized or interrupted build storage needs manual review. Studio kept it intact; check the \
                 helper terminal and build-storage guide before removing files."
            }
            Self::Unavailable => "Build storage could not be inspected. Check the helper terminal, then refresh.",
        }
    }

    /// The server rechecks ownership and live readers immediately before deletion.
    /// Returns the matching `id` only from a Ready report when unprotected and not
    /// Incomplete; otherwise `None`. This is UI eligibility, not deletion authority.
    pub fn cleanup_id(
        &self,
        id: &str,
    ) -> Option<AttemptId> {
        let Self::Ready(report) = self else { return None };
        report
            .attempts
            .iter()
            .find(|attempt| {
                attempt.attempt_id.as_str() == id
                    && !attempt.protected
                    && attempt.state != BuildAttemptState::Incomplete
            })
            .map(|attempt| attempt.attempt_id.clone())
    }
}
