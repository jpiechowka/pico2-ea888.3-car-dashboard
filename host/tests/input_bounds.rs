//! Bounded encoder events, wire identity and sampled debounce/hold admission.
use dashboard_host_validation::input_model::{
    DebouncedButton,
    EncoderEvents,
    brightness_after,
    decode_encoder_delta,
    scroll_after,
    valid_encoder_identity,
};

/// Checks Seesaw identity's upper version word, big-endian signed delta reversal and the ±128 admission bound.
#[test]
fn encoder_identity_endianness_direction_and_corruption_are_checked() {
    assert!(valid_encoder_identity(0x55, (4991u32 << 16).to_be_bytes()));
    assert!(!valid_encoder_identity(0, (4991u32 << 16).to_be_bytes()));
    assert!(!valid_encoder_identity(0x55, (5880u32 << 16).to_be_bytes()));
    assert!(!valid_encoder_identity(0x55, 4991u32.to_be_bytes()));
    assert_eq!(decode_encoder_delta((-1i32).to_be_bytes()), Some(1));
    assert_eq!(decode_encoder_delta(128i32.to_be_bytes()), Some(-128));
    for delta in [129i32, -129, i32::MAX, i32::MIN] {
        assert_eq!(decode_encoder_delta(delta.to_be_bytes()), None);
    }
}

/// Rotation/press queues saturate at 128/eight, account losses, and drain or clear each pending event once.
#[test]
fn pending_events_are_bounded_counted_and_consumed_once() {
    let events = EncoderEvents::new();
    events.publish_delta(100);
    events.publish_delta(100);
    assert_eq!(events.take().0, 128);
    assert_eq!(events.dropped(), 1);
    events.publish_delta(i32::MIN);
    assert_eq!(events.take(), (0, 0));
    for _ in 0..10 {
        events.publish_press();
    }
    assert_eq!(events.take(), (0, 8));
    assert_eq!(events.dropped(), 4);
    assert_eq!(events.take(), (0, 0));
    events.publish_press();
    events.publish_delta(-128);
    events.clear();
    assert_eq!(events.take(), (0, 0));
}

/// Brightness and scrolling clamp correctly even with extreme signed deltas and malformed initial positions.
#[test]
fn brightness_and_scroll_arithmetic_remains_bounded_at_integer_extremes() {
    assert_eq!(brightness_after(50, 1), 55);
    assert_eq!(brightness_after(50, -1), 45);
    for current in [0, 50, 100, u32::MAX] {
        assert_eq!(brightness_after(current, i32::MAX), 100);
        assert_eq!(brightness_after(current, i32::MIN), 0);
    }
    assert_eq!(scroll_after(10, 3, 100), 7);
    assert_eq!(scroll_after(i32::MAX, i32::MIN, usize::MAX), i32::MAX);
    assert_eq!(scroll_after(i32::MIN, i32::MAX, 100), 0);
    assert_eq!(scroll_after(30, -5, 32), 32);
    assert_eq!(scroll_after(30, -5, 0), 0);
}

/// Fifty-millisecond stable press edges reject bounce, held startup/reconnect and backward release observations.
#[test]
fn button_needs_stable_edges_and_suppresses_held_reconnects() {
    let mut button = DebouncedButton::new();
    assert!(!button.update(true, 0));
    assert!(!button.update(true, 1000));
    assert!(!button.update(false, 1010));
    assert!(!button.update(false, 1060));
    assert!(!button.update(true, 1070));
    assert!(!button.update(false, 1080));
    assert!(!button.update(true, 1090));
    assert!(!button.update(true, 1139));
    assert!(button.update(true, 1140));
    assert!(!button.update(true, 5000));
    assert!(!button.update(false, 4999)); // backwards clock cannot release
    assert!(!button.update(true, 5100));
    button = DebouncedButton::new();
    assert!(!button.update(true, 6000));
    assert!(!button.update(true, 6100));
}

/// A fully debounced release rearms the next press even near the u64 millisecond limit.
#[test]
fn normal_release_allows_a_second_press_including_large_timestamps() {
    let mut button = DebouncedButton::new();
    let base = u64::MAX - 200;
    for (pressed, at, edge) in [
        (false, 0, false),
        (true, 10, false),
        (true, 60, true),
        (false, 70, false),
        (false, 120, false),
        (true, 130, false),
        (true, 180, true),
    ] {
        assert_eq!(button.update(pressed, base + at), edge);
    }
}

/// Stable release appears only after the final bounce interval and cannot be fabricated by a backward clock.
#[test]
fn stable_release_survives_bounce_and_never_completes_a_phantom_short_press() {
    let mut button = DebouncedButton::new();
    assert_eq!(button.stable_pressed(), None);
    let mut armed = false;
    let mut edges = Vec::new();
    for (raw, at) in [
        (false, 0),
        (true, 10),
        (true, 60),
        (false, 100),
        (true, 110),
        (false, 200),
        (false, 249),
        (false, 250),
        (false, 251),
        (true, 300),
        (true, 350),
    ] {
        if button.update(raw, at) {
            armed = true;
            edges.push((true, at));
        }
        if armed && button.stable_pressed() == Some(false) {
            armed = false;
            edges.push((false, at));
        }
    }
    assert_eq!(edges, [(true, 60), (false, 250), (true, 350)]);
    assert_eq!(button.stable_pressed(), Some(true));
    button.update(false, 100); // a reversed clock cannot release the current press
    assert_eq!(button.stable_pressed(), Some(true));
    let mut held_at_boot = DebouncedButton::new();
    assert!(!held_at_boot.update(true, 0));
    assert_eq!(held_at_boot.stable_pressed(), Some(true));
    assert!(!held_at_boot.update(false, 1));
    assert!(!held_at_boot.update(false, 51));
    assert_eq!(held_at_boot.stable_pressed(), Some(false));
}

/// Deferred gestures distinguish short release, one-shot hold, late held release and short sampled release after a
/// stall.
#[test]
fn deferred_buttons_separate_short_release_long_hold_and_a_late_held_release() {
    use dashboard_host_validation::input_model::{ButtonEdges, DeferredButton};
    let mut button = DeferredButton::new();
    assert_eq!(button.update(false, 0), ButtonEdges::default());
    button.update(true, 10);
    assert_eq!(
        button.update(true, 60),
        ButtonEdges {
            pressed: true,
            ..ButtonEdges::default()
        }
    );
    assert_eq!(button.update(false, 100), ButtonEdges::default());
    assert_eq!(button.update(true, 110), ButtonEdges::default());
    assert_eq!(button.update(false, 120), ButtonEdges::default());
    assert_eq!(
        button.update(false, 170),
        ButtonEdges {
            released: true,
            short: true,
            ..ButtonEdges::default()
        }
    );
    button.update(true, 200);
    button.update(true, 250);
    assert_eq!(button.update(true, 2249), ButtonEdges::default());
    assert_eq!(
        button.update(true, 2250),
        ButtonEdges {
            held: true,
            ..ButtonEdges::default()
        }
    );
    assert_eq!(button.update(true, 2251), ButtonEdges::default());
    button.update(false, 2300);
    assert_eq!(
        button.update(false, 2350),
        ButtonEdges {
            released: true,
            ..ButtonEdges::default()
        }
    );
    button.update(true, 2400);
    button.update(true, 2450);
    button.update(false, 5000);
    assert_eq!(
        button.update(false, 5050),
        ButtonEdges {
            released: true,
            held: true,
            ..ButtonEdges::default()
        }
    );
    assert_eq!(button.update(true, 4999), ButtonEdges::default());
    button.update(true, 5100);
    button.update(true, 5150);
    button.update(false, 5200);
    assert_eq!(
        button.update(false, 9000),
        ButtonEdges {
            released: true,
            short: true,
            ..ButtonEdges::default()
        }
    );
    let mut boot = DeferredButton::new();
    assert_eq!(boot.update(true, 0), ButtonEdges::default());
    assert_eq!(boot.update(true, 3000), ButtonEdges::default());
    boot.update(false, 4000);
    assert_eq!(boot.update(false, 4050), ButtonEdges::default());
}

/// Concurrent producer/consumer accounting conserves all 100000 published observations as consumed or dropped.
#[test]
fn concurrent_input_is_consumed_or_counted_as_dropped_without_wrap() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let events = EncoderEvents::new();
    let done = AtomicBool::new(false);
    let mut consumed = 0u64;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..50_000 {
                events.publish_delta(1);
                events.publish_press();
            }
            done.store(true, Ordering::Release);
        });
        while !done.load(Ordering::Acquire) {
            let (delta, presses) = events.take();
            assert!((0..=128).contains(&delta));
            assert!(presses <= 8);
            consumed += delta as u64 + u64::from(presses);
            std::thread::yield_now();
        }
    });
    let (delta, presses) = events.take();
    consumed += delta as u64 + u64::from(presses);
    assert_eq!(consumed + u64::from(events.dropped()), 100_000);
}

/// Values and oldest observations drain atomically; stale publication is rejected and cancelling deltas retain event
/// counts.
#[test]
fn encoder_values_and_software_observations_are_consumed_coherently() {
    use dashboard_host_validation::input_trace::Observation;
    let events = EncoderEvents::new();
    events.publish_delta_at(2, 100);
    events.publish_press_at(Observation::new(90, 150).unwrap());
    events.publish_delta_at(1, 149); // stale producer time cannot modify data
    let batch = events.take_observed();
    assert_eq!((batch.delta, batch.presses), (2, 1));
    assert_eq!(batch.motion.unwrap().observed_us, 100);
    assert_eq!(batch.button.unwrap().observed_us, 90);
    assert_eq!(batch.button.unwrap().accepted_us, 150);
    assert_eq!(events.dropped(), 1);
    let empty = events.take_observed();
    assert_eq!(
        (empty.delta, empty.presses, empty.motion, empty.button),
        (0, 0, None, None)
    );
    events.publish_delta_at(-1, 160);
    events.publish_delta_at(1, 170);
    let cancelled = events.take_observed();
    assert_eq!(cancelled.delta, 0);
    assert_eq!(cancelled.motion.unwrap().events, 2); // count no-effect observations explicitly
    events.publish_press_at(Observation::new(180, 230).unwrap());
    events.clear();
    assert_eq!(events.discarded(), 1);
}
