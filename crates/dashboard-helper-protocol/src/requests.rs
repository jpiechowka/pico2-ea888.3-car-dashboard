use serde::{Deserialize, Serialize};

use crate::*;
pub const REQUEST_BYTES: usize = 65536;
pub const EAV_HEX_BYTES: usize = dashboard_config::MAX_FILE_BYTES * 2;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequest {
    pub editor_revision: u64,
    pub config_sha256: Sha256,
    pub eav_hex: Text<EAV_HEX_BYTES>,
    pub options: BuildOptions,
    pub retry_of: Option<JobId>,
}
impl Wire for BuildRequest {
    const MAX_BYTES: usize = REQUEST_BYTES;

    fn validate(&self) -> Result<(), Error> {
        if self.eav_hex.is_empty()
            || !self.eav_hex.len().is_multiple_of(2)
            || !self
                .eav_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Schema);
        }
        normalize(&self.options)?;
        Ok(())
    }
}
// Exact EAV codec and digest checks are required by the future build admission owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardContract {
    GenuinePico2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityProvenance {
    Manufacturer4MbUserConfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardAcknowledgement {
    pub device_id: DeviceId,
    pub session_id: SessionId,
    pub information_sha256: Sha256,
    pub board: BoardContract,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashRequest {
    pub artifact_id: ArtifactId,
    pub device_id: DeviceId,
    pub editor_revision: u64,
    /// Digest of the complete canonical converted-only accepted project.
    pub project_sha256: Sha256,
    pub config_sha256: Sha256,
    pub expected_options: BuildOptions,
    pub retry_of: Option<JobId>,
    pub board_acknowledgement: BoardAcknowledgement,
}
impl Wire for FlashRequest {
    const MAX_BYTES: usize = REQUEST_BYTES;

    fn validate(&self) -> Result<(), Error> {
        normalize(&self.expected_options)?;
        if self.board_acknowledgement.device_id != self.device_id {
            return Err(Error::Schema);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupRequest {
    pub attempts: BoundedVec<AttemptId, 16>,
}
impl Wire for CleanupRequest {
    const MAX_BYTES: usize = REQUEST_BYTES;

    fn validate(&self) -> Result<(), Error> {
        if self.attempts.is_empty()
            || self
                .attempts
                .iter()
                .enumerate()
                .any(|(i, x)| self.attempts[..i].contains(x))
        {
            Err(Error::Schema)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactGate {
    EmbeddedConfigValidation,
    ProgramImageValidation,
    Uf2Equality,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResponse {
    pub protocol_version: u16,
    pub session_id: SessionId,
    pub ready: bool,
    pub reason: Option<Text<512>>,
}
impl Wire for SessionResponse {
    const MAX_BYTES: usize = 65536;

    fn validate(&self) -> Result<(), Error> {
        if self.protocol_version != 1 || (self.ready && self.reason.is_some()) {
            Err(Error::Schema)
        } else {
            serialized_bound(self, Self::MAX_BYTES)
        }
    }
}

/// Binary project upload has a separate bound from small JSON commands.
pub const PROJECT_UPLOAD_BYTES: usize = 589824;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectUploadResponse {
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub project_sha256: Sha256,
    pub config_sha256: Sha256,
    pub startup_profile_sha256: Sha256,
    pub startup_asset_sha256: Sha256,
}
impl Wire for ProjectUploadResponse {
    const MAX_BYTES: usize = 4096;

    fn validate(&self) -> Result<(), Error> { Ok(()) }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildRequest {
    pub editor_revision: u64,
    pub project_id: ProjectId,
    pub project_sha256: Sha256,
    pub options: BuildOptions,
    pub retry_of: Option<JobId>,
}
impl Wire for ProjectBuildRequest {
    const MAX_BYTES: usize = REQUEST_BYTES;

    fn validate(&self) -> Result<(), Error> {
        normalize(&self.options)?;
        Ok(())
    }
}

pub const MAX_UF2_BYTES: usize = 8160 * 512;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSummary {
    pub artifact_id: ArtifactId,
    pub session_id: SessionId,
    pub editor_revision: u64,
    pub project_sha256: Sha256,
    pub config_sha256: Sha256,
    pub startup_profile_sha256: Sha256,
    pub startup_asset_sha256: Sha256,
    pub source_sha256: Sha256,
    pub elf_sha256: Sha256,
    pub uf2_sha256: Sha256,
    pub converter_sha256: Sha256,
    pub options: NormalizedBuildOptions,
    pub uf2_bytes: u32,
    pub hardware_qualified: bool,
    pub flash_ready: bool,
}
impl Wire for ArtifactSummary {
    const MAX_BYTES: usize = 16384;
    fn validate(&self)->Result<(),Error>{
        self.options.validate()?;
        if self.hardware_qualified||self.flash_ready||self.uf2_bytes==0||self.uf2_bytes as usize>MAX_UF2_BYTES||self.uf2_bytes%512!=0{return Err(Error::Schema)}
        serialized_bound(self,Self::MAX_BYTES)
    }
}
