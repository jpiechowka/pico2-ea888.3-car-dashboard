//! Widget components for the OBD dashboard display.
//!
//! This module organizes all visual components into logical submodules:
//!
//! - [`cells`]: Individual sensor display cells (boost, temp, battery, AFR, IAT, EGT)
//! - [`header`]: Header bar and grid divider lines
//! - [`popups`]: Overlay popup dialogs (reset notification, FPS toggle)
//! - [`primitives`]: Shared low-level drawing utilities
//!
//! # Architecture
//!
//! The widget system follows a compositional pattern where each cell draws:
//! 1. Background rectangle (with 2px inset for borders)
//! 2. Label text at top
//! 3. Main value in center
//! 4. Secondary info at bottom (max values, status, etc.)
//!
//! # Optimizations Applied
//!
//! ## Static Styles
//! All widgets use the optimizations from the [`styles`](crate::styles) module:
//! - Static `MonoTextStyle` constants (`LABEL_STYLE_WHITE`, `VALUE_STYLE_WHITE`)
//! - Static `TextStyle` constants (`CENTERED`, `LEFT_ALIGNED`, `RIGHT_ALIGNED`)
//! - `LABEL_FONT` reference for dynamic color styles
//! - `heapless::String` for all value formatting (no heap allocation)
//!
//! ## Pre-computed Constants
//! Pre-computed position constants are used in [`header`] and [`popups`]
//! to avoid per-frame calculations for fixed UI elements.
//!
//! ## Background Color Tracking
//! Cell drawing functions return the actual background color used after applying
//! blink effects and overrides. This return value is available for potential
//! future dirty-rect tracking or debugging. Note: smooth color fades are driven
//! by passing `bg_override: Some(ColorTransition::get_current())`, not by reading
//! returned colors. Cell backgrounds are always redrawn since values animate
//! continuously.
//!
//! ## Shake Animation Support
//! Cell drawing functions accept a `shake_offset` parameter for critical state
//! animation. See [`animations`](crate::animations) for offset calculation.

mod cells;
mod header;
mod popups;
mod primitives;

pub use cells::{
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_temp_cell,
    is_critical_afr,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};
pub use header::{draw_dividers, draw_header};
pub use popups::{draw_boost_unit_popup, draw_fps_toggle_popup, draw_reset_popup};
