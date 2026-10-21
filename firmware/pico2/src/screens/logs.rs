//! Incremental shared Logs rendering with stable sequence snapshots.
use dashboard_pico2::logging::VISIBLE_LINES;
use dashboard_pico2::logging_view::LogPage;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

/// Draws a stable log snapshot into `display` using optional sequence `anchor`
/// and current control `map`. Each bounded section updates `work` around a yield.
/// Source locks end before drawing; cancellation may leave a partial framebuffer,
/// which must not be submitted as a completed scene. Drawing errors are ignored
/// by the shared renderer as for the infallible production framebuffer.
pub async fn draw_logs_page<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    anchor: Option<u64>,
    map: &dashboard_pico2::view_config::ControlMap,
    work: &mut crate::profiling::RenderWork,
) {
    let page = LogPage::capture(&crate::profiling::LOGS, anchor);
    page.draw_header(display);
    work.checkpoint().await;
    for row in 0..=VISIBLE_LINES {
        page.draw_row(display, row);
        work.checkpoint().await;
    }
    page.draw_hint(display, map);
    work.checkpoint().await;
    page.draw_footer(display);
}
