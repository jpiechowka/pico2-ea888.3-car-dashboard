//! Fixed RGB565 legacy palette and encoded-channel contrast selection.
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

/// RGB565 zero: unlit black background/clear color.
pub const BLACK: Rgb565 = Rgb565::BLACK;

/// RGB565 maximum on all channels for white text.
pub const WHITE: Rgb565 = Rgb565::WHITE;

/// Full red channel, zero green/blue.
pub const RED: Rgb565 = Rgb565::RED;

/// Full green channel, zero red/blue.
pub const GREEN: Rgb565 = Rgb565::GREEN;

/// Full blue channel, zero red/green.
pub const BLUE: Rgb565 = Rgb565::BLUE;

/// Full red/green channels, zero blue.
pub const YELLOW: Rgb565 = Rgb565::YELLOW;

/// Legacy pink name for RGB565 magenta: full red/blue, zero green.
pub const PINK: Rgb565 = Rgb565::MAGENTA;

/// Legacy orange encoded channels R5=31, G6=32, B5=0.
pub const ORANGE: Rgb565 = Rgb565::new(31, 32, 0);

/// Legacy dark gray encoded channels R5=8, G6=16, B5=8.
pub const GRAY: Rgb565 = Rgb565::new(8, 16, 8);

/// Legacy dark teal encoded channels R5=0, G6=20, B5=10.
pub const DARK_TEAL: Rgb565 = Rgb565::new(0, 20, 10);

/// Legacy label/outline threshold in encoded RGB565, not the configuration
/// validator's gamma-corrected contrast ratio.
/// Returns BLACK for `color`'s expanded8-bit weighted luma>=128, otherwise WHITE.
/// Weights77/150/29 sum256; this heuristic is not a WCAG contrast guarantee.
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
