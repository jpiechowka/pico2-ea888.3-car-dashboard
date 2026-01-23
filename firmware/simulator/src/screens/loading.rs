//! Loading screen with console-style initialization messages.

use core::fmt::Write;
use std::thread;
use std::time::{Duration, Instant};

use dashboard_common::colors::{BLACK, RED, WHITE};
use dashboard_common::styles::{CENTERED, LEFT_ALIGNED};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::{SimulatorDisplay, SimulatorEvent, Window};
use heapless::String;

const TITLE_POS: Point = Point::new(160, 25);
const LINE_START: Point = Point::new(10, 35);
const LINE_END: Point = Point::new(310, 35);
const CONSOLE_X: i32 = 10;
const CONSOLE_START_Y: i32 = 50;
const CONSOLE_LINE_HEIGHT: i32 = 14;

const TITLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_10X20, RED);
const CONSOLE_STYLE: MonoTextStyle<'static, Rgb565> =
    MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_6X10, BLACK);
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RED, 1);

pub fn run_loading_screen(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> bool {
    let init_messages = [
        ("Initializing OBD-II interface...", 800),
        ("Connecting to ECU...", 1200),
        ("Reading vehicle info...", 1000),
        ("Leon Cupra 5F FL | 2.0 TSI 300HP", 600),
        ("DQ381-7F DSG MQB-EVO", 600),
        ("Loading sensors...", 800),
        ("Ready.", 500),
    ];

    let spinner_chars = ['|', '/', '-', '\\'];
    let mut spinner_idx = 0;
    let mut spinner_frame = 0u32;

    let mut console_lines: Vec<&str> = Vec::new();

    for (msg, duration_ms) in &init_messages {
        console_lines.push(msg);
        if console_lines.len() > 12 {
            console_lines.remove(0);
        }

        let msg_start = Instant::now();
        let msg_duration = Duration::from_millis(*duration_ms as u64);

        while msg_start.elapsed() < msg_duration {
            for ev in window.events() {
                if matches!(ev, SimulatorEvent::Quit) {
                    return false;
                }
            }

            display.clear(WHITE).ok();

            spinner_frame = spinner_frame.wrapping_add(1);
            if spinner_frame.is_multiple_of(8) {
                spinner_idx = (spinner_idx + 1) % spinner_chars.len();
            }
            let left_spinner = spinner_chars[spinner_idx];
            let right_spinner = spinner_chars[(spinner_idx + 2) % spinner_chars.len()];

            let mut loading_text: String<32> = String::new();
            let _ = write!(loading_text, "{left_spinner}  Loading shit  {right_spinner}");
            Text::with_text_style(&loading_text, TITLE_POS, TITLE_STYLE, CENTERED)
                .draw(display)
                .ok();

            Line::new(LINE_START, LINE_END)
                .into_styled(DIVIDER_STYLE)
                .draw(display)
                .ok();

            for (i, line) in console_lines.iter().enumerate() {
                let y_pos = CONSOLE_START_Y + (i as i32 * CONSOLE_LINE_HEIGHT);
                let prefix = if i == console_lines.len() - 1 { "> " } else { "  " };
                let mut full_line: String<64> = String::new();
                let _ = write!(full_line, "{prefix}{line}");
                Text::with_text_style(&full_line, Point::new(CONSOLE_X, y_pos), CONSOLE_STYLE, LEFT_ALIGNED)
                    .draw(display)
                    .ok();
            }

            window.update(display);
            thread::sleep(Duration::from_millis(16));
        }
    }

    thread::sleep(Duration::from_millis(1000));
    true
}
