//! OBD-II Dashboard Simulator for Windows/Desktop.
//!
//! This is the simulator binary that runs on desktop platforms using
//! the embedded-graphics-simulator crate.

// Crate-level lints
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]

mod popup;
mod profiling;
mod screens;
mod state;
mod timing;
mod widgets;

use std::thread;
use std::time::Instant;

use dashboard_common::Page;
use dashboard_common::animations::{ColorTransition, calculate_shake_offset};
use dashboard_common::colors::{BLACK, ORANGE, RED};
use dashboard_common::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH};
use dashboard_common::profiling::DebugLog;
use dashboard_common::render::{RenderState, cell_idx};
use dashboard_common::thresholds::{
    BAR_TO_PSI,
    BATT_CRITICAL,
    BATT_WARNING,
    BOOST_EASTER_EGG_BAR,
    BOOST_EASTER_EGG_PSI,
};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics_simulator::sdl2::Keycode;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window};

use crate::popup::Popup;
use crate::profiling::ProfilingMetrics;
use crate::screens::{draw_debug_page, run_loading_screen, run_welcome_screen};
use crate::state::SensorState;
use crate::timing::FRAME_TIME;
use crate::widgets::{
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_dividers,
    draw_fps_toggle_popup,
    draw_header,
    draw_reset_popup,
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

fn main() {
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT));
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Leon Cupra OBD Sim", &output_settings);

    display.clear(BLACK).ok();
    window.update(&display);

    if !run_loading_screen(&mut display, &mut window) {
        return;
    }
    if !run_welcome_screen(&mut display, &mut window) {
        return;
    }

    // Main loop state
    let mut t = 0.0f32;
    let mut frame_count = 0u32;

    // Min/Max tracking
    let mut boost_max_bar = 0.0f32;
    let mut boost_max_psi = 0.0f32;
    let mut oil_temp_max = 0.0f32;
    let mut water_temp_max = 0.0f32;
    let mut dsg_temp_max = 0.0f32;
    let mut iat_temp_max = f32::MIN;
    let mut egt_temp_max = 0.0f32;
    let mut batt_min = f32::MAX;
    let mut batt_max = 0.0f32;

    // Sensor states
    let mut oil_state = SensorState::new();
    let mut water_state = SensorState::new();
    let mut dsg_state = SensorState::new();
    let mut iat_state = SensorState::new();
    let mut egt_state = SensorState::new();
    let mut batt_state = SensorState::new();
    let mut afr_state = SensorState::new();

    // UI state
    let mut active_popup: Option<Popup> = None;
    let mut show_fps = true;
    let mut last_fps_calc = Instant::now();
    let mut fps_frame_count = 0u32;
    let mut current_fps = 0.0f32;
    let mut show_boost_psi = false;
    let mut boost_cycle_count = 0u32;
    let mut boost_was_low = true;

    // Render state
    let mut render_state = RenderState::new();
    let mut color_transition = ColorTransition::new();
    let mut current_page = Page::default();
    let mut page_just_switched = false;
    let mut reset_requested = false;

    // Profiling
    let mut metrics = ProfilingMetrics::new();
    let mut debug_log = DebugLog::new();
    debug_log.push("System started");

    loop {
        let frame_start = Instant::now();

        // Handle events
        for ev in window.events() {
            match ev {
                SimulatorEvent::Quit => return,
                SimulatorEvent::KeyDown { keycode, repeat, .. } => {
                    if repeat {
                        continue;
                    }
                    match keycode {
                        Keycode::X if current_page == Page::Dashboard => {
                            show_fps = !show_fps;
                            active_popup = Some(Popup::Fps(Instant::now()));
                            debug_log.push(if show_fps { "FPS: ON" } else { "FPS: OFF" });
                        }
                        Keycode::Y => {
                            current_page = current_page.toggle();
                            page_just_switched = true;
                            active_popup = None;
                            debug_log.push(match current_page {
                                Page::Dashboard => "Page: Dashboard",
                                Page::Debug => "Page: Debug",
                            });
                        }
                        Keycode::A if current_page == Page::Dashboard => {
                            show_boost_psi = !show_boost_psi;
                            active_popup = Some(Popup::BoostUnit(Instant::now()));
                            debug_log.push(if show_boost_psi { "Boost: PSI" } else { "Boost: BAR" });
                        }
                        Keycode::B if current_page == Page::Dashboard => {
                            reset_requested = true;
                            active_popup = Some(Popup::Reset(Instant::now()));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Check popup expiration
        if let Some(ref popup) = active_popup
            && popup.is_expired()
        {
            active_popup = None;
        }

        // Update render state
        render_state.update_popup(active_popup.as_ref().map(Popup::kind));

        // Clear display when needed
        if render_state.is_first_frame() || render_state.popup_just_closed() || page_just_switched {
            display.clear(BLACK).ok();
            if page_just_switched {
                render_state.mark_display_cleared();
            }
        }

        // Generate fake sensor data
        let boost_max_target = if boost_cycle_count % 3 == 2 { 2.0 } else { 1.8 };
        let boost = boost_signal(t, 0.0, boost_max_target, 0.08);

        if boost < 0.3 {
            boost_was_low = true;
        } else if boost_was_low && boost > 1.5 {
            boost_was_low = false;
            boost_cycle_count = boost_cycle_count.wrapping_add(1);
        }

        let boost_psi = boost * BAR_TO_PSI;
        let boost_easter_egg_active = if show_boost_psi {
            boost_psi >= BOOST_EASTER_EGG_PSI
        } else {
            boost >= BOOST_EASTER_EGG_BAR
        };

        let oil_temp = fake_signal(t, 30.0, 115.0, 0.08);
        let water_temp = fake_signal(t, 30.0, 95.0, 0.10);
        let dsg_temp = fake_signal(t, 30.0, 115.0, 0.07);
        let iat_temp = fake_signal(t, -10.0, 70.0, 0.05);
        let egt_temp = fake_signal(t, 200.0, 900.0, 0.04);
        let batt_voltage = fake_signal(t, 10.0, 15.0, 0.06);
        let afr = fake_signal(t, 10.0, 18.0, 0.09);

        // Handle reset
        if reset_requested {
            oil_state.reset_average();
            oil_state.reset_graph();
            oil_state.reset_peak();
            water_state.reset_average();
            water_state.reset_graph();
            water_state.reset_peak();
            dsg_state.reset_average();
            dsg_state.reset_graph();
            dsg_state.reset_peak();
            iat_state.reset_average();
            iat_state.reset_graph();
            iat_state.reset_peak();
            egt_state.reset_average();
            egt_state.reset_graph();
            egt_state.reset_peak();
            batt_state.reset_average();
            batt_state.reset_graph();
            batt_state.reset_peak();
            afr_state.reset_graph();

            boost_max_bar = boost;
            boost_max_psi = boost_psi;
            oil_temp_max = oil_temp;
            water_temp_max = water_temp;
            dsg_temp_max = dsg_temp;
            iat_temp_max = iat_temp;
            egt_temp_max = egt_temp;
            batt_min = batt_voltage;
            batt_max = batt_voltage;

            debug_log.push("MIN/AVG/MAX Reset");
            reset_requested = false;
        }

        // Update min/max
        let oil_max_updated = oil_temp > oil_temp_max;
        let water_max_updated = water_temp > water_temp_max;
        let dsg_max_updated = dsg_temp > dsg_temp_max;
        let iat_max_updated = iat_temp > iat_temp_max;
        let egt_max_updated = egt_temp > egt_temp_max;
        let batt_max_updated = batt_voltage > batt_max || batt_voltage < batt_min;

        boost_max_bar = boost_max_bar.max(boost);
        boost_max_psi = boost_max_psi.max(boost_psi);
        oil_temp_max = oil_temp_max.max(oil_temp);
        water_temp_max = water_temp_max.max(water_temp);
        dsg_temp_max = dsg_temp_max.max(dsg_temp);
        iat_temp_max = iat_temp_max.max(iat_temp);
        egt_temp_max = egt_temp_max.max(egt_temp);
        batt_min = batt_min.min(batt_voltage);
        batt_max = batt_max.max(batt_voltage);

        // Update sensor states
        oil_state.update(oil_temp, oil_max_updated);
        water_state.update(water_temp, water_max_updated);
        dsg_state.update(dsg_temp, dsg_max_updated);
        iat_state.update(iat_temp, iat_max_updated);
        egt_state.update(egt_temp, egt_max_updated);
        batt_state.update(batt_voltage, batt_max_updated);
        afr_state.update(afr, false);

        metrics.peaks_detected += u32::from(oil_max_updated)
            + u32::from(water_max_updated)
            + u32::from(dsg_max_updated)
            + u32::from(iat_max_updated)
            + u32::from(egt_max_updated)
            + u32::from(batt_max_updated);

        // FPS calculation
        fps_frame_count += 1;
        if last_fps_calc.elapsed().as_secs() >= 1 {
            current_fps = fps_frame_count as f32 / last_fps_calc.elapsed().as_secs_f32();
            fps_frame_count = 0;
            last_fps_calc = Instant::now();
        }

        // Render based on current page
        match current_page {
            Page::Dashboard => {
                if render_state.check_header_dirty(show_fps, current_fps) {
                    draw_header(&mut display, show_fps, current_fps);
                    metrics.inc_header_redraws();
                }

                let blink_on = (frame_count / 6).is_multiple_of(2);

                // Update color transitions
                let (oil_target_bg, _) = temp_color_oil_dsg(oil_temp);
                let (coolant_target_bg, _) = temp_color_water(water_temp);
                let (dsg_target_bg, _) = temp_color_oil_dsg(dsg_temp);
                let (iat_target_bg, _) = temp_color_iat(iat_temp);
                let (egt_target_bg, _) = temp_color_egt(egt_temp);
                let batt_target_bg = if batt_voltage < BATT_CRITICAL {
                    RED
                } else if batt_voltage < BATT_WARNING {
                    ORANGE
                } else {
                    BLACK
                };

                color_transition.set_target(cell_idx::OIL, oil_target_bg);
                color_transition.set_target(cell_idx::COOLANT, coolant_target_bg);
                color_transition.set_target(cell_idx::DSG, dsg_target_bg);
                color_transition.set_target(cell_idx::IAT, iat_target_bg);
                color_transition.set_target(cell_idx::EGT, egt_target_bg);
                color_transition.set_target(cell_idx::BATTERY, batt_target_bg);

                let changed_cells = color_transition.update();
                metrics.color_transitions += changed_cells.count_ones();

                // Calculate shake offsets
                let oil_shake = calculate_shake_offset(frame_count, is_critical_oil_dsg(oil_temp));
                let coolant_shake = calculate_shake_offset(frame_count, is_critical_water(water_temp));
                let dsg_shake = calculate_shake_offset(frame_count, is_critical_oil_dsg(dsg_temp));
                let iat_shake = calculate_shake_offset(frame_count, is_critical_iat(iat_temp));
                let egt_shake = calculate_shake_offset(frame_count, is_critical_egt(egt_temp));
                let batt_shake = calculate_shake_offset(frame_count, batt_voltage < BATT_CRITICAL);

                // Draw cells
                let boost_max_display = if show_boost_psi { boost_max_psi } else { boost_max_bar };
                draw_boost_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    boost,
                    boost_max_display,
                    show_boost_psi,
                    boost_easter_egg_active,
                    blink_on,
                    0,
                );

                let afr_shake = calculate_shake_offset(frame_count, is_critical_afr(afr));
                draw_afr_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    afr,
                    &afr_state,
                    blink_on,
                    afr_shake,
                    None,
                );

                draw_batt_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    batt_voltage,
                    batt_min,
                    batt_max,
                    &batt_state,
                    blink_on,
                    batt_shake,
                    Some(color_transition.get_current(cell_idx::BATTERY)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "COOL",
                    water_temp,
                    water_temp_max,
                    &water_state,
                    temp_color_water,
                    is_critical_water,
                    blink_on,
                    coolant_shake,
                    Some(color_transition.get_current(cell_idx::COOLANT)),
                );

                draw_temp_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "OIL",
                    oil_temp,
                    oil_temp_max,
                    &oil_state,
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    blink_on,
                    oil_shake,
                    Some(color_transition.get_current(cell_idx::OIL)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "DSG",
                    dsg_temp,
                    dsg_temp_max,
                    &dsg_state,
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    blink_on,
                    dsg_shake,
                    Some(color_transition.get_current(cell_idx::DSG)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "IAT",
                    iat_temp,
                    iat_temp_max,
                    &iat_state,
                    temp_color_iat,
                    is_critical_iat,
                    blink_on,
                    iat_shake,
                    Some(color_transition.get_current(cell_idx::IAT)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "EGT",
                    egt_temp,
                    egt_temp_max,
                    &egt_state,
                    temp_color_egt,
                    is_critical_egt,
                    blink_on,
                    egt_shake,
                    Some(color_transition.get_current(cell_idx::EGT)),
                );

                if render_state.need_dividers() {
                    draw_dividers(&mut display);
                    render_state.mark_dividers_drawn();
                    metrics.inc_divider_redraws();
                }

                if let Some(ref popup) = active_popup {
                    match popup {
                        Popup::Reset(_) => draw_reset_popup(&mut display),
                        Popup::Fps(_) => draw_fps_toggle_popup(&mut display, show_fps),
                        Popup::BoostUnit(_) => draw_boost_unit_popup(&mut display, show_boost_psi),
                    }
                }

                metrics.inc_cell_draws(8);
            }

            Page::Debug => {
                draw_debug_page(&mut display, &metrics, &debug_log, current_fps);
            }
        }

        let render_time = frame_start.elapsed();
        render_state.end_frame();
        page_just_switched = false;

        window.update(&display);

        t += 0.05;
        frame_count = frame_count.wrapping_add(1);

        let pre_sleep = frame_start.elapsed();
        if pre_sleep < FRAME_TIME {
            thread::sleep(FRAME_TIME.checked_sub(pre_sleep).unwrap());
        }
        let sleep_time = frame_start.elapsed().checked_sub(pre_sleep).unwrap();

        metrics.record_frame(frame_start.elapsed(), render_time, sleep_time);
    }
}

fn fake_signal(
    t: f32,
    min: f32,
    max: f32,
    freq: f32,
) -> f32 {
    let normalized = (t * freq).sin().mul_add(0.5, 0.5);
    min + normalized * (max - min)
}

fn boost_signal(
    t: f32,
    min: f32,
    max: f32,
    freq: f32,
) -> f32 {
    let cycle = (t * freq) % std::f32::consts::TAU;
    let normalized = if cycle > 1.2 && cycle < 1.9 {
        1.0
    } else {
        (cycle).sin().mul_add(0.5, 0.5)
    };
    min + normalized * (max - min)
}
