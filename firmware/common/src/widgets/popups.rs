//! Non-modal popup overlays for status messages.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::colors::{RED, WHITE};
use crate::config::{CENTER_X, CENTER_Y, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::styles::{CENTERED, TITLE_STYLE_WHITE};

const RESET_POPUP_WIDTH: u32 = 180;
const RESET_POPUP_HEIGHT: u32 = 60;
const RESET_POPUP_X: i32 = (SCREEN_WIDTH - RESET_POPUP_WIDTH) as i32 / 2;
const RESET_POPUP_Y: i32 = (SCREEN_HEIGHT - RESET_POPUP_HEIGHT) as i32 / 2;

const FPS_POPUP_WIDTH: u32 = 140;
const FPS_POPUP_HEIGHT: u32 = 50;
const FPS_POPUP_X: i32 = (SCREEN_WIDTH - FPS_POPUP_WIDTH) as i32 / 2;
const FPS_POPUP_Y: i32 = (SCREEN_HEIGHT - FPS_POPUP_HEIGHT) as i32 / 2;

const RESET_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 5);
const RESET_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);
const FPS_TEXT_POS: Point = Point::new(CENTER_X, CENTER_Y + 5);

const WHITE_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(WHITE);
const RED_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

const RESET_BORDER_POS: Point = Point::new(RESET_POPUP_X - 3, RESET_POPUP_Y - 3);
const RESET_BORDER_SIZE: Size = Size::new(RESET_POPUP_WIDTH + 6, RESET_POPUP_HEIGHT + 6);
const RESET_BG_POS: Point = Point::new(RESET_POPUP_X, RESET_POPUP_Y);
const RESET_BG_SIZE: Size = Size::new(RESET_POPUP_WIDTH, RESET_POPUP_HEIGHT);

const FPS_BORDER_POS: Point = Point::new(FPS_POPUP_X - 3, FPS_POPUP_Y - 3);
const FPS_BORDER_SIZE: Size = Size::new(FPS_POPUP_WIDTH + 6, FPS_POPUP_HEIGHT + 6);
const FPS_BG_POS: Point = Point::new(FPS_POPUP_X, FPS_POPUP_Y);
const FPS_BG_SIZE: Size = Size::new(FPS_POPUP_WIDTH, FPS_POPUP_HEIGHT);

pub fn draw_reset_popup<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(RESET_BORDER_POS, RESET_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    Rectangle::new(RESET_BG_POS, RESET_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    Text::with_text_style("MIN/AVG/MAX", RESET_TEXT1_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style("RESET", RESET_TEXT2_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

pub fn draw_fps_toggle_popup<D>(
    display: &mut D,
    fps_enabled: bool,
) where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(FPS_BORDER_POS, FPS_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    Rectangle::new(FPS_BG_POS, FPS_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    let status = if fps_enabled { "FPS ON" } else { "FPS OFF" };
    Text::with_text_style(status, FPS_TEXT_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

pub fn draw_boost_unit_popup<D>(
    display: &mut D,
    show_psi: bool,
) where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(FPS_BORDER_POS, FPS_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    Rectangle::new(FPS_BG_POS, FPS_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    let unit = if show_psi { "BOOST: PSI" } else { "BOOST: BAR" };
    Text::with_text_style(unit, FPS_TEXT_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}
