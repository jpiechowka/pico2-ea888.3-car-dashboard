use embassy_time::Instant;

use super::{ButtonState, Page, Popup};
use crate::render::FpsMode;

#[derive(Default)]
pub struct InputResult {
    pub new_fps_mode: Option<FpsMode>,
    pub new_page: Option<Page>,
    pub boost_unit_toggled: bool,
    pub reset_requested: bool,
    pub show_popup: Option<Popup>,
    pub clear_frames: bool,
    pub reset_fps_average: bool,
}

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

    if btn_x_state.just_pressed(btn_x_pressed) && current_page == Page::Dashboard {
        let new_mode = current_fps_mode.next();
        result.new_fps_mode = Some(new_mode);
        result.show_popup = Some(Popup::Fps(Instant::now()));
        result.clear_frames = true;
    }

    if btn_y_state.just_pressed(btn_y_pressed) {
        let new_page = current_page.toggle();
        result.new_page = Some(new_page);
        result.clear_frames = true;
        result.reset_fps_average = true;
    }

    if btn_a_state.just_pressed(btn_a_pressed) && current_page == Page::Dashboard {
        result.boost_unit_toggled = true;
        result.show_popup = Some(Popup::BoostUnit(Instant::now()));
    }

    if btn_b_state.just_pressed(btn_b_pressed) && current_page == Page::Dashboard {
        result.reset_requested = true;
        result.show_popup = Some(Popup::Reset(Instant::now()));
    }

    result
}
