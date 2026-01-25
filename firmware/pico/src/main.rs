//! OBD-II Dashboard Firmware for Raspberry Pi Pico 2 (RP2350)
//!
//! Displays the OBD-II dashboard on the Pimoroni PIM715 Display Pack 2.8".

#![no_std]
#![no_main]

mod display;

use dashboard_common::SensorState;
use dashboard_common::colors::BLACK;
use dashboard_common::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use dashboard_common::render::RenderState;
use dashboard_common::widgets::{
    SensorDisplayData,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_dividers,
    draw_header,
    draw_temp_cell,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    is_low_temp_oil,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};
use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::Spi;
use embassy_time::Timer;
use embedded_graphics::prelude::*;
use {defmt_rtt as _, panic_probe as _};

use crate::display::{display_spi_config, init_display};

// Program metadata for `picotool info`
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2-dashboard"),
    embassy_rp::binary_info::rp_program_description!(c"OBD-II Dashboard for EA888.3 on PIM715 Display"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("OBD-II Dashboard starting...");

    let p = embassy_rp::init(Default::default());

    // Initialize RGB LED (active-low: Low = ON)
    // PIM715: Red=26, Green=27, Blue=28
    let mut led_r = Output::new(p.PIN_26, Level::High); // Off
    let mut led_g = Output::new(p.PIN_27, Level::High); // Off
    let mut led_b = Output::new(p.PIN_28, Level::High); // Off

    // Flash red to indicate startup
    led_r.set_low(); // Red ON
    Timer::after_millis(200).await;
    led_r.set_high(); // Red OFF

    // Initialize display pins
    // PIM715 pinout: CS=17, DC=16, CLK=18, MOSI=19, Backlight=20
    let cs = Output::new(p.PIN_17, Level::High);
    let dc = Output::new(p.PIN_16, Level::Low);
    let mut _backlight = Output::new(p.PIN_20, Level::High); // Turn on backlight

    // Initialize SPI (TX-only, display doesn't need MISO)
    let spi = Spi::new_blocking_txonly(p.SPI0, p.PIN_18, p.PIN_19, display_spi_config());

    // Initialize display (no reset pin on PIM715)
    let mut display = init_display(spi, cs, dc);

    info!("Display initialized!");

    // Flash green to indicate display init success
    led_g.set_low(); // Green ON
    Timer::after_millis(200).await;
    led_g.set_high(); // Green OFF

    // Clear display
    display.clear(BLACK).ok();

    // Render state
    let mut render_state = RenderState::new();
    let mut frame_count = 0u32;

    // Demo sensor values (initialized in loop)
    let mut boost: f32;
    let mut oil_temp: f32;
    let mut water_temp: f32;
    let mut dsg_temp: f32;
    let mut iat_temp: f32;
    let mut egt_temp: f32;
    let mut batt_voltage: f32;
    let mut afr: f32;

    // Sensor states
    let mut oil_state = SensorState::new();
    let mut water_state = SensorState::new();
    let mut dsg_state = SensorState::new();
    let mut iat_state = SensorState::new();
    let mut egt_state = SensorState::new();
    let mut batt_state = SensorState::new();
    let mut afr_state = SensorState::new();

    // Max tracking
    let mut boost_max = 0.0f32;
    let mut oil_max = 0.0f32;
    let mut water_max = 0.0f32;
    let mut dsg_max = 0.0f32;
    let mut iat_max = f32::MIN;
    let mut egt_max = 0.0f32;
    let mut batt_min = f32::MAX;
    let mut batt_max = 0.0f32;

    info!("Starting main loop...");

    loop {
        let blink_on = (frame_count / 30).is_multiple_of(2);

        // Animate demo values (simple sine wave simulation)
        let t = frame_count as f32 * 0.02;
        boost = 0.5 + 1.0 * libm::sinf(t * 0.5).abs();
        oil_temp = 85.0 + 20.0 * libm::sinf(t * 0.3);
        water_temp = 88.0 + 7.0 * libm::sinf(t * 0.4);
        dsg_temp = 75.0 + 25.0 * libm::sinf(t * 0.35);
        iat_temp = 30.0 + 20.0 * libm::sinf(t * 0.25);
        egt_temp = 400.0 + 300.0 * libm::sinf(t * 0.2).abs();
        batt_voltage = 13.5 + 1.0 * libm::sinf(t * 0.15);
        afr = 14.0 + 2.0 * libm::sinf(t * 0.45);

        // Update max values
        let oil_updated = oil_temp > oil_max;
        let water_updated = water_temp > water_max;
        let dsg_updated = dsg_temp > dsg_max;
        let iat_updated = iat_temp > iat_max;
        let egt_updated = egt_temp > egt_max;
        let batt_updated = batt_voltage > batt_max || batt_voltage < batt_min;

        boost_max = boost_max.max(boost);
        oil_max = oil_max.max(oil_temp);
        water_max = water_max.max(water_temp);
        dsg_max = dsg_max.max(dsg_temp);
        iat_max = iat_max.max(iat_temp);
        egt_max = egt_max.max(egt_temp);
        batt_min = batt_min.min(batt_voltage);
        batt_max = batt_max.max(batt_voltage);

        // Update sensor states
        oil_state.update(oil_temp, oil_updated);
        water_state.update(water_temp, water_updated);
        dsg_state.update(dsg_temp, dsg_updated);
        iat_state.update(iat_temp, iat_updated);
        egt_state.update(egt_temp, egt_updated);
        batt_state.update(batt_voltage, batt_updated);
        afr_state.update(afr, false);

        // Draw header
        if render_state.check_header_dirty(false, 0.0) {
            draw_header(&mut display, false, 0.0);
        }

        // Draw cells
        draw_boost_cell(
            &mut display,
            0,
            HEADER_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            boost,
            boost_max,
            false,
            false,
            blink_on,
            0,
        );

        draw_afr_cell(
            &mut display,
            COL_WIDTH,
            HEADER_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            afr,
            &to_display_data(&afr_state),
            blink_on,
            0,
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
            &to_display_data(&batt_state),
            blink_on,
            0,
            None,
        );

        draw_temp_cell(
            &mut display,
            COL_WIDTH * 3,
            HEADER_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            "COOL",
            water_temp,
            water_max,
            &to_display_data(&water_state),
            temp_color_water,
            is_critical_water,
            None::<fn(f32) -> bool>,
            blink_on,
            0,
            None,
        );

        draw_temp_cell(
            &mut display,
            0,
            HEADER_HEIGHT + ROW_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            "OIL",
            oil_temp,
            oil_max,
            &to_display_data(&oil_state),
            temp_color_oil_dsg,
            is_critical_oil_dsg,
            Some(is_low_temp_oil),
            blink_on,
            0,
            None,
        );

        draw_temp_cell(
            &mut display,
            COL_WIDTH,
            HEADER_HEIGHT + ROW_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            "DSG",
            dsg_temp,
            dsg_max,
            &to_display_data(&dsg_state),
            temp_color_oil_dsg,
            is_critical_oil_dsg,
            None::<fn(f32) -> bool>,
            blink_on,
            0,
            None,
        );

        draw_temp_cell(
            &mut display,
            COL_WIDTH * 2,
            HEADER_HEIGHT + ROW_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            "IAT",
            iat_temp,
            iat_max,
            &to_display_data(&iat_state),
            temp_color_iat,
            is_critical_iat,
            None::<fn(f32) -> bool>,
            blink_on,
            0,
            None,
        );

        draw_temp_cell(
            &mut display,
            COL_WIDTH * 3,
            HEADER_HEIGHT + ROW_HEIGHT,
            COL_WIDTH,
            ROW_HEIGHT,
            "EGT",
            egt_temp,
            egt_max,
            &to_display_data(&egt_state),
            temp_color_egt,
            is_critical_egt,
            None::<fn(f32) -> bool>,
            blink_on,
            0,
            None,
        );

        // Draw dividers
        if render_state.need_dividers() {
            draw_dividers(&mut display);
            render_state.mark_dividers_drawn();
        }

        render_state.end_frame();
        frame_count = frame_count.wrapping_add(1);

        // Toggle blue LED every 30 frames (~1 sec) to show loop is running
        if frame_count.is_multiple_of(30) {
            led_b.toggle();
        }

        // Target ~30 FPS
        Timer::after_millis(33).await;
    }
}

/// Convert SensorState to SensorDisplayData for rendering.
fn to_display_data(state: &SensorState) -> SensorDisplayData<'_> {
    let (buffer, start_idx, count, min, max) = state.get_graph_data();
    SensorDisplayData {
        trend: state.get_trend(),
        is_new_peak: state.is_new_peak,
        graph_buffer: buffer,
        graph_buffer_size: dashboard_common::sensor_state::GRAPH_HISTORY_SIZE,
        graph_start_idx: start_idx,
        graph_count: count,
        graph_min: min,
        graph_max: max,
        average: state.get_average(),
    }
}
