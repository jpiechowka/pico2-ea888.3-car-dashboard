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
/// fixed lanes above the cross. All text is measured before
/// drawing and clipped to its own lane. Unexpected undersized geometry returns
/// without drawing; fitting failure displays `FIT!`, never a truncated number.
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
        let heading_height = if cell.size.height >= 40 {
            13
        } else if cell.size.height >= 28 {
            10
        } else {
            7
        };
        let heading_bounds = Rectangle::new(
            cell.top_left + Point::new(0, 1),
            Size::new(cell.size.width, heading_height),
        );
        draw_line(&mut display, heading_bounds, &heading, panel_color(palette.label))?;
        let number_top = heading_height + 3;
        let number_bounds = Rectangle::new(
            cell.top_left + Point::new(0, number_top as i32),
            cell.size - Size::new(0, number_top + 1),
        );
        draw_line(&mut display, number_bounds, number, panel_color(palette.value))?;
    }
    Ok(())
}

/// Returns the largest Latin-1 font whose complete `text` fits pixel `bounds`.
/// Unicode minus consumes one glyph and is mapped to the ASCII minus at draw
/// time; degree uses the Latin-1 glyph. No byte-count width or ellipsis is used.
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
    let (text, font) = match font(text, bounds.size) {
        Some(font) => (text, font),
        None => {
            match font("FIT!", bounds.size) {
                Some(font) => ("FIT!", font),
                None => return Ok(()),
            }
        }
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

        /// Counts in-bounds `pixels`, or fails immediately when injection is on.
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
            for Pixel(point, _) in pixels {
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
            };
            assert_eq!(
                draw(&mut target, area, "IGN RETARD", palette(), &lanes(), &warning),
                Ok(())
            );
            assert_eq!(target.pixels, 0);
        }
    }
}
