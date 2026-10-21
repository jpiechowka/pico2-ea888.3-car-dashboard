//! Two-buffer lease ownership, immutable presentation identity and delayed completion guards.
use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use dashboard_host_validation::display::pipeline::{BufferId, BufferState, FrameLifecycle, after_first_poll};

/// Rendering may overlap the other buffer's transfer, but in-flight storage cannot be reacquired.
#[test]
fn two_buffers_overlap_but_inflight_storage_cannot_be_reacquired() {
    let mut lifecycle = FrameLifecycle::new();
    let a = lifecycle.begin(BufferId::A).unwrap();
    assert!(lifecycle.begin(BufferId::A).is_err());
    lifecycle.ready(a).unwrap();
    lifecycle.queue(a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    lifecycle.started(a).unwrap();
    let b = lifecycle.begin(BufferId::B).unwrap();
    lifecycle.ready(b).unwrap();
    assert!(lifecycle.queue(b).is_err());
    assert_eq!(lifecycle.state(BufferId::A), BufferState::InFlight(a));
    assert!(lifecycle.begin(BufferId::A).is_err());
    lifecycle.complete(a).unwrap();
    lifecycle.queue(b).unwrap();
    lifecycle.started(b).unwrap();
    lifecycle.complete(b).unwrap();
    assert!(lifecycle.begin(BufferId::A).is_ok());
}

/// Wrong, duplicate and stale lifecycle events cannot release or advance a current buffer lease.
#[test]
fn old_wrong_duplicate_and_out_of_order_events_leave_ownership_unchanged() {
    let mut lifecycle = FrameLifecycle::new();
    let a = lifecycle.begin(BufferId::A).unwrap();
    let b = lifecycle.begin(BufferId::B).unwrap();
    assert!(lifecycle.queue(a).is_err());
    assert!(lifecycle.started(a).is_err());
    lifecycle.ready(a).unwrap();
    assert!(lifecycle.ready(a).is_err());
    lifecycle.queue(a).unwrap();
    assert!(lifecycle.started(b).is_err());
    assert!(lifecycle.complete(b).is_err());
    lifecycle.started(a).unwrap();
    assert!(lifecycle.started(a).is_err());
    lifecycle.complete(a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    let next_a = lifecycle.begin(BufferId::A).unwrap();
    lifecycle.ready(next_a).unwrap();
    lifecycle.queue(next_a).unwrap();
    lifecycle.started(next_a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    assert_eq!(lifecycle.state(BufferId::A), BufferState::InFlight(next_a));
}

/// Timeout quarantines queued or started storage and blocks further transfers until reset.
#[test]
fn timeout_quarantines_the_exact_buffer_without_releasing_it() {
    for started in [false, true] {
        let mut lifecycle = FrameLifecycle::new();
        let a = lifecycle.begin(BufferId::A).unwrap();
        lifecycle.ready(a).unwrap();
        lifecycle.queue(a).unwrap();
        if started {
            lifecycle.started(a).unwrap();
        }
        lifecycle.quarantine(a).unwrap();
        assert!(lifecycle.complete(a).is_err());
        assert!(lifecycle.begin(BufferId::A).is_err());
        assert_eq!(lifecycle.state(BufferId::A), BufferState::Quarantined(a));
        let b = lifecycle.begin(BufferId::B).unwrap();
        lifecycle.ready(b).unwrap();
        // A fault requires resetting the peripheral; no new transfers may start.
        assert!(lifecycle.queue(b).is_err());
    }
}

/// Start notification runs once after the first payload poll for both immediate and delayed completion.
#[test]
fn started_callback_runs_after_the_first_payload_poll_once() {
    for immediate in [false, true] {
        let polls = Cell::new(0);
        let callbacks = Cell::new(0);
        let complete = Cell::new(immediate);
        let payload = poll_fn(|_| {
            polls.set(polls.get() + 1);
            if complete.get() { Poll::Ready(17) } else { Poll::Pending }
        });
        let mut transfer = pin!(after_first_poll(payload, || {
            assert_eq!(polls.get(), 1);
            callbacks.set(callbacks.get() + 1);
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(polls.get(), 0);
        assert_eq!(callbacks.get(), 0);
        assert_eq!(
            transfer.as_mut().poll(&mut context),
            if immediate { Poll::Ready(17) } else { Poll::Pending }
        );
        assert_eq!(callbacks.get(), 1);
        if !immediate {
            assert_eq!(transfer.as_mut().poll(&mut context), Poll::Pending);
            complete.set(true);
            assert_eq!(transfer.as_mut().poll(&mut context), Poll::Ready(17));
            assert_eq!(callbacks.get(), 1);
        }
    }
}

/// A bounded channel moves the exclusive lease and retains completion while the other buffer changes.
#[test]
fn bounded_channel_moves_the_lease_and_retains_delayed_completion() {
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::display::pipeline::FrameLease;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    let mut a = Box::new(FrameBuffer::new());
    let mut b = Box::new(FrameBuffer::new());
    let [mut a, mut b] = FrameLease::pair(&mut a, &mut b);
    a.buffer()[0] = 123;
    let mut lifecycle = FrameLifecycle::new();
    let token = lifecycle.begin(a.id()).unwrap();
    lifecycle.ready(token).unwrap();
    lifecycle.queue(token).unwrap();
    let requests = Channel::<CriticalSectionRawMutex, _, 1>::new();
    let completions = Channel::<CriticalSectionRawMutex, _, 1>::new();
    assert!(
        requests
            .try_send(a.submit(
                token,
                dashboard_host_validation::display::telemetry::RenderTrace::instant(10)
            ))
            .is_ok()
    );
    let request = requests.try_receive().unwrap();
    lifecycle.started(request.token()).unwrap();
    for _ in 0..100 {
        assert!(completions.try_receive().is_err());
        assert!(lifecycle.begin(BufferId::A).is_err());
        b.buffer()[0] = b.buffer()[0].wrapping_add(1);
        assert_eq!(request.pixels()[0], 123);
    }
    assert!(completions.try_send(request).is_ok());
    // Completion remains available until consumed; there is no latest-wins slot.
    let completed = completions.try_receive().unwrap();
    lifecycle.complete(completed.token()).unwrap();
    let mut a = completed.into_lease();
    a.buffer()[0] = 55;
    assert!(completions.try_receive().is_err());
    assert_eq!(a.buffer()[0], 55);
}

/// Queued orientation survives new UI selection and discarded later renders until its own completion.
#[test]
fn queued_orientation_survives_new_selection_and_delayed_completion() {
    use dashboard_config::Orientation;
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::display::damage::{DamagePlan, PanelHistory};
    use dashboard_host_validation::display::pipeline::FrameLease;
    use dashboard_host_validation::display::telemetry::RenderTrace;
    let mut a = Box::<FrameBuffer>::default();
    let mut b = Box::<FrameBuffer>::default();
    let [a, _b] = FrameLease::pair(&mut a, &mut b);
    let mut lifecycle = FrameLifecycle::new();
    let mut history = PanelHistory::new();
    let token = lifecycle.begin(a.id()).unwrap();
    let mut desired = Orientation::Portrait;
    history.begin_render_in(token, desired).unwrap();
    lifecycle.ready(token).unwrap();
    lifecycle.queue(token).unwrap();
    history.queue(token, None).unwrap();
    let request = a.submit_plan_in(
        token,
        RenderTrace::instant(10),
        DamagePlan::full_in(desired.geometry()),
        desired,
    );
    desired = Orientation::LandscapeInverted;
    assert_eq!(request.orientation(), Orientation::Portrait);
    lifecycle.started(token).unwrap();
    // Rendering and discarding a newer maintenance frame cannot change the old
    // submitted orientation, completed history, or ownership.
    let discarded = lifecycle.begin(BufferId::B).unwrap();
    history.begin_render_in(discarded, desired).unwrap();
    lifecycle.ready(discarded).unwrap();
    lifecycle.discard(discarded).unwrap();
    assert!(history.complete(discarded).is_err());
    assert_eq!(history.panel_orientation(), None);
    lifecycle.complete(request.token()).unwrap();
    history.complete(request.token()).unwrap();
    assert_eq!(history.panel_orientation(), Some(Orientation::Portrait));
    assert_eq!(history.reference_for(desired), None);
    assert_eq!(desired, Orientation::LandscapeInverted);
}

/// Submission panics when immutable orientation and damage-plan geometry disagree.
#[test]
#[should_panic(expected = "frame orientation and plan geometry differ")]
fn queued_request_rejects_geometry_mismatch() {
    use dashboard_config::Orientation;
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::display::damage::DamagePlan;
    use dashboard_host_validation::display::pipeline::{FrameLease, FrameToken};
    use dashboard_host_validation::display::telemetry::RenderTrace;
    let mut a = Box::<FrameBuffer>::default();
    let mut b = Box::<FrameBuffer>::default();
    let [a, _b] = FrameLease::pair(&mut a, &mut b);
    let token = FrameToken {
        buffer: a.id(),
        sequence: 1,
    };
    a.submit_plan_in(
        token,
        RenderTrace::instant(10),
        DamagePlan::full(),
        Orientation::Portrait,
    );
}

/// Off waits for the rotated clear frame; old popup completions and post-recovery completions cannot darken it.
#[test]
fn dark_transition_remains_bound_to_the_rotated_clear_frame() {
    use dashboard_config::Orientation;
    use dashboard_host_validation::backlight::Backlight;
    use dashboard_host_validation::display::damage::PanelHistory;
    use dashboard_host_validation::pages::Page;
    let mut lifecycle = FrameLifecycle::new();
    let mut history = PanelHistory::new();
    let mut light = Backlight::new();
    let popup = light.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
    let popup_token = lifecycle.begin(BufferId::A).unwrap();
    history.begin_render_in(popup_token, Orientation::Landscape).unwrap();
    lifecycle.ready(popup_token).unwrap();
    history.queue(popup_token, None).unwrap();
    lifecycle.queue(popup_token).unwrap();
    light.frame_submitted(popup_token);
    lifecycle.started(popup_token).unwrap();
    lifecycle.complete(popup_token).unwrap();
    history.complete(popup_token).unwrap();
    light.frame_completed(popup_token, 200);
    assert!(light.expire_popup(popup, 2200));
    let clear_token = lifecycle.begin(BufferId::B).unwrap();
    history
        .begin_render_in(clear_token, Orientation::PortraitInverted)
        .unwrap();
    assert_eq!(history.reference_for(Orientation::PortraitInverted), None);
    lifecycle.ready(clear_token).unwrap();
    history.queue(clear_token, None).unwrap();
    lifecycle.queue(clear_token).unwrap();
    light.frame_submitted(clear_token);
    lifecycle.started(clear_token).unwrap();
    light.frame_completed(popup_token, 2201);
    assert_ne!(light.compare(), 0);
    assert_eq!(history.panel_orientation(), Some(Orientation::Landscape));
    lifecycle.complete(clear_token).unwrap();
    history.complete(clear_token).unwrap();
    light.frame_completed(clear_token, 2300);
    assert_eq!(light.compare(), 0);
    assert_eq!(history.panel_orientation(), Some(Orientation::PortraitInverted));
    light.wake_full();
    light.frame_completed(clear_token, 2301);
    assert_ne!(light.compare(), 0); // Protected wake cannot be undone by stale completion.
}
