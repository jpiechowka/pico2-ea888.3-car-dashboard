//! Allocation-free integer enlargement of the existing bitmap value fonts.
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};

use crate::ui::{LABEL_FONT, VALUE_FONT, VALUE_FONT_MEDIUM};
use crate::widgets::primitives::{VALUE_OUTLINE_OFFSETS, draw_value_with_outline, outline_color_for_text};

#[derive(Clone, Copy)]
pub struct ValueFont {
    pub font: &'static MonoFont<'static>,
    pub scale: u32,
    /// Includes one physical pixel of outline on each side.
    pub size: Size,
}

/// Only bounded single-line ASCII values are supported. Return no layout when
/// even the smallest complete string cannot fit. Never use clipping to fit it.
pub fn fit_value(
    text: &str,
    bounds: Size,
    max_scale: u32,
) -> Option<ValueFont> {
    if text.is_empty() || text.len() > 16 || !text.bytes().all(|b| (32..=126).contains(&b)) {
        return None;
    }
    let mut best: Option<ValueFont> = None;
    for scale in 1..=max_scale.min(3) {
        for font in [
            VALUE_FONT,
            VALUE_FONT_MEDIUM,
            &embedded_graphics::mono_font::ascii::FONT_10X20,
            LABEL_FONT,
            &embedded_graphics::mono_font::ascii::FONT_4X6,
        ] {
            let width = (text.len() as u32 * (font.character_size.width + font.character_spacing)
                - font.character_spacing)
                * scale
                + 2;
            let height = font.character_size.height * scale + 2;
            if width <= bounds.width && height <= bounds.height && best.is_none_or(|old| height > old.size.height) {
                best = Some(ValueFont {
                    font,
                    scale,
                    size: Size::new(width, height),
                });
            }
        }
    }
    best
}

/// Draw the already-fitted string inside its independent lane. The outline is
/// always one panel pixel, even when the glyphs are enlarged two or three times.
pub fn draw_fitted_value<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    text: &str,
    lane: Rectangle,
    max_scale: u32,
    color: Rgb565,
) -> u32 {
    // A lane cannot exceed this dashboard's panel. Bound arbitrary public API
    // coordinates before Rectangle/Point arithmetic, including signed overflow.
    if lane.size.width > 320
        || lane.size.height > 240
        || !(-320..=320).contains(&lane.top_left.x)
        || !(-240..=240).contains(&lane.top_left.y)
    {
        return 0;
    }
    let Some(value) = fit_value(text, lane.size, max_scale) else {
        return 0;
    };
    let position = lane.top_left + Point::new(((lane.size.width - value.size.width) / 2 + 1) as i32, 1);
    let source = Size::new((value.size.width - 2) / value.scale, value.font.character_size.height);
    let style = TextStyleBuilder::new().baseline(Baseline::Top).build();
    if value.scale == 1 {
        // Fitting proves the complete unscaled glyph + outline stays in its
        // lane. Keep the existing widget/panel clipping, without adding another
        // per-pixel adapter to the compact hot path.
        draw_value_with_outline(display, text, position, value.font, color, style);
        return value.size.height;
    }
    let mut display = display.clipped(&lane);
    for &(x, y) in VALUE_OUTLINE_OFFSETS {
        let mut target = Enlarged {
            display: &mut display,
            position: position + Point::new(x, y),
            scale: value.scale,
            source,
        };
        Text::with_text_style(
            text,
            Point::zero(),
            MonoTextStyle::new(value.font, outline_color_for_text(color)),
            style,
        )
        .draw(&mut target)
        .ok();
    }
    let mut target = Enlarged {
        display: &mut display,
        position,
        scale: value.scale,
        source,
    };
    Text::with_text_style(text, Point::zero(), MonoTextStyle::new(value.font, color), style)
        .draw(&mut target)
        .ok();
    value.size.height
}

struct Enlarged<'a, D> {
    display: &'a mut D,
    position: Point,
    scale: u32,
    source: Size,
}

impl<D> OriginDimensions for Enlarged<'_, D> {
    fn size(&self) -> Size { self.source }
}

impl<D: DrawTarget<Color = Rgb565>> DrawTarget for Enlarged<'_, D> {
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        // Join adjacent pixels in a glyph scanline into one rectangle. This keeps
        // enlargement cheap on the framebuffer without a scratch bitmap.
        let mut run: Option<(Point, Rgb565, u32)> = None;
        for Pixel(point, color) in pixels {
            if !self.bounding_box().contains(point) {
                continue;
            }
            if let Some((start, previous, length)) = &mut run {
                if color == *previous && point.y == start.y && point.x == start.x + *length as i32 {
                    *length += 1;
                    continue;
                }
                self.flush(*start, *previous, *length)?;
            }
            run = Some((point, color, 1));
        }
        if let Some((start, color, length)) = run {
            self.flush(start, color, length)?;
        }
        Ok(())
    }
}

impl<D: DrawTarget<Color = Rgb565>> Enlarged<'_, D> {
    fn flush(
        &mut self,
        start: Point,
        color: Rgb565,
        length: u32,
    ) -> Result<(), D::Error> {
        self.display.fill_solid(
            &Rectangle::new(
                self.position + start * self.scale as i32,
                Size::new(length * self.scale, self.scale),
            ),
            color,
        )
    }
}
