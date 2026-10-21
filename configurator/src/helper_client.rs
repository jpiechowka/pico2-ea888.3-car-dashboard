//! Bounded browser-side job history. Capabilities stay outside diagnostic state.
use std::collections::VecDeque;

use dashboard_config_tools::project::ExportMode;
use dashboard_config_tools::sha256_hex;
use dashboard_helper_protocol::{
    BuildOptions,
    Event,
    EventBatch,
    JobSummary,
    PROTOCOL_VERSION,
    ProjectBuildRequest,
    ProjectUploadResponse,
    SessionId,
    Text,
    Wire,
};
use serde::Deserialize;

use crate::build_flash::BuildSelection;
use crate::editor::{Editor, EditorStamp};

/// Content-digest caching for artifact freshness labels without build authority.
mod display_identity;
pub use display_identity::{ArtifactDisplayCache, ArtifactDisplayIdentity};

/// Explicit selected-descriptor, inspection and board-confirmation state.
mod device;
pub use device::DeviceSelection;
/// Bounded storage response decoding and cleanup eligibility presentation.
mod storage;
pub use storage::BuildStorageView;

/// Default maximum helper JSON response body in bytes.
pub const MAX_RESPONSE_BYTES: usize = 262_144;
/// Maximum retained browser event records; earlier records contribute to `omitted`.
pub const MAX_VISIBLE_EVENTS: usize = 512;

/// Frozen accepted project; originals never cross the native build boundary.
#[derive(PartialEq, Eq)]
pub struct PreparedBuild {
    /// Owner/revision stamp captured before asynchronous upload.
    stamp: EditorStamp,
    /// Editor action revision carried into the typed build request.
    revision: u64,
    /// Browser build-option generation used to reject changes during upload.
    options_revision: u64,
    /// Normalized requested profile/mode/options, captured atomically with content.
    options: BuildOptions,
    /// Converted-only project archive; original source artwork is excluded.
    bytes: Vec<u8>,
    /// Project/configuration/startup-profile/startup-asset SHA256 strings, in that order.
    hashes: [String; 4],
}
impl PreparedBuild {
    /// Called from a freshly captured accepted project immediately before dispatch.
    /// Returns a validated flash request for matching `artifact`, `connection` and
    /// `device` acknowledgement. Invalid artifact, content/options/session mismatch
    /// or absent/wrong-session confirmation returns a diagnostic. Does not write
    /// hardware or recheck an editor after this snapshot was captured.
    pub fn authorize_flash(
        &self,
        artifact: &dashboard_helper_protocol::ArtifactSummary,
        connection: &Connection,
        device: &DeviceSelection,
    ) -> Result<dashboard_helper_protocol::FlashRequest, &'static str> {
        artifact.validate().map_err(|_| "Invalid firmware artifact")?;
        if !self.matches_artifact(artifact) || artifact.session_id != *connection.session() {
            return Err("Build this accepted project and options in the current helper session first");
        }
        let acknowledgement = device
            .acknowledgement()
            .ok_or("Inspect and explicitly confirm the selected Pico 2 W first")?;
        if acknowledgement.session_id != *connection.session() {
            return Err("Device inspection belongs to another helper session");
        }
        let request = dashboard_helper_protocol::FlashRequest {
            artifact_id: artifact.artifact_id.clone(),
            device_id: acknowledgement.device_id.clone(),
            editor_revision: artifact.editor_revision,
            project_sha256: artifact.project_sha256.clone(),
            config_sha256: artifact.config_sha256.clone(),
            expected_options: self.options.clone(),
            retry_of: None,
            board_acknowledgement: acknowledgement.clone(),
        };
        request.validate().map_err(|_| "Invalid flash request")?;
        Ok(request)
    }

    /// Returns whether `artifact` matches captured requested options and all four
    /// digests; does not establish session, device or source-checkout authority.
    pub fn matches_artifact(
        &self,
        artifact: &dashboard_helper_protocol::ArtifactSummary,
    ) -> bool {
        display_identity::matches(&self.options, &self.hashes, artifact)
    }

    /// Display projection only; it cannot authorize upload or device operations.
    /// Returns cloned options/digests without project bytes or editor capabilities.
    pub fn display_identity(&self) -> ArtifactDisplayIdentity {
        ArtifactDisplayIdentity {
            options: self.options.clone(),
            hashes: self.hashes.clone(),
        }
    }

    /// Returns a frozen converted-only project and normalized `selection` for ready
    /// `editor`, recording `options_revision` for later stale-request rejection.
    /// Pending/invalid edits, invalid options, encoding/size/profile failures return
    /// diagnostics without changing editor or selection. Originals never enter bytes.
    pub fn capture(
        editor: &Editor,
        selection: &BuildSelection,
        options_revision: u64,
    ) -> Result<Self, &'static str> {
        let project = editor
            .accepted_project()
            .map_err(|_| "Finish or discard pending edits before building")?;
        let options = selection
            .normalized()
            .map_err(|_| "Correct build options before building")?
            .requested;
        let bytes = project
            .encode(ExportMode::ConvertedOnly)
            .map_err(|_| "Could not package accepted project")?;
        if bytes.len() > dashboard_helper_protocol::PROJECT_UPLOAD_BYTES {
            return Err("Project exceeds the build upload bound");
        }
        let hashes = [
            sha256_hex(&bytes),
            sha256_hex(project.dashboard_bytes()),
            sha256_hex(&project.profile().encode().ok_or("Invalid startup profile")?),
            sha256_hex(project.artwork().map_or(&[], |a| a.converted())),
        ];
        Ok(Self {
            stamp: editor.stamp(),
            revision: editor.revision().get(),
            options_revision,
            options,
            bytes,
            hashes,
        })
    }

    /// Returns the immutable converted-only archive bytes to upload to the helper.
    pub fn bytes(&self) -> &[u8] { &self.bytes }

    /// Recheck immediately before dispatch; a completed upload is not permission to build stale edits.
    /// Returns a build request only when `editor` readiness/stamp, `selection`,
    /// `options_revision`, `connection` session and upload `reply` digests still
    /// match this snapshot. Otherwise returns a diagnostic and issues no request.
    pub fn authorize(
        &self,
        reply: &ProjectUploadResponse,
        connection: &Connection,
        editor: &Editor,
        selection: &BuildSelection,
        options_revision: u64,
    ) -> Result<ProjectBuildRequest, &'static str> {
        if editor.stamp() != self.stamp
            || editor.action_readiness().block.is_some()
            || options_revision != self.options_revision
            || selection.normalized().map(|n| n.requested).ok().as_ref() != Some(&self.options)
        {
            return Err(
                "Project or build settings changed during upload. Click Build again for the current selection.",
            );
        }
        if reply.session_id != *connection.session()
            || [
                reply.project_sha256.as_str(),
                reply.config_sha256.as_str(),
                reply.startup_profile_sha256.as_str(),
                reply.startup_asset_sha256.as_str(),
            ] != self.hashes.each_ref().map(String::as_str)
        {
            return Err("Helper upload identity does not match the accepted project");
        }
        Ok(ProjectBuildRequest {
            editor_revision: self.revision,
            project_id: reply.project_id.clone(),
            project_sha256: reply.project_sha256.clone(),
            options: self.options.clone(),
            retry_of: None,
        })
    }
}

/// Returns `Ok(())` if downloaded `bytes` match validated `artifact` length and UF2
/// SHA256 within the protocol bound. Invalid metadata, length or hash returns a
/// diagnostic; this does not independently validate ELF/UF2 structure or hardware.
pub fn verify_artifact_download(
    artifact: &dashboard_helper_protocol::ArtifactSummary,
    bytes: &[u8],
) -> Result<(), &'static str> {
    artifact.validate().map_err(|_| "Invalid artifact metadata")?;
    if bytes.len() != artifact.uf2_bytes as usize || bytes.len() > dashboard_helper_protocol::MAX_UF2_BYTES {
        return Err("Artifact download length does not match its admitted metadata");
    }
    let hash = sha256_hex(bytes);
    if hash != artifact.uf2_sha256.as_str() {
        return Err("Artifact download checksum mismatch");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// Strict connection response envelope; the secret is never diagnostic state.
struct ConnectReply {
    /// Wire version that must exactly match the browser protocol.
    protocol_version: u16,
    /// Distinct helper process/session identity.
    session_id: SessionId,
    /// Exactly 64 lowercase hexadecimal secret characters after decode validation.
    capability: Text<64>,
}

/// Deliberately has no Debug or serialization implementation.
#[derive(Clone)]
pub struct Connection {
    /// Session identity used to reject stale job/device/upload responses.
    session: SessionId,
    /// Session secret; send only in the dedicated authorization header, never logs.
    capability: String,
}
impl Connection {
    /// Returns a connection from at-most-1024 JSON `bytes`, requiring exact schema,
    /// protocol and 64 lowercase hex capability characters. Returns sanitized
    /// diagnostics on invalid input; no request is sent and no state is published.
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 1024 {
            return Err("Invalid helper connection response");
        }
        let reply: ConnectReply = serde_json::from_slice(bytes).map_err(|_| "Invalid helper connection response")?;
        let capability = reply.capability.as_str();
        if reply.protocol_version != PROTOCOL_VERSION {
            return Err("Helper protocol mismatch; restart the updated helper and refresh Studio");
        }
        if capability.len() != 64
            || !capability
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Incompatible helper connection response");
        }
        Ok(Self {
            session: reply.session_id,
            capability: capability.into(),
        })
    }

    /// Returns the authenticated helper-session identity used for response matching.
    pub fn session(&self) -> &SessionId { &self.session }

    /// Returns the secret request capability; callers must exclude it from diagnostics.
    pub fn capability(&self) -> &str { &self.capability }
}

#[derive(Clone, Default)]
/// One selected job's bounded browser history and exact polling cursor.
pub struct JobFeed {
    /// Latest validated summary, absent before a selected job starts.
    pub summary: Option<JobSummary>,
    /// Up to 512 retained events in ascending sequence order.
    pub events: VecDeque<Event>,
    /// Saturating count omitted by helper retention gaps or local eviction.
    pub omitted: u64,
    /// Last consumed sequence, or the predecessor of the helper's retained window.
    after: u64,
    /// Latest producer next-sequence cursor; zero before the first event batch.
    next: u64,
}
impl JobFeed {
    /// Replace history with validated `summary` only when its session matches
    /// `connection`. Returns `Ok(())` on reset or a diagnostic preserving old state.
    pub fn start(
        &mut self,
        connection: &Connection,
        summary: JobSummary,
    ) -> Result<(), &'static str> {
        summary.validate().map_err(|_| "Invalid helper job")?;
        if summary.session_id != *connection.session() {
            return Err("Helper session changed; reconnect");
        }
        *self = Self {
            summary: Some(summary),
            ..Self::default()
        };
        Ok(())
    }

    /// Returns the exclusive sequence cursor for the next polling request.
    pub fn after(&self) -> u64 { self.after }

    /// Validate and append `batch` for this selected job, returning `Ok(())` only
    /// after identity/order/closure/gap checks. Rejections preserve all state.
    /// Helper retention gaps and local 512-record eviction increment `omitted`;
    /// duplicates, backward cursors and unexplained missing records are errors.
    pub fn accept(
        &mut self,
        batch: EventBatch,
    ) -> Result<(), &'static str> {
        batch.validate().map_err(|_| "Invalid helper event batch")?;
        let current = self.summary.as_ref().ok_or("No active helper job")?;
        if batch.session_id != current.session_id || batch.job_id != current.job_id {
            return Err("Helper job identity changed");
        }
        if batch.next < self.next || batch.records.iter().any(|event| event.sequence <= self.after) {
            return Err("Out-of-order helper events");
        }
        if current.events_closed && (!batch.summary.events_closed || current.final_next != batch.summary.final_next) {
            return Err("Closed helper job changed");
        }
        let mut expected = self.after.max(batch.oldest - 1).saturating_add(1);
        for event in batch.records.iter() {
            if event.sequence != expected {
                return Err("Missing helper events without a retention gap");
            }
            expected = expected.saturating_add(1);
        }
        if batch.records.is_empty() && expected < batch.next {
            return Err("Empty helper batch before pending events");
        }
        if batch.oldest > self.after.saturating_add(1) {
            self.omitted = self.omitted.saturating_add(batch.oldest - self.after - 1);
            self.after = batch.oldest - 1;
        }
        for event in batch.records.iter() {
            self.after = event.sequence;
            if self.events.len() == MAX_VISIBLE_EVENTS {
                self.events.pop_front();
                self.omitted = self.omitted.saturating_add(1);
            }
            self.events.push_back(event.clone());
        }
        self.next = batch.next;
        self.summary = Some(batch.summary);
        Ok(())
    }

    /// Returns true only after a closed job's nonzero final producer cursor is drained.
    /// A terminal-looking summary alone is insufficient before final batch admission.
    pub fn drained(&self) -> bool {
        self.summary
            .as_ref()
            .is_some_and(|s| s.events_closed && self.after.saturating_add(1) >= self.next && self.next != 0)
    }

    /// Returns whether a selected job's event stream remains open.
    pub fn active(&self) -> bool { self.summary.as_ref().is_some_and(|s| !s.events_closed) }
}

/// The native cache's explicit not-run response is not a completed zero-check report.
/// Returns a presentation summary of `report`, counting passed/attention statuses.
/// Missing check rows produce zero counts; this helper does not validate/admit tools.
pub fn prerequisite_summary(report: &serde_json::Value) -> String {
    if report["reason"] == "Check not run" {
        return "Not checked in this helper session. Choose Check prerequisites to inspect installed tool versions."
            .into();
    }
    let rows = report["checks"].as_array();
    let passed = rows
        .into_iter()
        .flatten()
        .filter(|row| row["status"] == "passed")
        .count();
    let missing = rows
        .into_iter()
        .flatten()
        .filter(|row| row["status"] != "passed" && row["status"] != "not_checked")
        .count();
    format!(
        "{passed} installed checks passed · {missing} need attention. Compilation and device programming are checked \
         separately."
    )
}
