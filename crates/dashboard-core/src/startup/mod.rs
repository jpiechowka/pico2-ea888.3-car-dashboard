//! Startup presentation, independent of acquisition and transfer ownership.
mod scene;
mod state;
pub use dashboard_config::startup::*;
pub use scene::{StartupFacts, draw};
pub use state::{Exit, Phase, Startup};

/// Decode directly into the existing caller-owned framebuffer; no scratch image.
pub fn draw_custom(
    asset: dashboard_config::startup_asset::Asset<'_>,
    elapsed: u64,
    looping: bool,
    reduced: bool,
    frame: &mut crate::display::FrameBuffer,
) -> Result<(), dashboard_config::startup_asset::AssetError> {
    let mut rows = asset.frame_at(elapsed, looping, reduced).rows();
    for row in frame
        .as_chunks_mut::<{ dashboard_config::startup_asset::WIDTH * 2 }>()
        .0
    {
        rows.next(row)?;
    }
    Ok(())
}
