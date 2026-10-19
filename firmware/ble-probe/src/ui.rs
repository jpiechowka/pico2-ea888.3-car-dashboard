//! Core1 model and two affine byte/framebuffer leases. Flash is owned by core0.
use core::sync::atomic::Ordering;

use dashboard_core::discovery::{self, Input, Probe, RadioPhase, capture, capture_ui as cap};
use dashboard_core::display::St7789Renderer;
use dashboard_core::display::telemetry::CompletionRate;
use embassy_time::{Instant, Timer};
use static_cell::StaticCell;

use crate::{bridge, maintenance, metrics, storage};

pub async fn run() {
    static MODEL: StaticCell<Probe> = StaticCell::new();
    let model = MODEL.init_with(Probe::new);
    let mut captures = cap::CaptureUi::new();
    let mut bytes = Some(storage::ui_buffer());
    let mut length = 0;
    let mut decoder = capture::Decoder::new();
    let mut last_frame = 0;
    let start = metrics::snapshot();
    let mut fps = CompletionRate::new(start.at_us, start.completed);
    let mut last_resume = None;
    loop {
        maintenance::UI_QUIET.store(maintenance::FRAME_INHIBIT.load(Ordering::SeqCst), Ordering::SeqCst);
        maintenance::ui_boundary(&mut last_resume).await;
        let snapshot = metrics::snapshot();
        let now = snapshot.at_us / 1000;
        if fps.sample(snapshot.at_us, snapshot.completed).is_err() {
            defmt::warn!("probe.metrics completion timeline reset");
            fps = CompletionRate::new(snapshot.at_us, snapshot.completed);
        }
        model.diagnostics = snapshot.values;
        model.diagnostics.fps = fps.latest(snapshot.at_us);
        if let Ok(reply) = storage::UI_REPLY.try_receive() {
            if let Some(returned) = reply.buffer {
                assert!(bytes.is_none());
                bytes = Some(returned);
                length = reply.length;
            }
            captures.complete(reply.id, reply.completion);
        }
        // Reliable inventory/completion evidence must precede a save decision.
        for _ in 0..8 {
            match bridge::EVENTS.try_receive() {
                Ok(e) => model.event(e),
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
                && let Some(action) = captures.input(input, now, context)
            {
                action_apply(action, &mut captures, model, &mut bytes, &mut decoder, now);
            }
            model.page = model.page.min(discovery::page_count(model).saturating_sub(1));
        }
        if let Some(action) = captures.tick(now) {
            action_apply(action, &mut captures, model, &mut bytes, &mut decoder, now);
        }
        // Controls change the generation before queued scan reports are admitted.
        for _ in 0..32 {
            match bridge::REPORTS.try_receive() {
                Ok(e) => model.event(e),
                Err(_) => break,
            }
        }
        model.dropped_reports = bridge::DROPPED_REPORTS.load(Ordering::Relaxed);
        model.scan_diagnostics = bridge::SCAN_DIAGNOSTICS.lock(core::cell::Cell::get);
        model.malformed_reports = model.scan_diagnostics.total;
        model.encoder_connected = bridge::ENCODER_CONNECTED.load(Ordering::Relaxed);
        bridge::PHASE.store(model.phase as u8, Ordering::Relaxed);
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
                        let _ = discovery::render_slots(&mut renderer, &captures, model.diagnostics);
                    }
                }
                _ => {
                    let _ = discovery::render_slots(&mut renderer, &captures, model.diagnostics);
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
fn live_input_apply(
    model: &mut Probe,
    input: Input,
    now: u64,
) {
    if matches!(input, Input::Recover) && model.phase == RadioPhase::Fault {
        bridge::RESET_REQUESTED.store(true, Ordering::Relaxed);
    }
    if let Some(request) = model.input(input, now) {
        bridge::set_request(request);
    }
}
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
            (id, storage::UiOperation::List, false)
        }
        cap::Action::Load { id, slot } => (id, storage::UiOperation::Load(slot), true),
        cap::Action::Reset { id, slot } => (id, storage::UiOperation::Reset(slot), false),
        cap::Action::Save {
            id,
            slot,
            revision,
            overwrite,
        } => {
            if model.request.revision != revision || context(model).save != cap::SaveAvailability::Ready {
                captures.complete(id, cap::Completion::Failed(cap::UiError::Changed));
                return;
            }
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
                    defmt::warn!("probe.capture encode {:?}", defmt::Debug2Format(&e));
                    captures.complete(id, cap::Completion::Failed(cap::UiError::Changed));
                    return;
                }
            }
        }
    };
    let buffer = if needs_bytes { bytes.take() } else { None };
    if needs_bytes && buffer.is_none() {
        captures.complete(id, cap::Completion::Failed(cap::UiError::Busy));
        return;
    }
    if let Err(embassy_sync::channel::TrySendError::Full(request)) =
        storage::UI_REQUEST.try_send(storage::UiRequest { id, operation, buffer })
    {
        if let Some(returned) = request.buffer {
            *bytes = Some(returned);
        }
        captures.complete(id, cap::Completion::Failed(cap::UiError::Busy));
    }
}
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
