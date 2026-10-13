use dashboard_configurator::events::*;

fn input(
    message: &str,
    severity: Severity,
    elapsed_ms: u64,
) -> EventInput<'_> {
    EventInput {
        category: Category::Editor,
        severity,
        code: "editor.accepted",
        subject: None,
        message,
        elapsed_ms,
        wall_clock_ms: None,
    }
}

#[test]
fn rolling_storage_reports_eviction_and_exact_filtered_omissions() {
    let mut store = EventStore::default();
    let anchor = store.record(input("old", Severity::Info, 0)).unwrap().seq;
    for n in 1..513 {
        store.record(input(&format!("event {n}"), Severity::Info, n)).unwrap();
    }
    let snapshot = store.snapshot(&EventFilter::new("event").limit(3));
    assert_eq!(snapshot.records().len(), 3);
    assert_eq!(snapshot.matching, 512);
    assert_eq!(snapshot.omitted, 509);
    assert_eq!(snapshot.oldest.unwrap().get(), 2);
    assert_eq!(snapshot.newest.unwrap().get(), 513);
    assert_eq!(snapshot.counters.dropped.value, 1);
    assert_eq!(snapshot.counters.inserted.value, 513);
    assert!(!store.contains(anchor));
    assert_eq!(snapshot.records()[0].message, "event 1");
}

#[test]
fn utf8_message_and_subject_caps_preserve_boundaries_and_flags() {
    for tail in ["é", "€", "🦀"] {
        let mut store = EventStore::default();
        let message = format!("{}{}", "x".repeat(511), tail);
        let subject = format!("{}{}", "s".repeat(127), tail);
        let mut event = input(&message, Severity::Error, 1);
        event.subject = Some(&subject);
        let seq = store.record(event).unwrap().seq;
        let retained = store.get(seq).unwrap();
        assert_eq!(retained.message.len(), 511);
        assert_eq!(retained.subject.as_ref().unwrap().len(), 127);
        assert!(retained.message_truncated && retained.subject_truncated);
    }
    let mut store = EventStore::default();
    let message = "🦀".repeat(128);
    let seq = store.record(input(&message, Severity::Info, 0)).unwrap().seq;
    assert_eq!(store.get(seq).unwrap().message.len(), 512);
    assert!(!store.get(seq).unwrap().message_truncated);
}

#[test]
fn exact_errors_coalesce_but_all_key_differences_remain_visible() {
    let mut store = EventStore::default();
    let first = store.record(input("failure", Severity::Error, 10)).unwrap();
    let repeat = store.record(input("failure", Severity::Error, 20)).unwrap();
    assert_eq!(repeat.kind, RecordKind::Repeated);
    assert_eq!(repeat.seq, first.seq);
    assert_eq!(repeat.console, ConsoleDisposition::Coalesced);
    let record = store.get(first.seq).unwrap();
    assert_eq!(record.count.value, 2);
    assert_eq!(record.elapsed_ms, 10);
    assert_eq!(record.last_elapsed_ms, 20);
    for variant in 0..5 {
        let mut next = input("failure", Severity::Error, 30);
        match variant {
            0 => next.category = Category::Storage,
            1 => next.severity = Severity::Warn,
            2 => next.code = "editor.rejected",
            3 => next.subject = Some(""),
            _ => next.message = "different failure",
        }
        assert_eq!(store.record(next).unwrap().kind, RecordKind::Inserted);
    }
    assert_eq!(store.snapshot(&EventFilter::default()).records().len(), 6);
    assert_eq!(store.counters().repeated.value, 1);
}

#[test]
fn oldest_coalescer_key_is_evicted_and_event_eviction_removes_keys() {
    let mut store = EventStore::default();
    for n in 0..65 {
        store
            .record(input(&format!("failure {n}"), Severity::Error, n))
            .unwrap();
    }
    assert_eq!(
        store.record(input("failure 0", Severity::Error, 70)).unwrap().kind,
        RecordKind::Inserted
    );
    assert_eq!(
        store.record(input("failure 64", Severity::Error, 71)).unwrap().kind,
        RecordKind::Repeated
    );
    for n in 0..512 {
        store
            .record(input(&format!("routine {n}"), Severity::Info, 80))
            .unwrap();
    }
    assert_eq!(
        store.record(input("failure 64", Severity::Error, 90)).unwrap().kind,
        RecordKind::Inserted
    );
}

#[test]
fn truncated_causes_never_claim_exact_equality_even_for_same_oversized_input() {
    let mut store = EventStore::default();
    let prefix = "x".repeat(512);
    for message in [format!("{prefix}a"), format!("{prefix}b"), format!("{prefix}a")] {
        assert_eq!(
            store.record(input(&message, Severity::Error, 0)).unwrap().kind,
            RecordKind::Inserted
        );
    }
    let subject = "s".repeat(129);
    for _ in 0..2 {
        let mut event = input("same", Severity::Warn, 1);
        event.subject = Some(&subject);
        assert_eq!(store.record(event).unwrap().kind, RecordKind::Inserted);
    }
    assert_eq!(store.counters().repeated.value, 0);
}

#[test]
fn clear_keeps_sequence_counters_and_console_rate_lineage() {
    let mut store = EventStore::default();
    store.set_debug_console(true);
    for _ in 0..10 {
        assert_eq!(
            store.record(input("trace", Severity::Trace, 0)).unwrap().console,
            ConsoleDisposition::Emit
        );
    }
    let before = store.snapshot(&EventFilter::default()).newest.unwrap();
    store.clear();
    assert!(store.snapshot(&EventFilter::default()).records().is_empty());
    let outcome = store.record(input("trace", Severity::Debug, 0)).unwrap();
    assert!(outcome.seq.get() > before.get());
    assert_eq!(outcome.console, ConsoleDisposition::RateLimited);
    assert_eq!(store.counters().inserted.value, 12); // ten originals, new debug, Info summary
}

#[test]
fn debug_trace_share_opt_in_rolling_budget_and_info_summaries_stay_retained() {
    let mut store = EventStore::default();
    assert_eq!(
        store.record(input("debug", Severity::Debug, 0)).unwrap().console,
        ConsoleDisposition::RetainedOnly
    );
    store.set_debug_console(true);
    for n in 0..10 {
        let severity = if n % 2 == 0 { Severity::Debug } else { Severity::Trace };
        assert_eq!(
            store.record(input("debug", severity, 0)).unwrap().console,
            ConsoleDisposition::Emit
        );
    }
    for ms in [0, 999] {
        assert_eq!(
            store.record(input("debug", Severity::Debug, ms)).unwrap().console,
            ConsoleDisposition::RateLimited
        );
    }
    assert_eq!(
        store.record(input("debug", Severity::Trace, 1000)).unwrap().console,
        ConsoleDisposition::Emit
    );
    let snapshot = store.snapshot(&EventFilter::new("rate limit"));
    assert_eq!(snapshot.records().len(), 1);
    assert_eq!(snapshot.records()[0].severity, Severity::Info);
    assert_eq!(snapshot.records()[0].count.value, 2);
    assert_eq!(snapshot.counters.console_suppressed.value, 2);
    assert_eq!(
        store.record(input("visible", Severity::Warn, 1000)).unwrap().console,
        ConsoleDisposition::Emit
    );
    assert_eq!(
        store
            .record(input("visible error", Severity::Error, 1000))
            .unwrap()
            .console,
        ConsoleDisposition::Emit
    );
}

#[test]
fn backward_elapsed_and_wall_clocks_cannot_rewind_history_or_reopen_console() {
    let mut store = EventStore::default();
    store.set_debug_console(true);
    for _ in 0..10 {
        store.record(input("debug", Severity::Debug, 1000)).unwrap();
    }
    assert_eq!(
        store.record(input("debug", Severity::Debug, 0)).unwrap().console,
        ConsoleDisposition::RateLimited
    );
    let first = store.record(input("cause", Severity::Error, 2000)).unwrap().seq;
    let mut next = input("cause", Severity::Error, 1);
    next.wall_clock_ms = Some(-1);
    store.record(next).unwrap();
    assert_eq!(store.get(first).unwrap().last_elapsed_ms, 2000);
    assert_eq!(store.get(first).unwrap().last_wall_clock_ms, Some(-1));
}

#[test]
fn invalid_codes_do_not_consume_identity_or_console_allowance() {
    let mut store = EventStore::default();
    store.set_debug_console(true);
    for code in ["", "ABC", "white space", "é", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"] {
        let mut event = input("debug", Severity::Debug, 9000);
        event.code = code;
        assert_eq!(store.record(event), Err(EventError::InvalidCode));
    }
    let outcome = store.record(input("debug", Severity::Debug, 0)).unwrap();
    assert_eq!(outcome.seq.get(), 1);
    assert_eq!(outcome.console, ConsoleDisposition::Emit);
    assert_eq!(store.get(outcome.seq).unwrap().elapsed_ms, 0);
}

#[test]
fn jsonl_escapes_control_data_and_keeps_exact_integer_and_optional_timestamps() {
    let mut store = EventStore::default();
    let message = "<script>\"\\\n\r\0🦀";
    let mut event = input(message, Severity::Info, u64::MAX);
    event.wall_clock_ms = Some(i64::MIN);
    store.record(event).unwrap();
    store.record(input("absent clock", Severity::Info, u64::MAX)).unwrap();
    let bytes = store.snapshot(&EventFilter::default()).jsonl().unwrap();
    let lines = std::str::from_utf8(&bytes).unwrap().lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let json: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(json["message"], message);
    assert_eq!(json["elapsed_ms"].as_u64(), Some(u64::MAX));
    assert_eq!(json["wall_clock_ms"].as_i64(), Some(i64::MIN));
    let absent: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert!(absent["wall_clock_ms"].is_null());
    assert!(!bytes.contains(&0));
    assert!(bytes.len() <= 2 * 1024 * 1024);
}

#[test]
fn severity_category_and_bounded_search_filter_retained_records() {
    let mut store = EventStore::default();
    for severity in [Severity::Info, Severity::Warn, Severity::Error] {
        store.record(input("Mixed CASE", severity, 0)).unwrap();
    }
    let snapshot = store.snapshot(
        &EventFilter::new("mixed case")
            .minimum(Severity::Warn)
            .category(Category::Editor),
    );
    assert_eq!(snapshot.matching, 2);
    assert_eq!(snapshot.omitted, 0);
    assert_eq!(snapshot.records()[0].severity, Severity::Warn);
    assert!(EventFilter::new(&"🦀".repeat(33)).query_truncated());
}

#[test]
fn escape_expansion_can_exceed_jsonl_limit_and_returns_no_partial_success() {
    let mut store = EventStore::default();
    let message = "\0".repeat(512);
    let subject = "\0".repeat(128);
    for _ in 0..512 {
        let mut event = input(&message, Severity::Info, u64::MAX);
        event.subject = Some(&subject);
        event.wall_clock_ms = Some(i64::MIN);
        store.record(event).unwrap();
    }
    assert_eq!(
        store.snapshot(&EventFilter::default()).jsonl(),
        Err(JsonlError::TooLarge)
    );
    let smaller = store.snapshot(&EventFilter::default().limit(1)).jsonl().unwrap();
    assert!(smaller.len() < 2 * 1024 * 1024);
}
