//! Core1 source owner, bounded sample publication and maintenance-aware liveness.
use core::sync::atomic::Ordering;

#[cfg(not(feature = "longan-rx12"))]
use dashboard_pico2::acquisition::AcquisitionModel;
use dashboard_pico2::acquisition::DashboardSnapshot;
use dashboard_pico2::cpu_math::CycleWindow;
use dashboard_pico2::diagnostic_values::WorkObservation;
#[cfg(feature = "longan-rx12")]
use dashboard_pico2::diagnostics::service::{LinkSnapshot, Vehicle};
use dashboard_pico2::maintenance::{Command, Owner};
use dashboard_pico2::stack_samples::StackSamples;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};

use crate::log_info;
use crate::tasks::CORE1_OBSERVATION;

/// Latest coherent sample/statistics/reset-ack snapshot; intermediate publications coalesce.
pub static DASHBOARD_VALUES: Watch<CriticalSectionRawMutex, DashboardSnapshot, 1> = Watch::new();
// Only the newest requested generation matters. The acquisition owner echoes
// its acknowledgement in the same snapshot as the reset statistics.
/// Capacity-one explicit warning acknowledgement command; producer retries identical pending requests.
pub static WARNING_COMMANDS: Channel<CriticalSectionRawMutex, dashboard_pico2::warning_state::WarningCommand, 1> =
    Channel::new();
/// Latest requested reset generation; newer requests replace earlier unconsumed ones.
pub static RESET_STATISTICS: Signal<CriticalSectionRawMutex, u64> = Signal::new();
#[cfg(feature = "longan-rx12")]
/// Latest optional wired transport state; independent from measurement freshness.
pub static VEHICLE_LINK: Watch<CriticalSectionRawMutex, LinkSnapshot, 1> = Watch::new();
#[cfg(all(feature = "longan-rx12", feature = "frame-trace"))]
/// Latest diagnostic export sequence, publication milliseconds and bounded session telemetry.
pub static VEHICLE_DIAGNOSTICS: Watch<
    CriticalSectionRawMutex,
    (u64, u64, dashboard_pico2::diagnostics::session::Telemetry),
    1,
> = Watch::new();

/// Immutable shared configuration and optional uninitialized core1 UART tokens.
pub struct Resources {
    /// Boot-admitted config whose immutable storage outlives both cores.
    pub configuration: &'static dashboard_pico2::view_config::ValidatedConfig,
    #[cfg(feature = "longan-rx12")]
    /// UART/pin ownership moved to core1 before constructing its IRQ-bound driver.
    pub uart: crate::drivers::longan::Resources,
}

// Return the unwrapped owner directly into the future's storage. Unwrapping a
// fallible preparation inside this boundary keeps a Result<Vehicle> carrier out
// of the async poll. Review every linked caller-plus-constructor frame.
#[cfg(feature = "longan-rx12")]
#[inline(never)]
/// Consumes `resources` on core1 and returns the prepared wired-source owner.
/// Panics on invalid profile or wrong hardware context; the non-inlined boundary
/// avoids retaining a large fallible constructor carrier in every async poll.
fn configured_vehicle(resources: Resources) -> Vehicle<'static, crate::drivers::longan::HardwarePort> {
    Vehicle::prepare_configured(
        crate::drivers::longan::HardwarePort::new(resources.uart),
        dashboard_pico2::config::vehicle::PROFILE,
        Instant::now().as_millis(),
        resources.configuration,
    )
    .expect("invalid vehicle profile")
    .assemble()
}

// Isolate one-time construction from the frame used by every resumed poll.
// This boundary also covers the historical unavailable acquisition backend.
#[cfg(not(feature = "longan-rx12"))]
#[inline(never)]
/// Consumes `resources` and returns demo-configured acquisition only for demo;
/// all other builds retain unavailable vehicle channels with the configured responder.
fn configured_model(resources: Resources) -> AcquisitionModel<'static> {
    if cfg!(feature = "demo") {
        AcquisitionModel::demo_configured(resources.configuration)
    } else {
        AcquisitionModel::configured(
            resources.configuration,
            dashboard_pico2::config::vehicle::PROFILE.responder,
        )
    }
}

// Publication owns the by-value Watch copies in a separate call frame. Keeping
// them in the async poll also reserves them during Vehicle construction.
#[inline(never)]
/// Publishes `model` through `sender` only when its identity differs from
/// `previous`, or identity is unavailable. Updates `previous` after the coherent
/// copy; separating the call frame avoids carrying snapshot copies during construction.
fn publish_snapshot(
    #[cfg(feature = "longan-rx12")] model: &Vehicle<'_, crate::drivers::longan::HardwarePort>,
    #[cfg(not(feature = "longan-rx12"))] model: &AcquisitionModel<'_>,
    sender: &embassy_sync::watch::DynSender<'_, DashboardSnapshot>,
    previous: &mut Option<dashboard_pico2::acquisition::PublicationIdentity>,
) {
    let snapshot = model.snapshot();
    let identity = snapshot.publication_identity();
    if identity.is_none() || *previous != identity {
        sender.send(snapshot);
        *previous = identity;
    }
}

#[embassy_executor::task]
/// Owns source `resources` indefinitely on core1. `start_time` anchors demo
/// animation; `stack_bottom` and `stack_top` bound stack observations. Publishes
/// samples/statistics coherently, acknowledges reset generations, and bounds normal
/// publication to 100 Hz. Maintenance drains wired commands before parking, keeps
/// IRQs available and resumes through fresh protocol qualification. No catch-up
/// burst or live ECU qualification is inferred; invalid stack/profile invariants panic.
pub async fn acquisition_task(
    start_time: Instant,
    stack_bottom: u32,
    stack_top: u32,
    resources: Resources,
) {
    let sender = DASHBOARD_VALUES.dyn_sender();
    log_info!(
        "Acquisition: {}",
        if cfg!(feature = "demo") {
            "DEMO"
        } else if cfg!(feature = "longan-rx12") {
            "VEHICLE raw UART, unqualified"
        } else {
            "VEHICLE unavailable"
        }
    );

    let mut last_util_calc = Instant::now();
    let mut work_window = CycleWindow::new();
    #[cfg(not(feature = "longan-rx12"))]
    let mut model = configured_model(resources);
    #[cfg(feature = "longan-rx12")]
    let mut model = configured_vehicle(resources);
    #[cfg(feature = "longan-rx12")]
    let link_sender = VEHICLE_LINK.dyn_sender();
    #[cfg(feature = "longan-rx12")]
    let mut previous_link = None;
    let mut previous_snapshot = None;
    let mut last_publication = Instant::now();
    let interval_ms = if cfg!(feature = "longan-rx12") {
        1
    } else if cfg!(feature = "demo") {
        10
    } else {
        100
    };
    let mut stack_samples = StackSamples::new(stack_bottom, stack_top).expect("invalid core1 stack bounds");
    let mut last_resume = None;
    #[cfg(all(feature = "longan-rx12", feature = "frame-trace"))]
    let mut diagnostic_export = 0u64;

    loop {
        if let Some(command) = super::maintenance::command() {
            let request = match command {
                Command::Park(r) | Command::Resume(r) => r,
            };
            if last_resume != Some(request) {
                if matches!(command, Command::Park(_)) {
                    #[cfg(feature = "longan-rx12")]
                    let accepted = model.request_pause(Instant::now().as_millis()).is_ok();
                    let mut published = false;
                    while super::maintenance::command() == Some(Command::Park(request)) {
                        let now = Instant::now().as_millis();
                        #[cfg(feature = "longan-rx12")]
                        let ready = {
                            if accepted {
                                core::future::poll_fn(|cx| {
                                    model.step(cx, now);
                                    core::task::Poll::Ready(())
                                })
                                .await;
                            }
                            accepted && model.paused()
                        };
                        #[cfg(not(feature = "longan-rx12"))]
                        let ready = true;
                        let in_time = now
                            .checked_sub(request.started_ms())
                            .is_some_and(|ms| ms <= dashboard_pico2::maintenance::PREPARE_MS);
                        if ready && !published {
                            // Publish intentional transport invalidation before
                            // acknowledging park; retain original sample ages.
                            publish_snapshot(&model, &sender, &mut None);
                            #[cfg(feature = "longan-rx12")]
                            link_sender.send(model.link());
                            published = true;
                            super::maintenance::acknowledge(
                                Command::Park(request),
                                Owner::Acquisition,
                                in_time && super::maintenance::core1_ready(),
                            );
                        } else if !in_time {
                            super::maintenance::acknowledge(Command::Park(request), Owner::Acquisition, false);
                        }
                        #[cfg(feature = "longan-rx12")]
                        if !accepted
                            || model.link().state == dashboard_pico2::diagnostics::service::LinkState::Quarantined
                        {
                            super::maintenance::acknowledge(Command::Park(request), Owner::Acquisition, false);
                        }
                        // Keep SIO/UART interrupts enabled. No HAL lockout or
                        // progress increment here; recovery has a fixed grant.
                        Timer::after_millis(1).await;
                    }
                }
                #[cfg(feature = "longan-rx12")]
                {
                    // Another owner may reject preparation before our command
                    // finishes. Even cancellation must complete the full frame.
                    while model.link().state == dashboard_pico2::diagnostics::service::LinkState::Maintenance
                        && !model.paused()
                    {
                        core::future::poll_fn(|cx| {
                            model.step(cx, Instant::now().as_millis());
                            core::task::Poll::Ready(())
                        })
                        .await;
                        Timer::after_millis(1).await;
                    }
                    if model.paused() {
                        model
                            .resume(Instant::now().as_millis())
                            .expect("invalid vehicle resume");
                    }
                }
                last_resume = Some(request);
                super::maintenance::acknowledge(
                    Command::Resume(request),
                    Owner::Acquisition,
                    super::maintenance::core1_ready(),
                );
            }
        }
        // Start useful-work measurement only after cooperative park ends.
        let work_started = Instant::now();
        let work_start = crate::profiling::read();

        if let Some(generation) = RESET_STATISTICS.try_take() {
            model.reset_statistics(generation);
        }
        #[cfg(not(feature = "longan-rx12"))]
        if cfg!(feature = "demo") {
            model.ingest_batch(
                dashboard_pico2::demo::values(start_time.elapsed().as_millis()),
                Instant::now().as_millis(),
            );
        }
        #[cfg(feature = "longan-rx12")]
        {
            let _ = start_time;
            let now = Instant::now().as_millis();
            // Always Ready: exposes this executor's waker to bounded HAL polls
            // without suspending between the segmented cycle measurements.
            core::future::poll_fn(|cx| {
                model.step_with_warning(cx, now, WARNING_COMMANDS.try_receive().ok());
                core::task::Poll::Ready(())
            })
            .await;
        }
        #[cfg(not(feature = "longan-rx12"))]
        {
            let now = Instant::now().as_millis();
            if let Ok(command) = WARNING_COMMANDS.try_receive() {
                model.apply_warning_command(command, now);
            }
            model.tick(now);
        }
        // Statistics include every accepted sample before Watch can coalesce.
        // At most 100 Hz publication; no repeated bounded snapshot copy
        // when the backend has neither new data, faults nor a reset ack.
        if previous_snapshot.is_none() || last_publication.elapsed() >= Duration::from_millis(10) {
            publish_snapshot(&model, &sender, &mut previous_snapshot);
            #[cfg(feature = "longan-rx12")]
            {
                let link = model.link();
                if let Some(fault) = link.last_fault
                    && previous_link.is_none_or(|previous: LinkSnapshot| {
                        previous.faults != link.faults || previous.last_fault != link.last_fault
                    })
                {
                    crate::log_fault!("Link: {:?}", fault);
                }
                if previous_link != Some(link) {
                    link_sender.send(link);
                    previous_link = Some(link);
                }
            }
            last_publication = Instant::now();
        }
        crate::tasks::recovery::ACQUISITION_PROGRESS.fetch_add(1, Ordering::Relaxed);

        let util_due = last_util_calc.elapsed() >= Duration::from_secs(1);
        #[cfg(all(feature = "longan-rx12", feature = "frame-trace"))]
        if util_due && let Some(sequence) = diagnostic_export.checked_add(1) {
            // Build outside the critical section; include export work in the
            // segmented core1 cycle count, and keep publish/session time apart.
            let snapshot = model.telemetry();
            VEHICLE_DIAGNOSTICS
                .dyn_sender()
                .send((sequence, Instant::now().as_millis(), snapshot));
            diagnostic_export = sequence;
        }
        let work_end = crate::profiling::read();
        work_window.record(
            dashboard_pico2::cpu_cycles::available(1)
                .then(|| crate::profiling::elapsed(work_start, work_end, work_started.elapsed().as_micros()))
                .flatten(),
        );

        if util_due {
            let now = Instant::now();
            let util = work_window.finish(embassy_rp::clocks::clk_sys_freq(), (now - last_util_calc).as_micros());

            // Sampled depth only; never read live stack memory or claim a HWM.
            let observed = stack_samples.observe(dashboard_pico2::memory::stack_pointer());
            CORE1_OBSERVATION.lock(|slot| {
                slot.set(WorkObservation {
                    at_us: Some(now.as_micros()),
                    work_tenths: util,
                    stack_bytes: observed,
                })
            });
            last_util_calc = now;
        }

        // Never run a burst of overdue catch-up ticks after scheduling delays.
        Timer::after_millis(interval_ms).await;
    }
}
