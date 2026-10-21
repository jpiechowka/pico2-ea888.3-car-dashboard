//! Shared fixed fonts and transparent text styles; callers still measure and clip actual bounds.
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::text::{Alignment, TextStyle, TextStyleBuilder};
use profont::{PROFONT_18_POINT, PROFONT_24_POINT};

use super::colors::{BLACK, ORANGE, WHITE};

/// Centers text horizontally about its supplied anchor, retaining default baseline behavior.
pub const CENTERED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Center).build();

/// Left-aligns text at its supplied anchor with default baseline behavior.
pub const LEFT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Left).build();

/// Right-aligns text to its supplied anchor with default baseline behavior.
pub const RIGHT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Right).build();

/// Six-by-ten-pixel ASCII label font.
pub const LABEL_FONT: &MonoFont = &FONT_6X10;

/// ProFont24-point value font; point name is not a pixel-width promise.
pub const VALUE_FONT: &MonoFont = &PROFONT_24_POINT;

/// Transparent6x10 ASCII labels with white foreground.
pub const LABEL_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, WHITE);

/// Transparent6x10 ASCII labels with black foreground.
pub const LABEL_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, BLACK);

/// Transparent6x10 ASCII labels with legacy orange foreground.
pub const LABEL_STYLE_ORANGE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, ORANGE);

/// Transparent10x20 ASCII titles with white foreground.
pub const TITLE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_10X20, WHITE);

/// Transparent ProFont24-point values with white foreground.
pub const VALUE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, WHITE);

/// Transparent ProFont24-point values with black foreground.
pub const VALUE_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, BLACK);

/// ProFont18-point alternate value font for tighter layouts.
pub const VALUE_FONT_MEDIUM: &MonoFont = &PROFONT_18_POINT;
