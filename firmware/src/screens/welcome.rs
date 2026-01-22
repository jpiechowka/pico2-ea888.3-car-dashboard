//! Welcome screen with Sanic meme and per-character rainbow-animated text.
//!
//! Displays the iconic "Sanic" (derpy Sonic) meme with both text labels
//! animated in a flowing rainbow pattern where each character has its own color.
//!
//! # Visual Layout
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │        Gotta go fast...            │  Rainbow per-character (flows down)
//! │                                    │
//! │            [SANIC]                 │  Pixel art Sanic
//! │                                    │
//! │       fast as fuck boi...          │  Rainbow per-character (continuous)
//! └────────────────────────────────────┘
//! ```
//!
//! # Sanic Pixel Art
//!
//! The Sanic sprite is a simplified ~64x88 pixel representation of the
//! famous "gotta go fast" meme, rendered using filled rectangles.
//! Colors: blue body, peach/tan face, red shoes, white eyes.
//!
//! # Rainbow Animation
//!
//! Both text labels use per-character rainbow coloring that flows continuously:
//! - 12 colors in the rainbow (extended palette for smoother gradients)
//! - Each character offset by 1 color index for wave effect
//! - Top and bottom labels form one continuous rainbow wave
//! - Animation advances 1 color step every 3 frames (~20 color changes/sec)
//!
//! The rainbow flows from left-to-right on the top label and continues
//! seamlessly into the bottom label, creating a "raining down" visual effect.
//!
//! # Optimizations Applied
//!
//! ## Const Rainbow Color Array
//! Extended 12-color rainbow stored as const array for smooth gradients.
//! Uses simple modulo arithmetic for color indexing (no floating-point).

use std::thread;
use std::time::{Duration, Instant};

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::{SimulatorDisplay, SimulatorEvent, Window};

use crate::colors::BLACK;

// =============================================================================
// Welcome Screen Layout Constants
// =============================================================================

/// Top text string for per-character rendering
const TOP_TEXT: &str = "Gotta go fast...";

/// Bottom text string for per-character rendering
const BOTTOM_TEXT: &str = "fast as fuck boi...";

/// Y position for top text baseline
const TOP_TEXT_Y: i32 = 35;

/// Y position for bottom text baseline
const BOTTOM_TEXT_Y: i32 = 200;

/// Character width for `FONT_10X20` (10 pixels per character)
const CHAR_WIDTH: i32 = 10;

/// Screen center X coordinate (320 / 2)
const SCREEN_CENTER_X: i32 = 160;

/// Sanic sprite top-left position (centered on screen)
const SANIC_POS: Point = Point::new(128, 55);

/// How long the welcome screen displays (seconds)
const WELCOME_DURATION_SECS: u64 = 5;

// =============================================================================
// Sanic Colors (Rgb565)
// =============================================================================

/// Sanic's iconic blue color
const SANIC_BLUE: Rgb565 = Rgb565::new(0, 16, 31);

/// Sanic's face/skin color (peach/tan)
const SANIC_SKIN: Rgb565 = Rgb565::new(31, 24, 16);

/// Sanic's red shoes
const SANIC_RED: Rgb565 = Rgb565::new(31, 0, 0);

/// Sanic's eyes (white)
const SANIC_WHITE: Rgb565 = Rgb565::WHITE;

/// Sanic's eye pupils (black)
const SANIC_BLACK: Rgb565 = Rgb565::BLACK;

// =============================================================================
// Rainbow Color Animation
// =============================================================================

/// Extended 12-color rainbow palette for smoother per-character gradients.
/// Using a const array eliminates per-frame floating-point calculations.
///
/// Colors transition through the full spectrum:
/// Red → Orange → Yellow → Lime → Green → Cyan → Sky → Blue → Purple → Magenta → Pink → Rose
const RAINBOW_COLORS: [Rgb565; 12] = [
    Rgb565::new(31, 0, 0),  // 0: Red
    Rgb565::new(31, 24, 0), // 1: Orange
    Rgb565::new(31, 48, 0), // 2: Yellow-Orange
    Rgb565::new(31, 63, 0), // 3: Yellow
    Rgb565::new(16, 63, 0), // 4: Lime
    Rgb565::new(0, 63, 0),  // 5: Green
    Rgb565::new(0, 63, 16), // 6: Cyan-Green
    Rgb565::new(0, 48, 31), // 7: Cyan
    Rgb565::new(0, 24, 31), // 8: Sky Blue
    Rgb565::new(16, 0, 31), // 9: Blue-Purple
    Rgb565::new(31, 0, 31), // 10: Magenta
    Rgb565::new(31, 0, 16), // 11: Pink-Red
];

/// Number of colors in the rainbow palette.
const RAINBOW_LEN: usize = 12;

/// Frames between color animation steps.
/// At ~60 FPS, 3 frames = ~20 color changes per second for smooth flow.
const FRAMES_PER_STEP: u32 = 3;

/// Get rainbow color for a specific character position and frame.
///
/// Each character is offset by 1 color index from its neighbor, creating
/// a flowing wave effect. The animation advances based on frame count.
///
/// # Parameters
/// - `char_index`: Position of character in the combined text sequence
/// - `frame`: Current animation frame (advances color base)
///
/// # Returns
/// RGB565 color from the rainbow palette.
#[inline]
const fn rainbow_color_for_char(
    char_index: usize,
    frame: u32,
) -> Rgb565 {
    // Animation base advances every FRAMES_PER_STEP frames
    let anim_offset = (frame / FRAMES_PER_STEP) as usize;
    // Each character offset by 1, animation flows in reverse for "raining" effect
    let color_index = (RAINBOW_LEN + anim_offset - (char_index % RAINBOW_LEN)) % RAINBOW_LEN;
    RAINBOW_COLORS[color_index]
}

// =============================================================================
// Sanic Pixel Art Drawing
// =============================================================================

/// Draw a filled rectangle (helper for pixel art).
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

/// Draw the iconic Sanic (derpy Sonic) pixel art.
///
/// This is a simplified ~64x88 representation of the meme.
/// The sprite is positioned at (`base_x`, `base_y`).
fn draw_sanic(
    display: &mut SimulatorDisplay<Rgb565>,
    x: i32,
    y: i32,
) {
    // Head spikes (blue) - the iconic messy spikes
    draw_rect(display, x + 40, y, 16, 8, SANIC_BLUE);
    draw_rect(display, x + 48, y + 8, 16, 8, SANIC_BLUE);
    draw_rect(display, x + 56, y + 16, 12, 8, SANIC_BLUE);

    // Head main body (blue)
    draw_rect(display, x + 16, y + 8, 32, 40, SANIC_BLUE);
    draw_rect(display, x + 8, y + 16, 12, 24, SANIC_BLUE);

    // Face (skin color) - the derpy face
    draw_rect(display, x + 4, y + 20, 20, 24, SANIC_SKIN);

    // Eyes (white circles, simplified as rectangles)
    draw_rect(display, x + 4, y + 20, 10, 12, SANIC_WHITE);
    draw_rect(display, x + 12, y + 24, 8, 10, SANIC_WHITE);

    // Pupils (black dots) - the derpy crossed eyes
    draw_rect(display, x + 8, y + 26, 4, 4, SANIC_BLACK);
    draw_rect(display, x + 14, y + 28, 4, 4, SANIC_BLACK);

    // Nose (skin, small bump)
    draw_rect(display, x, y + 32, 6, 6, SANIC_SKIN);

    // Mouth (black line)
    draw_rect(display, x + 4, y + 40, 12, 2, SANIC_BLACK);

    // Body (blue)
    draw_rect(display, x + 20, y + 48, 24, 20, SANIC_BLUE);

    // Belly (skin)
    draw_rect(display, x + 24, y + 52, 12, 12, SANIC_SKIN);

    // Arms (blue)
    draw_rect(display, x + 12, y + 52, 8, 12, SANIC_BLUE);
    draw_rect(display, x + 44, y + 52, 8, 12, SANIC_BLUE);

    // Hands (white gloves)
    draw_rect(display, x + 8, y + 60, 8, 8, SANIC_WHITE);
    draw_rect(display, x + 48, y + 60, 8, 8, SANIC_WHITE);

    // Legs (blue)
    draw_rect(display, x + 20, y + 68, 10, 12, SANIC_BLUE);
    draw_rect(display, x + 34, y + 68, 10, 12, SANIC_BLUE);

    // Shoes (red)
    draw_rect(display, x + 16, y + 80, 16, 8, SANIC_RED);
    draw_rect(display, x + 32, y + 80, 16, 8, SANIC_RED);

    // Shoe stripes (white)
    draw_rect(display, x + 20, y + 82, 8, 2, SANIC_WHITE);
    draw_rect(display, x + 36, y + 82, 8, 2, SANIC_WHITE);
}

// =============================================================================
// Per-Character Rainbow Text Drawing
// =============================================================================

/// Draw a string with per-character rainbow coloring.
///
/// Each character is drawn individually with its own color from the rainbow
/// palette, creating a flowing wave effect when animated across frames.
///
/// # Parameters
/// - `text`: The string to render
/// - `center_x`: X coordinate for text center
/// - `y`: Y coordinate for text baseline
/// - `char_offset`: Starting character index for rainbow continuity
/// - `frame`: Current animation frame
///
/// # Returns
/// The next character index (for chaining multiple text segments)
fn draw_rainbow_text(
    display: &mut SimulatorDisplay<Rgb565>,
    text: &str,
    center_x: i32,
    y: i32,
    char_offset: usize,
    frame: u32,
) -> usize {
    // Use chars().count() for proper UTF-8 character counting (not byte count)
    let char_count = text.chars().count() as i32;
    // Calculate starting X position (centered text)
    let start_x = center_x - (char_count * CHAR_WIDTH) / 2;

    // Draw each character with its own rainbow color
    for (i, ch) in text.chars().enumerate() {
        let color = rainbow_color_for_char(char_offset + i, frame);
        let style = MonoTextStyle::new(&FONT_10X20, color);
        let x = start_x + (i as i32 * CHAR_WIDTH);

        // Create single-character string (stack allocated via array)
        let mut char_buf = [0u8; 4]; // UTF-8 char max 4 bytes
        let char_str = ch.encode_utf8(&mut char_buf);

        Text::new(char_str, Point::new(x, y), style).draw(display).ok();
    }

    // Return next character index for continuity (character count, not bytes)
    char_offset + text.chars().count()
}

// =============================================================================
// Welcome Screen Function
// =============================================================================

/// Run the welcome screen with Sanic and per-character rainbow animation.
///
/// Both text labels ("Gotta go fast..." and "fast as fuck boi...") are animated
/// with a flowing rainbow effect where each character has its own color. The
/// animation flows continuously from the top label into the bottom label.
///
/// Displays for `WELCOME_DURATION_SECS` (5 seconds) then returns.
/// Returns `false` if window is closed, `true` when sequence completes.
pub fn run_welcome_screen(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> bool {
    let welcome_start = Instant::now();
    let welcome_duration = Duration::from_secs(WELCOME_DURATION_SECS);
    let mut frame: u32 = 0;

    while welcome_start.elapsed() < welcome_duration {
        // Handle window close event
        for ev in window.events() {
            if matches!(ev, SimulatorEvent::Quit) {
                return false;
            }
        }

        // Black background
        display.clear(BLACK).ok();

        // Top text: "Gotta go fast..." with per-character rainbow
        // Returns the next char index for continuous rainbow flow
        let next_char_idx = draw_rainbow_text(display, TOP_TEXT, SCREEN_CENTER_X, TOP_TEXT_Y, 0, frame);

        // Draw Sanic pixel art (centered)
        draw_sanic(display, SANIC_POS.x, SANIC_POS.y);

        // Bottom text: "fast as fuck boi..." continues the rainbow from top text
        draw_rainbow_text(
            display,
            BOTTOM_TEXT,
            SCREEN_CENTER_X,
            BOTTOM_TEXT_Y,
            next_char_idx,
            frame,
        );

        window.update(display);
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
        frame = frame.wrapping_add(1);
    }
    true
}
