//! Host/browser-worker conversion to validated, native-orientation RGB565 startup assets.
//!
//! Source limits precede decoding; output is at most 512 KiB. Decoders, RGBA canvas,
//! temporary frames and output have separate allocations, so no one limit describes
//! peak process memory. Conversion is synchronous; the browser owner enforces its
//! deadline by terminating the worker. No partial asset is returned on failure.
//! PNG/GIF color samples are treated as sRGB-like bytes without ICC/gamma conversion.
//! See the [PNG specification](https://www.w3.org/TR/png-3/) and
//! [GIF89a specification](https://www.w3.org/Graphics/GIF/spec-gif89a.txt).
mod encode;
mod gif_source;
mod png_source;
mod preflight;
use serde::{Deserialize, Serialize};
/// Maximum compressed input bytes, checked before source parsing or decoder creation.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum pixels in a PNG or GIF logical canvas and each GIF frame rectangle.
pub const MAX_PIXELS: usize = 1_048_576;
/// Maximum nonzero source dimension, in pixels; the area limit also applies.
pub const MAX_SIDE: u32 = 4096;
/// Maximum GIF images or sprite cells, including frames outside the selected clip.
pub const MAX_SOURCE_FRAMES: usize = 300;
/// Maximum sum of GIF frame-rectangle areas; excludes repeated canvas/output work.
pub const MAX_DECODED_PIXELS: usize = 32_000_000;
/// Maximum full source timeline in milliseconds, before trimming to at most 10 seconds.
pub const MAX_SOURCE_MS: u32 = 60_000;
/// Recipe provenance identifier; changing conversion semantics requires deliberate versioning.
pub const CONVERTER: &str = "dashboard-startup-tools/2";
/// Explicit parser/animation choice; the converter does not infer format from a filename.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    /// One static PNG treated as a 1000 ms source; APNG is rejected.
    Png,
    /// GIF87a/89a images composited with validated palettes and disposal methods.
    Gif,
    /// Static PNG partitioned into an exact, row-major rectangular grid.
    SpriteSheet,
}
/// Integer nearest-neighbor mapping into the selected native display geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fit {
    /// Preserve aspect ratio, center the image and fill uncovered pixels with the matte.
    Contain,
    /// Preserve aspect ratio and center-crop the scaled image to fill the frame.
    Cover,
    /// Scale both axes independently; aspect ratio may change.
    Stretch,
}
/// GIF initial-canvas and Background-disposal policy; transparency is flattened later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GifBackground {
    /// Use the global palette's background color; without a global palette use transparent.
    Logical,
    /// Use transparent pixels regardless of the logical background palette entry.
    Transparent,
}
/// Sprite grid recipe; validation also requires exact division of the decoded PNG dimensions.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Nonzero horizontal cell count; columns times rows must not exceed 300.
    pub columns: u16,
    /// Nonzero vertical cell count; cells are visited left-to-right, then top-to-bottom.
    pub rows: u16,
    /// Nonzero duration of each source cell in milliseconds, before output resampling.
    pub frame_ms: u16,
}
/// Serializable conversion recipe, not an admission token; call [`Options::validate`] or [`convert`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    /// Output dimensions/stride; both geometries contain exactly 76,800 pixels.
    pub geometry: dashboard_config::Geometry,
    /// Selected source decoder and optional sprite interpretation.
    pub kind: SourceKind,
    /// Resize/crop policy applied before RGB565 quantization.
    pub fit: Fit,
    /// Opaque red/green/blue matte channels, 0..=255; also fills contain letterboxes.
    pub matte_rgb: [u8; 3],
    /// Requested 1..=20 Hz sampling; actual integer step is floor(1000 / rate) ms.
    pub sample_fps: u8,
    /// Inclusive source-time start in milliseconds; must precede the clamped source end.
    pub trim_start_ms: u32,
    /// Exclusive requested end in milliseconds, at most 60,000 and 10,000 after start.
    /// Conversion clamps this to source duration; clips below 34 ms are extended in output.
    pub trim_end_ms: u32,
    /// Required nonzero grid for SpriteSheet; PNG and GIF conversion reject `Some`.
    pub sheet: Option<Sheet>,
    /// GIF-only canvas/disposal background; ignored for PNG and sprite sources.
    pub gif_background: GifBackground,
}
impl Options {
    /// Return `Ok(())` for source-independent rate/trim bounds and required nonzero sprite grid.
    /// Does not inspect image dimensions, grid divisibility, source duration, output size,
    /// or reject an extraneous grid on PNG/GIF; [`convert`] performs those checks.
    /// Returns [`ConvertError::Options`] without modifying the recipe.
    pub fn validate(&self) -> Result<(), ConvertError> {
        if self.sample_fps == 0
            || self.sample_fps > 20
            || self.trim_end_ms <= self.trim_start_ms
            || self.trim_end_ms - self.trim_start_ms > 10000
            || self.trim_end_ms > MAX_SOURCE_MS
        {
            return Err(ConvertError::Options);
        }
        if self.kind == SourceKind::SpriteSheet
            && self
                .sheet
                .is_none_or(|s| s.columns == 0 || s.rows == 0 || s.frame_ms == 0)
        {
            return Err(ConvertError::Options);
        }
        Ok(())
    }
}
impl Default for Options {
    /// Return landscape static-PNG contain/black settings at 20 Hz, requested clip 0..10,000 ms,
    /// no grid and logical GIF background; a still PNG clamps to its 1000 ms source duration.
    fn default() -> Self {
        Self {
            geometry: dashboard_config::Geometry::Landscape,
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
/// Conversion admission/decoding failure; no variant carries a partially usable asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// Compressed source exceeds 8 MiB.
    SourceLimit,
    /// Zero/oversized dimension, pixel-count overflow or oversized decoded output buffer.
    Dimensions,
    /// Frame count or cumulative GIF rectangle area exceeded, or decoder frame-count mismatch.
    Frames,
    /// Empty/out-of-source clip, excessive source duration, or no emitted frames.
    Timeline,
    /// Invalid rate/trim/grid or a grid supplied for a nonsprite source.
    Options,
    /// Invalid structure, CRC, reserved bits, truncation or bytes after the terminator.
    Malformed,
    /// APNG, GIF user-input/reserved disposal, or an unsupported GIF extension.
    Unsupported,
    /// Decoder failure or decoded shape/channel/bit-depth mismatch; lower-level detail is discarded.
    Decode,
    /// Missing/malformed GIF palette or an out-of-range color/transparency/background index.
    Palette,
    /// Serialized frame table plus payloads would exceed the native asset budget.
    AssetLimit,
    /// Generated output failed the shared native asset validator.
    Asset,
}
impl core::fmt::Display for ConvertError {
    /// Write a stable category explanation to `f`; return its formatting error if the sink fails.
    /// Messages omit source bytes and distinguish admission failure from successful conversion.
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
/// Validated output and bounded source facts; original bytes and options are retained by callers.
pub struct Converted {
    /// Complete current-version startup asset, including checksums and native pixel stride.
    pub asset: Vec<u8>,
    /// Original PNG width or GIF logical-canvas width in pixels, not sprite-cell width.
    pub source_width: u32,
    /// Original PNG height or GIF logical-canvas height in pixels, not sprite-cell height.
    pub source_height: u32,
    /// Total source frames/cells before trimming or duplicate-output coalescing, 1..=300.
    pub source_frames: u16,
    /// Full source duration in milliseconds after zero/missing GIF delay normalization.
    pub source_duration_ms: u32,
    /// Explicit GIF zero-delay controls replaced with 100 ms; absent controls are not counted.
    pub zero_delay_frames: u16,
    /// Whether an animated source delay differs from the integer sampling step.
    /// Not a complete comparison of source/output timestamps or final frame count.
    pub timing_changed: bool,
    /// Requested end clamped to source duration, in absolute source milliseconds.
    /// The output's relative duration can include minimum-frame padding.
    pub trim_end_ms: u32,
}
/// Convert compressed `source` under `options`; return a validated asset plus source metadata.
/// Reject oversized input before parsing; malformed/unsupported sources and resource/option
/// failures return [`ConvertError`] with no output. Allocation failure follows the allocator's
/// failure policy. This synchronous function has no cancellation/deadline hook and performs
/// no file, device or editor mutation; worker/process ownership belongs to the caller.
pub fn convert(
    source: &[u8],
    options: &Options,
) -> Result<Converted, ConvertError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ConvertError::SourceLimit);
    }
    options.validate()?;
    match options.kind {
        SourceKind::Gif => gif_source::convert(source, options),
        SourceKind::Png | SourceKind::SpriteSheet => png_source::convert(source, options),
    }
}
/// Return checked pixel area for nonzero `w`/`h` within per-side and area limits.
/// Inputs are source pixels; overflow or either violated bound returns `Dimensions`.
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
