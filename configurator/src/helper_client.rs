//! Bounded browser-side job history. Capabilities stay outside diagnostic state.
use std::collections::VecDeque;

use dashboard_config_tools::project::{ExportMode, digest};
use dashboard_helper_protocol::{
    BuildOptions,
    Event,
    EventBatch,
    JobSummary,
    ProjectBuildRequest,
    ProjectUploadResponse,
    SessionId,
    Text,
    Wire,
};
use serde::Deserialize;

use crate::build_flash::BuildSelection;
use crate::editor::{Editor, EditorStamp};

mod device;
pub use device::DeviceSelection;
mod storage;
pub use storage::BuildStorageView;

pub const MAX_RESPONSE_BYTES: usize = 262_144;
pub const MAX_VISIBLE_EVENTS: usize = 512;

/// Frozen accepted project; originals never cross the native build boundary.
#[derive(PartialEq, Eq)]
pub struct PreparedBuild {
    stamp: EditorStamp,
    revision: u64,
    options_revision: u64,
    options: BuildOptions,
    bytes: Vec<u8>,
    hashes: [String; 4],
}
impl PreparedBuild {
    /// Called from a freshly captured accepted project immediately before dispatch.
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
            .ok_or("Inspect and explicitly confirm the selected Pico 2 first")?;
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

    pub fn matches_artifact(
        &self,
        artifact: &dashboard_helper_protocol::ArtifactSummary,
    ) -> bool {
        artifact.options.requested == self.options
            && [
                artifact.project_sha256.as_str(),
                artifact.config_sha256.as_str(),
                artifact.startup_profile_sha256.as_str(),
                artifact.startup_asset_sha256.as_str(),
            ] == self.hashes.each_ref().map(String::as_str)
    }

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
        let hash = |data: &[u8]| digest(data).iter().map(|b| format!("{b:02x}")).collect();
        let hashes = [
            hash(&bytes),
            hash(project.dashboard_bytes()),
            hash(&project.profile().encode().ok_or("Invalid startup profile")?),
            hash(project.artwork().map_or(&[], |a| a.converted())),
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

    pub fn bytes(&self) -> &[u8] { &self.bytes }

    /// Recheck immediately before dispatch; a completed upload is not permission to build stale edits.
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

pub fn verify_artifact_download(
    artifact: &dashboard_helper_protocol::ArtifactSummary,
    bytes: &[u8],
) -> Result<(), &'static str> {
    artifact.validate().map_err(|_| "Invalid artifact metadata")?;
    if bytes.len() != artifact.uf2_bytes as usize || bytes.len() > dashboard_helper_protocol::MAX_UF2_BYTES {
        return Err("Artifact download length does not match its admitted metadata");
    }
    let hash: String = digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hash != artifact.uf2_sha256.as_str() {
        return Err("Artifact download checksum mismatch");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectReply {
    protocol_version: u16,
    session_id: SessionId,
    capability: Text<64>,
}

/// Deliberately has no Debug or serialization implementation.
#[derive(Clone)]
pub struct Connection {
    session: SessionId,
    capability: String,
}
impl Connection {
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 1024 {
            return Err("Invalid helper connection response");
        }
        let reply: ConnectReply = serde_json::from_slice(bytes).map_err(|_| "Invalid helper connection response")?;
        let capability = reply.capability.as_str();
        if reply.protocol_version != 1
            || capability.len() != 64
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

    pub fn session(&self) -> &SessionId { &self.session }

    pub fn capability(&self) -> &str { &self.capability }
}

#[derive(Clone, Default)]
pub struct JobFeed {
    pub summary: Option<JobSummary>,
    pub events: VecDeque<Event>,
    pub omitted: u64,
    after: u64,
    next: u64,
}
impl JobFeed {
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

    pub fn after(&self) -> u64 { self.after }

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

    pub fn drained(&self) -> bool {
        self.summary
            .as_ref()
            .is_some_and(|s| s.events_closed && self.after.saturating_add(1) >= self.next && self.next != 0)
    }

    pub fn active(&self) -> bool { self.summary.as_ref().is_some_and(|s| !s.events_closed) }
}
