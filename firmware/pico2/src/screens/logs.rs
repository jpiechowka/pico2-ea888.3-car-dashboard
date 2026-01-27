//! Logs page for on-device log viewing.
//!
//! Displays recent log entries with color-coded levels and timestamps.
//! Shows up to 14 log entries on a 320x240 display.
//!
//! # Layout
//!
//! ```text
//! LOGS                              (header)
//! [I] 12345 System started          (entries)
//! [W] 12350 Low battery warning
//! [E] 12355 Sensor timeout
//! ...
//! Press Y for Dashboard             (footer)
//! ```

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::colors::{BLACK, GREEN, WHITE};
use crate::log_buffer::{LOG_BUFFER, LogEntry};
use crate::styles::LABEL_FONT;

/// Draw the logs page with recent log entries.
pub fn draw_logs_page<D>(display: &mut D)
where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let footer_style = MonoTextStyle::new(LABEL_FONT, GREEN);

    // Clear screen
    display.clear(BLACK).ok();

    // Header
    Text::new("LOGS", Point::new(4, 12), header_style).draw(display).ok();

    // Try to get log entries
    if let Ok(buffer) = LOG_BUFFER.try_lock() {
        let line_height = 14;
        let mut y = 28;

        for entry in buffer.iter() {
            draw_log_entry(display, entry, y);
            y += line_height;

            // Stop if we'd go off screen (leave room for footer)
            if y > 210 {
                break;
            }
        }

        // Show entry count
        if buffer.is_empty() {
            let empty_style = MonoTextStyle::new(LABEL_FONT, WHITE);
            Text::new("No log entries", Point::new(4, 120), empty_style)
                .draw(display)
                .ok();
        }
    } else {
        // Couldn't acquire lock
        let busy_style = MonoTextStyle::new(LABEL_FONT, WHITE);
        Text::new("Log buffer busy...", Point::new(4, 120), busy_style)
            .draw(display)
            .ok();
    }

    // Footer
    Text::new("Press Y for Dashboard", Point::new(4, 226), footer_style)
        .draw(display)
        .ok();
}

/// Draw a single log entry.
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

    // Format: [L] TTTTT message
    // [L] = level prefix in color
    // TTTTT = timestamp (mod 100000 for 5 digits)
    let mut prefix: String<16> = String::new();
    let _ = write!(prefix, "[{}] {:05}", entry.level.prefix(), entry.timestamp_ms % 100_000);

    // Draw level prefix in color
    Text::new(&prefix, Point::new(4, y), level_style).draw(display).ok();

    // Draw message in white (offset after prefix)
    // Prefix is ~12 chars at 6px = 72px, add spacing
    Text::new(entry.message.as_str(), Point::new(84, y), msg_style)
        .draw(display)
        .ok();
}
