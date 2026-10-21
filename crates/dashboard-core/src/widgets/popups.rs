//! Centered native-panel overlays. Drawing never starts feedback timers or acknowledges presentation.
//! Callers supply native 320x240/240x320 targets and bounded labels; target errors are discarded.
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::config::{CENTER_X, CENTER_Y, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::render::FpsMode;
use crate::ui::{CENTERED, RED, TITLE_STYLE_WHITE, WHITE};

/// Transparent red 10x20 warning text for the white-background blink phase.
const TITLE_STYLE_RED: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_10X20, RED);

/// Reset panel interior width in pixels.
const RESET_POPUP_WIDTH: u32 = 180;
/// Reset panel interior height in pixels.
const RESET_POPUP_HEIGHT: u32 = 60;
/// Landscape reset interior left coordinate; portrait applies a center offset.
const RESET_POPUP_X: i32 = (SCREEN_WIDTH - RESET_POPUP_WIDTH) as i32 / 2;
/// Landscape reset interior top coordinate; portrait applies a center offset.
const RESET_POPUP_Y: i32 = (SCREEN_HEIGHT - RESET_POPUP_HEIGHT) as i32 / 2;

/// Compact feedback panel interior width in pixels.
const FPS_POPUP_WIDTH: u32 = 140;
/// Compact feedback panel interior height in pixels.
const FPS_POPUP_HEIGHT: u32 = 50;
/// Landscape compact interior left coordinate.
const FPS_POPUP_X: i32 = (SCREEN_WIDTH - FPS_POPUP_WIDTH) as i32 / 2;
/// Landscape compact interior top coordinate.
const FPS_POPUP_Y: i32 = (SCREEN_HEIGHT - FPS_POPUP_HEIGHT) as i32 / 2;

/// Landscape baseline/center for the reset statistics label.
const RESET_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 5);
/// Landscape baseline/center for the RESET confirmation line.
const RESET_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);
/// Landscape baseline/center for a single compact feedback label.
const FPS_TEXT_POS: Point = Point::new(CENTER_X, CENTER_Y + 5);

/// Legacy critical-warning interior width in pixels.
const CRITICAL_POPUP_WIDTH: u32 = 210;
/// Legacy critical-warning interior height in pixels.
const CRITICAL_POPUP_HEIGHT: u32 = 70;
/// Landscape critical-warning interior left coordinate.
const CRITICAL_POPUP_X: i32 = (SCREEN_WIDTH - CRITICAL_POPUP_WIDTH) as i32 / 2;
/// Landscape critical-warning interior top coordinate.
const CRITICAL_POPUP_Y: i32 = (SCREEN_HEIGHT - CRITICAL_POPUP_HEIGHT) as i32 / 2;
/// Landscape WARNING title baseline/center.
const CRITICAL_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 8);
/// Landscape sensor-label baseline/center.
const CRITICAL_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);

/// Opaque white panel/border fill.
const WHITE_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(WHITE);
/// Opaque red panel/border fill.
const RED_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

/// Landscape reset border origin, three pixels beyond its interior.
const RESET_BORDER_POS: Point = Point::new(RESET_POPUP_X - 3, RESET_POPUP_Y - 3);
/// Reset border's complete pixel dimensions, including both three-pixel margins.
const RESET_BORDER_SIZE: Size = Size::new(RESET_POPUP_WIDTH + 6, RESET_POPUP_HEIGHT + 6);
/// Landscape reset interior origin.
const RESET_BG_POS: Point = Point::new(RESET_POPUP_X, RESET_POPUP_Y);
/// Reset interior pixel dimensions.
const RESET_BG_SIZE: Size = Size::new(RESET_POPUP_WIDTH, RESET_POPUP_HEIGHT);

/// Landscape compact border origin, three pixels beyond its interior.
const FPS_BORDER_POS: Point = Point::new(FPS_POPUP_X - 3, FPS_POPUP_Y - 3);
/// Compact border pixel dimensions including both margins.
const FPS_BORDER_SIZE: Size = Size::new(FPS_POPUP_WIDTH + 6, FPS_POPUP_HEIGHT + 6);
/// Landscape compact interior origin.
const FPS_BG_POS: Point = Point::new(FPS_POPUP_X, FPS_POPUP_Y);
/// Compact interior pixel dimensions.
const FPS_BG_SIZE: Size = Size::new(FPS_POPUP_WIDTH, FPS_POPUP_HEIGHT);

/// Landscape critical-warning border origin, three pixels beyond its interior.
const CRITICAL_BORDER_POS: Point = Point::new(CRITICAL_POPUP_X - 3, CRITICAL_POPUP_Y - 3);
/// Critical-warning border pixel dimensions including both margins.
const CRITICAL_BORDER_SIZE: Size = Size::new(CRITICAL_POPUP_WIDTH + 6, CRITICAL_POPUP_HEIGHT + 6);
/// Landscape critical-warning interior origin.
const CRITICAL_BG_POS: Point = Point::new(CRITICAL_POPUP_X, CRITICAL_POPUP_Y);
/// Critical-warning interior pixel dimensions.
const CRITICAL_BG_SIZE: Size = Size::new(CRITICAL_POPUP_WIDTH, CRITICAL_POPUP_HEIGHT);

/// Draws centered MIN/AVG/MAX RESET feedback into `display`; caller must first
/// establish the producer's reset result. Drawing itself does not reset statistics.
pub fn draw_reset_popup<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
{
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    Rectangle::new(RESET_BORDER_POS + offset, RESET_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    Rectangle::new(RESET_BG_POS + offset, RESET_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    Text::with_text_style("MIN/AVG/MAX", RESET_TEXT1_POS + offset, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style("RESET", RESET_TEXT2_POS + offset, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

/// Draws bounded single-line `label` in a centered 140x50 interior on `display`.
/// Caller supplies text fitting the 10x20 font; this compatibility helper does not truncate it.
fn draw_compact_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    label: &str,
) {
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    Rectangle::new(FPS_BORDER_POS + offset, FPS_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();
    Rectangle::new(FPS_BG_POS + offset, FPS_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();
    Text::with_text_style(label, FPS_TEXT_POS + offset, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

/// Draws the selected `fps_mode` label into `display` without changing measurement or mode state.
pub fn draw_fps_toggle_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    fps_mode: FpsMode,
) {
    draw_compact_popup(display, fps_mode.label());
}

/// Draws BOOST PSI when `show_psi`, otherwise BOOST BAR, into `display`; no conversion state is mutated.
pub fn draw_boost_unit_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    show_psi: bool,
) {
    draw_compact_popup(display, if show_psi { "BOOST: PSI" } else { "BOOST: BAR" });
}

/// Draws a legacy warning for `sensor` into `display`, using `blink_on` to swap
/// foreground/background. Caller owns qualified warning eligibility and blink timing.
pub fn draw_critical_popup<D>(
    display: &mut D,
    sensor: crate::samples::SensorId,
    blink_on: bool,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    let (bg_style, text_style) = if blink_on {
        (RED_FILL, TITLE_STYLE_WHITE)
    } else {
        (WHITE_FILL, TITLE_STYLE_RED)
    };

    let border_style = if blink_on { WHITE_FILL } else { RED_FILL };

    Rectangle::new(CRITICAL_BORDER_POS + offset, CRITICAL_BORDER_SIZE)
        .into_styled(border_style)
        .draw(display)
        .ok();

    Rectangle::new(CRITICAL_BG_POS + offset, CRITICAL_BG_SIZE)
        .into_styled(bg_style)
        .draw(display)
        .ok();

    Text::with_text_style("WARNING", CRITICAL_TEXT1_POS + offset, text_style, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style(sensor.label(), CRITICAL_TEXT2_POS + offset, text_style, CENTERED)
        .draw(display)
        .ok();
}

/// Draws requested `brightness` feedback into `display`; no PWM or completed-popup timer is changed.
pub fn draw_brightness_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    brightness: crate::backlight::Brightness,
) {
    draw_compact_popup(display, brightness.label().as_str());
}

/// Draws validated, fitting view `name` into a centered 210x50 interior on `display`.
/// Caller owns selection and label admission; this helper does not truncate arbitrary text.
pub fn draw_view_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    name: &str,
) {
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    Rectangle::new(
        Point::new(CENTER_X + offset.x - 108, CENTER_Y + offset.y - 28),
        Size::new(216, 56),
    )
    .into_styled(WHITE_FILL)
    .draw(display)
    .ok();
    Rectangle::new(
        Point::new(CENTER_X + offset.x - 105, CENTER_Y + offset.y - 25),
        Size::new(210, 50),
    )
    .into_styled(RED_FILL)
    .draw(display)
    .ok();
    Text::with_text_style(name, FPS_TEXT_POS + offset, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

/// Draws immutable warning `page` into `display`, clipping all text/status/page
/// labels to a centered 216x136 outer panel. Uses its palette and captured blink
/// phase; caller retains episode identity until matching transfer completion.
pub fn draw_warning_page<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    page: crate::view_state::WarningPage,
) {
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    use core::fmt::Write;
    let fg = crate::configured_scene::panel_color(page.colors.foreground);
    let bg = crate::configured_scene::panel_color(page.colors.background);
    let (fg, bg) = if page.blink { (fg, bg) } else { (bg, fg) };
    let background = PrimitiveStyle::with_fill(bg);
    let border = PrimitiveStyle::with_fill(fg);
    let text = MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_10X20, fg);
    let outer = Rectangle::new(
        Point::new(CENTER_X + offset.x - 108, CENTER_Y + offset.y - 68),
        Size::new(216, 136),
    );
    let mut display = display.clipped(&outer);
    outer.into_styled(border).draw(&mut display).ok();
    Rectangle::new(
        Point::new(CENTER_X + offset.x - 105, CENTER_Y + offset.y - 65),
        Size::new(210, 130),
    )
    .into_styled(background)
    .draw(&mut display)
    .ok();
    Text::with_text_style(
        "WARNING",
        Point::new(CENTER_X + offset.x, CENTER_Y + offset.y - 40),
        text,
        CENTERED,
    )
    .draw(&mut display)
    .ok();
    for (row, line) in page.lines().iter().enumerate() {
        Text::with_text_style(
            line.as_str(),
            Point::new(CENTER_X + offset.x, CENTER_Y + offset.y - 24 + row as i32 * 28),
            text,
            CENTERED,
        )
        .draw(&mut display)
        .ok();
        let small = MonoTextStyle::new(crate::ui::LABEL_FONT, fg);
        Text::with_text_style(
            page.status(row),
            Point::new(CENTER_X + offset.x, CENTER_Y + offset.y - 13 + row as i32 * 28),
            small,
            CENTERED,
        )
        .draw(&mut display)
        .ok();
    }
    let mut page_text: heapless::String<8> = heapless::String::new();
    write!(page_text, "{}/{}", page.number, page.total).ok();
    let small = MonoTextStyle::new(crate::ui::LABEL_FONT, text.text_color.unwrap());
    Text::with_text_style(
        &page_text,
        Point::new(CENTER_X + offset.x, CENTER_Y + offset.y + 59),
        small,
        CENTERED,
    )
    .draw(&mut display)
    .ok();
}
