//! Bounded integer gauge geometry. No heap, per-frame trig or FPS-dependent state.
use dashboard_config::{GaugeKind, Widget};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::acquisition::DashboardSnapshot;
use crate::samples::SensorId;
use crate::thresholds::BAR_TO_PSI;
use crate::ui::{LEFT_ALIGNED, RIGHT_ALIGNED};
use crate::widgets::value_text::draw_fitted_value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub min: f32,
    pub max: f32,
    pub value: Option<f32>,
}
impl Reading {
    pub fn new(
        widget: &Widget,
        snapshot: &DashboardSnapshot,
        now: u64,
    ) -> Self {
        let id = SensorId::from_channel(widget.channel);
        let state = snapshot.sensor(id);
        let (min, max) = widget.gauge.limits(state.minimum(), state.maximum());
        Self {
            min,
            max,
            value: snapshot.value_at(id, now),
        }
    }

    pub fn level(self) -> Option<u16> {
        let value = self.value.filter(|v| v.is_finite())?;
        let span = self.max - self.min;
        if !self.min.is_finite() || !self.max.is_finite() || !span.is_finite() || span <= 0.0 {
            return None;
        }
        Some(micromath::F32Ext::round(((value - self.min) / span).clamp(0.0, 1.0) * 1024.0) as u16)
    }
}

const CIRCLE: [(i32, i32); 64] = [
    (1024, 0),
    (1019, 100),
    (1004, 200),
    (980, 297),
    (946, 392),
    (903, 483),
    (851, 569),
    (792, 650),
    (724, 724),
    (650, 792),
    (569, 851),
    (483, 903),
    (392, 946),
    (297, 980),
    (200, 1004),
    (100, 1019),
    (0, 1024),
    (-100, 1019),
    (-200, 1004),
    (-297, 980),
    (-392, 946),
    (-483, 903),
    (-569, 851),
    (-650, 792),
    (-724, 724),
    (-792, 650),
    (-851, 569),
    (-903, 483),
    (-946, 392),
    (-980, 297),
    (-1004, 200),
    (-1019, 100),
    (-1024, 0),
    (-1019, -100),
    (-1004, -200),
    (-980, -297),
    (-946, -392),
    (-903, -483),
    (-851, -569),
    (-792, -650),
    (-724, -724),
    (-650, -792),
    (-569, -851),
    (-483, -903),
    (-392, -946),
    (-297, -980),
    (-200, -1004),
    (-100, -1019),
    (0, -1024),
    (100, -1019),
    (200, -1004),
    (297, -980),
    (392, -946),
    (483, -903),
    (569, -851),
    (650, -792),
    (724, -724),
    (792, -650),
    (851, -569),
    (903, -483),
    (946, -392),
    (980, -297),
    (1004, -200),
    (1019, -100),
];
fn polar(
    center: Point,
    radius: i32,
    step: u32,
) -> Point {
    let index = (step / 1024) as usize % 64;
    let fraction = (step % 1024) as i32;
    let a = CIRCLE[index];
    let b = CIRCLE[(index + 1) % 64];
    let x = a.0 + (b.0 - a.0) * fraction / 1024;
    let y = a.1 + (b.1 - a.1) * fraction / 1024;
    center + Point::new(x * radius / 1024, y * radius / 1024)
}
fn line<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    a: Point,
    b: Point,
    color: Rgb565,
    width: u32,
) {
    Line::new(a, b)
        .into_styled(PrimitiveStyle::with_stroke(color, width))
        .draw(display)
        .ok();
}
fn fill<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    rect: Rectangle,
    color: Rgb565,
) {
    if rect.size.width > 0 && rect.size.height > 0 {
        display.fill_solid(&rect, color).ok();
    }
}

/// Geometry is capped before iterator construction, independently of clipping.
/// Numeric text has its own reserved lane and never shares pixels with the scale.
pub fn draw<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    kind: GaugeKind,
    bounds: Rectangle,
    reading: Reading,
    text: &str,
    psi: bool,
    colors: (Rgb565, Rgb565, Rgb565, Rgb565),
) {
    if !(-320..=320).contains(&bounds.top_left.x) || !(-240..=240).contains(&bounds.top_left.y) {
        return;
    }
    let (value_color, accent, track, ticks) = colors;
    let width = bounds.size.width.min(156);
    let height = bounds.size.height.min(198);
    if width < 20 || height < 28 {
        return;
    }
    let bounds = Rectangle::new(bounds.top_left, Size::new(width, height));
    let mut display = display.clipped(&bounds);
    let number_height = if height >= 100 { 36 } else { 22 };
    let graphic_height = height.saturating_sub(number_height + 10);
    let graphic = Rectangle::new(bounds.top_left, Size::new(width, graphic_height));
    let value = Rectangle::new(
        bounds.top_left + Point::new(0, graphic_height as i32),
        Size::new(width, number_height),
    );
    draw_fitted_value(
        &mut display,
        text,
        value,
        if height >= 100 { 2 } else { 1 },
        value_color,
    );
    let level = reading.level();
    let center = graphic.center();
    match kind {
        GaugeKind::Numeric => {}
        GaugeKind::Circular | GaugeKind::Dial => {
            let dial = kind == GaugeKind::Dial;
            let radius = if dial {
                (width as i32 / 2 - 4).min(graphic_height as i32 - 4)
            } else {
                (width.min(graphic_height) as i32 / 2) - 3
            };
            if radius >= 4 {
                let center = if dial {
                    Point::new(center.x, graphic.top_left.y + graphic_height as i32 - 3)
                } else {
                    center
                };
                let start = if dial { 32 } else { 24 };
                let segments = if dial { 32 } else { 48 };
                let stroke = if radius >= 35 { 3 } else { 2 };
                for i in 0..segments {
                    let a = polar(center, radius, (start + i) * 1024);
                    let b = polar(center, radius, (start + i + 1) * 1024);
                    line(&mut display, a, b, track, stroke);
                    if !dial && level.is_some_and(|n| i * 1024 / segments < u32::from(n)) {
                        let end = ((u32::from(level.unwrap()) * segments).min((i + 1) * 1024)).max(i * 1024);
                        line(
                            &mut display,
                            a,
                            polar(center, radius, start * 1024 + end),
                            accent,
                            stroke,
                        );
                    }
                }
                for tick in 0..=4 {
                    let step = (start + tick * segments / 4) * 1024;
                    line(
                        &mut display,
                        polar(center, radius - 3, step),
                        polar(center, radius, step),
                        ticks,
                        1,
                    );
                }
                if dial && let Some(level) = level {
                    line(
                        &mut display,
                        center,
                        polar(center, radius - 4, start * 1024 + u32::from(level) * segments),
                        accent,
                        2,
                    );
                    fill(
                        &mut display,
                        Rectangle::new(center - Point::new(1, 1), Size::new(3, 3)),
                        value_color,
                    );
                }
            }
        }
        GaugeKind::Horizontal | GaugeKind::Segmented => {
            let bar = Rectangle::new(
                Point::new(graphic.top_left.x + 2, center.y - 5),
                Size::new(width - 4, 10),
            );
            if kind == GaugeKind::Segmented {
                for segment in 0..12 {
                    let start = segment * bar.size.width / 12;
                    let end = (segment + 1) * bar.size.width / 12;
                    let color = if level.is_some_and(|v| u32::from(v) * 12 > segment * 1024) {
                        accent
                    } else {
                        track
                    };
                    fill(
                        &mut display,
                        Rectangle::new(
                            bar.top_left + Point::new(start as i32, 0),
                            Size::new((end - start).saturating_sub(1), 10),
                        ),
                        color,
                    );
                }
            } else {
                fill(&mut display, bar, track);
                if let Some(level) = level {
                    fill(
                        &mut display,
                        Rectangle::new(bar.top_left, Size::new(bar.size.width * u32::from(level) / 1024, 10)),
                        accent,
                    );
                }
            }
            for tick in 0..=4 {
                let x = bar.top_left.x + (tick * (bar.size.width - 1) / 4) as i32;
                line(
                    &mut display,
                    Point::new(x, bar.top_left.y + 11),
                    Point::new(x, bar.top_left.y + 13),
                    ticks,
                    1,
                );
            }
        }
        GaugeKind::Vertical => {
            let bar = Rectangle::new(
                Point::new(center.x - 6, graphic.top_left.y + 2),
                Size::new(12, graphic_height.saturating_sub(4)),
            );
            fill(&mut display, bar, track);
            if let Some(level) = level {
                let h = bar.size.height * u32::from(level) / 1024;
                fill(
                    &mut display,
                    Rectangle::new(
                        bar.top_left + Point::new(0, (bar.size.height - h) as i32),
                        Size::new(12, h),
                    ),
                    accent,
                );
            }
            for tick in 0..=4 {
                let y = bar.top_left.y + (tick * bar.size.height.saturating_sub(1) / 4) as i32;
                line(
                    &mut display,
                    Point::new(center.x + 8, y),
                    Point::new(center.x + 11, y),
                    ticks,
                    1,
                );
            }
        }
    }
    // Half-width endpoint lanes, precision reduced before clipping if needed.
    let factor = if psi { BAR_TO_PSI } else { 1.0 };
    for (n, position, style) in [
        (reading.min, bounds.top_left.x, LEFT_ALIGNED),
        (reading.max, bounds.top_left.x + width as i32 - 1, RIGHT_ALIGNED),
    ] {
        let label = endpoint(n * factor, width / 2 - 2);
        Text::with_text_style(
            &label,
            Point::new(position, bounds.top_left.y + height as i32 - 2),
            MonoTextStyle::new(&FONT_4X6, ticks),
            style,
        )
        .draw(&mut display)
        .ok();
    }
}
fn endpoint(
    value: f32,
    width: u32,
) -> heapless::String<16> {
    use core::fmt::Write;
    for decimals in (0..=3).rev() {
        let mut text = heapless::String::new();
        if value.is_finite() && write!(text, "{value:.decimals$}").is_ok() {
            if text.contains('.') {
                while text.ends_with('0') {
                    text.pop();
                }
                if text.ends_with('.') {
                    text.pop();
                }
            }
            if text == "-0" {
                text.clear();
                text.push('0').ok();
            }
            if text.len() as u32 * 4 <= width {
                return text;
            }
        }
    }
    heapless::String::try_from("--").unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_endpoint_labels_distinguish_narrow_scales_and_fit_extremes() {
        assert_eq!(super::endpoint(1.230, 36), "1.23");
        assert_eq!(super::endpoint(1.234, 36), "1.234");
        assert_eq!(super::endpoint(1200., 36), "1200");
        assert_eq!(super::endpoint(-0., 36), "0");
        assert_eq!(super::endpoint(0.0001, 36), "0");
        for value in [-1_000_000.0, -2000.0, 0.0, 2000.0, 1_000_000.0] {
            assert!(super::endpoint(value, 36).len() * 4 <= 36);
        }
    }
}
