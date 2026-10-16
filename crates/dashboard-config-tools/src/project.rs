//! Bounded complete Studio projects. Container integrity is not a signature.
use std::sync::Arc;

use dashboard_config::ValidatedConfig;
use dashboard_config::startup::StartupProfile;
use serde::{Deserialize, Serialize};

pub const MAX_PROJECT_BYTES: usize = 9 * 1024 * 1024;
pub const MAX_REDUCED_BYTES: usize = 576 * 1024;
pub const MAX_ORIGINAL_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RECIPE_BYTES: usize = 4096;
pub const CONVERTER: &str = "dashboard-startup-tools/1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectError {
    Length,
    Version,
    Structure,
    Integrity,
    Dashboard,
    Profile,
    Asset,
    Recipe,
    Source,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportMode {
    Complete,
    ConvertedOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Png,
    Gif,
    SpriteSheet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Contain,
    Cover,
    Stretch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GifBackground {
    Logical,
    Transparent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub columns: u16,
    pub rows: u16,
    pub frame_ms: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u16,
    pub converter: String,
    pub label: String,
    pub source_kind: SourceKind,
    pub source_sha256: [u8; 32],
    pub source_length: u32,
    pub original_included: bool,
    pub asset_sha256: [u8; 32],
    pub fit: Fit,
    pub matte_rgb: [u8; 3],
    pub sample_fps: u8,
    pub trim_start_ms: u32,
    pub trim_end_ms: u32,
    pub sheet: Option<Sheet>,
    pub gif_background: GifBackground,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    converted: Arc<[u8]>,
    recipe: Recipe,
    original: Option<Arc<[u8]>>,
}
#[derive(Clone, Debug)]
pub struct Project {
    dashboard: Arc<[u8]>,
    profile: StartupProfile,
    artwork: Option<Arc<Artwork>>,
}

pub fn digest(bytes: &[u8]) -> [u8; 32] { sha2_const_stable::Sha256::new().update(bytes).finalize() }
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
fn crc(bytes: &[u8]) -> u32 {
    let mut value = u32::MAX;
    for byte in bytes {
        value = (value >> 8) ^ CRC_TABLE[((value ^ u32::from(*byte)) & 255) as usize];
    }
    !value
}
impl Recipe {
    fn validate(&self) -> Result<(), ProjectError> {
        if self.version != 1
            || self.converter != CONVERTER
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
                    && u32::from(sheet.columns) * u32::from(sheet.rows) * u32::from(sheet.frame_ms) <= 60000 =>
            {
                ()
            }
            (None, SourceKind::Png | SourceKind::Gif) => (),
            _ => return Err(ProjectError::Recipe),
        }
        Ok(())
    }
}
impl Artwork {
    pub fn new(
        converted: Arc<[u8]>,
        recipe: Recipe,
        original: Option<Arc<[u8]>>,
    ) -> Result<Self, ProjectError> {
        recipe.validate()?;
        validate_asset(&converted)?;
        if digest(&converted) != recipe.asset_sha256 {
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
        })
    }

    pub fn identity(&self) -> [u8; 32] { digest(&serde_json::to_vec(&self.recipe).expect("typed recipe serializes")) }

    pub fn converted(&self) -> &[u8] { &self.converted }

    pub fn recipe(&self) -> &Recipe { &self.recipe }

    pub fn original(&self) -> Option<&Arc<[u8]>> { self.original.as_ref() }
}
fn validate_asset(bytes: &[u8]) -> Result<(), ProjectError> {
    dashboard_config::startup_asset::Asset::parse(bytes)
        .map(|_| ())
        .map_err(|_| ProjectError::Asset)
}

const HEADER: usize = 32;
const DESCRIPTOR: usize = 48;
const PREFIX: usize = HEADER + 5 * DESCRIPTOR;
impl Project {
    pub fn new(
        config: &ValidatedConfig,
        profile: StartupProfile,
        artwork: Option<Arc<Artwork>>,
    ) -> Result<Self, ProjectError> {
        if !profile.valid() || (profile.theme as u8 == 5) != artwork.is_some() {
            return Err(ProjectError::Profile);
        }
        let dashboard = crate::compile(config).map_err(|_| ProjectError::Dashboard)?.into();
        Ok(Self {
            dashboard,
            profile,
            artwork,
        })
    }

    pub fn profile(&self) -> StartupProfile { self.profile }

    pub fn artwork(&self) -> Option<&Arc<Artwork>> { self.artwork.as_ref() }

    pub fn dashboard_bytes(&self) -> &[u8] { &self.dashboard }

    pub fn encode(
        &self,
        mode: ExportMode,
    ) -> Result<Vec<u8>, ProjectError> {
        let profile = self.profile.encode().ok_or(ProjectError::Profile)?;
        let mut recipe_bytes = Vec::new();
        let (converted, original) = if let Some(art) = &self.artwork {
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
            (art.converted.as_ref(), source)
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
        out[8..10].copy_from_slice(&1u16.to_le_bytes());
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

    pub fn decode(bytes: &[u8]) -> Result<Self, ProjectError> {
        if bytes.len() < PREFIX || bytes.len() > MAX_PROJECT_BYTES {
            return Err(ProjectError::Length);
        }
        if &bytes[..8] != b"EAVPROJ\0" || u16le(bytes, 8) != 1 {
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
        if canonical != sections[0] {
            return Err(ProjectError::Dashboard);
        }
        let profile = StartupProfile::decode(sections[1]).ok_or(ProjectError::Profile)?;
        let artwork = if sections[2].is_empty() {
            if !sections[3].is_empty() || !sections[4].is_empty() {
                return Err(ProjectError::Structure);
            }
            None
        } else {
            let recipe: Recipe = serde_json::from_slice(sections[3]).map_err(|_| ProjectError::Recipe)?;
            if serde_json::to_vec(&recipe).map_err(|_| ProjectError::Recipe)? != sections[3] {
                return Err(ProjectError::Recipe);
            }
            Some(Arc::new(Artwork::new(
                Arc::from(sections[2]),
                recipe,
                (!sections[4].is_empty()).then(|| Arc::from(sections[4])),
            )?))
        };
        if (profile.theme as u8 == 5) != artwork.is_some() {
            return Err(ProjectError::Profile);
        }
        Ok(Self {
            dashboard: canonical.into(),
            profile,
            artwork,
        })
    }
}
fn u16le(
    b: &[u8],
    o: usize,
) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(
    b: &[u8],
    o: usize,
) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
