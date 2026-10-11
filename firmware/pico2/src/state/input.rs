use dashboard_pico2::input_trace::Observation;
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
    pub observations: Option<Observation>,
    pub reset_observation: Option<Observation>,
    pub ignored: u32,
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

    let observations = [
        btn_x_state.observe(btn_x_pressed),
        btn_y_state.observe(btn_y_pressed),
        btn_a_state.observe(btn_a_pressed),
        btn_b_state.observe(btn_b_pressed),
    ];
    for (index, observation) in observations.into_iter().enumerate() {
        let Some(observation) = observation else {
            continue;
        };
        if index != 1 && current_page != Page::Dashboard {
            result.ignored += 1;
            continue;
        }
        if index == 3 {
            result.reset_observation = Some(observation);
        } else if !Observation::append(&mut result.observations, observation) {
            result.ignored += observation.events;
        }
    }
    if observations[0].is_some() && current_page == Page::Dashboard {
        let new_mode = current_fps_mode.next();
        result.new_fps_mode = Some(new_mode);
        result.show_popup = Some(Popup::Fps(Instant::now().as_millis()));
    }

    if observations[1].is_some() {
        let new_page = current_page.toggle();
        result.new_page = Some(new_page);
    }

    if observations[2].is_some() && current_page == Page::Dashboard {
        result.boost_unit_toggled = true;
        result.show_popup = Some(Popup::BoostUnit(Instant::now().as_millis()));
    }

    if observations[3].is_some() && current_page == Page::Dashboard {
        result.reset_requested = true;
    }

    result
}
