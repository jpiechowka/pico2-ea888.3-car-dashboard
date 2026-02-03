//! Header bar and divider line rendering.

use core::fmt::Write;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

use crate::colors::{GRAY, RED};
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::render::FpsMode;
use crate::styles::{CENTERED, LABEL_STYLE_WHITE, RIGHT_ALIGNED, TITLE_STYLE_WHITE};

const HEADER_TITLE_POS: Point = Point::new(160, 19);
const HEADER_FPS_POS: Point = Point::new((SCREEN_WIDTH - 5) as i32, 17);
const HEADER_RECT_POS: Point = Point::new(0, 0);
const HEADER_RECT_SIZE: Size = Size::new(SCREEN_WIDTH, 26);

const DIV_V1_START: Point = Point::new(COL_WIDTH as i32, HEADER_HEIGHT as i32);
const DIV_V1_END: Point = Point::new(COL_WIDTH as i32, (SCREEN_HEIGHT - 1) as i32);
const DIV_V2_START: Point = Point::new((COL_WIDTH * 2) as i32, HEADER_HEIGHT as i32);
const DIV_V2_END: Point = Point::new((COL_WIDTH * 2) as i32, (SCREEN_HEIGHT - 1) as i32);
const DIV_V3_START: Point = Point::new((COL_WIDTH * 3) as i32, HEADER_HEIGHT as i32);
const DIV_V3_END: Point = Point::new((COL_WIDTH * 3) as i32, (SCREEN_HEIGHT - 1) as i32);
const DIV_H_START: Point = Point::new(0, (HEADER_HEIGHT + ROW_HEIGHT) as i32);
const DIV_H_END: Point = Point::new((SCREEN_WIDTH - 1) as i32, (HEADER_HEIGHT + ROW_HEIGHT) as i32);

const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(GRAY, 1);
const HEADER_FILL_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

/// Draw the header bar with optional FPS display.
///
/// # Arguments
/// * `fps_mode` - The FPS display mode (Off, Instant, or Average)
/// * `fps` - The FPS value to display (instant or average, depending on mode)
pub fn draw_header<D>(
    display: &mut D,
    fps_mode: FpsMode,
    fps: f32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(HEADER_RECT_POS, HEADER_RECT_SIZE)
        .into_styled(HEADER_FILL_STYLE)
        .draw(display)
        .ok();

    Text::with_text_style("OBD Sim", HEADER_TITLE_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();

    if fps_mode.is_visible() {
        let mut fps_str: String<16> = String::new();
        let _ = write!(fps_str, "{:.0}{}", fps, fps_mode.suffix());
        Text::with_text_style(&fps_str, HEADER_FPS_POS, LABEL_STYLE_WHITE, RIGHT_ALIGNED)
            .draw(display)
            .ok();
    }
}

pub fn draw_dividers<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
{
    Line::new(DIV_V1_START, DIV_V1_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    Line::new(DIV_V2_START, DIV_V2_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    Line::new(DIV_V3_START, DIV_V3_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    Line::new(DIV_H_START, DIV_H_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();
}
