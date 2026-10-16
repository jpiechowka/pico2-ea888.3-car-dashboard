//! Bounded host/Worker artwork conversion. Never a firmware dependency.
mod encode;
mod gif_source;
mod png_source;
mod preflight;
use serde::{Deserialize, Serialize};
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PIXELS: usize = 1_048_576;
pub const MAX_SIDE: u32 = 4096;
pub const MAX_SOURCE_FRAMES: usize = 300;
pub const MAX_DECODED_PIXELS: usize = 32_000_000;
pub const MAX_SOURCE_MS: u32 = 60_000;
pub const CONVERTER: &str = "dashboard-startup-tools/1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    Png,
    Gif,
    SpriteSheet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fit {
    Contain,
    Cover,
    Stretch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GifBackground {
    Logical,
    Transparent,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub columns: u16,
    pub rows: u16,
    pub frame_ms: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub kind: SourceKind,
    pub fit: Fit,
    pub matte_rgb: [u8; 3],
    pub sample_fps: u8,
    pub trim_start_ms: u32,
    pub trim_end_ms: u32,
    pub sheet: Option<Sheet>,
    pub gif_background: GifBackground,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            kind: SourceKind::Png,
            fit: Fit::Contain,
            matte_rgb: [0; 3],
            sample_fps: 20,
            trim_start_ms: 0,
            trim_end_ms: 10000,
            sheet: None,
            gif_background: GifBackground::Logical,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    SourceLimit,
    Dimensions,
    Frames,
    Timeline,
    Options,
    Malformed,
    Unsupported,
    Decode,
    Palette,
    AssetLimit,
    Asset,
}
impl core::fmt::Display for ConvertError {
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        f.write_str(match self {
            Self::SourceLimit => "Source exceeds 8 MiB",
            Self::Dimensions => "Image exceeds 4096 per side or 1048576 pixels",
            Self::Frames => "Source exceeds 300 frames or 32 million decoded pixels",
            Self::Timeline => "Select a nonempty clip up to 10 seconds from a source up to 60 seconds",
            Self::Options => "Invalid conversion options or incomplete sprite grid",
            Self::Malformed => "Malformed, truncated or trailing image data",
            Self::Unsupported => "Unsupported image control block or animated PNG",
            Self::Decode => "Image decoding failed",
            Self::Palette => "Invalid GIF palette or color index",
            Self::AssetLimit => "Converted animation exceeds 512 KiB; shorten the clip or reduce frame rate",
            Self::Asset => "Converted asset failed validation",
        })
    }
}
impl std::error::Error for ConvertError {}
pub struct Converted {
    pub asset: Vec<u8>,
    pub source_width: u32,
    pub source_height: u32,
    pub source_frames: u16,
    pub source_duration_ms: u32,
    pub zero_delay_frames: u16,
    pub timing_changed: bool,
    pub trim_end_ms: u32,
}
pub fn convert(
    source: &[u8],
    options: &Options,
) -> Result<Converted, ConvertError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ConvertError::SourceLimit);
    }
    if options.sample_fps == 0
        || options.sample_fps > 20
        || options.trim_end_ms <= options.trim_start_ms
        || options.trim_end_ms - options.trim_start_ms > 10000
        || options.trim_end_ms > MAX_SOURCE_MS
    {
        return Err(ConvertError::Options);
    }
    match options.kind {
        SourceKind::Gif => gif_source::convert(source, options),
        SourceKind::Png | SourceKind::SpriteSheet => png_source::convert(source, options),
    }
}
fn dimensions(
    w: u32,
    h: u32,
) -> Result<usize, ConvertError> {
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(ConvertError::Dimensions);
    }
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .filter(|n| *n <= MAX_PIXELS)
        .ok_or(ConvertError::Dimensions)?;
    Ok(pixels)
}
