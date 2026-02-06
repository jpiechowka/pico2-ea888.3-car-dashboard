//! Loading screen with console-style initialization messages.
//!
//! Displays initialization messages sequentially with delays between each message,
//! simulating a console boot sequence. The title bar has animated spinners that
//! rotate based on elapsed time.
//!
//! # Usage
//!
//! The caller should iterate over [`INIT_MESSAGES`] and render continuously during
//! each message's wait period so the spinners animate. Pass the total elapsed time
//! in milliseconds since boot for time-based spinner animation.
//! See `main.rs` boot sequence for the reference implementation.
//!
//! # Example
//!
//! ```ignore
//! let mut visible_lines: [&str; MAX_VISIBLE_LINES] = [""; MAX_VISIBLE_LINES];
//! let mut line_count = 0;
//! let boot_start = Instant::now();
//!
//! for (msg, duration_ms) in &INIT_MESSAGES {
//!     // Add message to visible lines (with scrolling)
//!     if line_count < MAX_VISIBLE_LINES {
//!         visible_lines[line_count] = msg;
//!         line_count += 1;
//!     }
//!     let msg_start = Instant::now();
//!     loop {
//!         let elapsed_ms = boot_start.elapsed().as_millis() as u32;
//!         draw_loading_frame(&mut display, &visible_lines, line_count, elapsed_ms);
//!         display.flush().await;
//!         if msg_start.elapsed().as_millis() >= *duration_ms as u64 { break; }
//!     }
//! }
//! ```

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::Text;
use heapless::String;

use crate::ui::{BLACK, CENTERED, LEFT_ALIGNED, RED, WHITE};

const TITLE_POS: Point = Point::new(160, 25);
const LINE_START: Point = Point::new(10, 35);
const LINE_END: Point = Point::new(310, 35);
const CONSOLE_X: i32 = 10;
const CONSOLE_START_Y: i32 = 50;
const CONSOLE_LINE_HEIGHT: i32 = 14;

/// Maximum number of console lines visible on the loading screen.
pub const MAX_VISIBLE_LINES: usize = 12;

const TITLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_10X20, RED);
const CONSOLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_6X10, BLACK);
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RED, 1);

/// Messages to display during loading (message, duration in milliseconds).
pub const INIT_MESSAGES: [(&str, u64); 7] = [
    ("Initializing OBD-II interface...", 800),
    ("Connecting to ECU...", 1200),
    ("Reading vehicle info...", 1000),
    ("Leon Cupra 5F FL | 2.0 TSI 300HP", 600),
    ("DQ381-7F DSG MQB-EVO", 600),
    ("Loading sensors...", 800),
    ("Ready.", 500),
];

const SPINNER_CHARS: [char; 4] = ['|', '/', '-', '\\'];

/// Draw a single frame of the loading screen.
///
/// # Arguments
/// * `elapsed_ms` - Milliseconds since the loading screen started. Used for time-based spinner animation (rotates every
///   150ms).
///
/// This is a non-async function that renders one frame. Call this in a loop
/// with appropriate timing and flush the display after each call.
pub fn draw_loading_frame<D>(
    display: &mut D,
    visible_lines: &[&str],
    line_count: usize,
    elapsed_ms: u32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(WHITE).ok();

    // Time-based spinner: rotates every 150ms
    let spinner_idx = (elapsed_ms / 150) as usize % SPINNER_CHARS.len();
    let left_spinner = SPINNER_CHARS[spinner_idx];
    let right_spinner = SPINNER_CHARS[(spinner_idx + 2) % SPINNER_CHARS.len()];

    // Draw title with spinners
    let mut loading_text: String<32> = String::new();
    let _ = write!(loading_text, "{left_spinner}  Loading shit  {right_spinner}");
    Text::with_text_style(&loading_text, TITLE_POS, TITLE_STYLE, CENTERED)
        .draw(display)
        .ok();

    // Draw divider line
    Line::new(LINE_START, LINE_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    // Draw console lines
    for (i, line) in visible_lines.iter().take(line_count).enumerate() {
        let y_pos = CONSOLE_START_Y + (i as i32 * CONSOLE_LINE_HEIGHT);
        let prefix = if i == line_count - 1 { "> " } else { "  " };
        let mut full_line: String<64> = String::new();
        let _ = write!(full_line, "{prefix}{line}");
        Text::with_text_style(&full_line, Point::new(CONSOLE_X, y_pos), CONSOLE_STYLE, LEFT_ALIGNED)
            .draw(display)
            .ok();
    }
}
