use dashboard_configurator::event_adapters::*;
use dashboard_configurator::events::*;

#[test]
fn event_categories_have_readable_labels_without_changing_wire_tags() {
    assert_eq!(Category::ImportExport.label(), "Import / export");
    assert_eq!(Category::HostTools.label(), "Host tools");
    assert_eq!(Category::Editor.label(), "Editor");
    let encoded = serde_json::to_string(&Category::ImportExport).unwrap();
    assert_eq!(encoded, "\"ImportExport\"");
}

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

#[derive(Default)]
struct CapturedConsole(Vec<(Severity, String)>);
impl ConsoleSink for CapturedConsole {
    fn write(
        &mut self,
        severity: Severity,
        text: &str,
    ) {
        self.0.push((severity, text.to_owned()));
    }
}

#[test]
fn adapter_emits_only_current_store_admission_and_disabled_console_is_quiet() {
    let mut store = EventStore::default();
    let mut console = CapturedConsole::default();
    emit(&mut store, &mut console, false, input("off", Severity::Error, 0)).unwrap();
    assert!(console.0.is_empty());
    emit(&mut store, &mut console, true, input("info", Severity::Info, 1)).unwrap();
    emit(&mut store, &mut console, true, input("warn", Severity::Warn, 2)).unwrap();
    emit(&mut store, &mut console, true, input("warn", Severity::Warn, 3)).unwrap();
    assert_eq!(console.0.len(), 1);
    assert_eq!(store.counters().repeated.value, 1);
    store.set_debug_console(true);
    for n in 0..12 {
        emit(&mut store, &mut console, true, input("debug", Severity::Debug, 4 + n)).unwrap();
    }
    assert_eq!(console.0.len(), 11);
    assert_eq!(store.counters().console_suppressed.value, 2);
    assert!(store.snapshot(&EventFilter::new("rate limit")).matching > 0);
}

#[test]
fn console_control_characters_cannot_forge_another_row() {
    let mut store = EventStore::default();
    let mut console = CapturedConsole::default();
    emit(
        &mut store,
        &mut console,
        true,
        input("<script>%s\n\r\u{1b} λ", Severity::Error, 0),
    )
    .unwrap();
    let text = &console.0[0].1;
    assert!(text.contains("<script>%s"));
    assert!(text.contains("λ"));
    assert!(!text.contains('\n') && !text.contains('\r') && !text.contains('\u{1b}'));
    assert_eq!(
        store.snapshot(&EventFilter::default()).records()[0].message,
        "<script>%s\n\r\u{1b} λ"
    );
}

#[test]
fn storage_areas_fail_and_recover_independently() {
    use dashboard_configurator::editor::StorageFailure;
    let mut adapter = AdapterState::default();
    let draft = adapter
        .storage(StorageArea::Draft, Err(StorageIssue::Failure(StorageFailure::Quota)))
        .unwrap();
    assert_eq!(draft.category, Category::Storage);
    assert_eq!(draft.severity, Severity::Warn);
    assert!(
        adapter
            .storage(StorageArea::Preferences, Err(StorageIssue::Corrupt))
            .is_some()
    );
    assert!(
        adapter
            .storage(StorageArea::Presets, Err(StorageIssue::Conflict))
            .is_some()
    );
    assert!(adapter.storage(StorageArea::Draft, Ok(())).is_some());
    assert!(adapter.storage(StorageArea::Draft, Ok(())).is_none());
    assert!(adapter.storage(StorageArea::Preferences, Ok(())).is_some());
    assert!(adapter.storage(StorageArea::Presets, Ok(())).is_some());
}

#[test]
fn import_mapping_retains_fixed_rejection_reasons_without_input_content() {
    use dashboard_configurator::editor::{Editor, EditorError, ImportFormat};
    let mut editor = Editor::default();
    let token = editor.begin_import().unwrap();
    let error = editor
        .complete_import(token, ImportFormat::Json, br#"{"secret":"CAPABILITY_ENV_C:\\private"}"#)
        .unwrap_err();
    let rejected = import_outcome(Err(ImportFailure::Rejected(error)));
    assert_eq!(rejected.category, Category::ImportExport);
    assert_eq!(rejected.severity, Severity::Warn);
    assert!(!rejected.message.contains("CAPABILITY") && !rejected.message.contains("private"));
    assert!(import_outcome(Err(ImportFailure::TooLarge)).message.contains("128 KiB"));
    assert!(import_outcome(Err(ImportFailure::Read)).message.contains("read"));
    assert!(
        import_outcome(Err(ImportFailure::Rejected(EditorError::JsonPending)))
            .message
            .contains("pending")
    );
}

#[test]
fn repeated_invalid_keystrokes_and_navigation_emit_no_acceptance() {
    use dashboard_configurator::editor::{Editor, RawField, Selection};
    let mut editor = Editor::default();
    let before = editor.has_invalid_draft();
    let token = editor.field_token();
    editor
        .set_raw(token, RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    assert!(validation_transition(before, editor.has_invalid_draft()).is_some());
    let token = editor.field_token();
    editor
        .set_raw(token, RawField::ControlHoldMilliseconds, "bad again".into())
        .unwrap();
    assert!(validation_transition(true, editor.has_invalid_draft()).is_none());
    editor.discard_raw().unwrap();
    assert!(validation_transition(true, editor.has_invalid_draft()).is_some());
    let accepted = editor.accepted_config_key();
    editor
        .select(Selection {
            view: editor.selection().view,
            widget: None,
        })
        .unwrap();
    assert_eq!(editor.accepted_config_key(), accepted);
    assert!(validation_transition(false, false).is_none());
}

#[test]
fn simulator_changes_use_shared_quality_control_and_reset_state() {
    use dashboard_configurator::config::ButtonId;
    use dashboard_configurator::editor::Editor;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::samples::{Quality, SensorId};
    let editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    let before = SimulationObservation::capture(&sim);
    sim.advance_to(sim.now_ms() + 250, Some([1., 90., 95., 80., 30., 700., 13.8, 1.]));
    let normal = SimulationObservation::capture(&sim);
    let _ = simulation_changes(&before, &normal);
    sim.advance_to(sim.now_ms() + 250, Some([1., 90., 95., 80., 30., 700., 13.8, 1.]));
    assert!(simulation_changes(&normal, &SimulationObservation::capture(&sim)).is_empty());
    let before = SimulationObservation::capture(&sim);
    sim.inject_fault(SensorId::Oil, Quality::TransportError, sim.now_ms() + 1);
    let changes = simulation_changes(&before, &SimulationObservation::capture(&sim));
    assert!(
        changes
            .iter()
            .any(|e| e.code == "simulation.quality" && e.subject == Some("OIL"))
    );
    let before = SimulationObservation::capture(&sim);
    sim.tap(ButtonId::A);
    assert!(!simulation_changes(&before, &SimulationObservation::capture(&sim)).is_empty());
    let before = SimulationObservation::capture(&sim);
    sim.advance(10);
    assert!(simulation_changes(&before, &SimulationObservation::capture(&sim)).is_empty());
}

#[test]
fn routine_renderer_success_is_quiet_and_failure_recovers_once() {
    let mut adapter = AdapterState::default();
    assert!(adapter.render(RenderLane::Appearance, true).is_none());
    assert!(adapter.render(RenderLane::Appearance, false).is_some());
    assert!(adapter.render(RenderLane::Appearance, false).is_none());
    assert!(adapter.render(RenderLane::Simulation, true).is_none());
    assert!(adapter.render(RenderLane::Appearance, true).is_some());
    assert!(adapter.render(RenderLane::Appearance, true).is_none());
    assert_eq!(frame_completion_error().severity, Severity::Error);
}

#[test]
fn paused_log_counts_insertions_and_reports_anchor_eviction_without_identity_reset() {
    let mut store = EventStore::default();
    store.record(input("anchor", Severity::Info, 0)).unwrap();
    let mut pause = PauseState::default();
    pause.pause(&store.snapshot(&EventFilter::default()));
    for n in 0..512 {
        store.record(input("new", Severity::Info, n + 1)).unwrap();
    }
    let report = pause.report(&store);
    assert!(report.gap);
    assert_eq!(report.new_events.value, 512);
    assert_eq!(report.anchor.unwrap().get(), 2);
    pause.resume();
    assert!(!pause.report(&store).gap);
    let newest = store.snapshot(&EventFilter::default()).newest.unwrap();
    store.clear();
    let after = store.record(input("after clear", Severity::Info, 600)).unwrap();
    assert!(after.seq.get() > newest.get());
}

#[test]
fn malformed_browser_wall_clock_never_saturates_into_plausible_timestamp() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, i64::MAX as f64, 1.5] {
        assert_eq!(checked_wall_clock(value), None);
    }
    assert_eq!(checked_wall_clock(i64::MIN as f64), Some(i64::MIN));
    assert_eq!(checked_wall_clock(1_797_000_000_000.), Some(1_797_000_000_000));
}

#[test]
fn transfer_failures_are_bounded_fixed_events_not_recursive_exports() {
    for failure in [
        TransferFailure::Clipboard,
        TransferFailure::Download,
        TransferFailure::Serialization,
    ] {
        let notice = transfer_failure(failure);
        assert_eq!(notice.severity, Severity::Warn);
        assert!(notice.message.len() <= 512);
        assert!(!notice.message.contains("C:\\"));
    }
}

#[test]
fn field_commit_is_deferred_and_external_replacement_cannot_claim_the_field() {
    use dashboard_configurator::editor::{Edit, Editor, RawField};
    let mut editor = Editor::default();
    let mut commit = FieldCommit::begin(&editor);
    for text in ["600", "700", "800"] {
        let before = editor.accepted_config_key();
        editor
            .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, text.into())
            .unwrap();
        commit.observe(before, &editor);
    }
    assert_eq!(commit.finish(&editor), Some(editor.accepted_revision()));
    assert_eq!(commit.finish(&editor), None);
    let mut commit = FieldCommit::begin(&editor);
    let before = editor.accepted_config_key();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "900".into())
        .unwrap();
    commit.observe(before, &editor);
    editor.apply(Edit::SetBootView(editor.selection().view)).unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "1000".into())
        .unwrap();
    assert_eq!(commit.finish(&editor), None);
}

#[test]
fn mapped_completion_rejection_preserves_real_presented_state() {
    use dashboard_configurator::editor::Editor;
    use dashboard_configurator::preview::Preview;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    let mut preview = Preview::new();
    let submitted = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut sim, submitted).unwrap();
    let before = (sim.completed_warning(), sim.led_duty());
    let rejected = complete_simulation_frame(
        &mut sim,
        FrameToken {
            buffer: BufferId::A,
            sequence: 2,
        },
    )
    .unwrap_err();
    assert_eq!(rejected.category, Category::Rendering);
    assert_eq!((sim.completed_warning(), sim.led_duty()), before);
    assert!(complete_simulation_frame(&mut sim, submitted).is_ok());
    assert!(complete_simulation_frame(&mut sim, submitted).is_err());
}

#[test]
fn mapped_reset_request_and_acknowledgment_follow_real_owner() {
    use dashboard_configurator::config::ButtonId;
    use dashboard_configurator::editor::Editor;
    use dashboard_configurator::simulator::ControlSimulator;
    let editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    let before = SimulationObservation::capture(&sim);
    sim.tap(ButtonId::B);
    let pending = SimulationObservation::capture(&sim);
    assert!(
        simulation_changes(&before, &pending)
            .iter()
            .any(|e| e.code == "simulation.reset_requested")
    );
    sim.acknowledge_reset();
    assert!(
        simulation_changes(&pending, &SimulationObservation::capture(&sim))
            .iter()
            .any(|e| e.code == "simulation.reset_ack")
    );
    let before = SimulationObservation::capture(&sim);
    sim.acknowledge_reset();
    assert!(simulation_changes(&before, &SimulationObservation::capture(&sim)).is_empty());
}

#[test]
fn distinct_renderer_failures_remain_visible_before_recovery() {
    let mut adapter = AdapterState::default();
    let first = adapter
        .render_result(RenderLane::Appearance, Err(RenderIssue::CanvasAllocation))
        .unwrap();
    assert!(
        adapter
            .render_result(RenderLane::Appearance, Err(RenderIssue::CanvasAllocation))
            .is_none()
    );
    let second = adapter
        .render_result(RenderLane::Appearance, Err(RenderIssue::CanvasDraw))
        .unwrap();
    assert_ne!(first.message, second.message);
    assert!(adapter.render_result(RenderLane::Appearance, Ok(())).is_some());
    assert!(adapter.render_result(RenderLane::Appearance, Ok(())).is_none());
}

#[test]
fn successful_storage_read_cannot_claim_recovery_of_failed_write() {
    use dashboard_configurator::editor::StorageFailure;
    let mut adapter = AdapterState::default();
    assert!(
        adapter
            .storage(StorageArea::Draft, Err(StorageIssue::Failure(StorageFailure::Quota)))
            .is_some()
    );
    assert!(adapter.storage_read(StorageArea::Draft, Ok(())).is_none());
    assert!(adapter.storage(StorageArea::Draft, Ok(())).is_some());
}

#[test]
fn draft_conflict_replacement_reports_actual_failure_then_one_recovery() {
    use dashboard_configurator::editor::{Editor, SaveStatus, StorageFailure};
    for failure in [
        StorageFailure::Quota,
        StorageFailure::Denied,
        StorageFailure::Unavailable,
    ] {
        let mut editor = Editor::default();
        let mut adapter = AdapterState::default();
        let mut events = EventStore::default();
        let mut console = CapturedConsole::default();
        let previous = Some("previous draft".to_owned());
        let current = Some("another tab's draft".to_owned());
        let request = editor.begin_save().unwrap();
        let conflict = attempt_draft_save(false, Ok(current.clone()), &previous, || {
            panic!("conflicting draft must not be written")
        });
        assert!(conflict.is_conflict());
        assert!(conflict.storage_result().is_none()); // conflict has its own once-only hook
        editor
            .acknowledge_save(request.token(), conflict.acknowledgment())
            .unwrap_err();
        let notice = adapter
            .storage(StorageArea::Draft, Err(StorageIssue::Conflict))
            .unwrap();
        emit(&mut events, &mut console, true, notice.input(0, None)).unwrap();

        let request = editor.begin_save().unwrap();
        let observation = if failure == StorageFailure::Unavailable {
            Err(failure)
        } else {
            Ok(current.clone())
        };
        let failed = attempt_draft_save(true, observation, &previous, || {
            assert_ne!(failure, StorageFailure::Unavailable, "failed read must not write");
            Err(failure)
        });
        assert!(!failed.is_conflict()); // the old conflict is not this operation's outcome
        editor
            .acknowledge_save(request.token(), failed.acknowledgment())
            .unwrap_err();
        assert_eq!(editor.save_status(), SaveStatus::Failed(failure));
        let notice = adapter
            .storage(StorageArea::Draft, failed.storage_result().unwrap())
            .unwrap();
        assert_eq!(notice.code, "storage.failed");
        emit(&mut events, &mut console, true, notice.input(1, None)).unwrap();
        assert_eq!(events.snapshot(&EventFilter::new("recovered")).matching, 0);
        assert_eq!(console.0.len(), 2);

        let request = editor.begin_save().unwrap();
        let saved = attempt_draft_save(true, Ok(current), &previous, || Ok(()));
        editor
            .acknowledge_save(request.token(), saved.acknowledgment())
            .unwrap();
        assert_eq!(editor.save_status(), SaveStatus::Saved);
        let notice = adapter
            .storage(StorageArea::Draft, saved.storage_result().unwrap())
            .unwrap();
        assert_eq!(notice.code, "storage.recovered");
        emit(&mut events, &mut console, true, notice.input(2, None)).unwrap();
        assert!(
            adapter
                .storage(StorageArea::Draft, saved.storage_result().unwrap())
                .is_none()
        );
        let snapshot = events.snapshot(&EventFilter::default());
        assert_eq!(snapshot.records().len(), 3);
        assert!(snapshot.records().iter().all(|e| e.count.value == 1));
        assert_eq!(events.snapshot(&EventFilter::new("recovered")).matching, 1);
    }
}

#[test]
fn preset_reload_reports_read_failure_and_recovery_without_recovering_failed_write() {
    use dashboard_configurator::appearance_presets::{PresetError, PresetStatus, PresetStore};
    use dashboard_configurator::editor::StorageFailure;
    let saved = Some(r#"{"version":1,"records":[]}"#.to_owned());
    for failure in [
        StorageFailure::Denied,
        StorageFailure::Quota,
        StorageFailure::Unavailable,
    ] {
        let mut presets = PresetStore::restore(Ok(saved.clone()));
        assert_eq!(presets.status(), PresetStatus::Saved);
        let mut adapter = AdapterState::default();
        let mut events = EventStore::default();
        let mut console = CapturedConsole::default();

        // A real failed write remains a separate recovery obligation.
        let write = presets.begin_save().unwrap();
        presets.finish_save(&write, Err(StorageFailure::Quota)).unwrap();
        assert_eq!(presets.status(), PresetStatus::Failed(StorageFailure::Quota));
        let notice = adapter
            .storage(StorageArea::Presets, Err(StorageIssue::Failure(StorageFailure::Quota)))
            .unwrap();
        emit(&mut events, &mut console, true, notice.input(0, None)).unwrap();

        let (result, read) = reload_preset_attempt(&mut presets, Err(failure));
        assert_eq!(result, Err(PresetError::Storage(failure)));
        assert_eq!(presets.status(), PresetStatus::Failed(failure));
        assert!(matches!(presets.begin_save(), Err(PresetError::Conflict)));
        let notice = adapter.storage_read(StorageArea::Presets, read).unwrap();
        assert_eq!(notice.code, "storage.failed");
        assert!(notice.message.starts_with("Read:"));
        emit(&mut events, &mut console, true, notice.input(1, None)).unwrap();

        let (result, read) = reload_preset_attempt(&mut presets, Ok(saved.clone()));
        assert_eq!(result, Ok(()));
        assert_eq!(presets.status(), PresetStatus::Saved);
        let notice = adapter.storage_read(StorageArea::Presets, read).unwrap();
        assert_eq!(notice.code, "storage.recovered");
        assert!(notice.message.starts_with("Read:"));
        emit(&mut events, &mut console, true, notice.input(2, None)).unwrap();
        let (result, read) = reload_preset_attempt(&mut presets, Ok(saved.clone()));
        assert_eq!(result, Ok(()));
        assert!(adapter.storage_read(StorageArea::Presets, read).is_none());
        assert_eq!(events.snapshot(&EventFilter::new("recovered")).matching, 1);
        assert_eq!(console.0.len(), 2);

        let write = presets.begin_save().unwrap();
        presets.finish_save(&write, Ok(())).unwrap();
        let notice = adapter.storage(StorageArea::Presets, Ok(())).unwrap();
        assert!(notice.message.starts_with("Write:"));
        emit(&mut events, &mut console, true, notice.input(3, None)).unwrap();
        assert!(adapter.storage(StorageArea::Presets, Ok(())).is_none());
        let snapshot = events.snapshot(&EventFilter::default());
        assert_eq!(snapshot.records().len(), 4);
        assert!(snapshot.records().iter().all(|e| e.count.value == 1));
    }
}
#[test]
fn synthetic_input_and_inspection_outcomes_are_bounded_quiet_and_distinct_from_dispatch() {
    use dashboard_configurator::config::*;
    use dashboard_configurator::event_adapters::{SimulationObservation, simulation_changes};
    use dashboard_configurator::simulator::{ControlSimulator, SimulationInput};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let before = SimulationObservation::capture(&sim);
    sim.custom_input(Channel::Boost, "1.5");
    let after = SimulationObservation::capture(&sim);
    assert!(
        simulation_changes(&before, &after)
            .iter()
            .any(|n| n.code == "simulation.input")
    );
    let before = SimulationObservation::capture(&sim);
    sim.custom_input(Channel::Boost, "1.6");
    assert!(simulation_changes(&before, &SimulationObservation::capture(&sim)).is_empty());
    let before = SimulationObservation::capture(&sim);
    sim.inject(SimulationInput::Channel {
        channel: Channel::Boost,
        value: f32::NAN,
        at_ms: sim.now_ms() + 1,
    });
    assert!(
        simulation_changes(&before, &SimulationObservation::capture(&sim))
            .iter()
            .any(|n| n.code == "simulation.input")
    );
    sim.inspect_invalid(sim.config().config().controls);
    let before = SimulationObservation::capture(&sim);
    sim.acknowledge_warnings();
    let notices = simulation_changes(&before, &SimulationObservation::capture(&sim));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].code, "simulation.inspection");
    assert!(notices[0].message.contains("not dispatched"));
    assert!(sim.pending_warning_command().is_none());
}
