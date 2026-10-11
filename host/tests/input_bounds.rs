use dashboard_host_validation::input_model::{
    DebouncedButton,
    EncoderEvents,
    brightness_after,
    decode_encoder_delta,
    scroll_after,
    valid_encoder_identity,
};

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
