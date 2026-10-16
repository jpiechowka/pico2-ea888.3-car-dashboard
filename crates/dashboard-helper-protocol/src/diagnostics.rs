//! Opt-in diagnostic context only. No messages, subjects, paths or capabilities.
use serde::{Deserialize, Serialize};

use crate::{BoundedVec, Error, Text, Wire};
pub const REQUEST_BYTES: usize = 16 * 1024;
pub const BATCH_EVENTS: usize = 4;
pub const EVENT_BYTES: usize = 512;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Warn,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Editor,
    Validation,
    Storage,
    ImportExport,
    Simulation,
    Rendering,
    HostTools,
    Device,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workspace {
    Views,
    DataWarnings,
    Device,
    Diagnostics,
    BuildFlash,
    EcuReference,
}
macro_rules! codes {
    ($($name:ident => $text:literal),+ $(,)?)=>{
        #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
        pub enum Code { $(#[serde(rename=$text)] $name),+ }
        impl Code {
            pub fn parse(text:&str)->Option<Self>{match text {$($text=>Some(Self::$name),)+_=>None}}
            pub const fn label(self)->&'static str{match self {$(Self::$name=>$text,)+}}
        }
    }
}
codes! {
    ClipboardFailed=>"clipboard.failed",DownloadFailed=>"download.failed",ExportFailed=>"export.failed",
    FrameRejected=>"frame.completion_rejected",ImportRejected=>"import.rejected",RenderFailed=>"render.failed",
    ControlRejected=>"simulation.control_rejected",Quality=>"simulation.quality",
    StorageConflict=>"storage.conflict",StorageCorrupt=>"storage.corrupt",StorageFailed=>"storage.failed",
    StorageExhausted=>"storage.identity_exhausted",ValidationInvalid=>"validation.invalid",
    BuildBlocked=>"build.recipe_blocked",BuildFailed=>"build.recipe_failed",HelperFailed=>"helper.request_failed",
    PresetRejected=>"presets.rejected",PresetReadFailed=>"presets.read_failed",ExportRejected=>"export.rejected",
    CacheExhausted=>"cache.exhausted"
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub severity: Severity,
    pub category: Category,
    pub code: Code,
    pub workspace: Workspace,
    pub sequence: Text<20>,
    pub client_elapsed_ms: Text<20>,
    pub accepted_revision: Text<20>,
}
impl Event {
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
pub struct Request {
    pub version: u8,
    pub events: BoundedVec<Event, BATCH_EVENTS>,
}
impl Wire for Request {
    const MAX_BYTES: usize = REQUEST_BYTES;

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
pub struct Response {
    pub version: u8,
    pub admitted: u8,
    pub dropped: u8,
}
impl Wire for Response {
    const MAX_BYTES: usize = 1024;

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
    times: std::collections::VecDeque<u64>,
    last: u64,
}
impl RateGate {
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
