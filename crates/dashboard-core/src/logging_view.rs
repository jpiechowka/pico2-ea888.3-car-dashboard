//! Pure log page rendering; all storage locks end before drawing or yielding.
use core::fmt::Write;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::logging::{Counters, Entry, Level, LogStore, VISIBLE_LINES, Window};
use crate::ui::{BLACK, GREEN, LABEL_FONT, RED, WHITE, YELLOW};

/// Owned log-page capture for resumable row drawing without retained storage locks.
pub struct LogPage {
    /// Copied history window, or `None` if its try-lock was busy during capture.
    history: Option<Window>,
    /// Independently captured latest explicit fault; no shared transaction with history.
    fault: Option<Entry>,
    /// Independently sampled loss/truncation statistics.
    counters: Counters,
    /// Whether the caller selected a fixed history anchor rather than follow-latest.
    anchored: bool,
}
impl LogPage {
    /// Returns owned snapshots from `store` for ending sequence `anchor` (`None`
    /// follows latest). Captures history, fault and counters separately; all locks
    /// are released before return and busy history remains visibly unavailable.
    pub fn capture<M: RawMutex>(
        store: &LogStore<M>,
        anchor: Option<u64>,
    ) -> Self {
        Self {
            history: store.window(anchor),
            fault: store.fault(),
            counters: store.counters(),
            anchored: anchor.is_some(),
        }
    }

    /// Clears the whole `display` to black and draws LOGS plus LIVE/HISTORY state.
    /// Draw errors are discarded; caller must handle transport completion separately.
    pub fn draw_header<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        display.clear(BLACK).ok();
        let style = MonoTextStyle::new(LABEL_FONT, GREEN);
        Text::new("LOGS", Point::new(4, 12), style).draw(display).ok();
        Text::new(
            if self.anchored { "HISTORY" } else { "LIVE" },
            Point::new(display.bounding_box().size.width as i32 - 60, 12),
            style,
        )
        .draw(display)
        .ok();
    }

    /// Draws captured `row` into `display`: zero is the protected fault, `1..=12`
    /// are history rows, larger indices are inert. Empty/busy history is labeled
    /// in its first row; drawing errors are discarded and no storage is reread.
    pub fn draw_row<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        row: usize,
    ) {
        if row == 0 {
            if let Some(entry) = &self.fault {
                draw_entry(display, entry, 28);
            } else {
                Text::new(
                    "FAULT: none this boot",
                    Point::new(4, 28),
                    MonoTextStyle::new(LABEL_FONT, YELLOW),
                )
                .draw(display)
                .ok();
            }
            return;
        }
        if row > VISIBLE_LINES {
            return;
        }
        if let Some(window) = &self.history {
            if let Some(record) = window.entries.get(row - 1) {
                draw_entry(
                    display,
                    &record.entry,
                    44 + (row as i32 - 1) * if portrait(display) { 20 } else { 14 },
                );
            } else if row == 1 {
                Text::new(
                    "No log entries",
                    Point::new(4, 44),
                    MonoTextStyle::new(LABEL_FONT, WHITE),
                )
                .draw(display)
                .ok();
            }
        } else if row == 1 {
            Text::new(
                "Log history busy",
                Point::new(4, 44),
                MonoTextStyle::new(LABEL_FONT, WHITE),
            )
            .draw(display)
            .ok();
        }
    }

    /// Draws loss/truncation/eviction counts into `display`, using `--` for unavailable
    /// history. Loss sums saturate; eviction display clamps to `u32::MAX`. Draw errors are discarded.
    pub fn draw_footer<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        let mut text: String<52> = String::new();
        if let Some(history) = &self.history {
            let overwritten = history.overwritten.min(u64::from(u32::MAX));
            let _ = write!(
                text,
                "lost:{} cut:{} old:{}",
                self.counters.contention.saturating_add(self.counters.exhausted),
                self.counters.truncated,
                overwritten
            );
        } else {
            let _ = write!(
                text,
                "lost:{} cut:{} old:--",
                self.counters.contention.saturating_add(self.counters.exhausted),
                self.counters.truncated
            );
        }
        Text::new(
            &text,
            Point::new(4, if portrait(display) { 302 } else { 226 }),
            MonoTextStyle::new(
                if portrait(display) {
                    &embedded_graphics::mono_font::ascii::FONT_4X6
                } else {
                    LABEL_FONT
                },
                YELLOW,
            ),
        )
        .draw(display)
        .ok();
    }

    /// Clears and draws this complete captured log page into `display`; ignores
    /// target errors. Navigation hints are a separate call so adapters can use current controls.
    pub fn draw_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        self.draw_header(display);
        for row in 0..=VISIBLE_LINES {
            self.draw_row(display, row);
        }
        self.draw_footer(display);
    }

    /// Independent of counters/fault/history; callers can yield between rows.
    /// Draws the Logs-to-Dashboard navigation route from `map` into `display`,
    /// selecting compact portrait text and discarding draw errors.
    pub fn draw_hint<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        map: &dashboard_config::ControlMap,
    ) {
        let hint = crate::controls::navigation_hint(
            map,
            dashboard_config::ControlPage::Logs,
            dashboard_config::ControlPage::Dashboard,
        );
        if portrait(display) {
            crate::debug_view::draw_compact_row(display, 4, 288, 232, hint.as_str(), GREEN);
        } else {
            crate::debug_view::draw_row(display, 4, 212, 316, hint.as_str(), GREEN);
        }
    }
}
/// Draws `entry` into `display` at baseline `y` pixels, replacing control
/// characters with `?`. Timestamp text wraps modulo 100000 ms; the stored timestamp
/// remains full-width. Chooses font/offset by target geometry and ignores draw errors.
fn draw_entry<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    entry: &Entry,
    y: i32,
) {
    let compact = portrait(display);
    let font = if compact {
        &embedded_graphics::mono_font::ascii::FONT_4X6
    } else {
        LABEL_FONT
    };
    let color = match entry.level {
        Level::Info => GREEN,
        Level::Fault => RED,
    };
    let mut prefix: String<16> = String::new();
    let _ = write!(prefix, "[{}] {:05}", entry.level.prefix(), entry.timestamp_ms % 100_000);
    Text::new(&prefix, Point::new(4, y), MonoTextStyle::new(font, color))
        .draw(display)
        .ok();
    // A log is one row. Control characters cannot move its cursor into a
    // neighbouring entry or the separately protected fault row.
    let mut message: String<40> = String::new();
    for character in entry.message.as_str().chars() {
        let _ = message.push(if character.is_control() { '?' } else { character });
    }
    Text::new(
        &message,
        Point::new(if compact { 52 } else { 76 }, y),
        MonoTextStyle::new(font, WHITE),
    )
    .draw(display)
    .ok();
}

/// Returns whether `display` has the exact shared portrait size, 240 by 320 pixels.
fn portrait<D: Dimensions>(display: &D) -> bool {
    crate::configured_scene::target_geometry(display) == dashboard_config::Geometry::Portrait
}
