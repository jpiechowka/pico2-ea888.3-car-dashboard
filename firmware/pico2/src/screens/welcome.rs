//! Welcome screen with AEZAKMI (GTA cheat code) logo and blinking golden stars.
//!
//! Displays golden "AEZAKMI" text with black shadow, a red/black stripe,
//! and 5 golden stars that blink in sequence (GTA San Andreas style).
//!
//! # Animation
//!
//! The star animation is time-based (7 seconds total):
//! - 0-4000ms: Stars light up sequentially (one every 800ms)
//! - 4000-7000ms: All 5 stars blink on/off slowly (toggle every 250ms)
//!
//! # Usage
//!
//! The caller should call [`draw_welcome_frame`] in a loop, passing the elapsed
//! time in milliseconds since the welcome screen started. This ensures consistent
//! animation speed regardless of actual frame rate.
//!
//! # Example
//!
//! ```ignore
//! let start = Instant::now();
//! loop {
//!     let elapsed_ms = start.elapsed().as_millis() as u32;
//!     if elapsed_ms >= 7000 { break; }
//!     draw_welcome_frame(&mut display, elapsed_ms);
//!     display.flush().await;
//! }
//! ```

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, Triangle};

use crate::colors::WHITE;

const SCREEN_CENTER_X: i32 = 160;

// Vertically centered layout (screen is 240px tall)
const TEXT_Y: i32 = 64;
const STRIPE_Y: i32 = 128;
const STARS_Y: i32 = 162;

const COLOR_BLACK: Rgb565 = Rgb565::BLACK;
const COLOR_DARK_GRAY: Rgb565 = Rgb565::new(4, 8, 4);
const COLOR_RED: Rgb565 = Rgb565::new(31, 0, 0);
const COLOR_GOLD: Rgb565 = Rgb565::new(31, 50, 0);
const COLOR_DARK_GOLD: Rgb565 = Rgb565::new(20, 32, 0);
const COLOR_DIM_GOLD: Rgb565 = Rgb565::new(12, 20, 0);

// Letter dimensions at 1.5x scale (base: 22x32, scaled: 33x48)
const LETTER_WIDTH: i32 = 33;
const LETTER_SPACING: i32 = 3;

// Letter pixel definitions (dx, dy, width, height) at base scale
const LETTER_A: &[(i32, i32, u32, u32)] = &[(0, 8, 5, 24), (15, 8, 5, 24), (5, 0, 10, 8), (5, 14, 10, 5)];

const LETTER_E: &[(i32, i32, u32, u32)] = &[(0, 0, 5, 32), (5, 0, 15, 5), (5, 13, 12, 5), (5, 27, 15, 5)];

const LETTER_Z: &[(i32, i32, u32, u32)] = &[
    (0, 0, 20, 5),
    (14, 5, 5, 5),
    (11, 9, 5, 5),
    (8, 13, 5, 5),
    (5, 17, 5, 5),
    (2, 21, 5, 5),
    (0, 27, 20, 5),
];

const LETTER_K: &[(i32, i32, u32, u32)] = &[
    (0, 0, 5, 32),
    (5, 12, 5, 5),
    (8, 8, 5, 5),
    (11, 4, 5, 5),
    (14, 0, 6, 5),
    (5, 15, 5, 5),
    (8, 19, 5, 5),
    (11, 23, 5, 5),
    (14, 27, 6, 5),
];

const LETTER_M: &[(i32, i32, u32, u32)] = &[(0, 0, 5, 32), (15, 0, 5, 32), (5, 4, 4, 6), (11, 4, 4, 6), (8, 8, 4, 6)];

const LETTER_I: &[(i32, i32, u32, u32)] = &[(2, 0, 16, 5), (7, 5, 6, 22), (2, 27, 16, 5)];

fn draw_rect<D>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display)
        .ok();
}

fn draw_letter<D>(
    display: &mut D,
    letter: &[(i32, i32, u32, u32)],
    base_x: i32,
    base_y: i32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    // Apply 1.5x scale (3/2) to all coordinates and sizes
    // Black shadow (offset scaled from 2 to 3)
    for &(dx, dy, w, h) in letter {
        let sx = base_x + dx * 3 / 2 + 3;
        let sy = base_y + dy * 3 / 2 + 3;
        draw_rect(display, sx, sy, w * 3 / 2, h * 3 / 2, COLOR_BLACK);
    }
    // Golden text
    for &(dx, dy, w, h) in letter {
        let sx = base_x + dx * 3 / 2;
        let sy = base_y + dy * 3 / 2;
        draw_rect(display, sx, sy, w * 3 / 2, h * 3 / 2, COLOR_GOLD);
    }
}

fn draw_aezakmi<D>(
    display: &mut D,
    y: i32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let letters: [&[(i32, i32, u32, u32)]; 7] = [LETTER_A, LETTER_E, LETTER_Z, LETTER_A, LETTER_K, LETTER_M, LETTER_I];

    let total_width = (LETTER_WIDTH + LETTER_SPACING) * 7 - LETTER_SPACING;
    let start_x = SCREEN_CENTER_X - total_width / 2;

    for (i, letter) in letters.iter().enumerate() {
        let x = start_x + i as i32 * (LETTER_WIDTH + LETTER_SPACING);
        draw_letter(display, letter, x, y);
    }
}

fn draw_stripe<D>(
    display: &mut D,
    y: i32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    // Scaled stripe (1.5x: 180->270, heights scaled proportionally)
    let stripe_width: u32 = 260;
    let start_x = SCREEN_CENTER_X - (stripe_width as i32) / 2;

    draw_rect(display, start_x - 3, y - 3, stripe_width + 6, 27, COLOR_BLACK);
    draw_rect(display, start_x, y, stripe_width, 9, COLOR_RED);
    draw_rect(display, start_x, y + 12, stripe_width, 9, COLOR_BLACK);
}

fn draw_star<D>(
    display: &mut D,
    center_x: i32,
    center_y: i32,
    size: i32,
    color: Rgb565,
    outline_color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let outer_radius = size;
    let inner_radius = size * 38 / 100;

    const COS_OUTER: [i32; 5] = [0, -95, -59, 59, 95];
    const SIN_OUTER: [i32; 5] = [100, 31, -81, -81, 31];
    const COS_INNER: [i32; 5] = [-59, -95, 0, 95, 59];
    const SIN_INNER: [i32; 5] = [81, -31, -100, -31, 81];

    // Draw outline triangles
    for i in 0..5 {
        let next = (i + 1) % 5;

        let ox = center_x + (COS_OUTER[i] * outer_radius / 100);
        let oy = center_y - (SIN_OUTER[i] * outer_radius / 100);
        let ix = center_x + (COS_INNER[i] * inner_radius / 100);
        let iy = center_y - (SIN_INNER[i] * inner_radius / 100);
        let nx = center_x + (COS_OUTER[next] * outer_radius / 100);
        let ny = center_y - (SIN_OUTER[next] * outer_radius / 100);

        Triangle::new(Point::new(center_x, center_y), Point::new(ox, oy), Point::new(ix, iy))
            .into_styled(PrimitiveStyle::with_fill(outline_color))
            .draw(display)
            .ok();

        Triangle::new(Point::new(center_x, center_y), Point::new(ix, iy), Point::new(nx, ny))
            .into_styled(PrimitiveStyle::with_fill(outline_color))
            .draw(display)
            .ok();
    }

    // Draw fill triangles (slightly smaller)
    let fill_outer = outer_radius * 85 / 100;
    let fill_inner = inner_radius * 85 / 100;

    for i in 0..5 {
        let next = (i + 1) % 5;

        let ox = center_x + (COS_OUTER[i] * fill_outer / 100);
        let oy = center_y - (SIN_OUTER[i] * fill_outer / 100);
        let ix = center_x + (COS_INNER[i] * fill_inner / 100);
        let iy = center_y - (SIN_INNER[i] * fill_inner / 100);
        let nx = center_x + (COS_OUTER[next] * fill_outer / 100);
        let ny = center_y - (SIN_OUTER[next] * fill_outer / 100);

        Triangle::new(Point::new(center_x, center_y), Point::new(ox, oy), Point::new(ix, iy))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)
            .ok();

        Triangle::new(Point::new(center_x, center_y), Point::new(ix, iy), Point::new(nx, ny))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)
            .ok();
    }
}

fn draw_stars<D>(
    display: &mut D,
    y: i32,
    elapsed_ms: u32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    // Scaled stars (1.5x: size 18->27, spacing 40->56)
    let star_size = 27;
    let star_spacing = 56;
    let total_width = star_spacing * 4;
    let start_x = SCREEN_CENTER_X - total_width / 2;

    // Time-based animation (7 seconds total):
    // - 0-4000ms: Stars light up sequentially (one every 800ms)
    // - 4000-7000ms: All 5 stars blink on/off slowly
    let cycle_ms = elapsed_ms % 7000;
    let lit_count = if cycle_ms < 4000 {
        // Star filling phase: one star every 800ms
        (cycle_ms / 800 + 1).min(5) as usize
    } else if ((cycle_ms - 4000) / 250).is_multiple_of(2) {
        // Blinking phase: toggle every 250ms (~2 blinks per second)
        5
    } else {
        0
    };

    for i in 0i32..5 {
        let x = start_x + i * star_spacing;
        let is_lit = (i as usize) < lit_count;

        if is_lit {
            draw_star(display, x, y, star_size, COLOR_GOLD, COLOR_DARK_GOLD);
        } else {
            draw_star(display, x, y, star_size, COLOR_DIM_GOLD, COLOR_DARK_GRAY);
        }
    }
}

/// Draw a single frame of the welcome screen.
///
/// # Arguments
/// * `elapsed_ms` - Milliseconds since the welcome screen started. Used for time-based star animation (0-4000ms: stars
///   fill, 4000-7000ms: blink).
///
/// This is a non-async function that renders one frame. Call this in a loop
/// with appropriate timing and flush the display after each call.
pub fn draw_welcome_frame<D>(
    display: &mut D,
    elapsed_ms: u32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(WHITE).ok();

    draw_aezakmi(display, TEXT_Y);
    draw_stripe(display, STRIPE_Y);
    draw_stars(display, STARS_Y, elapsed_ms);
}
