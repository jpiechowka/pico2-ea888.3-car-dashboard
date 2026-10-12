use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::text::{Alignment, TextStyle, TextStyleBuilder};
use profont::{PROFONT_18_POINT, PROFONT_24_POINT};

use super::colors::{BLACK, ORANGE, WHITE};

pub const CENTERED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Center).build();

pub const LEFT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Left).build();

pub const RIGHT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Right).build();

pub const LABEL_FONT: &MonoFont = &FONT_6X10;

pub const VALUE_FONT: &MonoFont = &PROFONT_24_POINT;

pub const LABEL_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, WHITE);

pub const LABEL_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, BLACK);

pub const LABEL_STYLE_ORANGE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, ORANGE);

pub const TITLE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_10X20, WHITE);

pub const VALUE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, WHITE);

pub const VALUE_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, BLACK);

pub const VALUE_FONT_MEDIUM: &MonoFont = &PROFONT_18_POINT;
