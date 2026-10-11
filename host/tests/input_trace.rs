use dashboard_host_validation::input_trace::{InputTracker, Observation};

#[test]
fn visible_warning_without_feedback_excludes_unrelated_input_damage() {
    use dashboard_host_validation::AlignedFrame;
    use dashboard_host_validation::acquisition::AcquisitionModel;
    use dashboard_host_validation::dashboard::Overlay;
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
    let mut model = AcquisitionModel::demo();
    for at in [0, 500, 1000] {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
    }
    let alerts = model.snapshot().alerts_at(1000);
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (frame, blink) in frames.iter_mut().zip([true, false]) {
        let overlay = Overlay::prioritize(None, alerts, blink).unwrap();
        assert!(matches!(overlay, Overlay::Critical(_, _)));
        overlay.draw(&mut St7789Renderer::new(&mut frame.0));
    }
    assert!(frames[0].0 != frames[1].0, "alert blink produces unrelated damage");
    let mut tracker = InputTracker::new();
    assert!(tracker.consume(Observation::new(1, 51).unwrap(), 52));
    tracker.exclude_obscured(alerts.priority().is_some());
    let completed = FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence: 1,
        },
        RenderTrace {
            input: tracker.submitted(),
            ..RenderTrace::instant(60)
        },
        70,
        80,
        153600,
    )
    .unwrap();
    assert_eq!(completed.input_age_us(), None);
    assert_eq!(tracker.counters().obscured_events, 1);
    tracker.exclude_obscured(false);
    assert!(
        tracker.submitted().is_none(),
        "hidden press cannot leak into a later frame"
    );
    assert!(tracker.consume(Observation::new(90, 140).unwrap(), 141));
    tracker.exclude_obscured(false);
    assert_eq!(tracker.submitted().unwrap().observed_us, 90);
}

#[test]
fn batches_keep_oldest_observation_and_distinct_consumption_identity() {
    let a = Observation::new(10, 60).unwrap();
    let b = Observation::new(30, 80).unwrap();
    let mut tracker = InputTracker::new();
    assert!(tracker.consume(a, 70));
    assert!(!tracker.consume(b, 79));
    assert!(tracker.consume(b, 90));
    let pending = tracker.pending().unwrap();
    assert_eq!((pending.first_id, pending.last_id, pending.events), (1, 2, 2));
    assert_eq!(
        (pending.observed_us, pending.accepted_us, pending.consumed_us),
        (10, 80, 90)
    );
    tracker.maintenance_gap();
    assert_eq!(tracker.pending(), Some(pending));
    assert_eq!(tracker.submitted(), Some(pending));
    assert!(tracker.submitted().is_none());
    assert!(tracker.consume(a, 100));
    assert_eq!(tracker.pending().unwrap().first_id, 3);
    tracker.no_damage();
    assert!(tracker.pending().is_none());
    assert_eq!(tracker.counters().no_damage_events, 1);
    tracker.ignore(4);
    assert_eq!(tracker.counters().ignored_events, 4);
    assert_eq!(tracker.counters().invalid, 1);
}

#[test]
fn malformed_and_regressed_metadata_never_changes_pending_trace() {
    assert!(Observation::new(2, 1).is_none());
    let mut tracker = InputTracker::new();
    assert!(tracker.consume(Observation::new(2, 3).unwrap(), 10));
    let before = tracker.pending();
    assert!(!tracker.consume(Observation::new(1, 2).unwrap(), 9));
    assert_eq!(tracker.pending(), before);
    assert!(tracker.consume(Observation::new(u64::MAX, u64::MAX).unwrap(), u64::MAX));
    assert_eq!(tracker.pending().unwrap().consumed_us, u64::MAX);
}

#[test]
fn reset_acknowledgement_keeps_original_consumption_and_old_completions_stay_separate() {
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
    let mut tracker = InputTracker::new();
    let old = RenderTrace::instant(0);
    let reset = Observation::new(10, 60).unwrap();
    assert!(tracker.consume(Observation::new(70, 70).unwrap(), 80));
    assert!(tracker.apply(reset, 61, 100));
    let trace = tracker.submitted().unwrap();
    assert_eq!((trace.observed_us, trace.consumed_us), (10, 80));
    let old = FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence: 1,
        },
        old,
        1,
        150,
        2,
    )
    .unwrap();
    assert_eq!(old.input_age_us(), None);
    let rendered = RenderTrace {
        input: Some(trace),
        ..RenderTrace::instant(110)
    };
    let completed = FrameTrace::finish(
        FrameToken {
            buffer: BufferId::B,
            sequence: 2,
        },
        rendered,
        151,
        200,
        2,
    )
    .unwrap();
    assert_eq!(completed.input_age_us(), Some(190));
    assert_eq!(completed.input_after_consume_us(), Some(120));
    assert!(
        FrameTrace::finish(
            completed.token(),
            RenderTrace {
                start_us: 79,
                ..rendered
            },
            151,
            200,
            2
        )
        .is_err()
    );
    assert!(FrameTrace::finish(completed.token(), rendered, 151, 200, 0).is_err());
}
