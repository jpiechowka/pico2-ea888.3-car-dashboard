//! UI styling and visual constants.
//!
//! - `colors`: RGB565 color constants
//! - `styles`: Pre-computed text styles and fonts
//! - `animations`: Color transitions for smooth background changes

mod animations;
mod colors;
mod styles;

pub use animations::ColorTransition;
pub use colors::{BLACK, BLUE, DARK_TEAL, GRAY, GREEN, ORANGE, PINK, RED, WHITE, YELLOW};
pub use styles::{
    CENTERED,
    LABEL_FONT,
    LABEL_STYLE_BLACK,
    LABEL_STYLE_ORANGE,
    LABEL_STYLE_WHITE,
    LEFT_ALIGNED,
    RIGHT_ALIGNED,
    TITLE_STYLE_WHITE,
    VALUE_FONT,
    VALUE_FONT_MEDIUM,
    VALUE_STYLE_BLACK,
    VALUE_STYLE_WHITE,
};
