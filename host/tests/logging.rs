use core::fmt::Write;

use dashboard_host_validation::logging::{Entry, LOG_ENTRIES, Level, LogBuffer, LogStore, Message, VISIBLE_LINES};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

fn entry(n: u64) -> Entry { Entry::new(Level::Info, n, format_args!("record {n}")) }

#[test]
fn message_truncation_preserves_a_utf8_prefix_and_reports_format_failure() {
    for input in [
        "a".repeat(40),
        "a".repeat(41),
        "🦀".repeat(11),
        format!("{}🦀z", "a".repeat(38)),
    ] {
        let e = Entry::new(Level::Info, u64::MAX, format_args!("{input}"));
        assert!(input.starts_with(e.message.as_str()));
        assert!(e.message.as_str().len() <= 40);
        assert_eq!(e.message.truncated(), input.len() > 40);
        assert_eq!(e.timestamp_ms, u64::MAX);
    }
    let mut message = Message::new();
    assert!(write!(message, "{}", "b".repeat(41)).is_err());
    assert!(write!(message, "x").is_err());
    assert_eq!(message.as_str(), "b".repeat(40));
}

#[test]
fn sequence_anchor_survives_full_ring_insertions_and_clamps_after_eviction() {
    let mut ring = LogBuffer::new();
    assert!(ring.window(None).entries.is_empty());
    assert_eq!(ring.scroll(None, i32::MIN), None);
    for n in 1..=LOG_ENTRIES as u64 {
        assert!(ring.push(entry(n)));
    }
    let anchor = ring.scroll(None, -20);
    let old = ring.window(anchor);
    assert_eq!(old.entries.len(), VISIBLE_LINES);
    assert_eq!(old.entries.last().unwrap().sequence, 108);
    for n in 129..=135 {
        ring.push(entry(n));
    }
    let stable = ring.window(anchor);
    assert_eq!(old.entries, stable.entries);
    assert_eq!(stable.overwritten, 7);
    assert_eq!(ring.scroll(anchor, i32::MAX), None);
    let oldest = ring.scroll(None, i32::MIN);
    assert_eq!(ring.window(oldest).entries.first().unwrap().sequence, 8);
    for n in 136..=300 {
        ring.push(entry(n));
    }
    let clamped = ring.window(anchor);
    assert_eq!(clamped.entries.first().unwrap().sequence, 173);
    assert_eq!(clamped.entries.last().unwrap().sequence, 173 + VISIBLE_LINES as u64 - 1);
    assert_eq!(clamped.overwritten, 172);
}

#[test]
fn fault_record_survives_informational_floods_and_counters_account_for_loss() {
    let store = LogStore::<CriticalSectionRawMutex>::new();
    store.push(Level::Fault, 1, format_args!("critical condition"));
    for n in 0..400 {
        store.push(Level::Info, n, format_args!("{}", "a".repeat(41)));
    }
    let fault = store.fault().unwrap();
    assert_eq!(fault.message.as_str(), "critical condition");
    let window = store.window(None).unwrap();
    assert_eq!(window.newest, Some(401));
    assert_eq!(window.overwritten, 401 - LOG_ENTRIES as u64);
    let counters = store.counters();
    assert_eq!(counters.truncated, 400);
    assert_eq!(counters.contention, 0);
    store.push(Level::Fault, 1000, format_args!("new failure"));
    assert_eq!(store.fault().unwrap().timestamp_ms, 1000);
    assert_eq!(store.counters().fault_replacements, 1);
}

#[test]
fn simultaneous_producers_account_for_every_attempt_without_waiting_for_history() {
    let store = LogStore::<CriticalSectionRawMutex>::new();
    std::thread::scope(|scope| {
        for core in 0..2 {
            let store = &store;
            scope.spawn(move || {
                for n in 0..5000 {
                    store.push(Level::Info, n, format_args!("core {core} n {n}"));
                }
            });
        }
    });
    let window = store.window(None).unwrap();
    let counters = store.counters();
    assert_eq!(window.newest.unwrap() + u64::from(counters.contention), 10_000);
    assert_eq!(counters.exhausted, 0);
    assert_eq!(window.entries.len(), VISIBLE_LINES);
    for pair in window.entries.windows(2) {
        assert_eq!(pair[1].sequence, pair[0].sequence + 1);
    }
}

#[test]
fn control_characters_cannot_draw_outside_their_log_row() {
    use dashboard_host_validation::AlignedFrame;
    use dashboard_host_validation::logging_view::LogPage;
    use dashboard_host_validation::renderer::St7789Renderer;
    let store = LogStore::<CriticalSectionRawMutex>::new();
    let safe = LogStore::<CriticalSectionRawMutex>::new();
    store.push(Level::Info, 123, format_args!("A\nB\rC\tD"));
    safe.push(Level::Info, 123, format_args!("A?B?C?D"));
    let mut actual = AlignedFrame::boxed();
    let mut expected = AlignedFrame::boxed();
    LogPage::capture(&store, None).draw_full(&mut St7789Renderer::new(&mut actual.0));
    LogPage::capture(&safe, None).draw_full(&mut St7789Renderer::new(&mut expected.0));
    assert!(actual.0 == expected.0);
}

#[test]
fn captured_page_is_stable_during_production_and_clears_both_framebuffers() {
    use dashboard_host_validation::AlignedFrame;
    use dashboard_host_validation::logging_view::LogPage;
    use dashboard_host_validation::renderer::St7789Renderer;
    let store = LogStore::<CriticalSectionRawMutex>::new();
    store.push(Level::Fault, u64::MAX, format_args!("explicit failure"));
    for n in 0..128 {
        store.push(Level::Info, n, format_args!("0123456789012345678901234567890123456789"));
    }
    let page = LogPage::capture(&store, None);
    let mut reference = AlignedFrame::boxed();
    page.draw_full(&mut St7789Renderer::new(&mut reference.0));
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for frame in &mut frames {
        frame.0.fill(0xCC);
        let mut display = St7789Renderer::new(&mut frame.0);
        page.draw_header(&mut display);
        for row in 0..=VISIBLE_LINES {
            // Models an await between rows: no retained storage guard and no
            // accidental mixture of snapshots even while the ring wraps.
            for n in 0..30 {
                store.push(Level::Info, n, format_args!("new"));
            }
            page.draw_row(&mut display, row);
        }
        page.draw_footer(&mut display);
        assert!(frame.0 == reference.0);
    }
    let new_page = LogPage::capture(&store, None);
    let mut new_reference = AlignedFrame::boxed();
    new_page.draw_full(&mut St7789Renderer::new(&mut new_reference.0));
    assert!(reference.0[16 * 320 * 2..31 * 320 * 2] == new_reference.0[16 * 320 * 2..31 * 320 * 2]);
    for frame in &mut frames {
        new_page.draw_full(&mut St7789Renderer::new(&mut frame.0));
        assert!(frame.0 == new_reference.0);
    }
    // An invalid row index is inert, including usize::MAX.
    new_page.draw_row(&mut St7789Renderer::new(&mut frames[0].0), usize::MAX);
    assert!(frames[0].0 == new_reference.0);
}
