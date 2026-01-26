//! Loading screen with console-style initialization messages.

use core::fmt::Write;

use dashboard_common::colors::{BLACK, RED, WHITE};
use dashboard_common::styles::{CENTERED, LEFT_ALIGNED};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::Text;
use heapless::String;

const TITLE_POS: Point = Point::new(160, 25);
const LINE_START: Point = Point::new(10, 35);
const LINE_END: Point = Point::new(310, 35);
const CONSOLE_X: i32 = 10;
const CONSOLE_START_Y: i32 = 50;
const CONSOLE_LINE_HEIGHT: i32 = 14;
const MAX_VISIBLE_LINES: usize = 12;

const TITLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_10X20, RED);
const CONSOLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_6X10, BLACK);
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RED, 1);

/// Messages to display during loading (message, duration in milliseconds).
const INIT_MESSAGES: [(&str, u64); 7] = [
    ("Initializing OBD-II interface...", 800),
    ("Connecting to ECU...", 1200),
    ("Reading vehicle info...", 1000),
    ("Leon Cupra 5F FL | 2.0 TSI 300HP", 600),
    ("DQ381-7F DSG MQB-EVO", 600),
    ("Loading sensors...", 800),
    ("Ready.", 500),
];

const SPINNER_CHARS: [char; 4] = ['|', '/', '-', '\\'];

/// Run the loading screen with console-style init messages.
pub async fn show_loading_screen<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
{
    // Track which lines are visible (circular buffer simulation)
    let mut visible_lines: [&str; MAX_VISIBLE_LINES] = [""; MAX_VISIBLE_LINES];
    let mut line_count: usize = 0;

    for (msg, duration_ms) in &INIT_MESSAGES {
        // Add message to visible lines
        if line_count < MAX_VISIBLE_LINES {
            visible_lines[line_count] = msg;
            line_count += 1;
        } else {
            // Shift lines up
            for i in 0..MAX_VISIBLE_LINES - 1 {
                visible_lines[i] = visible_lines[i + 1];
            }
            visible_lines[MAX_VISIBLE_LINES - 1] = msg;
        }

        let msg_start = Instant::now();
        let msg_duration = Duration::from_millis(*duration_ms);
        let mut spinner_frame = 0u32;

        while msg_start.elapsed() < msg_duration {
            display.clear(WHITE).ok();

            // Update spinner
            spinner_frame = spinner_frame.wrapping_add(1);
            let spinner_idx = (spinner_frame / 8) as usize % SPINNER_CHARS.len();
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

            // ~60 FPS update rate
            Timer::after(Duration::from_millis(16)).await;
        }
    }

    // Final pause before transitioning
    Timer::after(Duration::from_millis(1000)).await;
}
