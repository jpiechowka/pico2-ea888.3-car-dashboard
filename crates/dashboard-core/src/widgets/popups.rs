use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::config::{CENTER_X, CENTER_Y, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::render::FpsMode;
use crate::ui::{CENTERED, RED, TITLE_STYLE_WHITE, WHITE};

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

const CRITICAL_POPUP_WIDTH: u32 = 210;
const CRITICAL_POPUP_HEIGHT: u32 = 70;
const CRITICAL_POPUP_X: i32 = (SCREEN_WIDTH - CRITICAL_POPUP_WIDTH) as i32 / 2;
const CRITICAL_POPUP_Y: i32 = (SCREEN_HEIGHT - CRITICAL_POPUP_HEIGHT) as i32 / 2;
const CRITICAL_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 8);
const CRITICAL_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);

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

const CRITICAL_BORDER_POS: Point = Point::new(CRITICAL_POPUP_X - 3, CRITICAL_POPUP_Y - 3);
const CRITICAL_BORDER_SIZE: Size = Size::new(CRITICAL_POPUP_WIDTH + 6, CRITICAL_POPUP_HEIGHT + 6);
const CRITICAL_BG_POS: Point = Point::new(CRITICAL_POPUP_X, CRITICAL_POPUP_Y);
const CRITICAL_BG_SIZE: Size = Size::new(CRITICAL_POPUP_WIDTH, CRITICAL_POPUP_HEIGHT);

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

pub fn draw_fps_toggle_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    fps_mode: FpsMode,
) {
    draw_compact_popup(display, fps_mode.label());
}

pub fn draw_boost_unit_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    show_psi: bool,
) {
    draw_compact_popup(display, if show_psi { "BOOST: PSI" } else { "BOOST: BAR" });
}

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

pub fn draw_brightness_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    brightness: crate::backlight::Brightness,
) {
    draw_compact_popup(display, brightness.label().as_str());
}

pub fn draw_view_popup<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    name: &str,
) {
    let size = display.bounding_box().size;
    let offset = Point::new(size.width as i32 / 2 - CENTER_X, size.height as i32 / 2 - CENTER_Y);
    Rectangle::new(Point::new(CENTER_X + offset.x - 108, CENTER_Y + offset.y - 28), Size::new(216, 56))
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();
    Rectangle::new(Point::new(CENTER_X + offset.x - 105, CENTER_Y + offset.y - 25), Size::new(210, 50))
        .into_styled(RED_FILL)
        .draw(display)
        .ok();
    Text::with_text_style(name, FPS_TEXT_POS + offset, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

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
    let outer = Rectangle::new(Point::new(CENTER_X + offset.x - 108, CENTER_Y + offset.y - 68), Size::new(216, 136));
    let mut display = display.clipped(&outer);
    outer.into_styled(border).draw(&mut display).ok();
    Rectangle::new(Point::new(CENTER_X + offset.x - 105, CENTER_Y + offset.y - 65), Size::new(210, 130))
        .into_styled(background)
        .draw(&mut display)
        .ok();
    Text::with_text_style("WARNING", Point::new(CENTER_X + offset.x, CENTER_Y + offset.y - 40), text, CENTERED)
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
    Text::with_text_style(&page_text, Point::new(CENTER_X + offset.x, CENTER_Y + offset.y + 59), small, CENTERED)
        .draw(&mut display)
        .ok();
}
