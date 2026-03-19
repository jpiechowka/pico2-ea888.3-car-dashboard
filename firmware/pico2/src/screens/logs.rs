use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::profiling::{LOG_BUFFER, LogEntry};
use crate::ui::{BLACK, GREEN, LABEL_FONT, WHITE};

pub fn draw_logs_page<D>(display: &mut D)
where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let footer_style = MonoTextStyle::new(LABEL_FONT, GREEN);

    display.clear(BLACK).ok();

    Text::new("LOGS", Point::new(4, 12), header_style).draw(display).ok();

    if let Ok(buffer) = LOG_BUFFER.try_lock() {
        let line_height = 14;
        let mut y = 28;

        for entry in buffer.iter() {
            draw_log_entry(display, entry, y);
            y += line_height;

            if y > 210 {
                break;
            }
        }

        if buffer.is_empty() {
            let empty_style = MonoTextStyle::new(LABEL_FONT, WHITE);
            Text::new("No log entries", Point::new(4, 120), empty_style)
                .draw(display)
                .ok();
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
