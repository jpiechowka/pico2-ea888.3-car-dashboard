#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

mod drivers;
mod peripherals;
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

use dashboard_pico2::acquisition::{DashboardSnapshot, ResetRequests};
use dashboard_pico2::display::FrameBuffer;
use dashboard_pico2::display::pipeline::{FrameLease, FrameLifecycle};
use dashboard_pico2::display::telemetry::{CompletionRate, FrameTrace, RenderTrace};
use dashboard_pico2::input_model::{brightness_after, scroll_after};
use dashboard_pico2::sensor_state::SensorSnapshot;
use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::bind_interrupts;
use embassy_rp::dma::InterruptHandler as DmaInterruptHandler;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{self, InterruptHandler as I2cInterruptHandler};
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use embassy_rp::spi::Spi;
use embassy_time::{Duration, Instant};
use embedded_graphics::prelude::*;
use panic_probe as _;
use static_cell::{ConstStaticCell, StaticCell};

use crate::config::backlight::{PWM_TOP, brightness_compare, pwm_divider, pwm_frequency_hz};
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::drivers::{St7789Flusher, St7789Renderer, display_spi_config, get_actual_spi_freq};
use crate::peripherals::{ENCODER_EVENTS, encoder_task};
use crate::profiling as cpu_profiling;
use crate::render::{FpsMode, RenderState, cell_idx};
use crate::screens::{ProfilingData, draw_logs_page, draw_profiling_page, run_boot_sequence};
use crate::state::{ButtonState, Page, Popup, process_buttons};
use crate::tasks::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    COMPLETED_FRAMES,
    CORE1_STACK,
    DEMO_VALUES,
    EXECUTOR_CORE1,
    FLUSH_BUFFER_IDX,
    FLUSH_REQUESTS,
    demo_values_task,
    display_flush_task,
    wait_completed,
    wait_started,
};
use crate::thresholds::{
    AFR_LEAN_CRITICAL,
    AFR_OPTIMAL_MAX,
    AFR_RICH,
    AFR_RICH_AF,
    BATT_CRITICAL,
    BATT_WARNING,
    BOOST_EASTER_EGG_BAR,
    EGT_DANGER_MANIFOLD,
};
use crate::ui::{BLACK, BLUE, ColorTransition, DARK_TEAL, GREEN, ORANGE, RED};
use crate::widgets::{
    SensorDisplayData,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_brightness_popup,
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

fn vreg_setting_mv() -> Option<u32> {
    // HAL validates the RP2350 VSEL encoding. This is a setting, not an ADC reading.
    embassy_rp::clocks::core_voltage()
        .ok()
        .map(|voltage| 550 + u32::from(voltage as u8) * 50)
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
    stack_bottom: u32,
    stack_top: u32,
) -> ! {
    cpu_profiling::init(cpu_freq_hz);
    let executor = EXECUTOR_CORE1.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(demo_values_task(animation_start, stack_bottom, stack_top).unwrap());
    })
}

bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => DmaInterruptHandler<embassy_rp::peripherals::DMA_CH0>;
    I2C0_IRQ => I2cInterruptHandler<embassy_rp::peripherals::I2C0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    log_info!("OBD-II Dashboard starting");

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
            log_info!("OC: 300MHz @ 1.30V SPI 75MHz");
        }
        #[cfg(all(feature = "cpu290-spi72-1v30", not(feature = "cpu300-spi75-1v30")))]
        {
            config.clocks = ClockConfig::system_freq(290_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_30;
            log_info!("OC: 290MHz @ 1.30V SPI 72MHz");
        }
        #[cfg(all(
            feature = "cpu280-spi70-1v30",
            not(any(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30"))
        ))]
        {
            config.clocks = ClockConfig::system_freq(280_000_000).expect("Invalid overclock frequency");
            config.clocks.core_voltage = CoreVoltage::V1_30;
            log_info!("OC: 280MHz @ 1.30V SPI 70MHz");
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
            log_info!("OC: 250MHz @ 1.10V SPI 62MHz");
        }

        embassy_rp::init(config)
    };

    let watchdog = crate::drivers::watchdog::RecoveryWatchdog::start(p.WATCHDOG);
    let cpu_freq_hz = embassy_rp::clocks::clk_sys_freq();

    let animation_start = Instant::now();
    let stack = CORE1_STACK.init(embassy_rp::multicore::Stack::new());
    let stack_bottom = stack.mem.as_ptr() as u32;
    let stack_top = stack_bottom
        .checked_add(stack.mem.len() as u32)
        .expect("invalid core1 stack bounds");
    embassy_rp::multicore::spawn_core1(p.CORE1, stack, move || {
        core1_main(animation_start, cpu_freq_hz, stack_bottom, stack_top);
    });

    cpu_profiling::init(cpu_freq_hz);

    let mut _led_r = Output::new(p.PIN_26, Level::High);
    let mut _led_g = Output::new(p.PIN_27, Level::High);
    let mut led_b = Output::new(p.PIN_28, Level::High);

    let cs = Output::new(p.PIN_17, Level::High);
    let dc = Output::new(p.PIN_16, Level::Low);

    // Backlight via PWM for brightness control (GP20 = PWM slice 2, channel A)
    let backlight_divider = pwm_divider(cpu_freq_hz).expect("Unsupported backlight PWM clock");
    let mut pwm_config = PwmConfig::default();
    pwm_config.top = PWM_TOP;
    pwm_config.divider = backlight_divider.into();
    pwm_config.compare_a = PWM_TOP + 1;
    let mut backlight_pwm = Pwm::new_output_a(p.PWM_SLICE2, p.PIN_20, pwm_config.clone());
    log_info!(
        "Configured CPU {} Hz, backlight PWM {} Hz",
        cpu_freq_hz,
        pwm_frequency_hz(cpu_freq_hz, backlight_divider)
    );

    let spi = Spi::new_txonly(p.SPI0, p.PIN_18, p.PIN_19, p.DMA_CH0, Irqs, display_spi_config());

    let mut flusher = St7789Flusher::new(spi, dc, cs);
    flusher.init().await.expect("display initialization failed");

    log_info!("Display initialized");

    static FRAME_A: ConstStaticCell<FrameBuffer> = ConstStaticCell::new(FrameBuffer::new());
    static FRAME_B: ConstStaticCell<FrameBuffer> = ConstStaticCell::new(FrameBuffer::new());
    let [mut render_frame, spare_frame] = FrameLease::pair(FRAME_A.take(), FRAME_B.take());
    let mut spare_frame = Some(spare_frame);
    let mut lifecycle = FrameLifecycle::new();
    run_boot_sequence(&mut flusher, render_frame.buffer())
        .await
        .expect("boot display failed");
    spawner.spawn(crate::tasks::recovery::recovery_task(watchdog).unwrap());

    static FLUSHER: StaticCell<St7789Flusher<'static>> = StaticCell::new();
    let flusher: &'static mut St7789Flusher<'static> = FLUSHER.init(flusher);

    spawner.spawn(display_flush_task(flusher).unwrap());
    log_info!("Display flush task spawned");

    // I2C0 for STEMMA QT encoder (GP4=SDA, GP5=SCL, 400kHz)
    let mut i2c_config = i2c::Config::default();
    i2c_config.frequency = 400_000;
    let i2c = i2c::I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, i2c_config);

    // Spawn encoder task — only starts polling after init is fully complete
    spawner.spawn(encoder_task(i2c).unwrap());
    log_info!("Encoder task spawned");

    let btn_a = Input::new(p.PIN_12, Pull::Up);
    let btn_b = Input::new(p.PIN_13, Pull::Up);
    let btn_x = Input::new(p.PIN_14, Pull::Up);
    let btn_y = Input::new(p.PIN_15, Pull::Up);

    let mut btn_a_state = ButtonState::new();
    let mut btn_b_state = ButtonState::new();
    let mut btn_x_state = ButtonState::new();
    let mut btn_y_state = ButtonState::new();

    log_info!("Buttons initialized");

    let mut current_page = Page::Dashboard;
    let mut clear_frames_remaining: u8 = 2;
    let mut fps_mode = FpsMode::Off;
    let mut show_boost_psi = false;
    let mut active_popup: Option<Popup> = None;
    let mut prev_egt_danger_active = false;
    let mut reset_requested = false;

    let mut render_state = RenderState::new();
    let mut current_fps = 0.0f32;
    let mut average_fps = 0.0f32;
    let mut fps_meter = CompletionRate::new(Instant::now().as_micros(), COMPLETED_FRAMES.load(Ordering::Relaxed));
    let mut last_trace: Option<FrameTrace> = None;
    let mut last_profile_log = Instant::now();

    let mut frame_cycles_used = 0u32;
    let mut cpu0_util_percent = 0u32;
    let mut cpu1_util_percent = 0u32;

    let mut flush_in_progress = None;

    // Brightness state (PWM backlight, encoder-controlled)
    let mut brightness_percent: u32 = 100;
    let mut saved_brightness: u32 = 100; // for toggle on/off
    let mut backlight_off_pending = false; // deferred off: show popup first, then cut PWM
    let mut log_scroll_offset: i32 = 0;
    let mut prev_log_count: usize = 0; // for anchoring scroll when new logs arrive

    let mut measurements = DashboardSnapshot::EMPTY;
    let mut reset_requests = ResetRequests::default();

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
            log_info!("FPS: {}", fps_mode.label());
        }
        if let Some(new_page) = input.new_page {
            current_page = new_page;
            clear_frames_remaining = 2;
            // If backlight-off was deferred and popup is being cleared, apply PWM now
            if backlight_off_pending {
                pwm_config.compare_a = 0;
                backlight_pwm.set_config(&pwm_config);
                backlight_off_pending = false;
            }
            active_popup = None;
            fps_meter = CompletionRate::new(Instant::now().as_micros(), COMPLETED_FRAMES.load(Ordering::Relaxed));
            current_fps = 0.0;
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
            log_info!("Boost: {}", if show_boost_psi { "PSI" } else { "BAR" });
        }
        if input.reset_requested {
            reset_requested = true;
            log_info!("Stats reset requested");
        }
        if let Some(popup) = input.show_popup {
            // If a non-brightness popup replaces a pending backlight-off, apply it now
            if backlight_off_pending && !matches!(popup, Popup::Brightness(_, _)) {
                pwm_config.compare_a = 0;
                backlight_pwm.set_config(&pwm_config);
                backlight_off_pending = false;
            }
            active_popup = Some(popup);
        }

        // --- Encoder events (rotation + button press) ---
        let (enc_delta, enc_presses) = ENCODER_EVENTS.take();
        let enc_button = enc_presses % 2 == 1;

        match current_page {
            Page::Logs => {
                // Anchor scroll position when new log entries arrive while scrolled,
                // so the visible entries don't shift under the user.
                if let Ok(buf) = crate::profiling::LOG_BUFFER.try_lock() {
                    let current_count = buf.count();
                    if log_scroll_offset > 0 && current_count > prev_log_count {
                        log_scroll_offset = log_scroll_offset.saturating_add((current_count - prev_log_count) as i32);
                    }
                    prev_log_count = current_count;

                    // Clamp to valid range so excess rotations don't accumulate
                    let max_offset = current_count.saturating_sub(13);
                    log_scroll_offset = log_scroll_offset.clamp(0, max_offset as i32);
                }

                // Rotation scrolls logs
                if enc_delta != 0 {
                    log_scroll_offset = scroll_after(
                        log_scroll_offset,
                        enc_delta,
                        crate::profiling::LOG_ENTRIES.saturating_sub(13),
                    );
                }
                // Button does nothing on Logs page
            }
            _ => {
                // Rotation adjusts brightness in 5% steps
                if enc_delta != 0 {
                    let new_brightness = brightness_after(brightness_percent, enc_delta);
                    if new_brightness != brightness_percent {
                        brightness_percent = new_brightness;
                        saved_brightness = brightness_percent;
                        backlight_off_pending = false; // cancel any pending off
                        pwm_config.compare_a = brightness_compare(brightness_percent as u8);
                        backlight_pwm.set_config(&pwm_config);
                        active_popup = Some(Popup::Brightness(Instant::now(), brightness_percent));
                        clear_frames_remaining = 2;
                    }
                }
                // Button toggles backlight on/off
                if enc_button {
                    if brightness_percent > 0 {
                        // Turning off: defer PWM change until popup expires so user sees it
                        saved_brightness = brightness_percent;
                        brightness_percent = 0;
                        backlight_off_pending = true;
                        // Don't touch PWM yet — keep backlight on for the popup
                    } else {
                        // Turning on: apply immediately
                        brightness_percent = saved_brightness.max(5);
                        backlight_off_pending = false;
                        pwm_config.compare_a = brightness_compare(brightness_percent as u8);
                        backlight_pwm.set_config(&pwm_config);
                    }
                    active_popup = Some(Popup::Brightness(Instant::now(), brightness_percent));
                    clear_frames_remaining = 2;
                }
            }
        }

        // Reset log scroll when leaving Logs page
        if input.new_page.is_some() {
            log_scroll_offset = 0;
            prev_log_count = 0;
        }

        if let Some(ref popup) = active_popup
            && popup.is_expired()
        {
            // If a backlight-off was deferred and this is the Brightness popup expiring,
            // apply the PWM change now (don't apply on other popup types)
            if backlight_off_pending && matches!(popup, Popup::Brightness(_, _)) {
                pwm_config.compare_a = 0;
                backlight_pwm.set_config(&pwm_config);
                backlight_off_pending = false;
            }
            active_popup = None;
            clear_frames_remaining = 2;
        }

        if reset_requested {
            crate::tasks::demo::RESET_STATISTICS.signal(reset_requests.request());
            reset_requested = false;
        }
        if let Some(snapshot) = demo_receiver.try_changed()
            && measurements.accept(snapshot)
            && reset_requests.acknowledge(measurements.reset_generation)
        {
            clear_frames_remaining = 2;
            log_info!("Stats reset acknowledged");
            if current_page == Page::Dashboard {
                if backlight_off_pending {
                    pwm_config.compare_a = 0;
                    backlight_pwm.set_config(&pwm_config);
                    backlight_off_pending = false;
                }
                active_popup = Some(Popup::Reset(Instant::now()));
            }
        }
        let popup_kind = active_popup
            .as_ref()
            .map(Popup::kind)
            .or(if prev_egt_danger_active { Some(3) } else { None });
        render_state.update_popup(popup_kind);

        let [
            boost_state,
            oil_state,
            water_state,
            dsg_state,
            iat_state,
            egt_state,
            batt_state,
            afr_state,
        ] = &measurements.sensors;
        let boost = boost_state.value().unwrap_or(0.0);
        let oil_temp = oil_state.value().unwrap_or(0.0);
        let water_temp = water_state.value().unwrap_or(0.0);
        let dsg_temp = dsg_state.value().unwrap_or(0.0);
        let iat_temp = iat_state.value().unwrap_or(0.0);
        let egt_temp = egt_state.value().unwrap_or(0.0);
        let batt_voltage = batt_state.value().unwrap_or(0.0);
        let afr = afr_state.value().unwrap_or(0.0);
        // Presentation units never change this canonical measurement threshold.
        let show_boost_easter_egg = boost >= BOOST_EASTER_EGG_BAR;

        let now_ms = Instant::now().as_millis();

        let frame_count = COMPLETED_FRAMES.load(Ordering::Relaxed);
        if let Some(rate) = fps_meter
            .sample(Instant::now().as_micros(), frame_count)
            .expect("invalid completion rate")
        {
            current_fps = rate.current;
            average_fps = rate.average;
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
        let input_cycles = cpu_profiling::elapsed(frame_cycles_start, cpu_profiling::read());
        let mut render_work = cpu_profiling::RenderWork::new(cpu_freq_hz);

        let render_idx = render_frame.id().index();
        let token = lifecycle.begin(render_frame.id()).expect("render buffer is not free");
        let mut display = St7789Renderer::new(render_frame.buffer());

        if render_state.is_first_frame() || render_state.popup_just_closed() || clear_frames_remaining > 0 {
            display.clear(BLACK).ok();
            render_state.mark_display_cleared();

            if render_state.popup_just_closed() && clear_frames_remaining == 0 {
                clear_frames_remaining = 1;
            } else {
                clear_frames_remaining = clear_frames_remaining.saturating_sub(1);
            }
        }

        render_work.checkpoint().await;

        match current_page {
            Page::Dashboard => {
                if render_state.check_header_dirty(render_idx, fps_mode, current_fps, average_fps) {
                    draw_header(&mut display, fps_mode, current_fps, average_fps);
                }

                draw_boost_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    boost,
                    boost_state.maximum().unwrap_or(0.0),
                    show_boost_psi,
                    show_boost_easter_egg,
                    blink_on,
                    0,
                );
                render_work.checkpoint().await;

                draw_afr_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    afr,
                    &to_display_data(afr_state, now_ms),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::AFR)),
                );
                render_work.checkpoint().await;

                draw_batt_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    batt_voltage,
                    batt_state.minimum().unwrap_or(0.0),
                    batt_state.maximum().unwrap_or(0.0),
                    &to_display_data(batt_state, now_ms),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::BATTERY)),
                );
                render_work.checkpoint().await;

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "COOL",
                    water_temp,
                    water_state.maximum().unwrap_or(0.0),
                    &to_display_data(water_state, now_ms),
                    temp_color_water,
                    is_critical_water,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::COOLANT)),
                );
                render_work.checkpoint().await;

                draw_temp_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "OIL",
                    oil_temp,
                    oil_state.maximum().unwrap_or(0.0),
                    &to_display_data(oil_state, now_ms),
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    Some(is_low_temp_oil),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::OIL)),
                );
                render_work.checkpoint().await;

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "DSG",
                    dsg_temp,
                    dsg_state.maximum().unwrap_or(0.0),
                    &to_display_data(dsg_state, now_ms),
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::DSG)),
                );
                render_work.checkpoint().await;

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "IAT",
                    iat_temp,
                    iat_state.maximum().unwrap_or(0.0),
                    &to_display_data(iat_state, now_ms),
                    temp_color_iat,
                    is_critical_iat,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::IAT)),
                );
                render_work.checkpoint().await;

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "EGT",
                    egt_temp,
                    egt_state.maximum().unwrap_or(0.0),
                    &to_display_data(egt_state, now_ms),
                    temp_color_egt,
                    is_critical_egt,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::EGT)),
                );
                render_work.checkpoint().await;

                if render_state.need_dividers() {
                    draw_dividers(&mut display);
                    render_state.mark_dividers_drawn();
                }

                if let Some(ref popup) = active_popup {
                    match popup {
                        Popup::Reset(_) => draw_reset_popup(&mut display),
                        Popup::Fps(_) => draw_fps_toggle_popup(&mut display, fps_mode),
                        Popup::BoostUnit(_) => draw_boost_unit_popup(&mut display, show_boost_psi),
                        Popup::Brightness(_, pct) => draw_brightness_popup(&mut display, *pct),
                    }
                } else if egt_danger_active {
                    draw_danger_manifold_popup(&mut display, blink_on);
                }
            }

            Page::Debug => {
                let mem_stats = crate::profiling::MemoryStats::collect();

                let requested_spi_hz = display_spi_config().frequency;
                let actual_spi_hz = get_actual_spi_freq(embassy_rp::clocks::clk_peri_freq());

                draw_profiling_page(
                    &mut display,
                    &ProfilingData {
                        current_fps,
                        average_fps,
                        frame_count,
                        trace: last_trace,
                        brightness_percent,
                        buffer_swaps: BUFFER_SWAPS.load(Ordering::Relaxed),
                        buffer_waits: BUFFER_WAITS.load(Ordering::Relaxed),
                        render_buffer_idx: render_idx,
                        flush_buffer_idx: FLUSH_BUFFER_IDX.load(Ordering::Relaxed),
                        core0_stack_used_kb: if mem_stats.stack_used > 0 && mem_stats.stack_used < 1024 {
                            1
                        } else {
                            mem_stats.stack_used / 1024
                        },
                        core0_stack_total_kb: mem_stats.stack_total / 1024,
                        core1_stack_used_kb: crate::tasks::CORE1_STACK_USED_KB.load(Ordering::Relaxed),
                        core1_stack_total_kb: crate::tasks::CORE1_STACK_BYTES / 1024,
                        static_ram_kb: mem_stats.static_ram / 1024,
                        ram_total_kb: mem_stats.ram_total / 1024,
                        cpu0_util_percent,
                        cpu1_util_percent,
                        frame_cycles: frame_cycles_used,
                        requested_cpu_mhz: requested_cpu_mhz(),
                        configured_cpu_mhz: cpu_freq_hz / 1_000_000,
                        requested_spi_hz,
                        configured_spi_hz: actual_spi_hz,
                        requested_voltage_mv: requested_voltage_mv(),
                        vreg_setting_mv: vreg_setting_mv(),
                    },
                    &mut render_work,
                )
                .await;
            }

            Page::Logs => {
                draw_logs_page(&mut display, log_scroll_offset, &mut render_work).await;
            }
        }

        // Brightness popup overlay on Debug page (not Logs — encoder controls scroll there)
        if matches!(current_page, Page::Debug)
            && let Some(Popup::Brightness(_, pct)) = active_popup
        {
            draw_brightness_popup(&mut display, pct);
        }

        let render_end = Instant::now();
        let render_cycles = render_work.finish();
        frame_cycles_used = (u64::from(input_cycles) + render_cycles).min(u32::MAX as u64) as u32;

        lifecycle.ready(token).expect("render buffer is not owned");
        let wait_start = Instant::now();
        let next_frame = if let Some(expected) = flush_in_progress.take() {
            let completed = wait_completed(&mut lifecycle, expected).await;
            last_trace = Some(completed.trace);
            completed.lease
        } else {
            spare_frame.take().expect("missing spare framebuffer")
        };
        let blocked_us = wait_start.elapsed().as_micros();
        lifecycle.queue(token).expect("another frame is still in flight");
        let trace = RenderTrace {
            start_us: render_start.as_micros(),
            end_us: render_end.as_micros(),
            queued_us: Instant::now().as_micros(),
            blocked_us,
            sample_us: measurements
                .sensors
                .iter()
                .filter_map(|s| s.sampled_at_ms())
                .min()
                .and_then(|ms| ms.checked_mul(1000)),
            active_cycles: render_cycles,
        };
        let request = render_frame.submit(token, trace);
        assert!(FLUSH_REQUESTS.try_send(request).is_ok(), "unconsumed frame request");
        BUFFER_SWAPS.fetch_add(1, Ordering::Relaxed);
        wait_started(&mut lifecycle, token).await;
        if cfg!(feature = "serial-display") {
            let completed = wait_completed(&mut lifecycle, token).await;
            last_trace = Some(completed.trace);
            spare_frame = Some(completed.lease);
        } else {
            flush_in_progress = Some(token);
        }
        render_frame = next_frame;

        let total_frame_time_us = frame_start.elapsed().as_micros().min(u32::MAX as u64) as u32;

        cpu0_util_percent = cpu_profiling::calc_util_percent(frame_cycles_used, total_frame_time_us);
        cpu1_util_percent = crate::tasks::CORE1_UTIL_PERCENT.load(Ordering::Relaxed);

        if last_profile_log.elapsed() >= Duration::from_secs(2) {
            // Two log_info! lines to fit within 40-char message limit
            log_info!(
                "r={}us f={}us t={}us {}fps",
                last_trace.map_or(0, |t| t.render_us()),
                last_trace.map_or(0, |t| t.transfer_observed_us()),
                last_trace.map_or(0, |t| t.total_us()),
                current_fps as u32,
            );
            log_info!(
                "C0:{}.{}% C1:{}.{}% sw={} w={}",
                cpu0_util_percent / 10,
                cpu0_util_percent % 10,
                cpu1_util_percent / 10,
                cpu1_util_percent % 10,
                BUFFER_SWAPS.load(Ordering::Relaxed),
                BUFFER_WAITS.load(Ordering::Relaxed)
            );
            last_profile_log = Instant::now();
        }

        prev_egt_danger_active = egt_danger_active;

        render_state.end_frame();
        crate::tasks::recovery::RENDER_PROGRESS.fetch_add(1, Ordering::Relaxed);

        if (elapsed_ms / 1000).is_multiple_of(2) {
            led_b.set_low();
        } else {
            led_b.set_high();
        }
    }
}

fn to_display_data(
    state: &SensorSnapshot,
    now_ms: u64,
) -> SensorDisplayData<'_> {
    let (buffer, start_idx, count, min, max) = state.get_graph_data();
    SensorDisplayData {
        trend: state.get_trend_at(now_ms),
        is_new_peak: state.is_new_peak_at(now_ms),
        graph_buffer: buffer,
        graph_buffer_size: dashboard_pico2::sensor_state::GRAPH_HISTORY_SIZE,
        graph_start_idx: start_idx,
        graph_count: count,
        graph_min: min,
        graph_max: max,
        average: state.get_average(),
    }
}
