//! Native landscape/portrait headers and default-grid divider geometry.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::render::{FpsMode, format_fps};
use crate::samples::DataMode;
use crate::ui::{CENTERED, GRAY, LABEL_STYLE_WHITE, LEFT_ALIGNED, RED, RIGHT_ALIGNED, TITLE_STYLE_WHITE};
use crate::vehicle_status::VehicleStatus;

/// Landscape title center/baseline in pixels.
const HEADER_TITLE_POS: Point = Point::new(160, 19);
/// Landscape FPS right edge/baseline, five pixels inside the panel.
const HEADER_FPS_POS: Point = Point::new((SCREEN_WIDTH - 5) as i32, 17);
/// Landscape header top-left at the framebuffer origin.
const HEADER_RECT_POS: Point = Point::new(0, 0);
/// Landscape header dimensions, 320 by 26 pixels.
const HEADER_RECT_SIZE: Size = Size::new(SCREEN_WIDTH, 26);

/// First landscape column divider's top endpoint below the header.
const DIV_V1_START: Point = Point::new(COL_WIDTH as i32, HEADER_HEIGHT as i32);
/// First landscape column divider's bottom inclusive endpoint.
const DIV_V1_END: Point = Point::new(COL_WIDTH as i32, (SCREEN_HEIGHT - 1) as i32);
/// Middle landscape column divider's top endpoint below the header.
const DIV_V2_START: Point = Point::new((COL_WIDTH * 2) as i32, HEADER_HEIGHT as i32);
/// Middle landscape column divider's bottom inclusive endpoint.
const DIV_V2_END: Point = Point::new((COL_WIDTH * 2) as i32, (SCREEN_HEIGHT - 1) as i32);
/// Third landscape column divider's top endpoint below the header.
const DIV_V3_START: Point = Point::new((COL_WIDTH * 3) as i32, HEADER_HEIGHT as i32);
/// Third landscape column divider's bottom inclusive endpoint.
const DIV_V3_END: Point = Point::new((COL_WIDTH * 3) as i32, (SCREEN_HEIGHT - 1) as i32);
/// Landscape row divider's left endpoint at y=133 pixels.
const DIV_H_START: Point = Point::new(0, (HEADER_HEIGHT + ROW_HEIGHT) as i32);
/// Landscape row divider's right inclusive endpoint.
const DIV_H_END: Point = Point::new((SCREEN_WIDTH - 1) as i32, (HEADER_HEIGHT + ROW_HEIGHT) as i32);

/// One-pixel gray divider stroke.
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(GRAY, 1);
/// Opaque red header fill, clearing previous status/FPS text.
const HEADER_FILL_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

/// Draws the synthetic Demo header into `display` using `fps_mode`, instantaneous
/// `fps_instant` and window-average `fps_average` completed transfers/second.
/// Invalid rates become unavailable labels; target errors are discarded.
pub fn draw_header<D>(
    display: &mut D,
    fps_mode: FpsMode,
    fps_instant: f32,
    fps_average: f32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    draw_mode_header(display, DataMode::Demo, fps_mode, fps_instant, fps_average);
}

/// Draws `mode` into `display` with an empty Demo status label, selecting FPS text
/// via `fps_mode`, `fps_instant` and `fps_average` transfers/second. This compatibility
/// route does not derive vehicle status; target errors are discarded.
pub fn draw_mode_header<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    mode: DataMode,
    fps_mode: FpsMode,
    fps_instant: f32,
    fps_average: f32,
) {
    draw_status_header(display, mode, VehicleStatus::Demo, fps_mode, fps_instant, fps_average);
}

/// Clears/draws the 26-pixel header into native-geometry `display` with source
/// `mode`, already classified `status` and `fps_mode` applied to `fps_instant`/
/// `fps_average` completed transfers/second. Exact 240x320 uses compact portrait
/// rows; other targets use landscape coordinates. Errors are discarded; rates do not prove scanout.
pub fn draw_status_header<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    mode: DataMode,
    status: VehicleStatus,
    fps_mode: FpsMode,
    fps_instant: f32,
    fps_average: f32,
) {
    if crate::configured_scene::target_geometry(display) == dashboard_config::Geometry::Portrait {
        display
            .fill_solid(&Rectangle::new(Point::zero(), Size::new(240, 26)), RED)
            .ok();
        let tiny = embedded_graphics::mono_font::MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_4X6,
            crate::ui::WHITE,
        );
        Text::new(status.label(), Point::new(4, 7), tiny).draw(display).ok();
        Text::with_text_style(
            &format_fps(fps_mode, fps_instant, fps_average),
            Point::new(235, 7),
            tiny,
            RIGHT_ALIGNED,
        )
        .draw(display)
        .ok();
        Text::with_text_style(
            if mode == DataMode::Demo { "DEMO" } else { "VEHICLE" },
            Point::new(120, 22),
            LABEL_STYLE_WHITE,
            CENTERED,
        )
        .draw(display)
        .ok();
        return;
    }
    Rectangle::new(HEADER_RECT_POS, HEADER_RECT_SIZE)
        .into_styled(HEADER_FILL_STYLE)
        .draw(display)
        .ok();

    let title = match mode {
        DataMode::Vehicle => "VEHICLE",
        DataMode::Demo => "DEMO",
    };
    Text::with_text_style(title, HEADER_TITLE_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style(status.label(), Point::new(4, 17), LABEL_STYLE_WHITE, LEFT_ALIGNED)
        .draw(display)
        .ok();

    if fps_mode.is_visible() {
        let fps_str = format_fps(fps_mode, fps_instant, fps_average);
        Text::with_text_style(&fps_str, HEADER_FPS_POS, LABEL_STYLE_WHITE, RIGHT_ALIGNED)
            .draw(display)
            .ok();
    }
}

/// Draws default-grid divider lines into `display`: 2x4 for exact 240x320, otherwise
/// landscape 4x2. Does not clear cells or account for merged configured widgets;
/// target errors are discarded.
pub fn draw_dividers<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
{
    if crate::configured_scene::target_geometry(display) == dashboard_config::Geometry::Portrait {
        Line::new(Point::new(120, 26), Point::new(120, 319))
            .into_styled(DIVIDER_STYLE)
            .draw(display)
            .ok();
        for y in [99, 173, 246] {
            Line::new(Point::new(0, y), Point::new(239, y))
                .into_styled(DIVIDER_STYLE)
                .draw(display)
                .ok();
        }
        return;
    }
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
