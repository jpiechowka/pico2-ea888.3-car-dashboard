use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

pub const BLACK: Rgb565 = Rgb565::BLACK;

pub const WHITE: Rgb565 = Rgb565::WHITE;

pub const RED: Rgb565 = Rgb565::RED;

pub const GREEN: Rgb565 = Rgb565::GREEN;

pub const BLUE: Rgb565 = Rgb565::BLUE;

pub const YELLOW: Rgb565 = Rgb565::YELLOW;

pub const PINK: Rgb565 = Rgb565::MAGENTA;

pub const ORANGE: Rgb565 = Rgb565::new(31, 32, 0);

pub const GRAY: Rgb565 = Rgb565::new(8, 16, 8);

pub const DARK_TEAL: Rgb565 = Rgb565::new(0, 20, 10);

/// Legacy label/outline threshold in encoded RGB565, not the configuration
/// validator's gamma-corrected contrast ratio.
#[inline]
pub(crate) fn readable_foreground(color: Rgb565) -> Rgb565 {
    use embedded_graphics::prelude::IntoStorage;
    let raw = color.into_storage();
    let r5 = u32::from((raw >> 11) & 0x1F);
    let g6 = u32::from((raw >> 5) & 0x3F);
    let b5 = u32::from(raw & 0x1F);
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);
    let luma = (r8 * 77 + g8 * 150 + b8 * 29) >> 8;
    if luma >= 128 { BLACK } else { WHITE }
}
