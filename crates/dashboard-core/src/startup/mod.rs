//! Startup presentation, independent of acquisition and transfer ownership.
/// Procedural artwork and startup status facts, rendered directly into the owned framebuffer.
mod scene;
/// Completion identities, dwell deadlines and startup input suppression.
mod state;
pub use dashboard_config::startup::*;
pub use scene::{StartupFacts, draw};
pub use state::{Exit, Phase, Startup};

/// Decode directly into the existing caller-owned framebuffer; no scratch image.
/// Selects a frame from `asset` at `elapsed` milliseconds using `looping`; `reduced`
/// holds its reduced-motion frame. Returns decode success/error while mutating
/// `frame` in the asset's geometry; an error can leave earlier rows replaced.
pub fn draw_custom(
    asset: dashboard_config::startup_asset::Asset<'_>,
    elapsed: u64,
    looping: bool,
    reduced: bool,
    frame: &mut crate::display::FrameBuffer,
) -> Result<(), dashboard_config::startup_asset::AssetError> {
    draw_custom_in(asset, elapsed, looping, reduced, asset.geometry(), frame)
}

/// Reject equal-byte-count artwork of the wrong logical shape before decoding.
/// Requires `asset` geometry to equal `geometry`; mismatch returns Pixels before
/// touching `frame`. Otherwise decodes the frame selected by `elapsed` milliseconds,
/// `looping` and `reduced` directly into `frame`. Returns success or a decoder error;
/// row failures may leave a partially replaced framebuffer which must not be published as complete.
pub fn draw_custom_in(
    asset: dashboard_config::startup_asset::Asset<'_>,
    elapsed: u64,
    looping: bool,
    reduced: bool,
    geometry: dashboard_config::Geometry,
    frame: &mut crate::display::FrameBuffer,
) -> Result<(), dashboard_config::startup_asset::AssetError> {
    if asset.geometry() != geometry {
        return Err(dashboard_config::startup_asset::AssetError::Pixels);
    }
    let mut rows = asset.frame_at(elapsed, looping, reduced).rows();
    for row in frame.chunks_exact_mut(usize::from(geometry.width()) * 2) {
        rows.next(row)?;
    }
    Ok(())
}

/// Resolve exactly once for the screen whose pixels will be submitted.
/// Returns orientation from `config` and `profile` for startup `phase`, normal
/// `page` and stable `view_id`. Missing Dashboard views and Debug/Logs inherit
/// the device default; Status and Splash use their respective profile overrides.
pub fn screen_orientation(
    config: &dashboard_config::Config,
    profile: StartupProfile,
    phase: Phase,
    page: crate::pages::Page,
    view_id: u8,
) -> dashboard_config::Orientation {
    match phase {
        Phase::Status => profile.effective_status_orientation(config.orientation),
        Phase::Splash => profile.effective_splash_orientation(config.orientation),
        Phase::Dashboard => {
            match page {
                crate::pages::Page::Dashboard => config.view_orientation(view_id).unwrap_or(config.orientation),
                _ => config.orientation,
            }
        }
    }
}
