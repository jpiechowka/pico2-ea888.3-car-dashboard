//! Four-cylinder signed corrections; no scalar statistics, decoder or transport.
use core::fmt::Write;

use dashboard_config::{Readout, Widget, WidgetColors};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle, iso_8859_1};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::acquisition::DashboardSnapshot;
use crate::configured_scene::{WarningMarker, WidgetWarning, panel_color};
use crate::ignition::{Cylinder, IgnitionSource};
use crate::samples::{DataMode, Quality};
use crate::widgets::cell_text::CellText;

/// Returns four complete visible cylinder strings plus two key-only metadata
/// lanes for `widget` and coherent `snapshot` at monotonic `now_ms`. Each lane
/// is at most16 UTF-8 bytes. Numeric precision is capped at3; raw-negative values
/// are RET even when rounded to zero. Signed zero is OK and prints unsigned zero.
/// Unqualified values show their quality, never a number or OK. Formatting failure
/// replaces the entire lane with `INVALID`; no truncated prefix is published.
pub fn text(
    widget: &Widget,
    snapshot: &DashboardSnapshot,
    now_ms: u64,
) -> CellText {
    let mut result: CellText = core::array::from_fn(|_| heapless::String::new());
    let decimals = usize::from(widget.display.decimals.min(3));
    for cylinder in Cylinder::ALL {
        let value = snapshot.ignition.value_at(cylinder, snapshot.mode, now_ms);
        let quality = effective_quality(snapshot, cylinder, now_ms);
        let out = &mut result[cylinder.index()];
        if let Some(value) = value {
            let status = if value < 0.0 { "RET" } else { "OK" };
            let sign = if value < 0.0 { "−" } else { "" };
            let magnitude = value.abs();
            let formatted = match widget.advanced.readout {
                Readout::IgnitionNumeric => write!(out, "{sign}{magnitude:.decimals$}°"),
                Readout::IgnitionStatus => out.push_str(status).map_err(|_| core::fmt::Error),
                Readout::IgnitionStatusAndNumber => write!(out, "{status} {sign}{magnitude:.decimals$}°"),
                _ => Err(core::fmt::Error),
            };
            if formatted.is_err() {
                out.clear();
                out.push_str("INVALID").ok();
            }
        } else {
            out.push_str(quality.label()).ok();
        }
        // Distinct unavailable states/provenance still invalidate a retained
        // frame even when a future formatting policy gives them the same text.
        result[4]
            .push(match quality {
                Quality::Unknown => 'U',
                Quality::Valid => 'V',
                Quality::Stale => 'S',
                Quality::Unsupported => 'N',
                Quality::TransportError => 'T',
                Quality::InvalidPayload => 'I',
            })
            .ok();
        result[5]
            .push(match snapshot.ignition.source(cylinder) {
                IgnitionSource::Unknown => 'U',
                IgnitionSource::Demo => 'D',
            })
            .ok();
    }
    result[5]
        .push(match snapshot.mode {
            DataMode::Demo => 'D',
            DataMode::Vehicle => 'V',
        })
        .ok();
    result
}

/// Returns effective `cylinder` quality at monotonic `now_ms` in `snapshot`.
/// Vehicle is explicitly unsupported; an inconsistent Valid-without-value is
/// InvalidPayload. This does not modify or refresh the frozen observation.
fn effective_quality(
    snapshot: &DashboardSnapshot,
    cylinder: Cylinder,
    now_ms: u64,
) -> Quality {
    if snapshot.mode != DataMode::Demo {
        return Quality::Unsupported;
    }
    let quality = snapshot.ignition.quality_at(cylinder, now_ms);
    if quality == Quality::Valid && snapshot.ignition.value_at(cylinder, snapshot.mode, now_ms).is_none() {
        Quality::InvalidPayload
    } else {
        quality
    }
}

/// Draws to `display` all four `lanes` in cylinder-number order inside pixel `bounds`, using
/// `label`, `palette` and qualified `warning`. Lanes4/5 are key metadata only.
/// The caller owns background/border/blink policy; persistent warning badges use
/// fixed lanes above the cross. Heading/value pairs are vertically centered using
/// actual font heights, with1px inset in short cells (2px otherwise) and a2px/4px gap.
/// All text is measured before drawing and clipped to its own lane. Rejected outer
/// bounds draw nothing; an undersized pair may leave the title/cross already drawn.
/// Fitting failure displays `FIT!` when it fits, never a truncated number.
/// Returns the first target error; earlier pixels may already have been written.
pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    bounds: Rectangle,
    label: &str,
    palette: WidgetColors,
    lanes: &CellText,
    warning: &WidgetWarning,
) -> Result<(), D::Error> {
    let badge = warning.marker != WarningMarker::None;
    let title_height = 10
        + if badge {
            8 + 8 * u32::from(warning.unavailable)
        } else {
            0
        };
    if bounds.size.width < 20 || bounds.size.height < title_height + 26 {
        return Ok(());
    }
    // All callers use native panel rectangles. Reject unrepresentable caller
    // geometry before signed coordinate addition, even on a diagnostic target.
    let Some(right) = i32::try_from(bounds.size.width)
        .ok()
        .and_then(|width| bounds.top_left.x.checked_add(width))
    else {
        return Ok(());
    };
    let Some(bottom) = i32::try_from(bounds.size.height)
        .ok()
        .and_then(|height| bounds.top_left.y.checked_add(height))
    else {
        return Ok(());
    };
    let mut display = display.clipped(&bounds);
    let title = Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 10));
    draw_line(&mut display, title, label, panel_color(palette.label))?;
    if badge {
        let area = Rectangle::new(
            bounds.top_left + Point::new(0, 10),
            Size::new(bounds.size.width, title_height - 10),
        );
        display.fill_solid(&area, panel_color(palette.warning.background))?;
        draw_line(
            &mut display,
            Rectangle::new(area.top_left, Size::new(area.size.width, 8)),
            warning.marker.label(),
            panel_color(palette.warning.foreground),
        )?;
        if warning.unavailable {
            draw_line(
                &mut display,
                Rectangle::new(area.top_left + Point::new(0, 8), Size::new(area.size.width, 8)),
                "DATA UNAVAILABLE",
                panel_color(palette.warning.foreground),
            )?;
        }
    }
    let grid = Rectangle::new(
        bounds.top_left + Point::new(0, title_height as i32),
        bounds.size - Size::new(0, title_height),
    );
    let middle = grid.top_left + Point::new((grid.size.width / 2) as i32, (grid.size.height / 2) as i32);
    let right = right - 1;
    let bottom = bottom - 1;
    let style = PrimitiveStyle::with_stroke(panel_color(palette.border), 1);
    Line::new(Point::new(middle.x, grid.top_left.y), Point::new(middle.x, bottom))
        .into_styled(style)
        .draw(&mut display)?;
    Line::new(Point::new(grid.top_left.x, middle.y), Point::new(right, middle.y))
        .into_styled(style)
        .draw(&mut display)?;
    for (index, text) in lanes[..4].iter().enumerate() {
        let left = if index % 2 == 0 { grid.top_left.x } else { middle.x + 1 };
        let top = if index < 2 { grid.top_left.y } else { middle.y + 1 };
        let end_x = if index % 2 == 0 { middle.x } else { right + 1 };
        let end_y = if index < 2 { middle.y } else { bottom + 1 };
        let cell = Rectangle::new(
            Point::new(left, top),
            Size::new((end_x - left) as u32, (end_y - top) as u32),
        );
        let (status, number) = if let Some(number) = text.strip_prefix("RET ") {
            (" RET", number)
        } else if let Some(number) = text.strip_prefix("OK ") {
            (" OK", number)
        } else {
            ("", text.as_str())
        };
        let mut heading = heapless::String::<6>::new();
        // Four fixed labels and optional three-character status fit exactly.
        write!(heading, "C{}{status}", index + 1).expect("bounded cylinder heading");
        draw_pair(&mut display, cell, &heading, number, palette)?;
    }
    Ok(())
}

/// Centers complete `heading` and `value` in validated pixel `cell` on `display`,
/// using `palette` label/value colors. Uses actual font heights to keep the two
/// lines together;1px padding per edge when height<28, otherwise2px. The gap is
/// 2px (4px when height>=40). Short badged portrait cells retain6x10 value fonts.
/// If either line cannot fit, draws neither; native bounded cylinder strings admit both.
/// Returns the first target error, which may follow partially drawn heading ink.
/// Coordinates must fit i32, as checked by `draw` before quadrant construction.
fn draw_pair<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    cell: Rectangle,
    heading: &str,
    value: &str,
    palette: WidgetColors,
) -> Result<(), D::Error> {
    let heading_limit = if cell.size.height >= 40 {
        13
    } else if cell.size.height >= 28 {
        10
    } else {
        7
    };
    let gap = if cell.size.height >= 40 { 4 } else { 2 };
    let Some((_, heading_font)) = fitted_line(heading, Size::new(cell.size.width, heading_limit)) else {
        return Ok(());
    };
    let heading_height = heading_font.character_size.height;
    let padding = if cell.size.height < 28 { 2 } else { 4 };
    let value_room = cell.size.height.saturating_sub(padding + gap + heading_height);
    let Some((_, value_font)) = fitted_line(value, Size::new(cell.size.width, value_room)) else {
        return Ok(());
    };
    let value_height = value_font.character_size.height;
    let pair_height = heading_height + gap + value_height;
    let top = cell.top_left + Point::new(0, ((cell.size.height - pair_height) / 2) as i32);
    draw_line(
        display,
        Rectangle::new(top, Size::new(cell.size.width, heading_height)),
        heading,
        panel_color(palette.label),
    )?;
    draw_line(
        display,
        Rectangle::new(
            top + Point::new(0, (heading_height + gap) as i32),
            Size::new(cell.size.width, value_height),
        ),
        value,
        panel_color(palette.value),
    )
}

/// Returns the largest Latin-1 font whose complete `text` fits pixel `bounds`.
/// Unicode minus consumes one glyph and is mapped to the ASCII minus at draw
/// time; degree uses the Latin-1 glyph. No byte-count width or ellipsis is used.
/// Selected fonts have zero character spacing; sizing follows
/// [MonoFont metrics](https://docs.rs/embedded-graphics/0.8.2/embedded_graphics/mono_font/struct.MonoFont.html).
fn font(
    text: &str,
    bounds: Size,
) -> Option<&'static MonoFont<'static>> {
    let glyphs = u32::try_from(text.chars().count()).ok()?;
    [
        &iso_8859_1::FONT_10X20,
        &iso_8859_1::FONT_8X13,
        &iso_8859_1::FONT_6X10,
        &iso_8859_1::FONT_4X6,
    ]
    .into_iter()
    .find(|font| {
        glyphs
            .checked_mul(font.character_size.width)
            .is_some_and(|width| width <= bounds.width)
            && font.character_size.height <= bounds.height
    })
}

/// Returns complete `text` and its largest fitting font for pixel `bounds`, or
/// the same pair for `FIT!` when the original cannot fit. None means even that
/// fallback is too large. Shared by measurement and drawing to avoid drift.
fn fitted_line(
    text: &str,
    bounds: Size,
) -> Option<(&str, &'static MonoFont<'static>)> {
    font(text, bounds)
        .map(|font| (text, font))
        .or_else(|| font("FIT!", bounds).map(|font| ("FIT!", font)))
}

/// Draws to `display` centered complete `text` in pixel `bounds` with `color`; never draws
/// outside its lane. Unsupported codepoints use the font's replacement glyph;
/// producers here emit ASCII, degree or minus only. Returns first draw error,
/// allowing partial prior glyphs. Width overflow falls back to a bounded FIT!.
fn draw_line<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    bounds: Rectangle,
    text: &str,
    color: Rgb565,
) -> Result<(), D::Error> {
    let Some((text, font)) = fitted_line(text, bounds.size) else {
        return Ok(());
    };
    let width = text.chars().count() as u32 * font.character_size.width;
    let mut position = bounds.top_left
        + Point::new(
            ((bounds.size.width - width) / 2) as i32,
            ((bounds.size.height - font.character_size.height) / 2 + font.baseline) as i32,
        );
    let mut display = display.clipped(&bounds);
    let style = MonoTextStyle::new(font, color);
    for character in text.chars() {
        let mut bytes = [0; 4];
        let character = if character == '−' { '-' } else { character };
        Text::new(character.encode_utf8(&mut bytes), position, style).draw(&mut display)?;
        position.x += font.character_size.width as i32;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Pixel boundary/error sentinels and complete smallest-lane glyph admission.
    use dashboard_config::{Placement, Span, default_ignition_widget};

    use super::*;

    /// Rejects any pixel outside `allowed`; optional injected failure is sticky.
    struct Guarded {
        /// Exact writable rectangle, independent of the font's dimensions.
        allowed: Rectangle,
        /// Number of pixels actually delivered to the underlying target.
        pixels: usize,
        /// Fail the first draw request, preserving zero delivered pixels.
        fail: bool,
        /// Optional pixel color that injects a later failure before that pixel.
        fail_color: Option<Rgb565>,
    }
    impl Dimensions for Guarded {
        /// Returns the target's admitted rectangle without changing counters.
        fn bounding_box(&self) -> Rectangle { self.allowed }
    }
    impl DrawTarget for Guarded {
        /// Native panel color for every emitted test pixel.
        type Color = Rgb565;
        /// Injected draw failure, with no additional payload.
        type Error = ();

        /// Counts in-bounds `pixels`, or fails immediately when `fail` is on or
        /// before the first matching `fail_color` pixel after any earlier writes.
        /// Returns success after counting; a boundary violation panics as a regression.
        /// No clipping occurs here.
        fn draw_iter<I>(
            &mut self,
            pixels: I,
        ) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            if self.fail {
                return Err(());
            }
            for Pixel(point, color) in pixels {
                if self.fail_color == Some(color) {
                    return Err(());
                }
                assert!(self.allowed.contains(point), "pixel escaped: {point:?}");
                self.pixels += 1;
            }
            Ok(())
        }
    }

    /// Returns the admitted default group palette, with no external resources.
    fn palette() -> WidgetColors {
        default_ignition_widget(
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::OneByOne,
            },
        )
        .colors
    }

    /// Returns worst-width values/quality labels plus empty metadata lanes.
    fn lanes() -> CellText {
        ["RET −327.680°", "OK 327.670°", "LINK ERR", "INVALID", "", ""].map(|value| value.try_into().unwrap())
    }

    /// Records visible heading/value row extents in each quadrant; rejects escapes.
    struct InkRows {
        /// Complete writable widget interior in pixels.
        bounds: Rectangle,
        /// Cross origin after title/badges, in target pixels.
        grid_top: i32,
        /// Per-cylinder heading/value inclusive row ranges, absent before drawing.
        rows: [[Option<(i32, i32)>; 2]; 4],
    }

    impl Dimensions for InkRows {
        /// Returns this test target's admitted widget bounds.
        fn bounding_box(&self) -> Rectangle { self.bounds }
    }

    impl DrawTarget for InkRows {
        /// Distinct heading/value colors identify text without font assumptions.
        type Color = Rgb565;
        /// Memory-only recording cannot fail; violations panic the regression.
        type Error = core::convert::Infallible;

        /// Records `pixels` by quadrant/color; returns success after all pixels.
        /// Panics for any output outside the caller's rectangle or on the cross.
        fn draw_iter<I>(
            &mut self,
            pixels: I,
        ) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            let mid_x = self.bounds.top_left.x + self.bounds.size.width as i32 / 2;
            let bottom = self.bounds.top_left.y + self.bounds.size.height as i32;
            let mid_y = self.grid_top + (bottom - self.grid_top) / 2;
            for Pixel(point, color) in pixels {
                assert!(self.bounds.contains(point));
                if point.y < self.grid_top {
                    continue;
                }
                let lane = if color == Rgb565::WHITE {
                    0
                } else if color == Rgb565::YELLOW {
                    1
                } else {
                    continue;
                };
                assert_ne!(point.x, mid_x, "text reached vertical cross");
                assert_ne!(point.y, mid_y, "text reached horizontal cross");
                let cylinder = usize::from(point.x > mid_x) + 2 * usize::from(point.y > mid_y);
                let row = &mut self.rows[cylinder][lane];
                *row = Some(row.map_or((point.y, point.y), |(first, last)| {
                    (first.min(point.y), last.max(point.y))
                }));
            }
            Ok(())
        }
    }

    /// All sizes keep complete, inset heading/value pairs close together and centered.
    #[test]
    fn cylinder_ink_forms_compact_centered_pairs_in_every_native_size() {
        let mut colors = palette();
        colors.label = dashboard_config::Color(0xFFFF);
        colors.value = dashboard_config::Color(0xFFE0);
        colors.border = dashboard_config::Color(0x001F);
        for size in [
            Size::new(76, 103),
            Size::new(156, 103),
            Size::new(156, 210),
            Size::new(116, 69),
            Size::new(116, 70),
            Size::new(236, 69),
            Size::new(236, 70),
            Size::new(236, 143),
        ] {
            for title_height in [10, 18, 26] {
                let bounds = Rectangle::new(Point::new(19, 31), size);
                let grid_top = bounds.top_left.y + title_height;
                let bottom = bounds.top_left.y + size.height as i32;
                let middle = grid_top + (bottom - grid_top) / 2;
                let mut target = InkRows {
                    bounds,
                    grid_top,
                    rows: [[None; 2]; 4],
                };
                let warning = WidgetWarning {
                    marker: if title_height > 10 {
                        WarningMarker::Latched
                    } else {
                        WarningMarker::None
                    },
                    unavailable: title_height == 26,
                    blink: false,
                };
                draw(&mut target, bounds, "IGN RETARD", colors, &lanes(), &warning).unwrap();
                for (index, rows) in target.rows.iter().enumerate() {
                    let (label_first, label_last) = rows[0].expect("complete cylinder heading");
                    let (value_first, value_last) = rows[1].expect("complete value or quality");
                    let top = if index < 2 { grid_top } else { middle + 1 };
                    let end = if index < 2 { middle } else { bottom };
                    let gap = value_first - label_last - 1;
                    let above = label_first - top;
                    let below = end - value_last - 1;
                    // Visible-ink separation includes blank rows inside font cells.
                    assert!(
                        (2..=10).contains(&gap),
                        "pair separated: {size:?} C{} gap={gap}",
                        index + 1
                    );
                    let inset = if end - top < 28 { 1 } else { 2 };
                    assert!(
                        above >= inset && below >= inset,
                        "missing inset: {size:?} {above}/{below}"
                    );
                    assert!((above - below).abs() <= 6, "pair off center: {size:?} {above}/{below}");
                }
            }
        }
    }

    /// Both smallest native widgets retain all characters, even with two badge rows.
    #[test]
    fn smallest_quadrants_fit_extremes_and_all_quality_labels() {
        for size in [Size::new(37, 26), Size::new(57, 9)] {
            for text in [
                "−327.680°",
                "327.670°",
                "NO DATA",
                "LINK ERR",
                "INVALID",
                "STALE",
                "N/A",
                "RET",
                "OK",
            ] {
                assert!(font(text, size).is_some(), "{text:?} must fit {size:?} completely");
            }
        }
        assert!(font("−327.680°", Size::new(35, 6)).is_none());
        assert!(font("RET", Size::new(20, 5)).is_none());
    }

    /// Compact portrait cells retain readable6x10 values even below two warning rows.
    #[test]
    fn smallest_badged_portrait_preserves_value_font_height() {
        let bounds = Rectangle::new(Point::zero(), Size::new(116, 69));
        let mut target = InkRows {
            bounds,
            grid_top: 26,
            rows: [[None; 2]; 4],
        };
        let mut colors = palette();
        colors.label = dashboard_config::Color(0xFFFF);
        colors.value = dashboard_config::Color(0xFFE0);
        let warning = WidgetWarning {
            marker: WarningMarker::Latched,
            unavailable: true,
            blink: false,
        };
        let lanes = ["LINK ERR", "LINK ERR", "LINK ERR", "LINK ERR", "", ""].map(|text| text.try_into().unwrap());
        draw(&mut target, bounds, "IGN RETARD", colors, &lanes, &warning).unwrap();
        for rows in target.rows {
            let (first, last) = rows[1].unwrap();
            assert!(last - first + 1 >= 7, "quality label was shrunk below6x10 ink");
        }
    }

    /// Complete independent glyph masks survive narrow, short and badged pairs.
    #[test]
    fn compact_pairs_preserve_full_glyph_masks() {
        use embedded_graphics::mock_display::MockDisplay;
        use embedded_graphics::pixelcolor::BinaryColor;
        use embedded_graphics::text::Baseline;
        let mut colors = palette();
        colors.label = dashboard_config::Color(0xFFFF);
        colors.value = dashboard_config::Color(0xFFE0);
        for (size, value, expected_font) in [
            (Size::new(37, 46), "−327.680°", &iso_8859_1::FONT_4X6),
            (Size::new(57, 21), "LINK ERR", &iso_8859_1::FONT_6X10),
            (Size::new(57, 25), "327.670°", &iso_8859_1::FONT_6X10),
            (Size::new(57, 29), "−327.680°", &iso_8859_1::FONT_6X10),
            (Size::new(37, 46), "RET", &iso_8859_1::FONT_10X20),
        ] {
            let mut target = MockDisplay::<Rgb565>::new();
            draw_pair(
                &mut target,
                Rectangle::new(Point::zero(), size),
                "C1 RET",
                value,
                colors,
            )
            .unwrap();
            let mut expected = MockDisplay::<BinaryColor>::new();
            let normalized: heapless::String<16> = value.chars().map(|c| if c == '−' { '-' } else { c }).collect();
            Text::with_baseline(
                &normalized,
                Point::zero(),
                MonoTextStyle::new(expected_font, BinaryColor::On),
                Baseline::Top,
            )
            .draw(&mut expected)
            .unwrap();
            let width = normalized.chars().count() as u32 * expected_font.character_size.width;
            let height = expected_font.character_size.height;
            assert!(
                (0..=size.height - height).any(|y| {
                    (0..=size.width - width).any(|x| {
                        Rectangle::new(Point::zero(), size).points().all(|point| {
                            let local = point - Point::new(x as i32, y as i32);
                            let wanted = local.x >= 0
                                && local.y >= 0
                                && local.x < width as i32
                                && local.y < height as i32
                                && expected.get_pixel(local) == Some(BinaryColor::On);
                            (target.get_pixel(point) == Some(Rgb565::YELLOW)) == wanted
                        })
                    })
                }),
                "complete {value:?} glyph mask missing at {size:?}"
            );
        }
    }

    /// Unsupported tiny pairs are inert; failure on value ink preserves prior heading pixels.
    #[test]
    fn pair_tiny_geometry_and_partial_target_failure() {
        let bounds = Rectangle::new(Point::zero(), Size::new(38, 46));
        let mut target = Guarded {
            allowed: bounds,
            pixels: 0,
            fail: false,
            fail_color: Some(Rgb565::YELLOW),
        };
        let mut colors = palette();
        colors.label = dashboard_config::Color(0xFFFF);
        colors.value = dashboard_config::Color(0xFFE0);
        assert_eq!(draw_pair(&mut target, bounds, "C1 RET", "−2.5°", colors), Err(()));
        assert!(target.pixels > 0, "heading must precede the failing value draw");
        target.pixels = 0;
        target.fail = true;
        for size in [Size::zero(), Size::new(38, 13), Size::new(15, 46)] {
            assert_eq!(
                draw_pair(
                    &mut target,
                    Rectangle::new(Point::zero(), size),
                    "C1 RET",
                    "−2.5°",
                    colors
                ),
                Ok(())
            );
            assert_eq!(target.pixels, 0);
        }
    }

    /// Drawing stays in shifted lanes; failures propagate and impossible rectangles are inert.
    #[test]
    fn native_bounds_error_propagation_and_invalid_geometry() {
        let warning = WidgetWarning {
            marker: WarningMarker::Latched,
            unavailable: true,
            blink: false,
        };
        for size in [
            Size::new(76, 103),
            Size::new(116, 69),
            Size::new(156, 210),
            Size::new(236, 143),
        ] {
            let area = Rectangle::new(Point::new(19, 31), size);
            let mut target = Guarded {
                allowed: area,
                pixels: 0,
                fail: false,
                fail_color: None,
            };
            draw(&mut target, area, "IGN RETARD", palette(), &lanes(), &warning).unwrap();
            assert!(target.pixels > 100);
            target.fail = true;
            target.pixels = 0;
            assert_eq!(
                draw(&mut target, area, "IGN RETARD", palette(), &lanes(), &warning),
                Err(())
            );
            assert_eq!(target.pixels, 0);
        }
        for area in [
            Rectangle::new(Point::zero(), Size::new(0, 0)),
            Rectangle::new(Point::zero(), Size::new(76, 51)),
            Rectangle::new(Point::new(i32::MAX - 10, 0), Size::new(76, 103)),
            Rectangle::new(Point::new(0, i32::MAX - 10), Size::new(76, 103)),
            Rectangle::new(Point::zero(), Size::new(u32::MAX, 103)),
        ] {
            let mut target = Guarded {
                allowed: area,
                pixels: 0,
                fail: true,
                fail_color: None,
            };
            assert_eq!(
                draw(&mut target, area, "IGN RETARD", palette(), &lanes(), &warning),
                Ok(())
            );
            assert_eq!(target.pixels, 0);
        }
    }
}
