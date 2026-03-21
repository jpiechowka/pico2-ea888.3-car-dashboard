#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

mod drivers;
mod profiling;
mod screens;
mod state;
mod tasks;
mod ui;
mod widgets;

mod config {
    pub use dashboard_pico2::config::*;
}
mod render {
    pub use dashboard_pico2::render::*;
}
mod thresholds {
    pub use dashboard_pico2::thresholds::*;
}

use core::sync::atomic::Ordering;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::bind_interrupts;
use embassy_rp::dma::InterruptHandler as DmaInterruptHandler;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::spi::Spi;
use embassy_time::{Duration, Instant};
use embedded_graphics::prelude::*;
use panic_probe as _;
use static_cell::StaticCell;

use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::drivers::{DoubleBuffer, St7789Flusher, St7789Renderer, display_spi_config, get_actual_spi_freq};
use crate::profiling as cpu_profiling;
use crate::render::{FpsMode, RenderState, cell_idx};
use crate::screens::{ProfilingData, clear_framebuffers, draw_logs_page, draw_profiling_page, run_boot_sequence};
use crate::state::{ButtonState, Page, Popup, SensorState, process_buttons};
use crate::tasks::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    CORE1_STACK,
    DEMO_VALUES,
    EXECUTOR_CORE1,
    FLUSH_BUFFER_IDX,
    FLUSH_DONE,
    FLUSH_SIGNAL,
    LAST_FLUSH_TIME_US,
    demo_values_task,
    display_flush_task,
};
use crate::thresholds::{
    AFR_LEAN_CRITICAL,
    AFR_OPTIMAL_MAX,
    AFR_RICH,
    AFR_RICH_AF,
    BATT_CRITICAL,
    BATT_WARNING,
    BOOST_EASTER_EGG_BAR,
    BOOST_EASTER_EGG_PSI,
    EGT_DANGER_MANIFOLD,
};
use crate::ui::{BLACK, BLUE, ColorTransition, DARK_TEAL, GREEN, ORANGE, RED};
use crate::widgets::{
    SensorDisplayData,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_danger_manifold_popup,
    draw_dividers,
    draw_fps_toggle_popup,
    draw_header,
    draw_reset_popup,
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

#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2-dashboard"),
    embassy_rp::binary_info::rp_program_description!(c"OBD-II Dashboard for EA888.3 on PIM715 Display"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

pub use crate::drivers::{FRAMEBUFFER_A, FRAMEBUFFER_B};

#[cfg(any(
    all(
        feature = "cpu250-spi62-1v10",
        any(
            feature = "cpu280-spi70-1v30",
            feature = "cpu290-spi72-1v30",
            feature = "cpu300-spi75-1v30"
        )
    ),
    all(
        feature = "cpu280-spi70-1v30",
        any(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30")
    ),
    all(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30")
))]
compile_error!(
    "Only one overclock feature can be enabled at a time. Choose one of: cpu250-spi62-1v10, cpu280-spi70-1v30, \
     cpu290-spi72-1v30, cpu300-spi75-1v30"
);

fn read_vreg_voltage_mv() -> u32 {
    const VREG: *const u32 = 0x4010_000C as *const u32;
    let vreg_val = unsafe { core::ptr::read_volatile(VREG) };
    let vsel = (vreg_val >> 4) & 0x1F;
    550 + (vsel * 50)
}

const fn requested_voltage_mv() -> u32 {
    #[cfg(any(
        feature = "cpu280-spi70-1v30",
        feature = "cpu290-spi72-1v30",
        feature = "cpu300-spi75-1v30"
    ))]
    {
        1300
    }
    #[cfg(not(any(
        feature = "cpu280-spi70-1v30",
        feature = "cpu290-spi72-1v30",
        feature = "cpu300-spi75-1v30"
    )))]
    {
        1100
    }
}

const fn requested_cpu_mhz() -> u32 {
    #[cfg(feature = "cpu300-spi75-1v30")]
    {
        300
    }
    #[cfg(all(feature = "cpu290-spi72-1v30", not(feature = "cpu300-spi75-1v30")))]
    {
        290
    }
    #[cfg(all(
        feature = "cpu280-spi70-1v30",
        not(any(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30"))
    ))]
    {
        280
    }
    #[cfg(all(
        feature = "cpu250-spi62-1v10",
        not(any(
            feature = "cpu280-spi70-1v30",
            feature = "cpu290-spi72-1v30",
            feature = "cpu300-spi75-1v30"
        ))
    ))]
    {
        250
    }
    #[cfg(not(any(
        feature = "cpu250-spi62-1v10",
        feature = "cpu280-spi70-1v30",
        feature = "cpu290-spi72-1v30",
        feature = "cpu300-spi75-1v30"
    )))]
    {
        150
    }
}

fn core1_main(
    animation_start: Instant,
    cpu_freq_hz: u32,
    stack_top: u32,
) -> ! {
    cpu_profiling::init(cpu_freq_hz);
    let executor = EXECUTOR_CORE1.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(demo_values_task(animation_start, stack_top).unwrap());
    })
}

bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => DmaInterruptHandler<embassy_rp::peripherals::DMA_CH0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("OBD-II Dashboard starting...");

    let p = {
        #[cfg(any(
            feature = "cpu250-spi62-1v10",
            feature = "cpu280-spi70-1v30",
            feature = "cpu290-spi72-1v30",
            feature = "cpu300-spi75-1v30"
        ))]
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        #[allow(unused_mut)]
        let mut config = Config::default();

        #[cfg(feature = "cpu300-spi75-1v30")]
        {
            config.clocks = ClockConfig::system_freq(300_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_30;
            info!("Overclock: 300 MHz @ 1.30V (SPI 75 MHz)");
        }
        #[cfg(all(feature = "cpu290-spi72-1v30", not(feature = "cpu300-spi75-1v30")))]
        {
            config.clocks = ClockConfig::system_freq(290_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_30;
            info!("Overclock: 290 MHz @ 1.30V (SPI 72.5 MHz)");
        }
        #[cfg(all(
            feature = "cpu280-spi70-1v30",
            not(any(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30"))
        ))]
        {
            config.clocks = ClockConfig::system_freq(280_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_30;
            info!("Overclock: 280 MHz @ 1.30V (SPI 70 MHz)");
        }
        #[cfg(all(
            feature = "cpu250-spi62-1v10",
            not(any(
                feature = "cpu280-spi70-1v30",
                feature = "cpu290-spi72-1v30",
                feature = "cpu300-spi75-1v30"
            ))
        ))]
        {
            config.clocks = ClockConfig::system_freq(250_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_10;
            info!("Overclock: 250 MHz @ 1.10V (SPI 62.5 MHz)");
        }

        embassy_rp::init(config)
    };

    let cpu_freq_hz = requested_cpu_mhz() * 1_000_000;

    let animation_start = Instant::now();
    let stack = CORE1_STACK.init(embassy_rp::multicore::Stack::new());
    let stack_top = stack as *const _ as u32 + 8192 * 4;
    embassy_rp::multicore::spawn_core1(p.CORE1, stack, move || {
        core1_main(animation_start, cpu_freq_hz, stack_top);
    });

    cpu_profiling::init(cpu_freq_hz);

    let mut _led_r = Output::new(p.PIN_26, Level::High);
    let mut _led_g = Output::new(p.PIN_27, Level::High);
    let mut led_b = Output::new(p.PIN_28, Level::High);

    let cs = Output::new(p.PIN_17, Level::High);
    let dc = Output::new(p.PIN_16, Level::Low);
    let mut _backlight = Output::new(p.PIN_20, Level::High);

    let spi = Spi::new_txonly(p.SPI0, p.PIN_18, p.PIN_19, p.DMA_CH0, Irqs, display_spi_config());

    let mut flusher = St7789Flusher::new(spi, dc, cs);
    flusher.init().await;

    log_info!("Display initialized");

    let mut double_buffer = unsafe { DoubleBuffer::new() };

    clear_framebuffers(&mut flusher, &mut double_buffer).await;

    run_boot_sequence(&mut flusher, &mut double_buffer).await;

    static FLUSHER: StaticCell<St7789Flusher<'static>> = StaticCell::new();
    let flusher: &'static mut St7789Flusher<'static> = FLUSHER.init(flusher);

    spawner.spawn(display_flush_task(flusher).unwrap());
    info!("Display flush task spawned");

    let btn_a = Input::new(p.PIN_12, Pull::Up);
    let btn_b = Input::new(p.PIN_13, Pull::Up);
    let btn_x = Input::new(p.PIN_14, Pull::Up);
    let btn_y = Input::new(p.PIN_15, Pull::Up);

    let mut btn_a_state = ButtonState::new();
    let mut btn_b_state = ButtonState::new();
    let mut btn_x_state = ButtonState::new();
    let mut btn_y_state = ButtonState::new();

    info!("Buttons initialized!");

    let mut current_page = Page::Dashboard;
    let mut clear_frames_remaining: u8 = 2;
    let mut fps_mode = FpsMode::Off;
    let mut show_boost_psi = false;
    let mut active_popup: Option<Popup> = None;
    let mut prev_egt_danger_active = false;
    let mut reset_requested = false;

    let mut render_state = RenderState::new();
    let mut frame_count = 0u32;
    let mut current_fps = 0.0f32;
    let mut average_fps = 0.0f32;
    let mut fps_frame_count = 0u32;
    let mut fps_sample_count = 0u32;
    let mut fps_sum = 0.0f32;
    let mut last_fps_calc = Instant::now();

    let mut render_time_us = 0u32;
    let mut flush_time_us = 0u32;
    let mut total_frame_time_us = 0u32;
    let mut last_profile_log = Instant::now();

    let mut frame_cycles_used = 0u32;
    let mut cpu0_util_percent = 0u32;
    let mut cpu1_util_percent = 0u32;

    let mut flush_in_progress = false;

    let mut boost = 0.5f32;
    let mut oil_temp = 60.0f32;
    let mut water_temp = 88.0f32;
    let mut dsg_temp = 75.0f32;
    let mut iat_temp = 30.0f32;
    let mut egt_temp = 200.0f32;
    let mut batt_voltage = 12.0f32;
    let mut afr = 14.0f32;

    let mut oil_state = SensorState::new();
    let mut water_state = SensorState::new();
    let mut dsg_state = SensorState::new();
    let mut iat_state = SensorState::new();
    let mut egt_state = SensorState::new();
    let mut batt_state = SensorState::new();
    let mut afr_state = SensorState::new();

    let mut boost_max = 0.0f32;
    let mut oil_max = 0.0f32;
    let mut water_max = 0.0f32;
    let mut dsg_max = 0.0f32;
    let mut iat_max = f32::MIN;
    let mut egt_max = 0.0f32;
    let mut batt_min = f32::MAX;
    let mut batt_max = 0.0f32;

    log_info!("Main loop starting");

    let mut color_transitions = ColorTransition::new();

    let animation_start = Instant::now();

    let mut demo_receiver = DEMO_VALUES.dyn_receiver().unwrap();

    loop {
        let frame_start = Instant::now();
        let frame_cycles_start = cpu_profiling::read();

        let elapsed_ms = animation_start.elapsed().as_millis() as u32;
        let blink_on = (elapsed_ms / 200).is_multiple_of(2);

        let input = process_buttons(
            &mut btn_x_state,
            &mut btn_y_state,
            &mut btn_a_state,
            &mut btn_b_state,
            btn_x.is_low(),
            btn_y.is_low(),
            btn_a.is_low(),
            btn_b.is_low(),
            current_page,
            fps_mode,
        );

        if let Some(new_mode) = input.new_fps_mode {
            fps_mode = new_mode;
            clear_frames_remaining = 2;
            info!("FPS mode: {}", fps_mode.label());
        }
        if let Some(new_page) = input.new_page {
            current_page = new_page;
            clear_frames_remaining = 2;
            active_popup = None;
            fps_sample_count = 0;
            fps_sum = 0.0;
            average_fps = 0.0;
            log_info!(
                "Page: {}",
                match current_page {
                    Page::Dashboard => "Dashboard",
                    Page::Debug => "Debug",
                    Page::Logs => "Logs",
                }
            );
        }
        if input.boost_unit_toggled {
            show_boost_psi = !show_boost_psi;
            info!("Boost: {}", if show_boost_psi { "PSI" } else { "BAR" });
        }
        if input.reset_requested {
            reset_requested = true;
            info!("Reset requested");
        }
        if let Some(popup) = input.show_popup {
            active_popup = Some(popup);
        }

        if let Some(ref popup) = active_popup
            && popup.is_expired()
        {
            active_popup = None;
            clear_frames_remaining = 2;
        }

        let popup_kind = if active_popup.is_some() {
            active_popup.as_ref().map(Popup::kind)
        } else if prev_egt_danger_active {
            Some(3u8)
        } else {
            None
        };
        render_state.update_popup(popup_kind);

        if let Some(demo_values) = demo_receiver.try_get() {
            boost = demo_values.boost;
            oil_temp = demo_values.oil_temp;
            water_temp = demo_values.water_temp;
            dsg_temp = demo_values.dsg_temp;
            iat_temp = demo_values.iat_temp;
            egt_temp = demo_values.egt_temp;
            batt_voltage = demo_values.batt_voltage;
            afr = demo_values.afr;
        }

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
            afr_state.reset_average();
            afr_state.reset_graph();
            afr_state.reset_peak();

            boost_max = boost;
            oil_max = oil_temp;
            water_max = water_temp;
            dsg_max = dsg_temp;
            iat_max = iat_temp;
            egt_max = egt_temp;
            batt_min = batt_voltage;
            batt_max = batt_voltage;

            reset_requested = false;
            log_info!("Stats reset");
        }

        let show_boost_easter_egg = if show_boost_psi {
            boost * 14.5038 >= BOOST_EASTER_EGG_PSI
        } else {
            boost >= BOOST_EASTER_EGG_BAR
        };

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

        oil_state.update(oil_temp, oil_updated);
        water_state.update(water_temp, water_updated);
        dsg_state.update(dsg_temp, dsg_updated);
        iat_state.update(iat_temp, iat_updated);
        egt_state.update(egt_temp, egt_updated);
        batt_state.update(batt_voltage, batt_updated);
        afr_state.update(afr, false);

        fps_frame_count += 1;
        if last_fps_calc.elapsed() >= Duration::from_secs(1) {
            current_fps = fps_frame_count as f32 / last_fps_calc.elapsed().as_millis() as f32 * 1000.0;
            fps_frame_count = 0;
            last_fps_calc = Instant::now();

            fps_sample_count += 1;
            fps_sum += current_fps;
            average_fps = fps_sum / fps_sample_count as f32;
        }

        let egt_danger_active = egt_temp >= EGT_DANGER_MANIFOLD;

        let afr_target = if afr < AFR_RICH_AF {
            BLUE
        } else if afr < AFR_RICH {
            DARK_TEAL
        } else if afr < AFR_OPTIMAL_MAX {
            GREEN
        } else if afr <= AFR_LEAN_CRITICAL {
            ORANGE
        } else {
            RED
        };
        color_transitions.set_target(cell_idx::AFR, afr_target);

        let batt_target = if batt_voltage < BATT_CRITICAL {
            RED
        } else if batt_voltage < BATT_WARNING {
            ORANGE
        } else {
            BLACK
        };
        color_transitions.set_target(cell_idx::BATTERY, batt_target);

        let (water_target, _) = temp_color_water(water_temp);
        let (oil_target, _) = temp_color_oil_dsg(oil_temp);
        let (dsg_target, _) = temp_color_oil_dsg(dsg_temp);
        let (iat_target, _) = temp_color_iat(iat_temp);
        let (egt_target, _) = temp_color_egt(egt_temp);

        color_transitions.set_target(cell_idx::COOLANT, water_target);
        color_transitions.set_target(cell_idx::OIL, oil_target);
        color_transitions.set_target(cell_idx::DSG, dsg_target);
        color_transitions.set_target(cell_idx::IAT, iat_target);
        color_transitions.set_target(cell_idx::EGT, egt_target);

        color_transitions.update(Instant::now());

        let render_start = Instant::now();

        let buffer = unsafe { double_buffer.render_buffer() };
        let mut display = St7789Renderer::new(buffer);

        if render_state.is_first_frame() || render_state.popup_just_closed() || clear_frames_remaining > 0 {
            display.clear(BLACK).ok();
            render_state.mark_display_cleared();

            if render_state.popup_just_closed() && clear_frames_remaining == 0 {
                clear_frames_remaining = 1;
            } else {
                clear_frames_remaining = clear_frames_remaining.saturating_sub(1);
            }
        }

        match current_page {
            Page::Dashboard => {
                if render_state.check_header_dirty(fps_mode, current_fps, average_fps) {
                    draw_header(&mut display, fps_mode, current_fps, average_fps);
                }

                draw_boost_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    boost,
                    boost_max,
                    show_boost_psi,
                    show_boost_easter_egg,
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
                    Some(color_transitions.get_current(cell_idx::AFR)),
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
                    Some(color_transitions.get_current(cell_idx::BATTERY)),
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
                    Some(color_transitions.get_current(cell_idx::COOLANT)),
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
                    Some(color_transitions.get_current(cell_idx::OIL)),
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
                    Some(color_transitions.get_current(cell_idx::DSG)),
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
                    Some(color_transitions.get_current(cell_idx::IAT)),
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
                    Some(color_transitions.get_current(cell_idx::EGT)),
                );

                if render_state.need_dividers() {
                    draw_dividers(&mut display);
                    render_state.mark_dividers_drawn();
                }

                if let Some(ref popup) = active_popup {
                    match popup {
                        Popup::Reset(_) => draw_reset_popup(&mut display),
                        Popup::Fps(_) => draw_fps_toggle_popup(&mut display, fps_mode),
                        Popup::BoostUnit(_) => draw_boost_unit_popup(&mut display, show_boost_psi),
                    }
                } else if egt_danger_active {
                    draw_danger_manifold_popup(&mut display, blink_on);
                }
            }

            Page::Debug => {
                let mem_stats = crate::profiling::MemoryStats::collect();

                let requested_spi_hz = display_spi_config().frequency;
                let actual_spi_hz = get_actual_spi_freq(cpu_freq_hz);

                draw_profiling_page(
                    &mut display,
                    &ProfilingData {
                        current_fps,
                        average_fps,
                        frame_count,
                        render_time_us,
                        flush_time_us,
                        total_frame_time_us,
                        buffer_swaps: BUFFER_SWAPS.load(Ordering::Relaxed),
                        buffer_waits: BUFFER_WAITS.load(Ordering::Relaxed),
                        render_buffer_idx: double_buffer.render_idx(),
                        flush_buffer_idx: FLUSH_BUFFER_IDX.load(Ordering::Relaxed),
                        core0_stack_used_kb: if mem_stats.stack_used > 0 && mem_stats.stack_used < 1024 {
                            1
                        } else {
                            mem_stats.stack_used / 1024
                        },
                        core0_stack_total_kb: mem_stats.stack_total / 1024,
                        core1_stack_used_kb: crate::tasks::CORE1_STACK_USED_KB.load(Ordering::Relaxed),
                        core1_stack_total_kb: 32, // Fixed 32KB stack for core 1
                        static_ram_kb: mem_stats.static_ram / 1024,
                        ram_total_kb: mem_stats.ram_total / 1024,
                        cpu0_util_percent,
                        cpu1_util_percent,
                        frame_cycles: frame_cycles_used,
                        requested_cpu_mhz: requested_cpu_mhz(),
                        actual_cpu_mhz: cpu_freq_hz / 1_000_000,
                        requested_spi_mhz: requested_spi_hz / 1_000_000,
                        actual_spi_mhz: actual_spi_hz / 1_000_000,
                        requested_voltage_mv: requested_voltage_mv(),
                        actual_voltage_mv: read_vreg_voltage_mv(),
                    },
                );
            }

            Page::Logs => {
                draw_logs_page(&mut display);
            }
        }

        render_time_us = render_start.elapsed().as_micros() as u32;
        let frame_cycles_end = cpu_profiling::read();
        frame_cycles_used = cpu_profiling::elapsed(frame_cycles_start, frame_cycles_end);

        if flush_in_progress {
            FLUSH_DONE.wait().await;
            BUFFER_WAITS.fetch_add(1, Ordering::Relaxed);
        }

        let completed_idx = double_buffer.swap();
        BUFFER_SWAPS.fetch_add(1, Ordering::Relaxed);
        FLUSH_SIGNAL.signal(completed_idx);
        flush_in_progress = true;

        flush_time_us = LAST_FLUSH_TIME_US.load(Ordering::Relaxed);
        total_frame_time_us = frame_start.elapsed().as_micros() as u32;

        cpu0_util_percent = cpu_profiling::calc_util_percent(frame_cycles_used, total_frame_time_us);
        cpu1_util_percent = crate::tasks::CORE1_UTIL_PERCENT.load(Ordering::Relaxed);

        if last_profile_log.elapsed() >= Duration::from_secs(2) {
            info!(
                "PROFILE: render={}us flush={}us total={}us ({} FPS) CORE0: {}% CORE1: {}% swaps={} waits={}",
                render_time_us,
                flush_time_us,
                total_frame_time_us,
                current_fps as u32,
                cpu0_util_percent,
                cpu1_util_percent,
                BUFFER_SWAPS.load(Ordering::Relaxed),
                BUFFER_WAITS.load(Ordering::Relaxed)
            );
            last_profile_log = Instant::now();
        }

        prev_egt_danger_active = egt_danger_active;

        render_state.end_frame();
        frame_count = frame_count.wrapping_add(1);

        if (elapsed_ms / 1000).is_multiple_of(2) {
            led_b.set_low();
        } else {
            led_b.set_high();
        }
    }
}

fn to_display_data(state: &SensorState) -> SensorDisplayData<'_> {
    let (buffer, start_idx, count, min, max) = state.get_graph_data();
    SensorDisplayData {
        trend: state.get_trend(),
        is_new_peak: state.is_new_peak,
        graph_buffer: buffer,
        graph_buffer_size: crate::state::GRAPH_HISTORY_SIZE,
        graph_start_idx: start_idx,
        graph_count: count,
        graph_min: min,
        graph_max: max,
        average: state.get_average(),
    }
}
