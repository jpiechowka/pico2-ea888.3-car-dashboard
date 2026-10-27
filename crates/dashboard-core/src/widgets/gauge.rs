//! Bounded integer gauge geometry. No heap, per-frame trig or FPS-dependent state.
use dashboard_config::{AdvancedDisplay, GaugeKind, ReferenceMode, Widget};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::acquisition::DashboardSnapshot;
use crate::presentation::Unit;
use crate::samples::SensorId;
use crate::thresholds::BAR_TO_PSI;
use crate::ui::{LEFT_ALIGNED, RIGHT_ALIGNED};
use crate::widgets::value_text::draw_fitted_value;

#[derive(Clone, Copy, Debug, PartialEq)]
/// Current indicator, scale and optional held peak, all expressed in the same display units.
pub struct Reading {
    /// Lower scale bound, including configured or automatically expanded raw extrema.
    pub min: f32,
    /// Upper scale bound; indicator admission requires a finite positive span.
    pub max: f32,
    /// Current display value; absent/nonfinite hides the indicator without fabricating zero.
    pub value: Option<f32>,
    /// Converted producer-owned held peak; no renderer history.
    pub peak: Option<f32>,
}

/// Exact dynamic raster dependencies. Bounds stay unquantized: they also
/// determine endpoint text and the immutable configuration's bands/reference.
/// Current text, quality, palette and warning state belong to the cell key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RasterKey {
    /// Unquantized lower bound, retained for endpoint text and decorations.
    min: f32,
    /// Unquantized upper bound, retained for endpoint text and decorations.
    max: f32,
    /// Current indicator quantized to `0..=1024`, or unavailable.
    level: Option<u16>,
    /// In-range peak marker quantized to `0..=1024`, or absent/out of range.
    peak: Option<u16>,
}
impl Reading {
    /// Returns the exact dynamic scale plus quantized indicator/peak dependencies for retained drawing.
    pub(crate) fn raster_key(self) -> RasterKey {
        RasterKey {
            min: self.min,
            max: self.max,
            level: self.level(),
            peak: self.peak.and_then(|value| self.marker_level(value)),
        }
    }

    /// Returns canonical-unit reading for `widget` from coherent `snapshot` at
    /// freshness clock `now` milliseconds. Scale uses raw extrema and configured
    /// range policy; unavailable current data stays `None`, and no peak is supplied.
    /// A cylinder group has no scalar indicator and returns configured bounds only.
    pub fn new(
        widget: &Widget,
        snapshot: &DashboardSnapshot,
        now: u64,
    ) -> Self {
        let Some(channel) = widget.channel.scalar() else {
            return Self {
                min: widget.gauge.min,
                max: widget.gauge.max,
                value: None,
                peak: None,
            };
        };
        let id = SensorId::from_channel(channel);
        let state = snapshot.sensor(id);
        let (min, max) = widget.gauge.limits(state.minimum(), state.maximum());
        Self {
            min,
            max,
            value: snapshot.value_at(id, now),
            peak: None,
        }
    }

    /// Transform current and bounds together; raw extrema still set the scale.
    /// Returns display-unit reading for `widget` using `snapshot` extrema, supplied
    /// canonical `value`/`peak`, conversion `unit` and AFR stoichiometric assumption
    /// `stoich`. No freshness check is repeated for supplied values; caller owns qualification.
    /// Group selectors discard supplied scalar values/peaks and return inert configured bounds.
    pub fn presented(
        widget: &Widget,
        snapshot: &DashboardSnapshot,
        value: Option<f32>,
        peak: Option<f32>,
        unit: crate::presentation::Unit,
        stoich: f32,
    ) -> Self {
        let Some(channel) = widget.channel.scalar() else {
            return Self {
                min: widget.gauge.min,
                max: widget.gauge.max,
                value: None,
                peak: None,
            };
        };
        let state = snapshot.sensor(SensorId::from_channel(channel));
        let (min, max) = widget.gauge.limits(state.minimum(), state.maximum());
        Self {
            min: unit.value(min, stoich),
            max: unit.value(max, stoich),
            value: value.map(|v| unit.value(v, stoich)),
            peak: peak.map(|v| unit.value(v, stoich)),
        }
    }

    /// Markers outside the visible scale are omitted, never saturated.
    /// Returns the `0..=1024` quantized position of `value` in this reading's units,
    /// or `None` for out-of-range/nonfinite values or invalid scale.
    pub fn marker_level(
        self,
        value: f32,
    ) -> Option<u16> {
        if !value.is_finite() || value < self.min || value > self.max {
            return None;
        }
        Self {
            value: Some(value),
            ..self
        }
        .level()
    }

    /// Returns current value quantized/rounded to `0..=1024`, clamping outside
    /// the scale. Missing/nonfinite values or a nonfinite/nonpositive span return `None`.
    pub fn level(self) -> Option<u16> {
        let value = self.value.filter(|v| v.is_finite())?;
        let span = self.max - self.min;
        if !self.min.is_finite() || !self.max.is_finite() || !span.is_finite() || span <= 0.0 {
            return None;
        }
        Some(micromath::F32Ext::round(((value - self.min) / span).clamp(0.0, 1.0) * 1024.0) as u16)
    }
}

/// Unit-circle samples scaled by 1024, increasing clockwise in screen coordinates from the right.
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
/// Returns a point about bounded panel `center` at `radius` pixels and angular
/// `step` in 1/1024 of a 1/64-turn sample. Wraps turns and linearly interpolates
/// adjacent fixed samples; caller bounds radius/center to avoid integer overflow.
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
/// Draws from `a` to `b` on `display` in `color` with pixel `width`, discarding target errors.
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
/// Fills nonempty `rect` on `display` in `color`; zero dimensions are inert and target errors discarded.
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
/// Draws `kind` into clipped `bounds` on `display` using `reading`, primary `text`
/// and `colors` ordered value/accent/track/ticks. `psi` converts endpoint labels
/// from bar only; it does not convert the reading's indicator scale. No decorations
/// are supplied. See [`draw_decorated`] for admission and fallback behavior.
pub fn draw<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    kind: GaugeKind,
    bounds: Rectangle,
    reading: Reading,
    text: &str,
    psi: bool,
    colors: (Rgb565, Rgb565, Rgb565, Rgb565),
) {
    draw_decorated(
        display,
        kind,
        bounds,
        reading,
        text,
        psi,
        Paint {
            colors,
            decorations: None,
        },
    );
}

/// Immutable settings are borrowed for this synchronous draw, never retained.
#[derive(Clone, Copy)]
pub struct Decorations<'a> {
    /// Immutable canonical bands/reference settings; borrowed only during drawing.
    pub settings: &'a AdvancedDisplay,
    /// Conversion from canonical bands/reference to the reading's display units.
    pub unit: Unit,
    /// AFR conversion assumption; not a measured fuel composition or ECU target.
    pub stoich: f32,
    /// Held-peak marker color, drawn after reference so coincident peaks remain visible.
    pub peak_color: Rgb565,
}

/// Transient palette and borrowed decorations for one synchronous draw.
pub struct Paint<'a> {
    /// Value, current accent, background track and tick/endpoint RGB565 colors.
    pub colors: (Rgb565, Rgb565, Rgb565, Rgb565),
    /// Optional canonical bands/reference and peak palette; numeric gauges ignore this decoration set.
    pub decorations: Option<Decorations<'a>>,
}

/// Draws `kind` into bounded/clipped `bounds` on `display`, using `reading`,
/// primary `text`, legacy endpoint-only bar-to-PSI flag `psi` and transient `paint`.
/// Invalid coordinates/small bounds draw nothing; dimensions cap at 156x198
/// except exact width236 for portrait-wide cells. Unfittable primary text becomes
/// `FIT!`; unavailable values hide indicators. Out-of-range markers are omitted,
/// bands intersect the scale, and target errors are discarded without propagation.
pub fn draw_decorated<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    kind: GaugeKind,
    bounds: Rectangle,
    reading: Reading,
    text: &str,
    psi: bool,
    paint: Paint<'_>,
) {
    let Paint { colors, decorations } = paint;
    if !(-320..=320).contains(&bounds.top_left.x) || !(-320..=320).contains(&bounds.top_left.y) {
        return;
    }
    let (value_color, accent, track, ticks) = colors;
    let Some((bounds, graphic, value, scale)) = regions(bounds) else {
        return;
    };
    let width = bounds.size.width;
    let height = bounds.size.height;
    let graphic_width = graphic.size.width;
    let graphic_height = graphic.size.height;
    let mut display = display.clipped(&bounds);
    draw_fitted_value(
        &mut display,
        if crate::widgets::value_text::fit_value(text, value.size, 2).is_some() {
            text
        } else {
            "FIT!"
        },
        value,
        scale,
        value_color,
    );
    let decorations = decorations.filter(|_| !kind.is_numeric());
    let reference = decorations.and_then(|d| {
        let value = match d.settings.reference_mode {
            ReferenceMode::Off => return None,
            ReferenceMode::Zero => 0.0,
            ReferenceMode::Manual => d.settings.reference,
        };
        reading.marker_level(d.unit.value(value, d.stoich))
    });
    let peak = reading.peak.and_then(|v| reading.marker_level(v));
    let level = reading.level();
    let center = graphic.center();
    match kind {
        GaugeKind::Numeric => {}
        GaugeKind::Circular | GaugeKind::Dial => {
            let dial = kind == GaugeKind::Dial;
            let radius = if dial {
                (graphic_width as i32 / 2 - 4).min(graphic_height as i32 - 4)
            } else {
                (graphic_width.min(graphic_height) as i32 / 2) - 3
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
                // Bands use an inner parallel track, leaving their colour visible
                // underneath a filled current arc. Draw order is bands, current,
                // ticks, reference, peak; coincident peak wins.
                if let Some(d) = decorations {
                    for band in d.settings.bands() {
                        if let Some((lo, hi)) = band_levels(reading, d, band) {
                            let r = radius - 5;
                            if r >= 2 {
                                let begin = u32::from(lo) * segments;
                                let end = u32::from(hi) * segments;
                                let mut step = begin;
                                while step < end {
                                    let next = ((step / 1024 + 1) * 1024).min(end);
                                    line(
                                        &mut display,
                                        polar(center, r, start * 1024 + step),
                                        polar(center, r, start * 1024 + next),
                                        band_color(band),
                                        2,
                                    );
                                    step = next;
                                }
                            }
                        }
                    }
                }
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
                for (marker, is_peak) in [(reference, false), (peak, true)] {
                    if let Some(n) = marker {
                        let step = start * 1024 + u32::from(n) * segments;
                        let color = if is_peak {
                            decorations.map_or(ticks, |d| d.peak_color)
                        } else {
                            ticks
                        };
                        line(
                            &mut display,
                            polar(center, radius - 8, step),
                            polar(center, radius + 2, step),
                            color,
                            if is_peak { 2 } else { 1 },
                        );
                        if !is_peak {
                            let at = polar(center, radius - 8, step);
                            fill(
                                &mut display,
                                Rectangle::new(at - Point::new(1, 1), Size::new(3, 3)),
                                color,
                            );
                        }
                    }
                }
            }
        }
        GaugeKind::Horizontal | GaugeKind::Segmented => {
            let bar = Rectangle::new(
                Point::new(graphic.top_left.x + 2, center.y - 5),
                Size::new(graphic_width - 4, 10),
            );
            if let Some(d) = decorations {
                for band in d.settings.bands() {
                    if let Some((lo, hi)) = band_levels(reading, d, band) {
                        let left = u32::from(lo) * bar.size.width / 1024;
                        let right = u32::from(hi) * bar.size.width / 1024;
                        fill(
                            &mut display,
                            Rectangle::new(bar.top_left + Point::new(left as i32, -4), Size::new(right - left, 3)),
                            band_color(band),
                        );
                    }
                }
            }
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
            for (marker, is_peak) in [(reference, false), (peak, true)] {
                if let Some(n) = marker {
                    let x = bar.top_left.x + (u32::from(n) * (bar.size.width - 1) / 1024) as i32;
                    let color = if is_peak {
                        decorations.map_or(ticks, |d| d.peak_color)
                    } else {
                        ticks
                    };
                    line(
                        &mut display,
                        Point::new(x, bar.top_left.y - 5),
                        Point::new(x, bar.top_left.y + 10),
                        color,
                        if is_peak { 2 } else { 1 },
                    );
                    if !is_peak {
                        line(
                            &mut display,
                            Point::new(x - 2, bar.top_left.y - 5),
                            Point::new(x + 2, bar.top_left.y - 5),
                            color,
                            1,
                        );
                    }
                }
            }
        }
        GaugeKind::Vertical => {
            let bar = Rectangle::new(
                Point::new(center.x - 6, graphic.top_left.y + 2),
                Size::new(12, graphic_height.saturating_sub(4)),
            );
            if let Some(d) = decorations {
                for band in d.settings.bands() {
                    if let Some((lo, hi)) = band_levels(reading, d, band) {
                        let top = (1024 - u32::from(hi)) * bar.size.height / 1024;
                        let bottom = (1024 - u32::from(lo)) * bar.size.height / 1024;
                        fill(
                            &mut display,
                            Rectangle::new(bar.top_left + Point::new(-4, top as i32), Size::new(3, bottom - top)),
                            band_color(band),
                        );
                    }
                }
            }
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
            for (marker, is_peak) in [(reference, false), (peak, true)] {
                if let Some(n) = marker {
                    let y = bar.top_left.y + ((1024 - u32::from(n)) * bar.size.height.saturating_sub(1) / 1024) as i32;
                    let color = if is_peak {
                        decorations.map_or(ticks, |d| d.peak_color)
                    } else {
                        ticks
                    };
                    line(
                        &mut display,
                        Point::new(bar.top_left.x - 5, y),
                        Point::new(bar.top_left.x + 12, y),
                        color,
                        if is_peak { 2 } else { 1 },
                    );
                    if !is_peak {
                        line(
                            &mut display,
                            Point::new(bar.top_left.x - 5, y - 2),
                            Point::new(bar.top_left.x - 5, y + 2),
                            color,
                            1,
                        );
                    }
                }
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
/// Returns the RGB565 color encoded in `band` without altering its bits.
fn band_color(band: &dashboard_config::Band) -> Rgb565 {
    embedded_graphics::pixelcolor::raw::RawU16::new(band.color.0).into()
}

/// Converts canonical `band` using decorations `d`, intersects it with `reading`,
/// and returns quantized low/high positions; empty or invalid intersections return `None`.
fn band_levels(
    reading: Reading,
    d: Decorations<'_>,
    band: &dashboard_config::Band,
) -> Option<(u16, u16)> {
    let lo = d.unit.value(band.min, d.stoich).max(reading.min);
    let hi = d.unit.value(band.max, d.stoich).min(reading.max);
    if lo >= hi {
        return None;
    }
    Some((reading.marker_level(lo)?, reading.marker_level(hi)?))
}

/// Returns finite `value` formatted to the greatest precision up to three decimals
/// fitting `width` pixels in FONT_4X6, removing trailing zeros and normalizing -0.
/// Returns `--` when invalid/unfittable; that fallback itself needs eight pixels.
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

/// Exact complete-value lane used by gauge rendering and fit diagnostics.
/// Returns the bounded primary-value rectangle for `bounds`, or `None` when
/// coordinates/dimensions fail the same admission as gauge drawing.
pub fn value_lane(bounds: Rectangle) -> Option<Rectangle> { regions(bounds).map(|(_, _, value, _)| value) }

/// Returns capped bounds, graphic lane, complete-value lane and max integer scale
/// for `bounds`. Coordinates outside `-320..=320`, width<20 or height<28 return
/// `None`; ordinary width caps156, exact236 is retained, and height caps198.
fn regions(bounds: Rectangle) -> Option<(Rectangle, Rectangle, Rectangle, u32)> {
    if !(-320..=320).contains(&bounds.top_left.x) || !(-320..=320).contains(&bounds.top_left.y) {
        return None;
    }
    // The extra wide lane is the native portrait two-column widget. Arbitrary
    // oversized public inputs retain the historical bounded workload.
    let width = if bounds.size.width == 236 {
        236
    } else {
        bounds.size.width.min(156)
    };
    let height = bounds.size.height.min(198);
    if width < 20 || height < 28 {
        return None;
    }
    let bounds = Rectangle::new(bounds.top_left, Size::new(width, height));
    // Wide, short cells spend their extra width on the primary reading rather
    // than squeezing a larger font into the compact graphic's vertical lane.
    let wide = (width >= 120 || (width == 116 && height < 60)) && height < 100;
    let number_height = if height >= 100 || wide { 36.min(height - 10) } else { 22 };
    let graphic_height = if wide {
        height - 10
    } else {
        height.saturating_sub(number_height + 10)
    };
    let graphic_width = if wide { width / 2 - 4 } else { width };
    let graphic = Rectangle::new(bounds.top_left, Size::new(graphic_width, graphic_height));
    let value = if wide {
        Rectangle::new(
            bounds.top_left + Point::new((width / 2) as i32, ((graphic_height - number_height) / 2) as i32),
            Size::new(width - width / 2, number_height),
        )
    } else {
        Rectangle::new(
            bounds.top_left + Point::new(0, graphic_height as i32),
            Size::new(width, number_height),
        )
    };
    Some((bounds, graphic, value, if height >= 100 || wide { 2 } else { 1 }))
}

#[cfg(test)]
/// Endpoint precision, sign normalization and bounded-width regressions.
mod tests {
    #[test]
    /// Confirms narrow-scale precision is retained while extreme finite labels still fit their lanes.
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
