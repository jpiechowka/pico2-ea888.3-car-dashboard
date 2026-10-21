//! Session-scoped build/flash messages. Wire validation is necessary but does not
//! replace native file, project, device, digest or current-session admission.

use serde::{Deserialize, Serialize};

use crate::*;
/// Version 2 requires explicit Pico 2 W consent; version 1's board consent is incompatible.
pub const PROTOCOL_VERSION: u16 = 2;
/// Maximum JSON request bytes, including envelope and escape expansion.
pub const REQUEST_BYTES: usize = 65536;
/// Maximum ASCII hexadecimal characters for one bounded EAV configuration file.
pub const EAV_HEX_BYTES: usize = dashboard_config::MAX_FILE_BYTES * 2;
/// Configuration-only build request; native admission additionally verifies EAV codec and digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequest {
    /// Accepted editor revision for stale-result rejection; zero is valid.
    pub editor_revision: u64,
    /// Claimed digest of decoded EAV bytes; not recomputed by this wire validator.
    pub config_sha256: Sha256,
    /// Nonempty, even-length lowercase hexadecimal EAV bytes.
    pub eav_hex: Text<EAV_HEX_BYTES>,
    /// Requested compilation settings, checked by shared normalization.
    pub options: BuildOptions,
    /// Optional prior job correlation; does not authorize automatic retry.
    pub retry_of: Option<JobId>,
}
impl Wire for BuildRequest {
    /// Configuration request JSON cap in bytes.
    const MAX_BYTES: usize = REQUEST_BYTES;

    /// Return success for valid hexadecimal syntax and admitted options; do not decode EAV or verify its hash.
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
/// Explicit physical board contract accepted by this protocol revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardContract {
    /// Owner confirms a genuine Raspberry Pi Pico 2 W, including its manufacturer flash capacity.
    GenuinePico2W,
}
/// Origin of an admitted capacity claim; no measured-flash variant is supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityProvenance {
    /// Manufacturer's 4 MiB capacity accepted through explicit genuine-board confirmation.
    Manufacturer4MbUserConfirmed,
}
/// User board consent bound to one session, device and inspected-information digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardAcknowledgement {
    /// Device explicitly selected for this consent.
    pub device_id: DeviceId,
    /// Helper session in which the user confirmed the board.
    pub session_id: SessionId,
    /// Exact inspected-information identity; reconnect/reinspection can invalidate consent.
    pub information_sha256: Sha256,
    /// Physical board model asserted by the user.
    pub board: BoardContract,
}
/// Explicit flash request bound to an admitted artifact, project, settings and device consent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashRequest {
    /// Previously validated artifact selected for this operation.
    pub artifact_id: ArtifactId,
    /// Intended physical target identity; must match the acknowledgement.
    pub device_id: DeviceId,
    /// Accepted editor revision used to reject stale artifact selection.
    pub editor_revision: u64,
    /// Digest of the complete canonical converted-only accepted project.
    pub project_sha256: Sha256,
    /// Expected EAV configuration digest within the complete project.
    pub config_sha256: Sha256,
    /// Requested settings that must match the artifact during native admission.
    pub expected_options: BuildOptions,
    /// Previous job correlation only; interrupted writes never resume automatically.
    pub retry_of: Option<JobId>,
    /// Explicit genuine-board consent, independently checked against the active helper session.
    pub board_acknowledgement: BoardAcknowledgement,
}
impl Wire for FlashRequest {
    /// Flash request JSON cap in bytes.
    const MAX_BYTES: usize = REQUEST_BYTES;

    /// Return success if expected options normalize and consent names the same device.
    /// Artifact identity, consent freshness and actual hardware selection are native checks.
    fn validate(&self) -> Result<(), Error> {
        normalize(&self.expected_options)?;
        if self.board_acknowledgement.device_id != self.device_id {
            return Err(Error::Schema);
        }
        Ok(())
    }
}
/// Explicit deletion request for a nonempty, unique list of at most sixteen build attempts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupRequest {
    /// Attempt identities only; paths and arbitrary filesystem selectors are not accepted.
    pub attempts: BoundedVec<AttemptId, 16>,
}
impl Wire for CleanupRequest {
    /// Cleanup request JSON cap in bytes.
    const MAX_BYTES: usize = REQUEST_BYTES;

    /// Return success for nonempty unique attempts; native admission checks protection and ownership.
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
/// Software artifact gate category; success is not physical-device qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactGate {
    /// Embedded configuration/profile/asset data satisfies admission rules.
    EmbeddedConfigValidation,
    /// Linked program contents, architecture and memory layout satisfy image checks.
    ProgramImageValidation,
    /// UF2 payload agrees with the admitted ELF program bytes.
    Uf2Equality,
}
/// Current helper session availability; readiness never selects a device or authorizes flashing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResponse {
    /// Must equal [`PROTOCOL_VERSION`]; older non-W board consent is rejected.
    pub protocol_version: u16,
    /// Current helper session identity invalidating older consent/artifact bindings.
    pub session_id: SessionId,
    /// Whether the helper can accept operations under its current prerequisites.
    pub ready: bool,
    /// Optional unavailability explanation; forbidden when `ready` is true.
    pub reason: Option<Text<512>>,
}
impl Wire for SessionResponse {
    /// Session-response JSON cap in bytes.
    const MAX_BYTES: usize = 65536;

    /// Return success for the current protocol and consistent readiness/explanation within the JSON cap.
    fn validate(&self) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL_VERSION || (self.ready && self.reason.is_some()) {
            Err(Error::Schema)
        } else {
            serialized_bound(self, Self::MAX_BYTES)
        }
    }
}

/// Binary project upload has a separate bound from small JSON commands.
pub const PROJECT_UPLOAD_BYTES: usize = 589824;
/// Identities returned after admitting one complete converted-only project upload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectUploadResponse {
    /// Helper session retaining the uploaded project.
    pub session_id: SessionId,
    /// Session-local handle for the admitted immutable project snapshot.
    pub project_id: ProjectId,
    /// Digest of the canonical converted-only project container.
    pub project_sha256: Sha256,
    /// Digest of the contained EAV configuration bytes.
    pub config_sha256: Sha256,
    /// Digest of the contained startup profile bytes.
    pub startup_profile_sha256: Sha256,
    /// Digest of the contained converted startup asset bytes, including empty asset input.
    pub startup_asset_sha256: Sha256,
}
impl Wire for ProjectUploadResponse {
    /// Upload-response JSON cap in bytes.
    const MAX_BYTES: usize = 4096;

    /// Return success after field-type validation; session/digest ownership requires caller context.
    fn validate(&self) -> Result<(), Error> { Ok(()) }
}
/// Build an already uploaded project without reopening browser source files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBuildRequest {
    /// Accepted editor revision captured when requesting the build.
    pub editor_revision: u64,
    /// Uploaded project handle in the active helper session.
    pub project_id: ProjectId,
    /// Expected canonical project digest, checked against the retained snapshot.
    pub project_sha256: Sha256,
    /// Requested clock, acquisition, rendering and optimization choices.
    pub options: BuildOptions,
    /// Optional earlier job identity for reporting an explicit retry.
    pub retry_of: Option<JobId>,
}
impl Wire for ProjectBuildRequest {
    /// Project-build JSON cap in bytes.
    const MAX_BYTES: usize = REQUEST_BYTES;

    /// Return success when options normalize; project existence/digest/session binding is checked natively.
    fn validate(&self) -> Result<(), Error> {
        normalize(&self.options)?;
        Ok(())
    }
}

/// Maximum UF2 container length: 8,160 fixed 512-byte blocks, including per-block framing.
pub const MAX_UF2_BYTES: usize = 8160 * 512;
/// Software-validated artifact identities; the two readiness/qualification flags must remain false.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSummary {
    /// Immutable admitted artifact identity used for download/flash selection.
    pub artifact_id: ArtifactId,
    /// Session that admitted the artifact; historical metadata grants no current authority.
    pub session_id: SessionId,
    /// Accepted editor revision from the build request.
    pub editor_revision: u64,
    /// Canonical converted-only project digest.
    pub project_sha256: Sha256,
    /// Embedded EAV configuration digest.
    pub config_sha256: Sha256,
    /// Embedded startup-profile digest.
    pub startup_profile_sha256: Sha256,
    /// Embedded converted startup-asset digest.
    pub startup_asset_sha256: Sha256,
    /// Admitted source snapshot identity, including build inputs bound by the native recipe.
    pub source_sha256: Sha256,
    /// Exact linked ELF file digest.
    pub elf_sha256: Sha256,
    /// Exact UF2 file digest.
    pub uf2_sha256: Sha256,
    /// ELF-to-UF2 converter executable digest.
    pub converter_sha256: Sha256,
    /// Recomputable normalized request, feature list and actual clock metadata.
    pub options: NormalizedBuildOptions,
    /// UF2 container bytes, nonzero, divisible by 512, and at most [`MAX_UF2_BYTES`].
    pub uf2_bytes: u32,
    /// Always false: software validation cannot certify physical hardware behavior.
    pub hardware_qualified: bool,
    /// Always false: artifact metadata alone cannot establish device readiness.
    pub flash_ready: bool,
}
impl Wire for ArtifactSummary {
    /// Artifact-summary JSON cap in bytes.
    const MAX_BYTES: usize = 16384;

    /// Return success for canonical options, bounded aligned UF2 length and false hardware/readiness claims.
    /// Digest contents and image bytes are verified by native artifact admission, not by this method.
    fn validate(&self) -> Result<(), Error> {
        self.options.validate()?;
        if self.hardware_qualified
            || self.flash_ready
            || self.uf2_bytes == 0
            || self.uf2_bytes as usize > MAX_UF2_BYTES
            || !self.uf2_bytes.is_multiple_of(512)
        {
            return Err(Error::Schema);
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
