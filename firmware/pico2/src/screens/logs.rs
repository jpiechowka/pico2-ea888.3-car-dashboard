use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::profiling::{LOG_BUFFER, LogEntry};
use crate::ui::{BLACK, GREEN, LABEL_FONT, WHITE, YELLOW};

/// Maximum number of log lines visible on screen at once.
const VISIBLE_LINES: usize = 13;

/// Draw the logs page with scroll support.
///
/// `scroll_offset`: number of lines scrolled UP from the bottom (newest).
/// 0 = showing the most recent entries, positive = showing older entries.
pub async fn draw_logs_page<D>(
    display: &mut D,
    scroll_offset: i32,
    work: &mut crate::profiling::RenderWork,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let scroll_style = MonoTextStyle::new(LABEL_FONT, YELLOW);
    display.clear(BLACK).ok();
    Text::new("LOGS", Point::new(4, 12), header_style).draw(display).ok();
    work.checkpoint().await;

    // Copy only visible entries. Release the log mutex before formatting,
    // rasterization or an await, so core1 logging never waits for a screen.
    let snapshot = if let Ok(buffer) = LOG_BUFFER.try_lock() {
        let total = buffer.count();
        let visible = VISIBLE_LINES.min(total);
        let max_offset = total.saturating_sub(visible);
        let offset = (scroll_offset.max(0) as usize).min(max_offset);
        let skip = max_offset - offset;
        let mut entries: heapless::Vec<LogEntry, VISIBLE_LINES> = heapless::Vec::new();
        for entry in buffer.iter().skip(skip).take(visible) {
            let _ = entries.push(entry.clone());
        }
        Some((total, skip, entries))
    } else {
        None
    };
    if let Some((total, skip, entries)) = snapshot {
        if entries.is_empty() {
            let style = MonoTextStyle::new(LABEL_FONT, WHITE);
            Text::new("No log entries", Point::new(4, 120), style)
                .draw(display)
                .ok();
        }
        for (index, entry) in entries.iter().enumerate() {
            draw_log_entry(display, entry, 28 + index as i32 * 14);
            work.checkpoint().await;
        }
        if total > VISIBLE_LINES {
            let mut indicator: String<16> = String::new();
            let _ = write!(indicator, "{}-{}/{}", skip + 1, skip + entries.len(), total);
            Text::new(&indicator, Point::new(220, 12), scroll_style)
                .draw(display)
                .ok();
        }
    } else {
        let style = MonoTextStyle::new(LABEL_FONT, WHITE);
        Text::new("Log buffer busy...", Point::new(4, 120), style)
            .draw(display)
            .ok();
    }
    Text::new("Press Y for Dashboard", Point::new(4, 226), header_style)
        .draw(display)
        .ok();
}

fn draw_log_entry<D>(
    display: &mut D,
    entry: &LogEntry,
    y: i32,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let level_color = entry.level.color();
    let level_style = MonoTextStyle::new(LABEL_FONT, level_color);
    let msg_style = MonoTextStyle::new(LABEL_FONT, WHITE);

    let mut prefix: String<16> = String::new();
    let _ = write!(prefix, "[{}] {:05}", entry.level.prefix(), entry.timestamp_ms % 100_000);

    Text::new(&prefix, Point::new(4, y), level_style).draw(display).ok();

    Text::new(entry.message.as_str(), Point::new(84, y), msg_style)
        .draw(display)
        .ok();
}
