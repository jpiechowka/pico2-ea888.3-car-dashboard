//! Welcome screen with Sanic meme and per-character rainbow-animated text.

use std::thread;
use std::time::{Duration, Instant};

use dashboard_common::colors::BLACK;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::{SimulatorDisplay, SimulatorEvent, Window};

const TOP_TEXT: &str = "Gotta go fast...";
const BOTTOM_TEXT: &str = "fast as fuck boi...";
const TOP_TEXT_Y: i32 = 35;
const BOTTOM_TEXT_Y: i32 = 200;
const CHAR_WIDTH: i32 = 10;
const SCREEN_CENTER_X: i32 = 160;
const SANIC_POS: Point = Point::new(128, 55);
const WELCOME_DURATION_SECS: u64 = 5;

const SANIC_BLUE: Rgb565 = Rgb565::new(0, 16, 31);
const SANIC_SKIN: Rgb565 = Rgb565::new(31, 24, 16);
const SANIC_RED: Rgb565 = Rgb565::new(31, 0, 0);
const SANIC_WHITE: Rgb565 = Rgb565::WHITE;
const SANIC_BLACK: Rgb565 = Rgb565::BLACK;

const RAINBOW_COLORS: [Rgb565; 12] = [
    Rgb565::new(31, 0, 0),
    Rgb565::new(31, 24, 0),
    Rgb565::new(31, 48, 0),
    Rgb565::new(31, 63, 0),
    Rgb565::new(16, 63, 0),
    Rgb565::new(0, 63, 0),
    Rgb565::new(0, 63, 16),
    Rgb565::new(0, 48, 31),
    Rgb565::new(0, 24, 31),
    Rgb565::new(16, 0, 31),
    Rgb565::new(31, 0, 31),
    Rgb565::new(31, 0, 16),
];

const RAINBOW_LEN: usize = 12;
const FRAMES_PER_STEP: u32 = 3;

#[inline]
const fn rainbow_color_for_char(
    char_index: usize,
    frame: u32,
) -> Rgb565 {
    let anim_offset = (frame / FRAMES_PER_STEP) as usize;
    let color_index = (RAINBOW_LEN + anim_offset - (char_index % RAINBOW_LEN)) % RAINBOW_LEN;
    RAINBOW_COLORS[color_index]
}

fn draw_rect(
    display: &mut SimulatorDisplay<Rgb565>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: Rgb565,
) {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display)
        .ok();
}

fn draw_sanic(
    display: &mut SimulatorDisplay<Rgb565>,
    x: i32,
    y: i32,
) {
    draw_rect(display, x + 40, y, 16, 8, SANIC_BLUE);
    draw_rect(display, x + 48, y + 8, 16, 8, SANIC_BLUE);
    draw_rect(display, x + 56, y + 16, 12, 8, SANIC_BLUE);
    draw_rect(display, x + 16, y + 8, 32, 40, SANIC_BLUE);
    draw_rect(display, x + 8, y + 16, 12, 24, SANIC_BLUE);
    draw_rect(display, x + 4, y + 20, 20, 24, SANIC_SKIN);
    draw_rect(display, x + 4, y + 20, 10, 12, SANIC_WHITE);
    draw_rect(display, x + 12, y + 24, 8, 10, SANIC_WHITE);
    draw_rect(display, x + 8, y + 26, 4, 4, SANIC_BLACK);
    draw_rect(display, x + 14, y + 28, 4, 4, SANIC_BLACK);
    draw_rect(display, x, y + 32, 6, 6, SANIC_SKIN);
    draw_rect(display, x + 4, y + 40, 12, 2, SANIC_BLACK);
    draw_rect(display, x + 20, y + 48, 24, 20, SANIC_BLUE);
    draw_rect(display, x + 24, y + 52, 12, 12, SANIC_SKIN);
    draw_rect(display, x + 12, y + 52, 8, 12, SANIC_BLUE);
    draw_rect(display, x + 44, y + 52, 8, 12, SANIC_BLUE);
    draw_rect(display, x + 8, y + 60, 8, 8, SANIC_WHITE);
    draw_rect(display, x + 48, y + 60, 8, 8, SANIC_WHITE);
    draw_rect(display, x + 20, y + 68, 10, 12, SANIC_BLUE);
    draw_rect(display, x + 34, y + 68, 10, 12, SANIC_BLUE);
    draw_rect(display, x + 16, y + 80, 16, 8, SANIC_RED);
    draw_rect(display, x + 32, y + 80, 16, 8, SANIC_RED);
    draw_rect(display, x + 20, y + 82, 8, 2, SANIC_WHITE);
    draw_rect(display, x + 36, y + 82, 8, 2, SANIC_WHITE);
}

fn draw_rainbow_text(
    display: &mut SimulatorDisplay<Rgb565>,
    text: &str,
    center_x: i32,
    y: i32,
    char_offset: usize,
    frame: u32,
) -> usize {
    let char_count = text.chars().count() as i32;
    let start_x = center_x - (char_count * CHAR_WIDTH) / 2;

    for (i, ch) in text.chars().enumerate() {
        let color = rainbow_color_for_char(char_offset + i, frame);
        let style = MonoTextStyle::new(&FONT_10X20, color);
        let x = start_x + (i as i32 * CHAR_WIDTH);

        let mut char_buf = [0u8; 4];
        let char_str = ch.encode_utf8(&mut char_buf);

        Text::new(char_str, Point::new(x, y), style).draw(display).ok();
    }

    char_offset + text.chars().count()
}

pub fn run_welcome_screen(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> bool {
    let welcome_start = Instant::now();
    let welcome_duration = Duration::from_secs(WELCOME_DURATION_SECS);
    let mut frame: u32 = 0;

    while welcome_start.elapsed() < welcome_duration {
        for ev in window.events() {
            if matches!(ev, SimulatorEvent::Quit) {
                return false;
            }
        }

        display.clear(BLACK).ok();

        let next_char_idx = draw_rainbow_text(display, TOP_TEXT, SCREEN_CENTER_X, TOP_TEXT_Y, 0, frame);

        draw_sanic(display, SANIC_POS.x, SANIC_POS.y);

        draw_rainbow_text(
            display,
            BOTTOM_TEXT,
            SCREEN_CENTER_X,
            BOTTOM_TEXT_Y,
            next_char_idx,
            frame,
        );

        window.update(display);
        thread::sleep(Duration::from_millis(16));
        frame = frame.wrapping_add(1);
    }
    true
}
