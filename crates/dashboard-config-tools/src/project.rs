//! Bounded complete Studio projects. Container integrity is not a signature.
use std::sync::Arc;

use dashboard_config::startup::{StartupProfile, StartupTheme};
use dashboard_config::{Geometry, ValidatedConfig};
use serde::{Deserialize, Serialize};

/// Complete container size ceiling, including original artwork and descriptors, bytes.
pub const MAX_PROJECT_BYTES: usize = 9 * 1024 * 1024;
/// Container ceiling when no original bytes are embedded, bytes.
pub const MAX_REDUCED_BYTES: usize = 576 * 1024;
/// Maximum original PNG/GIF/sheet source bytes; zero-length originals are invalid.
pub const MAX_ORIGINAL_BYTES: usize = 8 * 1024 * 1024;
/// Maximum canonical JSON recipe bytes within the container.
pub const MAX_RECIPE_BYTES: usize = 4096;
/// Current converter identity, paired with version-2 converted assets.
pub const CONVERTER: &str = "dashboard-startup-tools/2";
/// Frozen version-1 converter identity, admitted only with landscape geometry.
const LEGACY_CONVERTER: &str = "dashboard-startup-tools/1";
/// Emitted project container version; decoding also admits version 1.
pub const PROJECT_VERSION: u16 = 2;
/// Current recipe shape, which explicitly records target geometry.
pub const RECIPE_VERSION: u16 = 2;
/// Rejection category; APIs construct replacements without mutating an existing project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectError {
    /// Container/section sizes, offset arithmetic or trailing bytes violate limits.
    Length,
    /// Container magic or version is unsupported.
    Version,
    /// Descriptor order, reserved fields, section presence or offsets are invalid.
    Structure,
    /// CRC or SHA-256 disagrees with its referenced bytes; not an authenticity verdict.
    Integrity,
    /// Dashboard encoding, validation, migration or canonical form is invalid.
    Dashboard,
    /// Startup profile is invalid or Custom/artwork presence disagrees.
    Profile,
    /// Converted startup asset fails bounded format validation.
    Asset,
    /// Recipe shape, canonical JSON, conversion parameters or asset version mismatch.
    Recipe,
    /// Original identity/length/signature or reference disagrees with its recipe.
    Source,
    /// Complete export needs original bytes that currently exist only in external backing.
    SourceUnavailable,
    /// Converted artwork does not match the effective Splash geometry.
    Orientation,
}
/// Explicit project export policy; source omission is never an implicit size fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportMode {
    /// Include resident originals; external references must first be materialized.
    Complete,
    /// Omit originals deliberately, retaining converted pixels and their recipe.
    ConvertedOnly,
}
/// Original artwork encoding recorded by the conversion recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Single PNG image.
    Png,
    /// GIF animation; conversion handles its disposal/background rules.
    Gif,
    /// PNG grid interpreted as ordered equal-size frames using [`Sheet`].
    SpriteSheet,
}
/// Spatial mapping from source pixels into the selected startup geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Preserve aspect ratio and letterbox with the matte color.
    Contain,
    /// Preserve aspect ratio and crop to cover the target frame.
    Cover,
    /// Scale axes independently to fill the target frame.
    Stretch,
}
/// GIF compositing background policy retained for reproducible reconversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GifBackground {
    /// Use the GIF logical-screen background policy.
    Logical,
    /// Treat cleared background as transparent before compositing on the matte.
    Transparent,
}
/// Sprite-grid parameters; recipe validation bounds count and total duration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Positive column count; columns times rows must not exceed 300.
    pub columns: u16,
    /// Positive row count; frames are interpreted by the converter in grid order.
    pub rows: u16,
    /// Uniform source-frame duration, 1–60000 ms; total duration must be at most 60000 ms.
    pub frame_ms: u16,
}
/// Canonical conversion provenance and parameters, validated when admitting [`Artwork`].
///
/// Hashes bind recorded bytes; they do not prove those pixels were produced from the
/// source by the claimed converter. Byte-identical compact JSON is required on import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Current recipe schema, [`RECIPE_VERSION`].
    pub version: u16,
    /// Admitted producer identity; legacy producer remains unchanged after migration.
    pub converter: String,
    /// Display label, at most 128 UTF-8 bytes; controls and path separators are rejected.
    pub label: String,
    /// Declared original encoding; embedded originals must match its file signature.
    pub source_kind: SourceKind,
    /// SHA-256 of the original source, even when its bytes are omitted.
    pub source_sha256: [u8; 32],
    /// Original length in bytes, 1 through [`MAX_ORIGINAL_BYTES`].
    pub source_length: u32,
    /// Original is owned/resolvable in memory; encoding rewrites this to actual embedded presence.
    pub original_included: bool,
    /// SHA-256 of the complete converted asset container.
    pub asset_sha256: [u8; 32],
    /// Spatial fitting rule used by the converter.
    pub fit: Fit,
    /// Matte color as 8-bit red, green, blue channels.
    pub matte_rgb: [u8; 3],
    /// Requested conversion sampling frequency, 1–20 frames per second.
    pub sample_fps: u8,
    /// Inclusive selected timeline start in ms; must precede `trim_end_ms`.
    pub trim_start_ms: u32,
    /// Exclusive selected end in ms, at most 60000; selected duration at most 10000 ms.
    pub trim_end_ms: u32,
    /// Required only for sprite sheets; PNG/GIF require `None`.
    pub sheet: Option<Sheet>,
    /// GIF background/disposal compositing choice retained even for non-GIF sources.
    pub gif_background: GifBackground,
    /// Converted pixel geometry; must match the effective startup Splash orientation.
    pub geometry: Geometry,
}
/// Frozen v1 JSON shape. Canonical bytes are checked before migration.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecipeV1 {
    /// Frozen schema value 1.
    version: u16,
    /// Frozen version-1 producer identity.
    converter: String,
    /// Original display label, validated after migration.
    label: String,
    /// Original source encoding.
    source_kind: SourceKind,
    /// Original source SHA-256.
    source_sha256: [u8; 32],
    /// Original source byte length.
    source_length: u32,
    /// Whether the legacy container embeds original source bytes.
    original_included: bool,
    /// Converted version-1 asset SHA-256.
    asset_sha256: [u8; 32],
    /// Original spatial fitting choice.
    fit: Fit,
    /// Original matte RGB channels.
    matte_rgb: [u8; 3],
    /// Requested sampling rate in frames per second.
    sample_fps: u8,
    /// Selected source start in ms.
    trim_start_ms: u32,
    /// Selected source end in ms.
    trim_end_ms: u32,
    /// Optional sprite grid retained unchanged through migration.
    sheet: Option<Sheet>,
    /// Original GIF background policy.
    gif_background: GifBackground,
}
impl LegacyRecipeV1 {
    /// Return the geometry-aware recipe with implicit legacy landscape made explicit.
    /// Reject unknown v1 producer/version; no pixels are reconverted or relabeled as v2.
    fn upgrade(self) -> Result<Recipe, ProjectError> {
        if self.version != 1 || self.converter != LEGACY_CONVERTER {
            return Err(ProjectError::Recipe);
        }
        Ok(Recipe {
            version: RECIPE_VERSION,
            // Pixels were not reconverted; retain their actual producer.
            converter: self.converter,
            label: self.label,
            source_kind: self.source_kind,
            source_sha256: self.source_sha256,
            source_length: self.source_length,
            original_included: self.original_included,
            asset_sha256: self.asset_sha256,
            fit: self.fit,
            matte_rgb: self.matte_rgb,
            sample_fps: self.sample_fps,
            trim_start_ms: self.trim_start_ms,
            trim_end_ms: self.trim_end_ms,
            sheet: self.sheet,
            gif_background: self.gif_background,
            geometry: Geometry::Landscape,
        })
    }
}
/// Immutable backing identity. Browser ownership/pins retain this Arc, not source bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct OriginalReference {
    /// Original byte identity; lookup key is its lowercase hex encoding.
    sha256: [u8; 32],
    /// Nonzero original byte length, bounded at construction.
    length: u32,
}
impl OriginalReference {
    /// Return a bounded reference for `sha256` and byte `length`; reject zero/oversized length.
    /// Hash possession does not prove backing bytes exist; the caller owns that guarantee.
    pub fn new(
        sha256: [u8; 32],
        length: usize,
    ) -> Result<Self, ProjectError> {
        if length == 0 || length > MAX_ORIGINAL_BYTES {
            return Err(ProjectError::Source);
        }
        Ok(Self {
            sha256,
            length: length as u32,
        })
    }

    /// Return the recorded 32-byte source digest without reading external backing.
    pub fn sha256(&self) -> [u8; 32] { self.sha256 }

    /// Return the admitted nonzero source length in bytes.
    pub fn len(&self) -> usize { self.length as usize }

    /// Return false; construction excludes empty original sources.
    pub fn is_empty(&self) -> bool { false }

    /// Return the 64-character lowercase content key used to locate immutable backing.
    pub fn key(&self) -> String { crate::hex_bytes(&self.sha256) }
}
/// Admitted converted pixels and recipe with resident or externally pinned originals.
/// Clones share large immutable byte buffers; they do not repeat pixel validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    /// Bounded validated asset; owns shared encoded pixels and parsed metadata.
    converted: dashboard_config::startup_asset::OwnedAsset,
    /// Admitted conversion parameters and source/asset identities.
    recipe: Recipe,
    /// Resident original bytes, if materialized; mutually exclusive with `reference`.
    original: Option<Arc<[u8]>>,
    /// Immutable external-backing identity/pin; complete export requires materialization.
    reference: Option<Arc<OriginalReference>>,
}
/// Admitted complete project snapshot; cloning shares dashboard/artwork bytes.
#[derive(Clone, Debug)]
pub struct Project {
    /// Canonical current-schema dashboard binary, not original imported legacy bytes.
    dashboard: Arc<[u8]>,
    /// Validated startup controls and effective orientation override.
    profile: StartupProfile,
    /// Present exactly when the startup profile selects Custom artwork.
    artwork: Option<Arc<Artwork>>,
    /// Effective splash pixel shape used to admit replacement artwork.
    splash_geometry: Geometry,
}

/// Return raw SHA-256 bytes for `bytes`; integrity identity is not an authentication signature.
pub fn digest(bytes: &[u8]) -> [u8; 32] { sha2_const_stable::Sha256::new().update(bytes).finalize() }
/// Reflected CRC-32 lookup table using polynomial 0xEDB88320.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut n = i as u32;
        let mut bit = 0;
        while bit < 8 {
            n = if n & 1 != 0 { (n >> 1) ^ 0xEDB88320 } else { n >> 1 };
            bit += 1;
        }
        table[i] = n;
        i += 1;
    }
    table
};
/// Return reflected CRC-32 of `bytes`, with all-one initialization and final inversion.
fn crc(bytes: &[u8]) -> u32 {
    let mut value = u32::MAX;
    for byte in bytes {
        value = (value >> 8) ^ CRC_TABLE[((value ^ u32::from(*byte)) & 255) as usize];
    }
    !value
}
impl Recipe {
    /// Return success only for bounded current/legacy producer, label, timing and sheet fields.
    /// Does not validate source/asset bytes or hashes; [`Artwork::new`] completes admission.
    fn validate(&self) -> Result<(), ProjectError> {
        if self.version != RECIPE_VERSION
            || !(self.converter == CONVERTER
                || (self.converter == LEGACY_CONVERTER && self.geometry == Geometry::Landscape))
            || self.label.len() > 128
            || self.label.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
            || self.source_length == 0
            || self.source_length as usize > MAX_ORIGINAL_BYTES
            || !(1..=20).contains(&self.sample_fps)
            || self.trim_end_ms <= self.trim_start_ms
            || self.trim_end_ms > 60000
            || self.trim_end_ms - self.trim_start_ms > 10000
        {
            return Err(ProjectError::Recipe);
        }
        match (&self.sheet, self.source_kind) {
            (Some(sheet), SourceKind::SpriteSheet)
                if sheet.columns > 0
                    && sheet.rows > 0
                    && u32::from(sheet.columns) * u32::from(sheet.rows) <= 300
                    && (1..=60000).contains(&sheet.frame_ms)
                    && u32::from(sheet.columns) * u32::from(sheet.rows) * u32::from(sheet.frame_ms) <= 60000 => {}
            (None, SourceKind::Png | SourceKind::Gif) => (),
            _ => return Err(ProjectError::Recipe),
        }
        Ok(())
    }
}
impl Artwork {
    /// Admit `converted`, `recipe` and optional resident `original`; return immutable artwork.
    ///
    /// Check bounded asset decoding, geometry/version, asset digest, original presence,
    /// length/digest and PNG/GIF signature. Full original decoding/reconversion is not
    /// performed here. Any mismatch returns its category without a partial object.
    pub fn new(
        converted: Arc<[u8]>,
        recipe: Recipe,
        original: Option<Arc<[u8]>>,
    ) -> Result<Self, ProjectError> {
        recipe.validate()?;
        let converted = dashboard_config::startup_asset::OwnedAsset::new(converted).map_err(|_| ProjectError::Asset)?;
        let asset = converted.as_asset();
        if asset.geometry() != recipe.geometry
            || u16le(converted.bytes(), 8) != if recipe.converter == LEGACY_CONVERTER { 1 } else { 2 }
        {
            return Err(ProjectError::Recipe);
        }
        if digest(converted.bytes()) != recipe.asset_sha256 {
            return Err(ProjectError::Integrity);
        }
        if recipe.original_included != original.is_some() {
            return Err(ProjectError::Source);
        }
        if let Some(source) = &original {
            if source.is_empty()
                || source.len() > MAX_ORIGINAL_BYTES
                || source.len() != recipe.source_length as usize
                || digest(source) != recipe.source_sha256
            {
                return Err(ProjectError::Source);
            }
            let valid = match recipe.source_kind {
                SourceKind::Png | SourceKind::SpriteSheet => source.starts_with(b"\x89PNG\r\n\x1a\n"),
                SourceKind::Gif => source.starts_with(b"GIF87a") || source.starts_with(b"GIF89a"),
            };
            if !valid {
                return Err(ProjectError::Source);
            }
        }
        Ok(Self {
            converted,
            recipe,
            original,
            reference: None,
        })
    }

    /// Return a clone using `reference` instead of resident original bytes.
    ///
    /// The caller must have committed exact original bytes to immutable backing first;
    /// only digest/length agreement is checked here. A mismatch returns `Source`.
    /// Converted pixels stay shared and complete export waits for materialization.
    pub fn externalized(
        &self,
        reference: Arc<OriginalReference>,
    ) -> Result<Self, ProjectError> {
        if reference.sha256() != self.recipe.source_sha256 || reference.len() != self.recipe.source_length as usize {
            return Err(ProjectError::Source);
        }
        let mut recipe = self.recipe.clone();
        recipe.original_included = true;
        Ok(Self {
            converted: self.converted.clone(),
            recipe,
            original: None,
            reference: Some(reference),
        })
    }

    /// Return artwork with resident `source`, rechecking its signature, size and digest.
    /// Errors preserve this instance; success drops external-reference ownership in the new one.
    pub fn materialized(
        &self,
        source: Arc<[u8]>,
    ) -> Result<Self, ProjectError> {
        let mut recipe = self.recipe.clone();
        recipe.original_included = true;
        Self::new(self.converted_arc(), recipe, Some(source))
    }

    /// Return the external backing pin, or `None` for resident/omitted originals.
    pub fn original_reference(&self) -> Option<&Arc<OriginalReference>> { self.reference.as_ref() }

    /// Return whether an original is resident or externally referenced; not a backing-read check.
    pub fn has_original(&self) -> bool { self.original.is_some() || self.reference.is_some() }

    /// Return SHA-256 of canonical recipe JSON, including parameters and original-presence state.
    pub fn identity(&self) -> [u8; 32] { digest(&serde_json::to_vec(&self.recipe).expect("typed recipe serializes")) }

    /// Return admitted encoded asset bytes without reparsing or retaining original backing.
    pub fn converted(&self) -> &[u8] { self.converted.bytes() }

    /// Return shared firmware pixels without retaining an original-source pin.
    pub fn converted_arc(&self) -> Arc<[u8]> { self.converted.bytes_arc() }

    /// Return the borrowed admitted view without parsing or cloning its pixels.
    pub fn asset(&self) -> dashboard_config::startup_asset::Asset<'_> { self.converted.as_asset() }

    /// Return shared validated pixels without retaining original-source pins.
    pub fn owned_asset(&self) -> dashboard_config::startup_asset::OwnedAsset { self.converted.clone() }

    /// Return the admitted immutable conversion recipe.
    pub fn recipe(&self) -> &Recipe { &self.recipe }

    /// Return the asset/recipe's common pixel geometry.
    pub fn geometry(&self) -> Geometry { self.recipe.geometry }

    /// Return resident originals only; externally referenced originals return `None`.
    pub fn original(&self) -> Option<&Arc<[u8]>> { self.original.as_ref() }
}
/// Container header length, bytes; includes CRC over its first 28 bytes.
const HEADER: usize = 32;
/// Section descriptor length, bytes; includes CRC-32 and SHA-256.
const DESCRIPTOR: usize = 48;
/// Fixed header plus five section descriptors, bytes.
const PREFIX: usize = HEADER + 5 * DESCRIPTOR;
impl Project {
    /// Return a project from validated `config`, startup `profile` and optional `artwork`.
    /// Reject profile/artwork/geometry disagreement or dashboard encoding failure;
    /// serialize the dashboard once and share immutable artwork without reconversion.
    pub fn new(
        config: &ValidatedConfig,
        profile: StartupProfile,
        artwork: Option<Arc<Artwork>>,
    ) -> Result<Self, ProjectError> {
        let splash_geometry = check_project(config, profile, artwork.as_deref())?;
        let dashboard = crate::compile(config).map_err(|_| ProjectError::Dashboard)?.into();
        Ok(Self {
            dashboard,
            profile,
            artwork,
            splash_geometry,
        })
    }

    /// Return a replacement snapshot sharing `artwork` and the existing dashboard.
    /// Require Custom theme and matching splash geometry; errors retain the original snapshot.
    pub fn with_artwork(
        &self,
        artwork: Arc<Artwork>,
    ) -> Result<Self, ProjectError> {
        if self.profile.theme != StartupTheme::Custom {
            return Err(ProjectError::Profile);
        }
        if artwork.geometry() != self.splash_geometry {
            return Err(ProjectError::Orientation);
        }
        Ok(Self {
            dashboard: self.dashboard.clone(),
            profile: self.profile,
            artwork: Some(artwork),
            splash_geometry: self.splash_geometry,
        })
    }

    /// Return the admitted startup profile by value.
    pub fn profile(&self) -> StartupProfile { self.profile }

    /// Return shared artwork, or `None` for procedural startup themes.
    pub fn artwork(&self) -> Option<&Arc<Artwork>> { self.artwork.as_ref() }

    /// Return canonical dashboard binary bytes without decoding or copying.
    pub fn dashboard_bytes(&self) -> &[u8] { &self.dashboard }

    /// Encode using explicit `mode`; return a bounded version-2 project container.
    ///
    /// Complete export refuses external-only originals with `SourceUnavailable`;
    /// ConvertedOnly deliberately removes them and updates recipe presence. Sections
    /// are contiguous with length/CRC/SHA metadata. No existing snapshot is modified,
    /// and oversized content is rejected rather than dropping settings or artwork.
    pub fn encode(
        &self,
        mode: ExportMode,
    ) -> Result<Vec<u8>, ProjectError> {
        let profile = self.profile.encode().ok_or(ProjectError::Profile)?;
        let mut recipe_bytes = Vec::new();
        let (converted, original) = if let Some(art) = &self.artwork {
            if mode == ExportMode::Complete && art.reference.is_some() {
                return Err(ProjectError::SourceUnavailable);
            }
            let source = if mode == ExportMode::Complete {
                art.original.as_deref().unwrap_or(&[])
            } else {
                &[]
            };
            let mut recipe = art.recipe.clone();
            recipe.original_included = !source.is_empty();
            recipe_bytes = serde_json::to_vec(&recipe).map_err(|_| ProjectError::Recipe)?;
            if recipe_bytes.len() > MAX_RECIPE_BYTES {
                return Err(ProjectError::Recipe);
            }
            (art.converted(), source)
        } else {
            (&[][..], &[][..])
        };
        let sections = [
            self.dashboard.as_ref(),
            &profile,
            converted,
            recipe_bytes.as_slice(),
            original,
        ];
        let size = sections
            .iter()
            .try_fold(PREFIX, |n, part| n.checked_add(part.len()))
            .ok_or(ProjectError::Length)?;
        if size > MAX_PROJECT_BYTES || (original.is_empty() && size > MAX_REDUCED_BYTES) {
            return Err(ProjectError::Length);
        }
        let mut out = vec![0; PREFIX];
        out[..8].copy_from_slice(b"EAVPROJ\0");
        out[8..10].copy_from_slice(&PROJECT_VERSION.to_le_bytes());
        out[10..12].copy_from_slice(&5u16.to_le_bytes());
        out[12..16].copy_from_slice(&(size as u32).to_le_bytes());
        out[16..20].copy_from_slice(&(HEADER as u32).to_le_bytes());
        let header_crc = crc(&out[..28]);
        out[28..32].copy_from_slice(&header_crc.to_le_bytes());
        for (index, part) in sections.into_iter().enumerate() {
            let d = HEADER + index * DESCRIPTOR;
            out[d..d + 2].copy_from_slice(&((index + 1) as u16).to_le_bytes());
            if !part.is_empty() {
                let offset = out.len() as u32;
                out[d + 4..d + 8].copy_from_slice(&offset.to_le_bytes());
                out[d + 8..d + 12].copy_from_slice(&(part.len() as u32).to_le_bytes());
                out[d + 12..d + 16].copy_from_slice(&crc(part).to_le_bytes());
                out[d + 16..d + 48].copy_from_slice(&digest(part));
                out.extend_from_slice(part);
            }
        }
        Ok(out)
    }

    /// Decode bounded `bytes`; return an admitted project or a precise rejection category.
    ///
    /// Validate header/reserved fields, five ordered contiguous descriptors, per-section
    /// lengths and hashes, canonical recipe JSON, dashboard/profile versions and splash
    /// geometry before publication. Version 1 migrates its v4 dashboard and v1 startup profile;
    /// version 2 accepts v5/v6/current dashboards. Trailing bytes and implicit omissions fail.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProjectError> {
        if bytes.len() < PREFIX || bytes.len() > MAX_PROJECT_BYTES {
            return Err(ProjectError::Length);
        }
        let version = u16le(bytes, 8);
        if &bytes[..8] != b"EAVPROJ\0" || !matches!(version, 1 | PROJECT_VERSION) {
            return Err(ProjectError::Version);
        }
        if u16le(bytes, 10) != 5
            || u32le(bytes, 12) as usize != bytes.len()
            || u32le(bytes, 16) != HEADER as u32
            || bytes[20..28].iter().any(|b| *b != 0)
        {
            return Err(ProjectError::Structure);
        }
        if crc(&bytes[..28]) != u32le(bytes, 28) {
            return Err(ProjectError::Integrity);
        }
        let limits = [
            dashboard_config::MAX_FILE_BYTES,
            16,
            524288,
            MAX_RECIPE_BYTES,
            MAX_ORIGINAL_BYTES,
        ];
        let mut cursor = PREFIX;
        let mut sections = [&[][..]; 5];
        for index in 0..5 {
            let d = HEADER + index * DESCRIPTOR;
            let offset = u32le(bytes, d + 4) as usize;
            let length = u32le(bytes, d + 8) as usize;
            if u16le(bytes, d) != (index + 1) as u16 || u16le(bytes, d + 2) != 0 || length > limits[index] {
                return Err(ProjectError::Structure);
            }
            if length == 0 {
                if index < 2 || bytes[d + 4..d + 48].iter().any(|b| *b != 0) {
                    return Err(ProjectError::Structure);
                }
                continue;
            }
            if offset != cursor {
                return Err(ProjectError::Structure);
            }
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= bytes.len())
                .ok_or(ProjectError::Length)?;
            let part = &bytes[offset..end];
            if crc(part) != u32le(bytes, d + 12) || digest(part) != bytes[d + 16..d + 48] {
                return Err(ProjectError::Integrity);
            }
            sections[index] = part;
            cursor = end;
        }
        if cursor != bytes.len() || (sections[4].is_empty() && bytes.len() > MAX_REDUCED_BYTES) {
            return Err(ProjectError::Length);
        }
        let dashboard = dashboard_config::decode(sections[0]).map_err(|_| ProjectError::Dashboard)?;
        let canonical = crate::compile(&dashboard).map_err(|_| ProjectError::Dashboard)?;
        // The old project writer emitted canonical EAVv4 and EASPv1 only.
        // decode() already checks the frozen EAV payload before upgrading it.
        let schema = u16le(sections[0], 4);
        let allowed_schema = if version == 1 {
            schema == 4
        } else {
            matches!(schema, 5 | 6) || schema == dashboard_config::VERSION
        };
        if !allowed_schema || (schema == dashboard_config::VERSION && canonical != sections[0]) {
            return Err(ProjectError::Dashboard);
        }
        let profile = StartupProfile::decode(sections[1]).ok_or(ProjectError::Profile)?;
        if sections[1][4] != if version == 1 { 1 } else { 2 } {
            return Err(ProjectError::Profile);
        }
        let artwork = if sections[2].is_empty() {
            if !sections[3].is_empty() || !sections[4].is_empty() {
                return Err(ProjectError::Structure);
            }
            None
        } else {
            let recipe = if version == 1 {
                parse_canonical::<LegacyRecipeV1>(sections[3])?.upgrade()?
            } else {
                parse_canonical::<Recipe>(sections[3])?
            };
            Some(Arc::new(Artwork::new(
                Arc::from(sections[2]),
                recipe,
                (!sections[4].is_empty()).then(|| Arc::from(sections[4])),
            )?))
        };
        let splash_geometry = check_project(&dashboard, profile, artwork.as_deref())?;
        Ok(Self {
            dashboard: canonical.into(),
            profile,
            artwork,
            splash_geometry,
        })
    }
}
/// Return effective splash geometry for `config`, `profile` and admitted `artwork`.
/// Delegate presence/orientation checks to the same path used by direct build inputs.
fn check_project(
    config: &ValidatedConfig,
    profile: StartupProfile,
    artwork: Option<&Artwork>,
) -> Result<Geometry, ProjectError> {
    validate_startup_geometry(config.config().orientation, profile, artwork.map(Artwork::geometry))
}

/// Return splash geometry from device `default`, startup `profile` and optional `artwork` shape.
///
/// Shared by project import and direct Cargo admission. Invalid profiles or Custom
/// presence mismatch return `Profile`; incompatible asset geometry returns `Orientation`.
pub fn validate_startup_geometry(
    default: dashboard_config::Orientation,
    profile: StartupProfile,
    artwork: Option<Geometry>,
) -> Result<Geometry, ProjectError> {
    if !profile.valid() || (profile.theme == StartupTheme::Custom) != artwork.is_some() {
        return Err(ProjectError::Profile);
    }
    let geometry = profile.effective_splash_orientation(default).geometry();
    if artwork.is_some_and(|shape| shape != geometry) {
        return Err(ProjectError::Orientation);
    }
    Ok(geometry)
}
/// Return `T` decoded from `bytes` only when reserialization reproduces every input byte.
///
/// Whitespace, alternate field order/spellings and invalid JSON return `Recipe`;
/// callers enforce outer byte limits and semantic validation separately.
fn parse_canonical<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, ProjectError> {
    let value = serde_json::from_slice(bytes).map_err(|_| ProjectError::Recipe)?;
    if serde_json::to_vec(&value).map_err(|_| ProjectError::Recipe)? != bytes {
        return Err(ProjectError::Recipe);
    }
    Ok(value)
}
/// Return little-endian u16 from `b` at byte offset `o`; caller must prove two bytes exist.
/// Panics on invalid offsets; used only after container/asset minimum-size admission.
fn u16le(
    b: &[u8],
    o: usize,
) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
/// Return little-endian u32 from `b` at byte offset `o`; caller must prove four bytes exist.
/// Panics on invalid offsets; descriptor iteration is bounded by the validated prefix.
fn u32le(
    b: &[u8],
    o: usize,
) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
