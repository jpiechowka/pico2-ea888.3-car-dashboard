//! Non-modal popup overlays for status messages.
//!
//! Popups are temporary overlays that display status information:
//! - **Reset popup**: "MIN/AVG/MAX RESET" when statistics are cleared
//! - **FPS popup**: Shows current FPS mode ("FPS OFF", "FPS: INST", "FPS: AVG", "FPS: BOTH")
//! - **Boost unit popup**: Shows current boost unit ("BOOST: BAR" or "BOOST: PSI")

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::config::{CENTER_X, CENTER_Y, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::render::FpsMode;
use crate::ui::{CENTERED, RED, TITLE_STYLE_WHITE, WHITE};

/// Red text style for danger popup on white background.
const TITLE_STYLE_RED: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_10X20, RED);

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

const DANGER_POPUP_WIDTH: u32 = 210;
const DANGER_POPUP_HEIGHT: u32 = 70;
const DANGER_POPUP_X: i32 = (SCREEN_WIDTH - DANGER_POPUP_WIDTH) as i32 / 2;
const DANGER_POPUP_Y: i32 = (SCREEN_HEIGHT - DANGER_POPUP_HEIGHT) as i32 / 2;
const DANGER_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 8);
const DANGER_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);

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

const DANGER_BORDER_POS: Point = Point::new(DANGER_POPUP_X - 3, DANGER_POPUP_Y - 3);
const DANGER_BORDER_SIZE: Size = Size::new(DANGER_POPUP_WIDTH + 6, DANGER_POPUP_HEIGHT + 6);
const DANGER_BG_POS: Point = Point::new(DANGER_POPUP_X, DANGER_POPUP_Y);
const DANGER_BG_SIZE: Size = Size::new(DANGER_POPUP_WIDTH, DANGER_POPUP_HEIGHT);

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

/// Draw FPS mode toggle popup.
///
/// Shows the current FPS mode: "FPS OFF", "FPS: INST", "FPS: AVG", or "FPS: BOTH".
pub fn draw_fps_toggle_popup<D>(
    display: &mut D,
    fps_mode: FpsMode,
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

    Text::with_text_style(fps_mode.label(), FPS_TEXT_POS, TITLE_STYLE_WHITE, CENTERED)
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

/// Draw "DANGER TO MANIFOLD" popup with blinking background.
///
/// Fast & Furious easter egg when EGT >= 1100°C.
/// `blink_on`: alternates between RED bg / WHITE bg for visual alarm effect.
pub fn draw_danger_manifold_popup<D>(
    display: &mut D,
    blink_on: bool,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let (bg_style, text_style) = if blink_on {
        (RED_FILL, TITLE_STYLE_WHITE)
    } else {
        (WHITE_FILL, TITLE_STYLE_RED)
    };

    // Border is always the opposite color of background for contrast
    let border_style = if blink_on { WHITE_FILL } else { RED_FILL };

    Rectangle::new(DANGER_BORDER_POS, DANGER_BORDER_SIZE)
        .into_styled(border_style)
        .draw(display)
        .ok();

    Rectangle::new(DANGER_BG_POS, DANGER_BG_SIZE)
        .into_styled(bg_style)
        .draw(display)
        .ok();

    Text::with_text_style("WARNING", DANGER_TEXT1_POS, text_style, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style("DANGER TO MANIFOLD", DANGER_TEXT2_POS, text_style, CENTERED)
        .draw(display)
        .ok();
}
