//! Input handling for button events.
//!
//! Processes button presses and converts them to UI actions like
//! page navigation, FPS mode toggling, and statistics reset.

use embassy_time::Instant;

use super::{ButtonState, Page, Popup};
use crate::render::FpsMode;

/// Result of processing button inputs for a single frame.
#[derive(Default)]
pub struct InputResult {
    /// New FPS mode if X was pressed (Dashboard only)
    pub new_fps_mode: Option<FpsMode>,
    /// New page if Y was pressed
    pub new_page: Option<Page>,
    /// Whether boost unit was toggled (A pressed on Dashboard)
    pub boost_unit_toggled: bool,
    /// Whether reset was requested (B pressed on Dashboard)
    pub reset_requested: bool,
    /// Popup to show (if any button triggered one)
    pub show_popup: Option<Popup>,
    /// Whether we need to clear frames (page switch, FPS toggle)
    pub clear_frames: bool,
    /// Whether to reset FPS averaging (page switch)
    pub reset_fps_average: bool,
}

/// Process button inputs and return the resulting actions.
///
/// # Arguments
///
/// * `btn_x_state` - X button debounce state
/// * `btn_y_state` - Y button debounce state
/// * `btn_a_state` - A button debounce state
/// * `btn_b_state` - B button debounce state
/// * `btn_x_pressed` - Whether X button is currently pressed (low)
/// * `btn_y_pressed` - Whether Y button is currently pressed (low)
/// * `btn_a_pressed` - Whether A button is currently pressed (low)
/// * `btn_b_pressed` - Whether B button is currently pressed (low)
/// * `current_page` - Current page being displayed
/// * `current_fps_mode` - Current FPS display mode
///
/// # Returns
///
/// An `InputResult` containing all the actions to take based on button presses.
#[allow(clippy::too_many_arguments)]
pub fn process_buttons(
    btn_x_state: &mut ButtonState,
    btn_y_state: &mut ButtonState,
    btn_a_state: &mut ButtonState,
    btn_b_state: &mut ButtonState,
    btn_x_pressed: bool,
    btn_y_pressed: bool,
    btn_a_pressed: bool,
    btn_b_pressed: bool,
    current_page: Page,
    current_fps_mode: FpsMode,
) -> InputResult {
    let mut result = InputResult::default();

    // X button: Cycle FPS display mode (Dashboard only)
    if btn_x_state.just_pressed(btn_x_pressed) && current_page == Page::Dashboard {
        let new_mode = current_fps_mode.next();
        result.new_fps_mode = Some(new_mode);
        result.show_popup = Some(Popup::Fps(Instant::now()));
        result.clear_frames = true;
    }

    // Y button: Cycle through pages
    if btn_y_state.just_pressed(btn_y_pressed) {
        let new_page = current_page.toggle();
        result.new_page = Some(new_page);
        result.clear_frames = true;
        result.reset_fps_average = true;
    }

    // A button: Toggle boost unit BAR/PSI (Dashboard only)
    if btn_a_state.just_pressed(btn_a_pressed) && current_page == Page::Dashboard {
        result.boost_unit_toggled = true;
        result.show_popup = Some(Popup::BoostUnit(Instant::now()));
    }

    // B button: Reset min/max/avg statistics (Dashboard only)
    if btn_b_state.just_pressed(btn_b_pressed) && current_page == Page::Dashboard {
        result.reset_requested = true;
        result.show_popup = Some(Popup::Reset(Instant::now()));
    }

    result
}
