use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Text, TextStyle};

pub fn draw_cell_background<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bg_color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    if w < 4 || h < 4 {
        return;
    }
    Rectangle::new(Point::new(x as i32 + 2, y as i32 + 2), Size::new(w - 4, h - 4))
        .into_styled(PrimitiveStyle::with_fill(bg_color))
        .draw(display)
        .ok();
}

pub fn draw_trend_arrow<D>(
    display: &mut D,
    x: i32,
    y: i32,
    rising: bool,
    color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let arrow_style = PrimitiveStyle::with_stroke(color, 1);
    if rising {
        Line::new(Point::new(x, y + 4), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    } else {
        Line::new(Point::new(x, y - 4), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_mini_graph<D, F>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    buffer: &[f32],
    buffer_size: usize,
    start_idx: usize,
    count: usize,
    data_min: f32,
    data_max: f32,
    color_fn: F,
) where
    D: DrawTarget<Color = Rgb565>,
    F: Fn(f32) -> Rgb565,
{
    if count == 0 || buffer_size == 0 || buffer_size > buffer.len() || count > buffer_size || start_idx >= buffer_size {
        return;
    }

    draw_graph_points(
        display,
        x,
        y,
        w,
        h,
        count,
        data_min,
        data_max,
        |i| {
            let v = buffer[(start_idx + i) % buffer_size];
            v.is_finite().then_some(v)
        },
        color_fn,
    );
}

/// The window always occupies sixty positions; absent buckets break connectivity.
#[allow(clippy::too_many_arguments)]
pub fn draw_history_graph<D, F>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    history: crate::history::HistoryView<'_>,
    bounds: Option<(f32, f32)>,
    color_fn: F,
) where
    D: DrawTarget<Color = Rgb565>,
    F: Fn(f32) -> Rgb565,
{
    use core::fmt::Write;
    if w < 5 || h < 12 || w > crate::config::SCREEN_WIDTH || h > crate::config::SCREEN_HEIGHT {
        return;
    }
    let (Some(label_x), Some(label_y)) = (x.checked_add(w as i32 / 2), y.checked_add(h as i32 - 1)) else {
        return;
    };
    let mut label = heapless::String::<8>::new();
    write!(label, "{}s", history.duration_seconds()).ok();
    Text::with_text_style(
        &label,
        Point::new(label_x, label_y),
        MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_4X6, color_fn(0.0)),
        crate::ui::CENTERED,
    )
    .draw(display)
    .ok();
    if let Some((min, max)) = bounds {
        draw_graph_points(
            display,
            x,
            y,
            w,
            h - 7,
            60,
            min,
            max,
            |i| history.point(i).and_then(|(_, v)| v),
            color_fn,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_graph_points<D, F, V>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    count: usize,
    data_min: f32,
    data_max: f32,
    value_at: V,
    color_fn: F,
) where
    D: DrawTarget<Color = Rgb565>,
    F: Fn(f32) -> Rgb565,
    V: Fn(usize) -> Option<f32>,
{
    let data_range = data_max - data_min;
    if w < 5
        || h < 5
        // Keep raster work bounded and line deltas within the display geometry.
        || w > crate::config::SCREEN_WIDTH
        || h > crate::config::SCREEN_HEIGHT
        || !data_min.is_finite()
        || !data_max.is_finite()
        || !data_range.is_finite()
        || data_range < 0.0
    {
        return;
    }

    let (Some(max_x), Some(max_y)) = (x.checked_add(w as i32 - 3), y.checked_add(h as i32 - 3)) else {
        return;
    };
    let graph_width = w as i32 - 4;
    let graph_height = h as i32 - 4;
    let graph_x = x + 2;
    let graph_y = y + 2;
    // Exact quotient/remainder spacing, with division only once per graph.
    // Repeated floating point multiplication could miss the newest endpoint.
    let intervals = (count - 1).max(1);
    let x_step = (graph_width - 1) as usize / intervals;
    let x_remainder = (graph_width - 1) as usize % intervals;
    let mut x_error = 0;
    let mut next_x = graph_x;
    let mut previous = None;

    // Sixty history samples fit the graph. Drawing every sample preserves
    // short spikes; invalid measurements break the line instead of bridging it.
    for i in 0..count {
        let screen_x = if count == 1 { max_x } else { next_x };
        if i + 1 < count {
            next_x += x_step as i32;
            x_error += x_remainder;
            if x_error >= intervals {
                x_error -= intervals;
                next_x += 1;
            }
        }
        let Some(value) = value_at(i) else {
            previous = None;
            continue;
        };

        let screen_y = if data_range > 0.0 {
            let fraction = (value.clamp(data_min, data_max) - data_min) / data_range;
            max_y - ((fraction * (graph_height - 1) as f32) as i32).clamp(0, graph_height - 1)
        } else {
            graph_y + (graph_height - 1) / 2
        };

        let point = Point::new(screen_x, screen_y);
        let line_color = color_fn(value);
        let line_style = PrimitiveStyle::with_stroke(line_color, 1);

        if let Some(previous) = previous {
            Line::new(previous, point).into_styled(line_style).draw(display).ok();
        } else {
            Pixel(point, line_color).draw(display).ok();
        }
        previous = Some(point);
    }
}

#[inline]
pub(crate) fn outline_color_for_text(text_color: Rgb565) -> Rgb565 {
    crate::ui::colors::readable_foreground(text_color)
}

#[cfg(feature = "simple-outline")]
pub(crate) const VALUE_OUTLINE_OFFSETS: &[(i32, i32)] = &[(1, 1), (1, 0)];
#[cfg(not(feature = "simple-outline"))]
pub(crate) const VALUE_OUTLINE_OFFSETS: &[(i32, i32)] =
    &[(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

pub fn draw_value_with_outline<D>(
    display: &mut D,
    text: &str,
    position: Point,
    font: &MonoFont<'_>,
    text_color: Rgb565,
    text_style: TextStyle,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let outline_color = outline_color_for_text(text_color);
    let outline_char_style = MonoTextStyle::new(font, outline_color);
    let main_char_style = MonoTextStyle::new(font, text_color);

    for &(dx, dy) in VALUE_OUTLINE_OFFSETS {
        let offset_pos = Point::new(position.x + dx, position.y + dy);
        Text::with_text_style(text, offset_pos, outline_char_style, text_style)
            .draw(display)
            .ok();
    }

    Text::with_text_style(text, position, main_char_style, text_style)
        .draw(display)
        .ok();
}
