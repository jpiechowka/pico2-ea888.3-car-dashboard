use core::sync::atomic::Ordering;

use dashboard_core::ble::runtime::Liveness;
use dashboard_core::discovery::{Input as UiInput, Probe, RadioPhase};
use dashboard_core::display::St7789Renderer;
use dashboard_core::display::damage::DamagePlan;
use dashboard_core::display::telemetry::CompletionRate;
use dashboard_core::display::transfer::flush_plan;
use dashboard_core::health::{Progress, ProgressMonitor};
use dashboard_core::input_model::DebouncedButton;
use dashboard_pico2::panel::St7789Flusher;
use dashboard_pico2::recovery_watchdog::RecoveryWatchdog;
use dashboard_pico2::seesaw;
use embassy_rp::Peri;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::I2c;
use embassy_rp::peripherals::*;
use embassy_rp::spi::{Async, Spi};
use embassy_time::{Instant, Timer};
use static_cell::StaticCell;

use crate::{bridge, metrics, radio};

#[embassy_executor::task]
pub async fn probe_radio_task(resources: radio::Resources) { metrics::measure(0, radio::run(resources)).await; }

#[embassy_executor::task]
pub async fn probe_ui_task() {
    metrics::measure(1, async {
        static MODEL: StaticCell<Probe> = StaticCell::new();
        let model = MODEL.init_with(Probe::new);
        let mut last_frame = 0;
        let start = metrics::snapshot();
        let mut fps_meter = CompletionRate::new(start.at_us, start.completed);
        let mut fps = None;
        loop {
            let snapshot = metrics::snapshot();
            let now = snapshot.at_us / 1000;
            match fps_meter.sample(snapshot.at_us, snapshot.completed) {
                Ok(Some(value)) => fps = Some(value),
                Ok(None) => {}
                Err(_) => {
                    defmt::warn!("probe.metrics invalid completion timeline; restart FPS window");
                    fps = None;
                    fps_meter = CompletionRate::new(snapshot.at_us, snapshot.completed);
                }
            }
            model.diagnostics = snapshot.values;
            model.diagnostics.fps = fps;
            // Controls precede observations, so cancellation immediately changes the
            // accepted generation. RF pressure cannot starve input or frame updates.
            for _ in 0..16 {
                let Ok(input) = bridge::INPUTS.try_receive() else {
                    break;
                };
                if matches!(input, UiInput::Recover) && model.phase == RadioPhase::Fault {
                    bridge::RESET_REQUESTED.store(true, Ordering::Relaxed);
                }
                if let Some(request) = model.input(input, now) {
                    bridge::set_request(request);
                }
                model.page = model
                    .page
                    .min(dashboard_core::discovery::page_count(model).saturating_sub(1));
            }
            for _ in 0..8 {
                match bridge::EVENTS.try_receive() {
                    Ok(event) => model.event(event),
                    Err(_) => break,
                }
            }
            for _ in 0..32 {
                match bridge::REPORTS.try_receive() {
                    Ok(event) => model.event(event),
                    Err(_) => break,
                }
            }
            model.dropped_reports = bridge::DROPPED_REPORTS.load(Ordering::Relaxed);
            model.malformed_reports = bridge::RADIO_MALFORMED.load(Ordering::Relaxed);
            model.encoder_connected = bridge::ENCODER_CONNECTED.load(Ordering::Relaxed);
            // Only accepted-generation state drives the LED, including local errors.
            bridge::PHASE.store(model.phase as u8, Ordering::Relaxed);
            if now.saturating_sub(last_frame) >= 50
                && let Ok(frame) = bridge::FREE.try_receive()
            {
                let mut renderer = St7789Renderer::new(frame);
                let _ = dashboard_core::discovery::render(&mut renderer, model, now);
                // Two leases exist; READY cannot be full while this task owns one.
                if let Err(embassy_sync::channel::TrySendError::Full(frame)) = bridge::READY.try_send(frame) {
                    bridge::FREE.send(frame).await;
                }
                last_frame = now;
            }
            bridge::UI_PROGRESS.fetch_add(1, Ordering::Relaxed);
            Timer::after_millis(5).await;
        }
    })
    .await;
}

#[embassy_executor::task]
pub async fn probe_display_task(
    spi: Spi<'static, SPI0, Async>,
    dc: Peri<'static, PIN_16>,
    cs: Peri<'static, PIN_17>,
    bl: Peri<'static, PIN_20>,
) {
    metrics::measure(0, async {
        let mut light = Output::new(bl, Level::Low);
        let mut display = St7789Flusher::new(spi, Output::new(dc, Level::Low), Output::new(cs, Level::High));
        bridge::DISPLAY_ACTIVE.store(true, Ordering::Relaxed);
        if display.init().await.is_err() {
            defmt::error!("Probe display init failed; watchdog recovery");
            bridge::DISPLAY_FATAL.store(true, Ordering::Relaxed);
            core::future::pending::<()>().await;
        }
        bridge::DISPLAY_ACTIVE.store(false, Ordering::Relaxed);
        loop {
            let frame = bridge::READY.receive().await;
            bridge::DISPLAY_ACTIVE.store(true, Ordering::Relaxed);
            // Never timeout/drop the SPI future or return its buffer on failure.
            if flush_plan(&mut display, frame, &DamagePlan::full(), || {})
                .await
                .is_err()
            {
                defmt::error!("Probe display transfer failed; buffer quarantined");
                bridge::DISPLAY_FATAL.store(true, Ordering::Relaxed);
                core::future::pending::<()>().await;
            }
            light.set_high();
            bridge::DISPLAY_ACTIVE.store(false, Ordering::Relaxed);
            metrics::frame_completed();
            bridge::FREE.send(frame).await;
        }
    })
    .await;
}

#[embassy_executor::task]
pub async fn probe_inputs_task(
    a: Peri<'static, PIN_12>,
    b: Peri<'static, PIN_13>,
    x: Peri<'static, PIN_14>,
    y: Peri<'static, PIN_15>,
) {
    metrics::measure(0, async {
        let buttons = [
            Input::new(a, Pull::Up),
            Input::new(b, Pull::Up),
            Input::new(x, Pull::Up),
            Input::new(y, Pull::Up),
        ];
        let mut debounce = [const { DebouncedButton::new() }; 4];
        let mut y_start = None;
        let mut recovered = false;
        loop {
            let now = Instant::now().as_millis();
            for (i, button) in buttons.iter().enumerate() {
                if debounce[i].update(button.is_low(), now) {
                    bridge::INPUTS
                        .send(match i {
                            0 => UiInput::Rotate(-1),
                            1 => UiInput::Rotate(1),
                            2 => UiInput::Details,
                            _ => UiInput::Back,
                        })
                        .await;
                }
            }
            if buttons[3].is_low() {
                let start = *y_start.get_or_insert(now);
                if !recovered && now.saturating_sub(start) >= 2000 {
                    bridge::INPUTS.send(UiInput::Recover).await;
                    recovered = true;
                }
            } else {
                y_start = None;
                recovered = false;
            }
            bridge::INPUT_PROGRESS.fetch_add(1, Ordering::Relaxed);
            Timer::after_millis(10).await;
        }
    })
    .await;
}

#[embassy_executor::task]
pub async fn probe_encoder_task(mut bus: I2c<'static, I2C0, embassy_rp::i2c::Async>) {
    metrics::measure(0, async {
        Timer::after_millis(50).await;
        let mut connected = false;
        let mut button = DebouncedButton::new();
        loop {
            // Shared driver does not cancel I2C. Supervisor resets a stalled owner.
            let result = async {
                if !connected {
                    seesaw::init_encoder(&mut bus).await?;
                    let _ = seesaw::read_delta(&mut bus).await?;
                    button = DebouncedButton::new();
                    connected = true;
                    bridge::ENCODER_CONNECTED.store(true, Ordering::Relaxed);
                    defmt::info!("Probe encoder connected");
                }
                if let Some(delta) = seesaw::read_delta(&mut bus).await?
                    && delta != 0
                {
                    bridge::INPUTS.send(UiInput::Rotate(delta)).await;
                }
                if button.update(seesaw::read_button(&mut bus).await?, Instant::now().as_millis()) {
                    bridge::INPUTS.send(UiInput::Click).await;
                }
                Ok::<(), seesaw::EncoderError>(())
            }
            .await;
            bridge::ENCODER_PROGRESS.fetch_add(1, Ordering::Relaxed);
            if result.is_err() {
                if connected {
                    defmt::warn!("Probe encoder lost; onboard controls remain");
                }
                connected = false;
                bridge::ENCODER_CONNECTED.store(false, Ordering::Relaxed);
                Timer::after_millis(200).await;
            } else {
                Timer::after_millis(20).await;
            }
        }
    })
    .await;
}

#[embassy_executor::task]
pub async fn probe_supervisor_task(
    mut watchdog: RecoveryWatchdog,
    red: Peri<'static, PIN_26>,
    green: Peri<'static, PIN_27>,
    blue: Peri<'static, PIN_28>,
) {
    metrics::measure(0, async {
        let mut red = Output::new(red, Level::High);
        let mut green = Output::new(green, Level::High);
        let mut blue = Output::new(blue, Level::High);
        let start = Instant::now().as_millis();
        let mut monitor = ProgressMonitor::new(start, progress());
        let mut radio = Liveness::new(start);
        let mut failed = false;
        loop {
            let now = Instant::now().as_millis();
            let progress_result = monitor.observe(now, progress());
            let radio_alive = radio.observe(bridge::RADIO_PROGRESS.load(Ordering::Relaxed), now, false);
            let healthy = !failed
                && progress_result.is_ok()
                && radio_alive
                && (bridge::RADIO_READY.load(Ordering::Relaxed) || now.saturating_sub(start) < 12_000)
                && !bridge::RADIO_FATAL.load(Ordering::Relaxed)
                && !bridge::DISPLAY_FATAL.load(Ordering::Relaxed)
                && !bridge::RESET_REQUESTED.load(Ordering::Relaxed);
            if !healthy && !failed {
                defmt::error!(
                    "Probe watchdog recovery: progress={:?} radio_alive={} radio_ready={} radio_fatal={} \
                     display_fatal={} requested={}",
                    defmt::Debug2Format(&progress_result),
                    radio_alive,
                    bridge::RADIO_READY.load(Ordering::Relaxed),
                    bridge::RADIO_FATAL.load(Ordering::Relaxed),
                    bridge::DISPLAY_FATAL.load(Ordering::Relaxed),
                    bridge::RESET_REQUESTED.load(Ordering::Relaxed)
                );
                failed = true;
            }
            let phase = bridge::PHASE.load(Ordering::Relaxed);
            let fault = !healthy || phase == RadioPhase::Fault as u8;
            red.set_level(if fault || phase == RadioPhase::Paused as u8 {
                Level::Low
            } else {
                Level::High
            });
            green.set_level(
                if !fault && (phase == RadioPhase::Scanning as u8 || phase == RadioPhase::Paused as u8) {
                    Level::Low
                } else {
                    Level::High
                },
            );
            blue.set_level(
                if !fault && phase != RadioPhase::Paused as u8 && phase != RadioPhase::Scanning as u8 {
                    Level::Low
                } else {
                    Level::High
                },
            );
            if healthy {
                watchdog.feed();
            }
            Timer::after_millis(100).await;
        }
    })
    .await;
}

fn progress() -> Progress {
    // The utility's acquisition owner is the encoder. Display tracks actual
    // transfer activity/completion; it does not publish dashboard generations.
    Progress {
        acquisition: bridge::ENCODER_PROGRESS.load(Ordering::Relaxed),
        rendering: bridge::UI_PROGRESS.load(Ordering::Relaxed),
        input: bridge::INPUT_PROGRESS.load(Ordering::Relaxed),
        completed: metrics::completed(),
        submitted: 0,
        display_pending: bridge::DISPLAY_ACTIVE.load(Ordering::Relaxed),
    }
}
