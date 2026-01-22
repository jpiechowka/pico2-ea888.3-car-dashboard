//! Loading screen with console-style initialization messages.
//!
//! Displays a retro-style "booting" sequence with animated spinner and
//! sequential messages simulating OBD-II system initialization.
//!
//! # Visual Layout
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │    |  Loading shit  /              │  Title with spinner
//! │────────────────────────────────────│  Divider line
//! │ > Initializing OBD-II interface... │
//! │   Connecting to ECU...             │  Console output
//! │   Reading vehicle info...          │  (scrolling)
//! │ > Leon Cupra 5F FL | 2.0 TSI 300HP │
//! └────────────────────────────────────┘
//! ```
//!
//! # Features
//!
//! - **Animated spinner**: Cycles through `|`, `/`, `-`, `\` characters
//! - **Sequential messages**: Each displays for a configured duration
//! - **Console scrolling**: Maximum 12 lines visible, older lines scroll off
//! - **Current line marker**: `>` prefix indicates latest message
//!
//! # Optimizations Applied
//!
//! ## Pre-computed Position Constants
//! All fixed positions are `const Point`:
//! - `TITLE_POS`: Center of spinner/title area
//! - `LINE_START`/`LINE_END`: Divider line endpoints
//! - `CONSOLE_X`, `CONSOLE_START_Y`: Console text origin
//!
//! ## Const `MonoTextStyle` and `PrimitiveStyle`
//! Text styles and divider line style are defined as `const` using the const fn
//! constructors in embedded-graphics 0.8, eliminating per-frame construction.
//!
//! ## Heapless Strings
//! Uses `heapless::String<32>` and `<64>` for the title and console lines,
//! avoiding heap allocation in the render loop.
//!
//! ## Static Alignment Styles
//! Uses `CENTERED` and `LEFT_ALIGNED` from [`crate::styles`] instead of
//! constructing `TextStyle` objects per frame.

use core::fmt::Write;
use std::thread;
use std::time::{Duration, Instant};

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::{SimulatorDisplay, SimulatorEvent, Window};
use heapless::String;

use crate::colors::{BLACK, RED, WHITE};
use crate::styles::{CENTERED, LEFT_ALIGNED};

// =============================================================================
// Loading Screen Layout Constants (Optimization: pre-computed at compile time)
// =============================================================================

/// Title text position (horizontally centered)
const TITLE_POS: Point = Point::new(160, 25);

/// Divider line start (left edge with margin)
const LINE_START: Point = Point::new(10, 35);

/// Divider line end (right edge with margin)
const LINE_END: Point = Point::new(310, 35);

/// Console text X position (left margin)
const CONSOLE_X: i32 = 10;

/// Console first line Y position
const CONSOLE_START_Y: i32 = 50;

/// Vertical spacing between console lines
const CONSOLE_LINE_HEIGHT: i32 = 14;

// =============================================================================
// Pre-computed Styles (Optimization: const fn in embedded-graphics 0.8)
// =============================================================================

/// Red title text style (`FONT_10X20`).
/// `MonoTextStyle::new` is const fn, so this is computed at compile time.
const TITLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_10X20, RED);

/// Black console text style (`FONT_6X10`).
/// `MonoTextStyle::new` is const fn, so this is computed at compile time.
const CONSOLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_6X10, BLACK);

/// Red stroke style for divider line (1px wide).
/// `PrimitiveStyle::with_stroke` is const fn, so this is computed at compile time.
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RED, 1);

// =============================================================================
// Loading Screen Function
// =============================================================================

/// Run the loading screen boot sequence.
///
/// Displays initialization messages with animated spinner.
/// Returns `false` if window is closed, `true` when sequence completes.
pub fn run_loading_screen(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> bool {
    // Init messages: (text, display duration in ms)
    // These simulate OBD-II connection and vehicle identification
    let init_messages = [
        ("Initializing OBD-II interface...", 800),
        ("Connecting to ECU...", 1200),
        ("Reading vehicle info...", 1000),
        ("Leon Cupra 5F FL | 2.0 TSI 300HP", 600), // Vehicle identification
        ("DQ381-7F DSG MQB-EVO", 600),             // Transmission info
        ("Loading sensors...", 800),
        ("Ready.", 500),
    ];

    // Spinner animation characters (classic text-mode spinner)
    let spinner_chars = ['|', '/', '-', '\\'];
    let mut spinner_idx = 0;
    let mut spinner_frame = 0u32;

    // Console line buffer (scrolls when > 12 lines)
    let mut console_lines: Vec<&str> = Vec::new();

    // Process each initialization message
    for (msg, duration_ms) in &init_messages {
        // Add message to console buffer
        console_lines.push(msg);
        if console_lines.len() > 12 {
            console_lines.remove(0); // Scroll off oldest line
        }

        let msg_start = Instant::now();
        let msg_duration = Duration::from_millis(*duration_ms as u64);

        // Animate while this message is displayed
        while msg_start.elapsed() < msg_duration {
            // Handle window close event
            for ev in window.events() {
                if matches!(ev, SimulatorEvent::Quit) {
                    return false;
                }
            }

            // White background for console appearance
            display.clear(WHITE).ok();

            // Update spinner every 8 frames (~130ms) for a calmer spin
            spinner_frame = spinner_frame.wrapping_add(1);
            if spinner_frame.is_multiple_of(8) {
                spinner_idx = (spinner_idx + 1) % spinner_chars.len();
            }
            let left_spinner = spinner_chars[spinner_idx];
            let right_spinner = spinner_chars[(spinner_idx + 2) % spinner_chars.len()];

            // Title with animated spinners using const TITLE_STYLE
            // Optimization: heapless::String avoids heap allocation
            let mut loading_text: String<32> = String::new();
            let _ = write!(loading_text, "{left_spinner}  Loading shit  {right_spinner}");
            Text::with_text_style(&loading_text, TITLE_POS, TITLE_STYLE, CENTERED)
                .draw(display)
                .ok();

            // Divider line between title and console using const DIVIDER_STYLE
            Line::new(LINE_START, LINE_END)
                .into_styled(DIVIDER_STYLE)
                .draw(display)
                .ok();

            // Draw console lines with current line marker using const CONSOLE_STYLE
            for (i, line) in console_lines.iter().enumerate() {
                let y_pos = CONSOLE_START_Y + (i as i32 * CONSOLE_LINE_HEIGHT);
                // Current (last) line gets ">" prefix, others get "  " for alignment
                let prefix = if i == console_lines.len() - 1 { "> " } else { "  " };
                // Optimization: heapless::String avoids heap allocation
                let mut full_line: String<64> = String::new();
                let _ = write!(full_line, "{prefix}{line}");
                Text::with_text_style(&full_line, Point::new(CONSOLE_X, y_pos), CONSOLE_STYLE, LEFT_ALIGNED)
                    .draw(display)
                    .ok();
            }

            window.update(display);
            thread::sleep(Duration::from_millis(16)); // ~60 FPS for smooth spinner
        }
    }

    // Brief pause after "Ready." before transitioning to welcome screen
    thread::sleep(Duration::from_millis(1000));
    true
}
