//! Core1 model and two affine byte/framebuffer leases. Flash is owned by core0.
use core::sync::atomic::Ordering;

use dashboard_core::discovery::{self, Input, Probe, RadioPhase, capture, capture_ui as cap};
use dashboard_core::display::St7789Renderer;
use dashboard_core::display::telemetry::CompletionRate;
use embassy_time::{Instant, Timer};
use static_cell::StaticCell;

use crate::{bridge, maintenance, metrics, storage};

// Preserve the task identity in the concrete future for named stack admission.
/// Runs the core1 catalogue/capture UI indefinitely with one model, decoder and
/// affine byte lease. Reliable evidence precedes controls/save decisions; controls
/// update generations before lossy scan reports. Renders at most every 50 ms using
/// a FREE framebuffer, returns stale reply leases, and parks before acquisition
/// during flash maintenance. Never cancel an outstanding affine exchange.
pub async fn probe_ui_task_run() {
    /// Sole in-place bounded live catalogue and inspection state.
    static MODEL: StaticCell<Probe> = StaticCell::new();
    let model = MODEL.init_with(Probe::new);
    model.boot = Some(bridge::BOOT.receive().await);
    model.identity_mode = crate::tool_kind() == crate::ToolKind::NgmIdentity;
    model.radio_bus_hz = dashboard_core::ble::runtime::radio_bus_hz(embassy_rp::clocks::clk_sys_freq());
    // Sole UI-owner state lives in place, like MODEL. Keeping these larger
    // values out of the async future avoids whole-future construction copies.
    /// Sole in-place saved-slot workflow, confirmations and request identities.
    static CAPTURES: StaticCell<cap::CaptureUi> = StaticCell::new();
    /// Reusable bounded capture codec workspace; no duplicate payload allocation.
    static DECODER: StaticCell<capture::Decoder> = StaticCell::new();
    let captures = CAPTURES.init_with(cap::CaptureUi::new);
    let decoder = DECODER.init_with(capture::Decoder::new);
    let mut bytes = Some(storage::ui_buffer());
    let mut length = 0;
    let mut last_frame = 0;
    let start = metrics::snapshot_into(&mut model.diagnostics);
    let mut fps = CompletionRate::new(start.at_us, start.completed);
    let mut last_resume = None;
    loop {
        maintenance::UI_QUIET.store(maintenance::FRAME_INHIBIT.load(Ordering::SeqCst), Ordering::SeqCst);
        maintenance::ui_boundary(&mut last_resume).await;
        let snapshot = metrics::snapshot_into(&mut model.diagnostics);
        let now = snapshot.at_us / 1000;
        if fps.sample(snapshot.at_us, snapshot.completed).is_err() {
            bridge::STATUS.record(crate::status::Fault::Timing);
            defmt::warn!("probe.metrics completion timeline reset");
            fps = CompletionRate::new(snapshot.at_us, snapshot.completed);
        }
        model.diagnostics.fps = fps.latest(snapshot.at_us);
        storage::UI_REPLY.consume(|reply| {
            if let Some(returned) = reply.buffer.take() {
                assert!(bytes.is_none());
                bytes = Some(returned);
                length = reply.length;
            }
            bridge::STATUS.capture_complete(captures, reply.id, &reply.completion);
        });
        // Reliable inventory/completion evidence must precede a save decision.
        for _ in 0..8 {
            match bridge::EVENTS.try_receive() {
                Ok(e) => bridge::STATUS.event(model, e),
                Err(_) => break,
            }
        }
        for _ in 0..16 {
            let Ok(control) = bridge::INPUTS.try_receive() else {
                break;
            };
            let context = context(model);
            let live = captures.screen() == cap::Screen::Live;
            let (capture_input, live_input) = match control {
                bridge::Control::XShort => (Some(cap::Input::XShort), None),
                bridge::Control::XHold => (Some(cap::Input::XHold), None),
                bridge::Control::Recover => (Some(cap::Input::Recover), None),
                bridge::Control::Rotate(delta) if live => (None, Some(Input::Rotate(delta))),
                bridge::Control::Click if live => (None, Some(Input::Click)),
                bridge::Control::Back if live => (Some(cap::Input::Back), Some(Input::Back)),
                bridge::Control::Rotate(delta) => {
                    (
                        Some(cap::Input::Turn(delta.clamp(i16::MIN as i32, i16::MAX as i32) as i16)),
                        None,
                    )
                }
                bridge::Control::Click => (Some(cap::Input::Click), None),
                bridge::Control::Back => (Some(cap::Input::Back), None),
            };
            if let Some(input) = live_input {
                live_input_apply(model, input, now);
            }
            if let Some(input) = capture_input
                && let Some(action) = bridge::STATUS.capture_input(captures, input, now, context)
            {
                action_apply(action, captures, model, &mut bytes, decoder, now);
            }
            model.page = model.page.min(discovery::page_count(model).saturating_sub(1));
        }
        if let Some(action) = bridge::STATUS.capture_tick(captures, now) {
            action_apply(action, captures, model, &mut bytes, decoder, now);
        }
        // Controls change the generation before queued scan reports are admitted.
        for _ in 0..32 {
            match bridge::REPORTS.try_receive() {
                Ok(e) => bridge::STATUS.event(model, e),
                Err(_) => break,
            }
        }
        model.dropped_reports = bridge::DROPPED_REPORTS.load(Ordering::Relaxed);
        model.scan_diagnostics = bridge::SCAN_DIAGNOSTICS.lock(core::cell::Cell::get);
        model.malformed_reports = model.scan_diagnostics.total;
        model.encoder_connected = bridge::ENCODER_CONNECTED.load(Ordering::Relaxed);
        bridge::STATUS.publish(model, captures);
        model.fault_latch = bridge::STATUS.fault_latch();
        if !maintenance::FRAME_INHIBIT.load(Ordering::SeqCst)
            && now.saturating_sub(last_frame) >= 50
            && let Ok(frame) = bridge::FREE.try_receive()
        {
            maintenance::UI_QUIET.store(false, Ordering::SeqCst);
            let mut renderer = St7789Renderer::new(frame);
            match captures.screen() {
                cap::Screen::Live => {
                    let _ = discovery::render(&mut renderer, model, now);
                }
                cap::Screen::Saved => {
                    if let Some(view) = bytes
                        .as_deref()
                        .and_then(|b| b.get(..length))
                        .and_then(|b| decoder.decode(b).ok())
                    {
                        let _ = discovery::render_saved(
                            &mut renderer,
                            &view,
                            captures.selected(),
                            captures.page(),
                            model.diagnostics,
                        );
                    } else {
                        bridge::STATUS.record(crate::status::Fault::Storage);
                        let _ = discovery::render_slots(&mut renderer, captures, model.diagnostics);
                    }
                }
                _ => {
                    let _ = discovery::render_slots(&mut renderer, captures, model.diagnostics);
                }
            }
            if let Err(embassy_sync::channel::TrySendError::Full(frame)) = bridge::READY.try_send(frame) {
                bridge::FREE.send(frame).await;
            }
            last_frame = now;
        }
        bridge::UI_PROGRESS.fetch_add(1, Ordering::Relaxed);
        Timer::after_millis(5).await;
    }
}
/// Returns save eligibility from `model`: a selected peer and paused radio are
/// required; the revision binds any confirmation to that exact live generation.
fn context(model: &Probe) -> cap::Context {
    cap::Context {
        revision: model.request.revision,
        save: if model.selected_device().is_none() {
            cap::SaveAvailability::NoPeer
        } else if model.phase != RadioPhase::Paused {
            cap::SaveAvailability::Busy
        } else {
            cap::SaveAvailability::Ready
        },
    }
}
/// Applies `input` to live `model` at boot-relative `now`, publishing a new radio
/// request when produced. Recover from terminal Fault also requests watchdog reset.
fn live_input_apply(
    model: &mut Probe,
    input: Input,
    now: u64,
) {
    if matches!(input, Input::Recover) && model.phase == RadioPhase::Fault {
        bridge::RESET_REQUESTED.store(true, Ordering::Relaxed);
    }
    if let Some(request) = bridge::STATUS.input(model, input, now) {
        bridge::set_request(request);
    }
}
/// Applies confirmed capture `action` using `captures`, live `model`, affine
/// `bytes`, reusable `decoder` and boot-relative `now`. Save freezes current latch
/// and revision before queuing; failure restores any unsent lease and completes
/// Busy/Changed instead. Local storage actions never initiate inspection traffic.
fn action_apply(
    action: cap::Action,
    captures: &mut cap::CaptureUi,
    model: &mut Probe,
    bytes: &mut Option<&'static mut storage::Buffer>,
    decoder: &mut capture::Decoder,
    now: u64,
) {
    let (id, operation, needs_bytes) = match action {
        cap::Action::LiveDetails => {
            live_input_apply(model, Input::Details, now);
            return;
        }
        cap::Action::Recover => {
            live_input_apply(model, Input::Recover, now);
            return;
        }
        cap::Action::Refresh { id } => {
            if model.screen == discovery::Screen::List && model.selected_device().is_some() {
                live_input_apply(model, Input::Details, now);
            }
            (id, storage::UiOperation::List, true)
        }
        cap::Action::Load { id, slot } => (id, storage::UiOperation::Load(slot), true),
        cap::Action::Reset { id, slot } => {
            if !matches!(model.phase, RadioPhase::Paused | RadioPhase::Scanning) {
                bridge::STATUS.capture_complete(captures, id, &cap::Completion::Failed(cap::UiError::Busy));
                return;
            }
            (id, storage::UiOperation::Reset(slot), false)
        }
        cap::Action::Save {
            id,
            slot,
            revision,
            overwrite,
        } => {
            if model.request.revision != revision || context(model).save != cap::SaveAvailability::Ready {
                bridge::STATUS.capture_complete(captures, id, &cap::Completion::Failed(cap::UiError::Changed));
                return;
            }
            // Freeze the current atomic latch, not the previous rendered frame.
            model.fault_latch = bridge::STATUS.fault_latch();
            let result = bytes
                .as_deref_mut()
                .ok_or(capture::CaptureError::InvalidState)
                .and_then(|b| decoder.encode(model, build_identity(), Instant::now().as_millis(), b));
            match result {
                Ok(length) => {
                    (
                        id,
                        storage::UiOperation::Save {
                            slot,
                            length,
                            overwrite,
                        },
                        true,
                    )
                }
                Err(e) => {
                    defmt::warn!("probe.capture encode {=str}", e.label());
                    bridge::STATUS.capture_complete(captures, id, &cap::Completion::Failed(cap::UiError::Changed));
                    return;
                }
            }
        }
    };
    let buffer = if needs_bytes { bytes.take() } else { None };
    if needs_bytes && buffer.is_none() {
        bridge::STATUS.record(crate::status::Fault::Storage);
        bridge::STATUS.capture_complete(captures, id, &cap::Completion::Failed(cap::UiError::Busy));
        return;
    }
    if let Err(embassy_sync::channel::TrySendError::Full(request)) =
        storage::UI_REQUEST.try_send(storage::UiRequest { id, operation, buffer })
    {
        if let Some(returned) = request.buffer {
            *bytes = Some(returned);
        }
        bridge::STATUS.record(crate::status::Fault::Storage);
        bridge::STATUS.capture_complete(captures, id, &cap::Completion::Failed(cap::UiError::Busy));
    }
}
/// Returns current build digest/availability and configured clock/profile metadata
/// for capture provenance; no clock, hardware capacity or RF behavior is measured.
fn build_identity() -> capture::BuildIdentity {
    capture::BuildIdentity {
        source_digest: crate::BLE_PROBE_SOURCE_DIGEST,
        source_identity_available: crate::SOURCE_IDENTITY_AVAILABLE,
        cpu_hz: embassy_rp::clocks::clk_sys_freq(),
        spi_hz: dashboard_pico2::clock_profile::ACTUAL_SPI_HZ,
        profile: if cfg!(feature = "cpu280-spi70-1v30") {
            capture::BuildProfile::Expert280
        } else {
            capture::BuildProfile::Stock
        },
    }
}
