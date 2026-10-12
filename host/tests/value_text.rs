use std::collections::BTreeMap;

use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::widgets::value_text::{draw_fitted_value, fit_value};
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};

#[derive(Default)]
struct Pixels(BTreeMap<(i32, i32), Rgb565>);
impl OriginDimensions for Pixels {
    fn size(&self) -> Size { Size::new(320, 240) }
}
impl DrawTarget for Pixels {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, color) in pixels {
            self.0.insert((p.x, p.y), color);
        }
        Ok(())
    }
}

#[test]
fn enlarged_glyphs_match_independent_expansion_and_one_pixel_outline() {
    for (text, size, cap) in [
        ("1.7", Size::new(156, 64), 2),
        ("82C", Size::new(156, 150), 3),
        ("-1000000.000C", Size::new(76, 20), 3),
        ("0.800", Size::new(156, 160), 3),
    ] {
        let layout = fit_value(text, size, cap).unwrap();
        let lane = Rectangle::new(Point::new(7, 31), size);
        for color in [colors::WHITE, colors::BLACK] {
            let mut glyphs = Pixels::default();
            Text::with_text_style(
                text,
                Point::zero(),
                MonoTextStyle::new(layout.font, color),
                TextStyleBuilder::new().baseline(Baseline::Top).build(),
            )
            .draw(&mut glyphs)
            .unwrap();
            // Independent nearest-neighbour oracle, including transparent holes.
            let origin = lane.top_left + Point::new(((size.width - layout.size.width) / 2 + 1) as i32, 1);
            let mut body = BTreeMap::new();
            for &(x, y) in glyphs.0.keys() {
                for dy in 0..layout.scale as i32 {
                    for dx in 0..layout.scale as i32 {
                        body.insert(
                            (
                                origin.x + x * layout.scale as i32 + dx,
                                origin.y + y * layout.scale as i32 + dy,
                            ),
                            color,
                        );
                    }
                }
            }
            let outline = if color == colors::WHITE {
                colors::BLACK
            } else {
                colors::WHITE
            };
            let mut expected = BTreeMap::new();
            for &(x, y) in body.keys() {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if cfg!(feature = "simple-outline") && !((dx == 1 && dy == 1) || (dx == 1 && dy == 0)) {
                            continue;
                        }
                        if dx != 0 || dy != 0 {
                            expected.insert((x + dx, y + dy), outline);
                        }
                    }
                }
            }
            expected.extend(body);
            assert!(
                expected.keys().all(|&(x, y)| lane.contains(Point::new(x, y))),
                "oracle overflows"
            );
            let mut actual = Pixels::default();
            assert_eq!(
                draw_fitted_value(&mut actual, text, lane, cap, color),
                layout.size.height
            );
            assert_eq!(actual.0, expected, "{text} scale={} color={color:?}", layout.scale);
        }
    }
}

#[test]
fn all_formatted_widths_fit_with_outline_and_invalid_inputs_draw_nothing() {
    for text in [
        "--",
        "-0.000C",
        "1000000.000V",
        "-1000000.000C",
        "14503800.000",
        "0.001",
        "99.9",
        "100.0",
    ] {
        for width in [0, 1, 2, 4, 25, 76, 156] {
            for height in [0, 1, 7, 12, 19, 32, 80, 196] {
                for scale in [0, 1, 2, 3, u32::MAX] {
                    let Some(value) = fit_value(text, Size::new(width, height), scale) else {
                        continue;
                    };
                    let bounds =
                        Text::new(text, Point::zero(), MonoTextStyle::new(value.font, colors::WHITE)).bounding_box();
                    assert!(value.scale <= 3 && value.scale <= scale);
                    assert!(bounds.size.width * value.scale + 2 <= width);
                    assert!(bounds.size.height * value.scale + 2 <= height);
                }
            }
        }
    }
    for text in ["", "1\n2", "\r", "3\t4", "λ", "12345678901234567"] {
        let mut pixels = Pixels::default();
        assert_eq!(
            draw_fitted_value(
                &mut pixels,
                text,
                Rectangle::new(Point::zero(), Size::new(156, 200)),
                3,
                colors::WHITE
            ),
            0
        );
        assert!(pixels.0.is_empty());
    }
}

#[test]
fn framebuffer_clips_unscaled_and_enlarged_values_at_every_panel_edge() {
    for point in [
        Point::new(-100, -40),
        Point::new(310, 20),
        Point::new(10, 230),
        Point::new(-320, 0),
    ] {
        for cap in [1, 3] {
            let lane = Rectangle::new(point, Size::new(156, 100));
            let mut pixels = Pixels::default();
            draw_fitted_value(&mut pixels, "82C", lane, cap, colors::WHITE);
            let mut frame = AlignedFrame::boxed();
            let mut target = St7789Renderer::new(&mut frame.0);
            target.clear(colors::BLUE).unwrap();
            draw_fitted_value(&mut target, "82C", lane, cap, colors::WHITE);
            for y in 0..240 {
                for x in 0..320 {
                    let color = pixels
                        .0
                        .get(&(x, y))
                        .copied()
                        .unwrap_or(colors::BLUE)
                        .into_storage()
                        .to_be_bytes();
                    let at = ((y * 320 + x) * 2) as usize;
                    assert_eq!(&frame.0[at..at + 2], &color);
                }
            }
        }
    }
}

#[test]
fn unrepresentable_or_nonpanel_lanes_cannot_overflow_coordinate_arithmetic() {
    for lane in [
        Rectangle::new(Point::new(i32::MAX, i32::MAX), Size::new(156, 100)),
        Rectangle::new(Point::new(i32::MIN, i32::MIN), Size::new(156, 100)),
        Rectangle::new(Point::zero(), Size::new(u32::MAX, u32::MAX)),
    ] {
        let mut pixels = Pixels::default();
        assert_eq!(draw_fitted_value(&mut pixels, "1.7", lane, 3, colors::WHITE), 0);
        assert!(pixels.0.is_empty());
    }
}
