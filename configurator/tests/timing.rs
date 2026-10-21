//! Trace-capability, bounded evidence and monotonic playback regressions without a browser clock.
use dashboard_configurator::timing::{Playback, TraceCapture, TraceError, TraceSample, TraceStatus};

/// Returns a two-widget observation submitted 1 ms and presented 2 ms after `input`.
fn trace_sample(input: f64) -> TraceSample {
    TraceSample {
        input_ms: input,
        canvas_ms: input + 1.0,
        presentation_boundary_ms: input + 2.0,
        render_ms: 1.0,
        widgets: 2,
        views: 1,
        warning_preview: false,
    }
}

#[test]
/// Assert duplicate/foreign permits cannot consume another callback's outstanding slot.
fn timing_duplicate_and_foreign_owner_permits_cannot_complete_other_work() {
    let mut trace = TraceCapture::default();
    trace.start(0.0).unwrap();
    let mut a = trace.begin(1.0).unwrap();
    let mut b = trace.begin(2.0).unwrap();
    assert_eq!(trace.in_flight(), 2);
    trace.finish(&mut a, trace_sample(1.0)).unwrap();
    assert_eq!(trace.finish(&mut a, trace_sample(1.0)), Err(TraceError::StalePermit));
    assert_eq!(trace.in_flight(), 1);
    let mut replacement = TraceCapture::default();
    replacement.start(0.0).unwrap();
    let mut foreign = replacement.begin(2.0).unwrap();
    assert_eq!(
        trace.finish(&mut foreign, trace_sample(2.0)),
        Err(TraceError::StalePermit)
    );
    assert_eq!(trace.in_flight(), 1);
    replacement.abandon(&mut foreign).unwrap();
    trace.finish(&mut b, trace_sample(2.0)).unwrap();
    assert_eq!(trace.in_flight(), 0);
    assert_eq!(trace.samples().len(), 2);
}

#[test]
/// Assert restart invalidates old permits, stop allows in-range late evidence and abandon is single-use.
fn timing_restart_stop_and_abandon_keep_callback_accounting_explicit() {
    let mut trace = TraceCapture::default();
    trace.start(0.0).unwrap();
    let mut old = trace.begin(1.0).unwrap();
    trace.start(10.0).unwrap();
    let mut current = trace.begin(11.0).unwrap();
    assert_eq!(trace.finish(&mut old, trace_sample(1.0)), Err(TraceError::StalePermit));
    assert_eq!(trace.in_flight(), 1);
    trace.stop(12.0).unwrap();
    assert_eq!(trace.status(), TraceStatus::Stopped);
    trace.finish(&mut current, trace_sample(11.0)).unwrap();
    let mut queued_before_stop = trace.begin(11.5).unwrap();
    assert_eq!(trace.begin(13.0).unwrap_err(), TraceError::OutsideRun);
    trace.abandon(&mut queued_before_stop).unwrap();
    assert_eq!(trace.abandon(&mut queued_before_stop), Err(TraceError::StalePermit));
    assert_eq!(trace.in_flight(), 0);
}

#[test]
/// Assert invalid timestamps do not create evidence and a rejected completion still consumes its permit.
fn timing_invalid_sample_finishes_permit_without_inventing_evidence() {
    let mut trace = TraceCapture::default();
    assert_eq!(trace.start(f64::NAN), Err(TraceError::InvalidTime));
    trace.start(10.0).unwrap();
    for value in [f64::NAN, f64::INFINITY, 9.0] {
        assert_eq!(trace.begin(value).unwrap_err(), TraceError::OutsideRun);
    }
    let mut permit = trace.begin(11.0).unwrap();
    let mut invalid = trace_sample(11.0);
    invalid.canvas_ms = f64::NAN;
    assert_eq!(trace.finish(&mut permit, invalid), Err(TraceError::InvalidTime));
    assert_eq!(trace.in_flight(), 0);
    assert!(trace.samples().is_empty());
}

#[test]
/// Assert render/long-task capacities are independent and all overflow shares one drop count.
fn timing_sample_and_long_task_limits_are_independent() {
    let mut trace = TraceCapture::default();
    trace.start(0.0).unwrap();
    for n in 0..513 {
        let input = n as f64;
        let mut permit = trace.begin(input).unwrap();
        trace.finish(&mut permit, trace_sample(input)).unwrap();
    }
    assert_eq!(trace.samples().len(), 512);
    assert_eq!(trace.dropped().value, 1);
    for n in 0..513 {
        trace.record_long_task(n as f64, 25.0).unwrap();
    }
    assert_eq!(trace.long_tasks().len(), 512);
    assert_eq!(trace.samples().len(), 512);
    assert_eq!(trace.dropped().value, 2);
    assert_eq!(trace.in_flight(), 0);
}

#[test]
/// Assert exported evidence preserves timings/counters while omitting the live owner capability.
fn timing_export_keeps_sample_shape_and_exposes_no_owner_authority() {
    let mut trace = TraceCapture::default();
    trace.start(0.0).unwrap();
    let mut permit = trace.begin(1.0).unwrap();
    trace.finish(&mut permit, trace_sample(1.0)).unwrap();
    let json = serde_json::to_value(&trace).unwrap();
    assert_eq!(json["id"].as_u64(), Some(1));
    assert_eq!(json["samples"][0]["canvas_ms"].as_f64(), Some(2.0));
    assert!(json.get("owner").is_none());
    assert_eq!(json["in_flight"].as_u64(), Some(0));
    assert_eq!(json["status"], "Recording");
}

#[test]
/// Assert identical elapsed playback across sampling intervals, pauses and resumed anchors.
fn elapsed_time_dropped_frames_pause_and_restart_do_not_depend_on_fps() {
    for interval in [1, 14, 25, 50, 1000] {
        let mut clock = Playback::new(118_000);
        clock.start(10.0);
        for time in (10..3010).step_by(interval) {
            clock.sample(f64::from(time));
        }
        assert_eq!(clock.pause(3010.75), 121_000);
        assert_eq!(clock.sample(99_000.0), 121_000);
        clock.start(100_000.0);
        assert_eq!(clock.sample(130_123.5), 151_123);
    }
}

#[test]
/// Assert nonfinite/backward observations preserve time and large elapsed advances saturate.
fn invalid_clock_rollback_and_overflow_never_rewind_or_wrap() {
    let mut clock = Playback::new(1);
    clock.start(f64::NAN);
    assert_eq!(clock.sample(200.0), 1);
    clock.start(1000.0);
    assert_eq!(clock.sample(1005.0), 6);
    for value in [f64::NAN, f64::INFINITY, -1.0, 999.0] {
        assert_eq!(clock.sample(value), 6);
    }
    assert_eq!(clock.sample(1006.0), 7);
    let mut end = Playback::new(u64::MAX - 1);
    end.start(1.0);
    assert_eq!(end.sample(999.0), u64::MAX);
}

#[test]
/// Assert repeated Play, jitter and invalid pause timestamps cannot re-anchor or rewind playback.
fn jitter_repeated_start_and_invalid_pause_keep_deterministic_elapsed_time() {
    for step in [1, 3, 16, 17, 33, 50, 250, 1000] {
        let mut clock = Playback::new(42);
        clock.start(100.25);
        for time in (101..10_000).step_by(step) {
            clock.start(time as f64); // repeated Play cannot re-anchor an active run
            clock.sample(time as f64);
            clock.sample(f64::NEG_INFINITY);
            clock.sample(f64::NAN);
            clock.sample(time as f64 - 50.0); // clock rewind never rewinds the sample
        }
        assert_eq!(clock.pause(10_100.25), 10_042);
        assert_eq!(clock.pause(f64::NAN), 10_042);
        clock.start(f64::INFINITY);
        clock.start(-0.1);
        assert_eq!(clock.sample(20_000.0), 10_042);
        clock.start(20_000.5);
        assert_eq!(clock.pause(20_010.5), 10_052);
    }
}
