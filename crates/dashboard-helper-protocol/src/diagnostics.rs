//! Opt-in diagnostic context only. No messages, subjects, paths or capabilities.
use serde::{Deserialize, Serialize};

use crate::{BoundedVec, Error, Text, Wire};
/// Maximum encoded/decoded relay request bytes; not a queue or process-output budget.
pub const REQUEST_BYTES: usize = 16 * 1024;
/// Maximum events in one admitted request and response accounting total.
pub const BATCH_EVENTS: usize = 4;
/// Maximum JSON bytes for one metadata-only event.
pub const EVENT_BYTES: usize = 512;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Relay allowlist; local informational/debug records remain local.
pub enum Severity {
    /// A recoverable warning selected by the originating event.
    Warn,
    /// A failure selected by the originating event; not an automatic device-health assertion.
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Coarse source area, deliberately without free-form subjects or paths.
pub enum Category {
    /// Editing, selection or accepted-state changes.
    Editor,
    /// Configuration or input admission.
    Validation,
    /// Browser persistence/cache operations.
    Storage,
    /// Import, export, clipboard or download operations.
    ImportExport,
    /// Synthetic controls and data quality.
    Simulation,
    /// Render and frame-completion processing.
    Rendering,
    /// Native helper or build tools.
    HostTools,
    /// Device discovery, inspection or explicit programming workflow.
    Device,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Visible Studio workspace at event creation; not a native filesystem workspace.
pub enum Workspace {
    /// Layout/view editor.
    Views,
    /// Data-source and warning configuration.
    DataWarnings,
    /// Device configuration.
    Device,
    /// Diagnostic event workspace.
    Diagnostics,
    /// Build/helper/flash controls.
    BuildFlash,
    /// Qualified and unqualified ECU research references.
    EcuReference,
}
/// Generate the closed code allowlist and exact string mapping from documented variants.
macro_rules! codes {
    ($($(#[$doc:meta])* $name:ident => $text:literal),+ $(,)?)=>{
        /// Stable allowlisted event code; unknown strings are rejected instead of relayed as private text.
        #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
        pub enum Code { $($(#[$doc])* #[serde(rename=$text)] $name),+ }
        impl Code {
            /// Return the exact allowlisted code for `text`, or `None` without normalization.
            pub fn parse(text:&str)->Option<Self>{match text {$($text=>Some(Self::$name),)+_=>None}}
            /// Return the stable serialized label; contains no event payload.
            pub const fn label(self)->&'static str{match self {$(Self::$name=>$text,)+}}
        }
    }
}
codes! {
    /// Clipboard write failed.
    ClipboardFailed=>"clipboard.failed",
    /// Browser download creation failed.
    DownloadFailed=>"download.failed",
    /// Export production failed after dispatch.
    ExportFailed=>"export.failed",
    /// A stale/incoherent render completion was rejected.
    FrameRejected=>"frame.completion_rejected",
    /// Imported content failed admission.
    ImportRejected=>"import.rejected",
    /// Shared rendering or frame production failed.
    RenderFailed=>"render.failed",
    /// Synthetic control input failed admission.
    ControlRejected=>"simulation.control_rejected",
    /// Synthetic sample quality changed or requires attention.
    Quality=>"simulation.quality",
    /// Persisted state changed concurrently.
    StorageConflict=>"storage.conflict",
    /// Stored content failed integrity/schema admission.
    StorageCorrupt=>"storage.corrupt",
    /// Storage operation failed.
    StorageFailed=>"storage.failed",
    /// Storage identity issuance exhausted its nonwrapping range.
    StorageExhausted=>"storage.identity_exhausted",
    /// Proposed settings remain invalid.
    ValidationInvalid=>"validation.invalid",
    /// Build input could not be admitted.
    BuildBlocked=>"build.recipe_blocked",
    /// Build recipe production failed.
    BuildFailed=>"build.recipe_failed",
    /// Helper request failed or returned an unusable response.
    HelperFailed=>"helper.request_failed",
    /// Preset content failed admission.
    PresetRejected=>"presets.rejected",
    /// Preset storage could not be read.
    PresetReadFailed=>"presets.read_failed",
    /// Export was blocked by current admission policy.
    ExportRejected=>"export.rejected",
    /// Cache identity or capacity was exhausted.
    CacheExhausted=>"cache.exhausted"
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Metadata-only diagnostic record; unknown fields are rejected to exclude message/subject payloads.
pub struct Event {
    /// Warning/error severity admitted by the originating store.
    pub severity: Severity,
    /// Coarse event source area.
    pub category: Category,
    /// Closed, stable event code allowlist.
    pub code: Code,
    /// Active Studio workspace when offered to the relay.
    pub workspace: Workspace,
    /// Canonical decimal u64 sequence encoded as text to preserve values above JavaScript's integer precision.
    pub sequence: Text<20>,
    /// Canonical decimal u64 client-relative milliseconds; not UTC or a helper clock reading.
    pub client_elapsed_ms: Text<20>,
    /// Canonical decimal u64 accepted-project revision, not a draft revision.
    pub accepted_revision: Text<20>,
}
impl Event {
    /// Return success for canonical unsigned decimal counters and the per-event byte bound.
    /// Reject signs, leading zeros, overflow and nonnumeric text; zero itself is valid.
    pub fn validate(&self) -> Result<(), Error> {
        for value in [&self.sequence, &self.client_elapsed_ms, &self.accepted_revision] {
            let parsed = value.as_str().parse::<u64>().map_err(|_| Error::Schema)?;
            if parsed.to_string() != value.as_str() {
                return Err(Error::Schema);
            }
        }
        crate::serialized_bound(self, EVENT_BYTES)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One opt-in diagnostic batch, with no automatic retry after uncertain dispatch.
pub struct Request {
    /// Wire revision; only 1 is admitted.
    pub version: u8,
    /// One through four individually validated metadata records.
    pub events: BoundedVec<Event, BATCH_EVENTS>,
}
impl Wire for Request {
    /// Maximum JSON bytes enforced by the outer codec.
    const MAX_BYTES: usize = REQUEST_BYTES;

    /// Return success for revision 1 and a nonempty batch of valid events; no rate admission occurs here.
    fn validate(&self) -> Result<(), Error> {
        if self.version != 1 || self.events.is_empty() {
            return Err(Error::Schema);
        }
        for event in self.events.iter() {
            event.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Accounting for one batch; caller must also match the total to its outstanding lease count.
pub struct Response {
    /// Wire revision; only 1 is admitted.
    pub version: u8,
    /// Events admitted by the helper's own rate/queue policy.
    pub admitted: u8,
    /// Events explicitly discarded by that policy.
    pub dropped: u8,
}
impl Wire for Response {
    /// Maximum encoded/decoded response JSON bytes.
    const MAX_BYTES: usize = 1024;

    /// Return success for revision 1 and a combined count of at most four, using widened arithmetic.
    /// Zero is structurally valid; matching an actual request's count is a separate caller check.
    fn validate(&self) -> Result<(), Error> {
        if self.version != 1 || usize::from(self.admitted) + usize::from(self.dropped) > BATCH_EVENTS {
            Err(Error::Schema)
        } else {
            Ok(())
        }
    }
}
/// Both sides enforce ten admitted records per rolling second. Backward time never resets capacity.
#[derive(Default)]
pub struct RateGate {
    /// At most ten admitted monotonic timestamps, oldest first; denied attempts do not enter the window.
    times: std::collections::VecDeque<u64>,
    /// Highest observed milliseconds, including denied attempts; prevents backward-time refills.
    last: u64,
}
impl RateGate {
    /// Return whether an event at client/helper-relative `now` milliseconds fits the ten-per-1000 ms window.
    /// Clamp backward time to the prior observation; expire entries at exactly 1000 ms. At u64 exhaustion,
    /// time cannot advance further, so outstanding entries do not acquire artificial refill capacity.
    pub fn admit(
        &mut self,
        now: u64,
    ) -> bool {
        let now = now.max(self.last);
        self.last = now;
        while self.times.front().is_some_and(|t| now.saturating_sub(*t) >= 1000) {
            self.times.pop_front();
        }
        if self.times.len() >= 10 {
            return false;
        }
        self.times.push_back(now);
        true
    }
}
