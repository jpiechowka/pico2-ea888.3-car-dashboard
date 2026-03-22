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
pub fn draw_logs_page<D>(
    display: &mut D,
    scroll_offset: i32,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let footer_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let scroll_style = MonoTextStyle::new(LABEL_FONT, YELLOW);

    display.clear(BLACK).ok();

    Text::new("LOGS", Point::new(4, 12), header_style).draw(display).ok();

    if let Ok(buffer) = LOG_BUFFER.try_lock() {
        let total = buffer.count();

        if total == 0 {
            let empty_style = MonoTextStyle::new(LABEL_FONT, WHITE);
            Text::new("No log entries", Point::new(4, 120), empty_style)
                .draw(display)
                .ok();
        } else {
            let visible = VISIBLE_LINES.min(total);
            let max_offset = if total > visible { total - visible } else { 0 };
            let offset = (scroll_offset as usize).min(max_offset);

            // Determine start index: skip older entries based on scroll position.
            // iter() yields oldest-first. We want to show entries from
            // (total - visible - offset) to (total - 1 - offset).
            let skip = if total > visible { total - visible - offset } else { 0 };

            let line_height = 14;
            let mut y = 28;
            let mut drawn = 0;

            for (i, entry) in buffer.iter().enumerate() {
                if i < skip {
                    continue;
                }
                if drawn >= visible {
                    break;
                }
                draw_log_entry(display, entry, y);
                y += line_height;
                drawn += 1;
            }

            // Scroll indicator (right side of header)
            if max_offset > 0 {
                let mut indicator: String<16> = String::new();
                let line_from = skip + 1;
                let line_to = skip + drawn;
                let _ = write!(indicator, "{}-{}/{}", line_from, line_to, total);
                Text::new(indicator.as_str(), Point::new(220, 12), scroll_style)
                    .draw(display)
                    .ok();
            }
        }
    } else {
        let busy_style = MonoTextStyle::new(LABEL_FONT, WHITE);
        Text::new("Log buffer busy...", Point::new(4, 120), busy_style)
            .draw(display)
            .ok();
    }

    Text::new("Press Y for Dashboard", Point::new(4, 226), footer_style)
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
