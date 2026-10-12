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
mod view_config;

mod config {
    pub use dashboard_pico2::config::*;
}
mod render {
    pub use dashboard_pico2::render::*;
}
use core::sync::atomic::Ordering;

use dashboard_pico2::acquisition::{DashboardSnapshot, ResetRequests};
use dashboard_pico2::backlight::Backlight;
use dashboard_pico2::colors::RED;
use dashboard_pico2::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_pico2::display::FrameBuffer;
use dashboard_pico2::display::damage::{DamagePlan, PanelHistory};
use dashboard_pico2::display::pipeline::{FrameLease, FrameLifecycle};
use dashboard_pico2::display::telemetry::{CompletionRate, FrameTrace, RenderTrace};
use dashboard_pico2::settings::Settings;
use dashboard_pico2::settings::policy::Policy;
use dashboard_pico2::status_led::{PresentedLed, StatusLed};
use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::bind_interrupts;
use embassy_rp::dma::InterruptHandler as DmaInterruptHandler;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{self, InterruptHandler as I2cInterruptHandler};
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use embassy_rp::spi::Spi;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::prelude::*;
use panic_probe as _;
use static_cell::{ConstStaticCell, StaticCell};

use crate::config::backlight::{PWM_TOP, pwm_divider, pwm_frequency_hz};
use crate::drivers::{St7789Flusher, St7789Renderer, display_spi_config, get_actual_spi_freq};
use crate::peripherals::{ENCODER_EVENTS, encoder_task};
use crate::profiling as cpu_profiling;
use crate::screens::{ProfilingData, draw_logs_page, draw_profiling_page};
use crate::state::{ButtonState, Page, Popup, process_buttons};
use crate::tasks::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    COMPLETED_FRAMES,
    CORE1_STACK,
    DASHBOARD_VALUES,
    EXECUTOR_CORE1,
    FLUSH_BUFFER_IDX,
    FLUSH_REQUESTS,
    acquisition_task,
    display_flush_task,
    wait_completed,
    wait_started,
};
use crate::ui::{BLACK, ColorTransition};

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

fn apply_backlight(
    pwm: &mut Pwm<'_>,
    config: &mut PwmConfig,
    backlight: &Backlight,
) {
    if config.compare_a != backlight.compare() {
        config.compare_a = backlight.compare();
        pwm.set_config(config);
    }
}

fn apply_indicator(
    red: &mut Output<'_>,
    green: &mut Output<'_>,
    presentation: &PresentedLed,
    backlight: &Backlight,
) {
    let led = presentation.output(backlight.compare() != 0);
    // Active-low cathodes. Disable the other color before enabling a new one.
    if led == StatusLed::Red {
        green.set_high();
        red.set_low();
    } else {
        red.set_high();
        green.set_level(if led == StatusLed::Green {
            Level::Low
        } else {
            Level::High
        });
    }
}

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

fn data_mode() -> dashboard_pico2::samples::DataMode {
    if cfg!(feature = "demo") {
        dashboard_pico2::samples::DataMode::Demo
    } else {
        dashboard_pico2::samples::DataMode::Vehicle
    }
}

fn log_settings_error(error: &drivers::settings_flash::JournalError) {
    use dashboard_pico2::settings::Error as JournalError;
    use drivers::settings_flash::Error;
    match error {
        JournalError::Storage(Error::Hardware(cause)) => log_fault!("Flash: {:?}", cause),
        JournalError::Storage(Error::Identity(id)) => log_fault!("Unknown flash: {:06x}", id),
        JournalError::Storage(Error::DebugBuild) => log_info!("Debug settings: volatile"),
        _ => log_fault!("Settings: {:?}", error),
    }
}

#[cfg(all(feature = "demo", feature = "longan-rx12"))]
compile_error!("Select either demo or longan-rx12; vehicle transport never falls back to demo.");

fn core1_main(
    animation_start: Instant,
    cpu_freq_hz: u32,
    stack_bottom: u32,
    stack_top: u32,
    resources: tasks::acquisition::Resources,
) -> ! {
    cpu_profiling::init(cpu_freq_hz);
    let executor = EXECUTOR_CORE1.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(acquisition_task(animation_start, stack_bottom, stack_top, resources).unwrap());
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
    let reset_cause = watchdog.reset_label();
    log_info!("Previous reset: {}", reset_cause);
    let dashboard_configuration = view_config::initialize();
    log_info!("Dashboard config SHA256: {}", view_config::SHA256);
    if let Some(fault) = dashboard_configuration.fault {
        log_fault!("Configuration fallback: {:?}", fault);
    }
    let cpu_freq_hz = embassy_rp::clocks::clk_sys_freq();

    let mut settings_flash = drivers::settings_flash::SettingsFlash::new(p.FLASH);
    let (initial_settings, mut has_saved_settings, settings_enabled) = match settings_flash.load() {
        Ok(loaded) => (loaded.unwrap_or_default(), loaded.is_some(), true),
        Err(error) => {
            log_settings_error(&error);
            (Settings::default(), false, false)
        }
    };
    let initial_settings = initial_settings.for_boot();
    let mut settings_policy = if settings_enabled {
        Policy::new(initial_settings, Instant::now().as_millis())
    } else {
        Policy::disabled(initial_settings, Instant::now().as_millis())
    };
    let mut pending_save = None;
    let mut last_settings_status = settings_policy.status();
    log_info!("Settings: {}", last_settings_status.label(has_saved_settings));

    let animation_start = Instant::now();
    let stack = CORE1_STACK.init(embassy_rp::multicore::Stack::new());
    let stack_bottom = stack.mem.as_ptr() as u32;
    let stack_top = stack_bottom
        .checked_add(stack.mem.len() as u32)
        .expect("invalid core1 stack bounds");
    let acquisition_resources = tasks::acquisition::Resources {
        configuration: &dashboard_configuration.config,
        #[cfg(feature = "longan-rx12")]
        uart: drivers::longan::Resources {
            uart: p.UART0,
            tx: p.PIN_0,
            rx: p.PIN_1,
        },
    };
    embassy_rp::multicore::spawn_core1(p.CORE1, stack, move || {
        core1_main(
            animation_start,
            cpu_freq_hz,
            stack_bottom,
            stack_top,
            acquisition_resources,
        );
    });

    cpu_profiling::init(cpu_freq_hz);

    let mut led_r = Output::new(p.PIN_26, Level::High);
    let _led_b = Output::new(p.PIN_28, Level::High);
    let mut led_g = Output::new(p.PIN_27, Level::Low);

    let cs = Output::new(p.PIN_17, Level::High);
    let dc = Output::new(p.PIN_16, Level::Low);

    // Backlight via PWM for brightness control (GP20 = PWM slice 2, channel A)
    let mut backlight = Backlight::new();
    let backlight_divider = pwm_divider(cpu_freq_hz).expect("Unsupported backlight PWM clock");
    let mut pwm_config = PwmConfig::default();
    pwm_config.top = PWM_TOP;
    pwm_config.divider = backlight_divider.into();
    pwm_config.compare_a = backlight.compare();
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
    #[cfg(feature = "target-bench")]
    profiling::target_bench::run(render_frame.buffer());
    #[cfg(feature = "te-trace")]
    spawner.spawn(crate::tasks::te::observe_te(Input::new(p.PIN_21, Pull::None)).unwrap());
    let mut spare_frame = Some(spare_frame);
    let mut lifecycle = FrameLifecycle::new();
    let mut panel_history = PanelHistory::new();
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
    let mut fps_mode = initial_settings.fps();
    let mut show_boost_psi = initial_settings.boost_psi();
    let mut active_popup: Option<Popup> = None;
    let mut reset_requested = false;
    let mut input_tracker = dashboard_pico2::input_trace::InputTracker::new();
    let mut reset_input: Option<(dashboard_pico2::input_trace::Observation, u64)> = None;

    let mut dashboard_cache = DashboardCache::new();
    let mut current_fps = 0.0f32;
    let mut average_fps = 0.0f32;
    let mut fps_meter = CompletionRate::new(Instant::now().as_micros(), COMPLETED_FRAMES.load(Ordering::Relaxed));
    let mut last_trace: Option<FrameTrace> = None;
    let mut led_presentation = PresentedLed::new();
    let mut last_profile_log = Instant::now();

    let mut frame_cycles_used = 0u32;
    let mut cpu0_util_percent = 0u32;
    let mut cpu1_util_percent = 0u32;

    let mut flush_in_progress = None;

    let mut log_anchor: Option<u64> = None;

    let mut measurements = DashboardSnapshot::EMPTY;
    measurements.mode = data_mode();
    let mut reset_requests = ResetRequests::default();

    log_info!("Main loop starting");

    let mut color_transitions = ColorTransition::new();

    let animation_start = Instant::now();

    let mut measurement_receiver = DASHBOARD_VALUES.dyn_receiver().unwrap();
    #[cfg(feature = "longan-rx12")]
    let mut link_receiver = tasks::acquisition::VEHICLE_LINK.dyn_receiver().unwrap();
    #[cfg(feature = "longan-rx12")]
    let mut vehicle_link = None;
    #[cfg(not(feature = "longan-rx12"))]
    let vehicle_link = None;
    #[cfg(all(feature = "longan-rx12", feature = "frame-trace"))]
    let mut diagnostic_receiver = tasks::acquisition::VEHICLE_DIAGNOSTICS.dyn_receiver().unwrap();
    #[cfg(feature = "frame-trace")]
    let mut frame_window_receiver = tasks::flush::FRAME_WINDOWS.dyn_receiver().unwrap();

    loop {
        let frame_start = Instant::now();
        // Give the SAVING state one normal presentation before maintenance.
        // The immutable ticket also makes changes during that frame explicit.
        let save_this_frame = pending_save.is_some();
        let frame_cycles_start = cpu_profiling::read();

        let elapsed_ms = animation_start.elapsed().as_millis();
        let blink_on = dashboard_pico2::animations::blink_on(elapsed_ms);

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
        let consumed_us = Instant::now().as_micros();
        input_tracker.ignore(input.ignored);
        if let Some(observation) = input.observations {
            input_tracker.consume(observation, consumed_us);
        }
        if let Some(observation) = input.reset_observation {
            if let Some((old, consumed)) = &mut reset_input {
                if old.merge(observation) {
                    *consumed = consumed_us;
                } else {
                    input_tracker.ignore(observation.events);
                }
            } else {
                reset_input = Some((observation, consumed_us));
            }
        }

        if let Some(new_mode) = input.new_fps_mode {
            fps_mode = new_mode;
            log_info!("FPS: {}", fps_mode.label());
        }
        if let Some(new_page) = input.new_page {
            current_page = new_page;
            backlight.interrupt_feedback();
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
            backlight.interrupt_feedback();
            active_popup = Some(popup);
        }

        // --- Encoder events (rotation + button press) ---
        let encoder = ENCODER_EVENTS.take_observed();
        let (enc_delta, enc_presses) = (encoder.delta, encoder.presses);
        let mut motion_applied = false;
        let mut press_applied = false;

        match current_page {
            Page::Logs => {
                // Pin the visible end sequence; ring count stops growing at
                // capacity and cannot anchor a view during overwrites.
                if enc_delta != 0 {
                    let old_anchor = log_anchor;
                    crate::profiling::LOGS.scroll(&mut log_anchor, enc_delta);
                    motion_applied = old_anchor != log_anchor;
                }
                // Button does nothing on Logs page
            }
            Page::Dashboard | Page::Debug => {
                let effect = backlight.encoder(current_page, enc_delta, enc_presses, Instant::now().as_millis());
                motion_applied = effect.motion_applied;
                press_applied = effect.press_applied;
                if let Some(popup) = effect.popup {
                    active_popup = Some(popup);
                }
            }
        }
        for (observation, applied) in [(encoder.motion, motion_applied), (encoder.button, press_applied)] {
            if let Some(observation) = observation {
                if applied {
                    input_tracker.consume(observation, Instant::now().as_micros());
                } else {
                    input_tracker.ignore(observation.events);
                }
            }
        }

        // Reset log scroll when leaving Logs page
        if input.new_page.is_some() {
            log_anchor = None;
        }

        if let Some(ref popup) = active_popup
            && backlight.expire_popup(*popup, Instant::now().as_millis())
        {
            active_popup = None;
        }

        if reset_requested {
            crate::tasks::acquisition::RESET_STATISTICS.signal(reset_requests.request());
            reset_requested = false;
        }
        if let Some(snapshot) = measurement_receiver.try_changed()
            && measurements.accept(snapshot)
            && reset_requests.acknowledge(measurements.reset_generation)
        {
            log_info!("Stats reset acknowledged");
            if let Some((observation, consumed_us)) = reset_input.take() {
                if current_page == Page::Dashboard {
                    input_tracker.apply(observation, consumed_us, Instant::now().as_micros());
                } else {
                    input_tracker.ignore(observation.events);
                }
            }
            if current_page == Page::Dashboard {
                backlight.interrupt_feedback();
                active_popup = Some(Popup::Reset(Instant::now().as_millis()));
            }
        }
        let now_ms = Instant::now().as_millis();
        apply_backlight(&mut backlight_pwm, &mut pwm_config, &backlight);
        apply_indicator(&mut led_r, &mut led_g, &led_presentation, &backlight);
        // Brightness is runtime-only. Keep legacy journal fields canonical,
        // preserving saved boost/FPS without writing flash for every click.
        let desired = Settings::new(100, 100, show_boost_psi, fps_mode).expect("invalid live preferences");
        settings_policy.set(desired, now_ms).expect("settings clock regressed");
        if pending_save.is_none() {
            pending_save = settings_policy.begin(now_ms).expect("invalid save policy");
        }
        let settings_status = settings_policy.status();
        if settings_status != last_settings_status {
            log_info!("Settings: {}", settings_status.label(has_saved_settings));
            last_settings_status = settings_status;
        }
        #[cfg(feature = "longan-rx12")]
        if let Some(link) = link_receiver.try_changed() {
            vehicle_link = Some(link);
        }
        let vehicle_status = dashboard_pico2::vehicle_status::VehicleStatus::derive(
            cfg!(feature = "longan-rx12"),
            vehicle_link,
            &measurements,
            now_ms,
        );

        let frame_count = COMPLETED_FRAMES.load(Ordering::Relaxed);
        if let Some(rate) = fps_meter
            .sample(Instant::now().as_micros(), frame_count)
            .expect("invalid completion rate")
        {
            current_fps = rate.current;
            average_fps = rate.average;
        }

        let alerts = measurements.alerts_at(now_ms);
        let user_overlay = active_popup.as_ref().map(|popup| {
            match popup {
                Popup::Reset(_) => Overlay::Reset,
                Popup::Fps(_) => Overlay::Fps(fps_mode),
                Popup::BoostUnit(_) => Overlay::BoostUnit(show_boost_psi),
                Popup::Brightness(_, brightness) => Overlay::Brightness(*brightness),
            }
        });
        let visible_overlay = Overlay::for_page(current_page, user_overlay, alerts, blink_on);
        input_tracker.exclude_obscured(matches!(visible_overlay, Some(Overlay::Critical(..))));
        for (cell, sensor) in dashboard_pico2::dashboard::CELL_SENSORS.into_iter().enumerate() {
            let id = dashboard_pico2::samples::SensorId::ALL[sensor];
            color_transitions.set_target(cell, if alerts.active(id) { RED } else { BLACK }, now_ms);
        }

        color_transitions.update(now_ms);

        let render_start = Instant::now();
        let input_cycles = cpu_profiling::elapsed(frame_cycles_start, cpu_profiling::read());
        let mut render_work = cpu_profiling::RenderWork::new(cpu_freq_hz);

        let render_idx = render_frame.id().index();
        let token = lifecycle.begin(render_frame.id()).expect("render buffer is not free");
        panel_history.begin_render(token).expect("invalid render generation");
        let mut display = St7789Renderer::new(render_frame.buffer());

        match current_page {
            Page::Dashboard => {
                let scene = DashboardScene {
                    measurements: &measurements,
                    status: vehicle_status,
                    now_ms,
                    blink_on,
                    boost_psi: show_boost_psi,
                    fps_mode,
                    fps_instant: current_fps,
                    fps_average: average_fps,
                    backgrounds: core::array::from_fn(|cell| color_transitions.get_current(cell)),
                    overlay: user_overlay,
                };
                let redraw = dashboard_cache.prepare(render_idx, &scene);
                redraw.begin(&mut display, &scene);
                render_work.checkpoint().await;
                for cell in 0..8 {
                    if redraw.cells & (1 << cell) != 0 {
                        scene.draw_cell(&mut display, cell);
                        render_work.checkpoint().await;
                    }
                }
                redraw.finish(&mut display, &scene);
            }

            Page::Debug => {
                dashboard_cache.invalidate(render_idx);
                display.clear(BLACK).ok();
                render_work.checkpoint().await;
                let mem_stats = crate::profiling::MemoryStats::collect();

                let requested_spi_hz = display_spi_config().frequency;
                let actual_spi_hz = get_actual_spi_freq(embassy_rp::clocks::clk_peri_freq());

                draw_profiling_page(
                    &mut display,
                    &ProfilingData {
                        current_fps,
                        reset_cause,
                        vehicle_status: vehicle_status.label(),
                        average_fps,
                        frame_count,
                        trace: last_trace,
                        brightness: backlight.brightness(),
                        settings_status: settings_status.label(has_saved_settings),
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
                dashboard_cache.invalidate(render_idx);
                display.clear(BLACK).ok();
                render_work.checkpoint().await;
                draw_logs_page(&mut display, log_anchor, &mut render_work).await;
            }
        }

        if current_page != Page::Dashboard
            && let Some(overlay) = visible_overlay
        {
            overlay.draw(&mut display);
        }

        let render_end = Instant::now();
        let render_cycles = render_work.finish();
        frame_cycles_used = (u64::from(input_cycles) + render_cycles).min(u32::MAX as u64) as u32;

        lifecycle.ready(token).expect("render buffer is not owned");
        let wait_start = Instant::now();
        let next_frame = if let Some(expected) = flush_in_progress.take() {
            let completed = wait_completed(&mut lifecycle, expected).await;
            assert!(led_presentation.complete(expected), "wrong indicator completion");
            backlight.frame_completed(expected, Instant::now().as_millis());
            apply_backlight(&mut backlight_pwm, &mut pwm_config, &backlight);
            apply_indicator(&mut led_r, &mut led_g, &led_presentation, &backlight);
            panel_history
                .complete(expected)
                .expect("invalid completed panel generation");
            last_trace = Some(completed.trace);
            completed.lease
        } else {
            spare_frame.take().expect("missing spare framebuffer")
        };
        let blocked_us = wait_start.elapsed().as_micros();
        crate::tasks::recovery::DISPLAY_PENDING.store(false, Ordering::Relaxed);
        if save_this_frame {
            input_tracker.maintenance_gap();
            tasks::flush::publish_input_counts(input_tracker.counters());
            let ticket = pending_save.take().expect("missing save ticket");
            // Both affine leases are owned here and the matching completion
            // has returned. Queue no display work until all owners resume.
            crate::tasks::recovery::RENDER_PROGRESS.fetch_add(1, Ordering::Relaxed);
            let result = tasks::maintenance::run(|permit| settings_flash.save(ticket.settings(), permit)).await;
            settings_policy
                .finish(ticket, result.is_ok(), Instant::now().as_millis())
                .expect("invalid save completion");
            match result {
                Ok(()) => {
                    has_saved_settings = true;
                    log_info!("Settings saved and verified");
                }
                Err(tasks::maintenance::SaveError::Preparation(error)) => log_fault!("Save preparation: {:?}", error),
                Err(tasks::maintenance::SaveError::Operation(error)) => log_settings_error(&error),
            }
            btn_a_state = ButtonState::new();
            btn_b_state = ButtonState::new();
            btn_x_state = ButtonState::new();
            btn_y_state = ButtonState::new();
            // Render again with current wall-clock ages and post-park link
            // quality. Never present this pre-maintenance sensor snapshot.
            lifecycle.discard(token).expect("maintenance retained a live frame");
            dashboard_cache.invalidate(render_idx);
            spare_frame = Some(next_frame);
            continue;
        }
        let reference = panel_history
            .reference()
            .filter(|reference| reference.buffer == next_frame.id());
        let plan = if cfg!(feature = "full-frame") || reference.is_none() {
            DamagePlan::full()
        } else {
            DamagePlan::between(next_frame.pixels(), render_frame.pixels())
        };
        let idle = plan.is_empty();
        if idle {
            input_tracker.no_damage();
        }
        tasks::flush::publish_input_counts(input_tracker.counters());
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
            input: input_tracker.pending(),
        };
        if idle {
            lifecycle.discard(token).expect("cannot discard active frame");
            assert!(led_presentation.unchanged(visible_overlay), "pending indicator at idle");
            backlight.frame_unchanged(Instant::now().as_millis());
            apply_backlight(&mut backlight_pwm, &mut pwm_config, &backlight);
            apply_indicator(&mut led_r, &mut led_g, &led_presentation, &backlight);
            spare_frame = Some(next_frame);
            crate::tasks::recovery::DISPLAY_PENDING.store(false, Ordering::Relaxed);
            // Keep the completed reference unchanged and reuse this render buffer.
            // A timed suspension prevents a stationary scene from busy-spinning.
            Timer::after_millis(16).await;
        } else {
            panel_history
                .queue(token, if plan.is_full() { None } else { reference })
                .expect("partial update does not match panel generation");
            lifecycle.queue(token).expect("another frame is still in flight");
            crate::tasks::recovery::DISPLAY_PENDING.store(true, Ordering::Relaxed);
            let request = render_frame.submit_plan(token, trace, plan);
            assert!(
                led_presentation.submit(token, visible_overlay),
                "pending indicator submission"
            );
            backlight.frame_submitted(token);
            input_tracker.submitted();
            assert!(FLUSH_REQUESTS.try_send(request).is_ok(), "unconsumed frame request");
            BUFFER_SWAPS.fetch_add(1, Ordering::Relaxed);
            wait_started(&mut lifecycle, token).await;
            if cfg!(feature = "serial-display") {
                let completed = wait_completed(&mut lifecycle, token).await;
                assert!(led_presentation.complete(token), "wrong serial indicator completion");
                backlight.frame_completed(token, Instant::now().as_millis());
                apply_backlight(&mut backlight_pwm, &mut pwm_config, &backlight);
                apply_indicator(&mut led_r, &mut led_g, &led_presentation, &backlight);
                panel_history.complete(token).expect("invalid serial panel generation");
                crate::tasks::recovery::DISPLAY_PENDING.store(false, Ordering::Relaxed);
                last_trace = Some(completed.trace);
                spare_frame = Some(completed.lease);
            } else {
                flush_in_progress = Some(token);
            }
            render_frame = next_frame;
        }

        let total_frame_time_us = frame_start.elapsed().as_micros().min(u32::MAX as u64) as u32;

        cpu0_util_percent = cpu_profiling::calc_util_percent(frame_cycles_used, total_frame_time_us);
        cpu1_util_percent = crate::tasks::CORE1_UTIL_PERCENT.load(Ordering::Relaxed);

        if last_profile_log.elapsed() >= Duration::from_secs(2) {
            #[cfg(feature = "frame-trace")]
            if let Some(report) = frame_window_receiver.try_changed() {
                profiling::telemetry_export::frames(report);
            }
            #[cfg(all(feature = "longan-rx12", feature = "frame-trace"))]
            if let Some((sequence, published_ms, snapshot)) = diagnostic_receiver.try_changed() {
                profiling::telemetry_export::diagnostics(sequence, published_ms, snapshot);
            }
            #[cfg(feature = "te-trace")]
            crate::tasks::te::log_status();
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

        crate::tasks::recovery::RENDER_PROGRESS.fetch_add(1, Ordering::Relaxed);

        apply_indicator(&mut led_r, &mut led_g, &led_presentation, &backlight);
    }
}
