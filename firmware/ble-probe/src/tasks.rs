use core::pin::pin;
use core::sync::atomic::Ordering;

use dashboard_core::ble::runtime::Liveness;
use dashboard_core::display::damage::DamagePlan;
use dashboard_core::display::transfer::flush_plan;
use dashboard_core::health::reset::RecoveryCause;
use dashboard_core::health::{Progress, ProgressMonitor};
use dashboard_core::input_model::{DebouncedButton, DeferredButton};
use dashboard_pico2::panel::St7789Flusher;
use dashboard_pico2::recovery_watchdog::RecoveryWatchdog;
use dashboard_pico2::seesaw;
use embassy_rp::Peri;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::I2c;
use embassy_rp::peripherals::*;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

use crate::bridge::Control as UiInput;
use crate::{bridge, metrics, radio};

#[embassy_executor::task]
pub async fn probe_radio_task(resources: radio::Resources) {
    let mut owner = pin!(radio::run(resources));
    metrics::measure(0, core::future::poll_fn(|cx| radio::poll_owner(owner.as_mut(), cx))).await;
}

#[embassy_executor::task]
pub fn probe_ui_task() -> impl core::future::Future<Output = ()> {
    // Own the future once; an outer async wrapper materializes another full
    // UI future on the stack before its first poll on the pinned compiler.
    metrics::measure(1, crate::ui::probe_ui_task_run())
}

#[embassy_executor::task]
pub async fn probe_display_task(
    spi: Spi<'static, SPI0, Async>,
    dc: Peri<'static, PIN_16>,
    cs: Peri<'static, PIN_17>,
    bl: Peri<'static, PIN_20>,
) {
    metrics::measure(
        0,
        pin!(async {
            let mut light = Output::new(bl, Level::Low);
            let mut display = St7789Flusher::new(spi, Output::new(dc, Level::Low), Output::new(cs, Level::High));
            bridge::DISPLAY_ACTIVE.store(true, Ordering::SeqCst);
            if display.init().await.is_err() {
                defmt::error!("Probe display init failed; watchdog recovery");
                bridge::DISPLAY_FATAL.store(true, Ordering::Relaxed);
                core::future::pending::<()>().await;
            }
            bridge::DISPLAY_ACTIVE.store(false, Ordering::SeqCst);
            loop {
                let frame = bridge::READY.receive().await;
                bridge::DISPLAY_ACTIVE.store(true, Ordering::SeqCst);
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
                bridge::DISPLAY_ACTIVE.store(false, Ordering::SeqCst);
                metrics::frame_completed();
                bridge::FREE.send(frame).await;
            }
        }),
    )
    .await;
}

#[embassy_executor::task]
pub async fn probe_inputs_task(
    a: Peri<'static, PIN_12>,
    b: Peri<'static, PIN_13>,
    x: Peri<'static, PIN_14>,
    y: Peri<'static, PIN_15>,
) {
    metrics::measure(
        0,
        pin!(async {
            let buttons = [
                Input::new(a, Pull::Up),
                Input::new(b, Pull::Up),
                Input::new(x, Pull::Up),
                Input::new(y, Pull::Up),
            ];
            let mut deferred = [const { DeferredButton::new() }; 4];
            let mut next = DebouncedButton::new();
            loop {
                let now = Instant::now().as_millis();
                for (i, button) in buttons.iter().enumerate() {
                    if i == 1 {
                        if next.update(button.is_low(), now) {
                            bridge::INPUTS.send(UiInput::Rotate(1)).await;
                        }
                        continue;
                    }
                    let edges = deferred[i].update(button.is_low(), now);
                    if edges.short {
                        bridge::INPUTS
                            .send(match i {
                                0 => UiInput::Rotate(-1),
                                2 => UiInput::XShort,
                                _ => UiInput::Back,
                            })
                            .await;
                    }
                    if edges.held {
                        bridge::INPUTS
                            .send(match i {
                                0 => UiInput::Click,
                                2 => UiInput::XHold,
                                _ => UiInput::Recover,
                            })
                            .await;
                    }
                }
                bridge::INPUT_PROGRESS.fetch_add(1, Ordering::Relaxed);
                Timer::after_millis(10).await;
            }
        }),
    )
    .await;
}

#[embassy_executor::task]
pub async fn probe_encoder_task(mut bus: I2c<'static, I2C0, embassy_rp::i2c::Async>) {
    metrics::measure(
        0,
        pin!(async {
            Timer::after_millis(50).await;
            let mut connected = false;
            let mut button = DebouncedButton::new();
            let mut last_resume = None;
            loop {
                crate::maintenance::input_boundary(&mut last_resume).await;
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
                    bridge::STATUS.encoder_failed(connected);
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
        }),
    )
    .await;
}

pub struct SupervisorResources {
    pub watchdog: RecoveryWatchdog,
    pub red: Peri<'static, PIN_26>,
    pub green: Peri<'static, PIN_27>,
    pub blue: Peri<'static, PIN_28>,
}
// Main transfers the sole owner before spawning. Argument-free task creation
// avoids materializing a second complete supervisor future on the stack.
pub static SUPERVISOR: Channel<CriticalSectionRawMutex, SupervisorResources, 1> = Channel::new();

#[embassy_executor::task]
pub fn probe_supervisor_task() -> impl core::future::Future<Output = ()> {
    metrics::measure(0, probe_supervisor_task_run())
}

async fn probe_supervisor_task_run() {
    let SupervisorResources {
        mut watchdog,
        red,
        green,
        blue,
    } = SUPERVISOR.receive().await;
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
        let radio_alive = radio.observe(
            bridge::RADIO_PROGRESS.load(Ordering::Relaxed),
            now,
            !monitor.feeding_allowed(),
        );
        let radio_ready = bridge::RADIO_READY.load(Ordering::Relaxed);
        let radio_fatal = bridge::RADIO_FATAL.load(Ordering::Relaxed);
        let display_fatal = bridge::DISPLAY_FATAL.load(Ordering::Relaxed);
        let requested = bridge::RESET_REQUESTED.load(Ordering::Relaxed);
        let startup_expired = !radio_ready && now.saturating_sub(start) >= 12_000;
        let healthy = !failed
            && progress_result.is_ok()
            && radio_alive
            && !startup_expired
            && !radio_fatal
            && !display_fatal
            && !requested;
        if !healthy && !failed {
            watchdog.record_failure(if let Err(fault) = progress_result {
                fault.into()
            } else if radio_fatal {
                RecoveryCause::RadioFatal
            } else if display_fatal {
                RecoveryCause::DisplayFatal
            } else if requested {
                RecoveryCause::RecoveryRequested
            } else if startup_expired {
                RecoveryCause::RadioStartup
            } else {
                RecoveryCause::RadioStalled
            });
            defmt::error!(
                "Probe watchdog recovery: progress={:?} radio_alive={} radio_ready={} radio_fatal={} display_fatal={} \
                 requested={}",
                defmt::Debug2Format(&progress_result),
                radio_alive,
                radio_ready,
                radio_fatal,
                display_fatal,
                requested
            );
            failed = true;
        }
        let [r, g, b] = bridge::STATUS.levels(now, !healthy);
        red.set_level(if r { Level::High } else { Level::Low });
        green.set_level(if g { Level::High } else { Level::Low });
        blue.set_level(if b { Level::High } else { Level::Low });
        if healthy {
            use crate::maintenance::{HealthReply, HealthRequest};
            let result = match crate::maintenance::HEALTH_REQUEST.try_receive() {
                Ok(HealthRequest::Begin) => {
                    monitor.begin_maintenance(now, progress()).map(|ticket| {
                        watchdog.feed();
                        crate::maintenance::reply(HealthReply::Begun(ticket));
                    })
                }
                Ok(HealthRequest::End(ticket)) => {
                    monitor.end_maintenance(ticket, now, progress()).map(|()| {
                        crate::maintenance::reply(HealthReply::Ended);
                    })
                }
                Err(_) => Ok(()),
            };
            if let Err(error) = result {
                watchdog.record_failure(error.into());
                defmt::error!(
                    "probe.flash health {:?}; watchdog recovery",
                    defmt::Debug2Format(&error)
                );
                failed = true;
            }
            if !failed && monitor.feeding_allowed() {
                watchdog.feed();
            }
        }
        Timer::after_millis(50).await;
    }
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
