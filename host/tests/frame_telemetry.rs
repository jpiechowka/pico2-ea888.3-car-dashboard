use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::telemetry::{CompletionRate, FrameTrace, RenderTrace, TraceRing};

fn frame(sequence: u64) -> FrameTrace {
    FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence,
        },
        RenderTrace {
            start_us: 100,
            end_us: 140,
            queued_us: 170,
            blocked_us: 25,
            sample_us: Some(90),
            active_cycles: 200,
            input: None,
        },
        180,
        220,
        153600,
    )
    .unwrap()
}

#[test]
fn trace_joins_one_frame_and_distinguishes_each_interval() {
    let trace = frame(1);
    assert_eq!(trace.render_us(), 40);
    assert_eq!(trace.queue_us(), 10);
    assert_eq!(trace.transfer_observed_us(), 40);
    assert_eq!(trace.total_us(), 120);
    assert_eq!(trace.sample_age_us(), Some(130));
    assert_eq!(trace.render().blocked_us, 25);
    assert_eq!(trace.payload_bytes(), 153600);
}

#[test]
fn malformed_trace_cannot_underflow_or_claim_zero_byte_frames() {
    let good = frame(1);
    for render in [
        RenderTrace {
            end_us: 99,
            ..good.render()
        },
        RenderTrace {
            queued_us: 139,
            ..good.render()
        },
        RenderTrace {
            blocked_us: 31,
            ..good.render()
        },
        RenderTrace {
            sample_us: Some(101),
            ..good.render()
        },
    ] {
        assert!(FrameTrace::finish(good.token(), render, 180, 220, 153600).is_err());
    }
    for (start, end, bytes) in [(169, 220, 10), (180, 179, 10), (180, 220, 0), (180, 220, 153602)] {
        assert!(FrameTrace::finish(good.token(), good.render(), start, end, bytes).is_err());
    }
    let instant = RenderTrace::instant(u64::MAX);
    let trace = FrameTrace::finish(good.token(), instant, u64::MAX, u64::MAX, 2).unwrap();
    assert_eq!(trace.total_us(), 0);
    assert_eq!(trace.sample_age_us(), None);
}

#[test]
fn bounded_trace_ring_retains_order_and_reports_overwrite() {
    let mut ring = TraceRing::<3>::new();
    assert!(ring.latest().is_none());
    for seq in 1..=6 {
        assert!(ring.push(frame(seq)));
    }
    assert_eq!(ring.len(), 3);
    assert_eq!(ring.overwritten(), 3);
    assert_eq!(ring.get(0).unwrap().token().sequence, 4);
    assert_eq!(ring.get(1).unwrap().token().sequence, 5);
    assert_eq!(ring.latest().unwrap().token().sequence, 6);
    assert!(ring.get(3).is_none());
    assert!(!ring.push(frame(6)));
    assert!(!ring.push(frame(1)));
    assert_eq!(ring.overwritten(), 3);
}

#[test]
fn completed_fps_uses_elapsed_time_and_weighted_average() {
    let mut meter = CompletionRate::new(0, 0);
    assert!(meter.sample(999999, 10).unwrap().is_none());
    let a = meter.sample(1_000_000, 20).unwrap().unwrap();
    assert_eq!(a.current, 20.0);
    assert_eq!(a.average, 20.0);
    let b = meter.sample(3_000_000, 80).unwrap().unwrap();
    assert_eq!(b.current, 30.0);
    assert!((b.average - 80.0 / 3.0).abs() < 0.00001);
    // No completion means zero current rate, even if rendering still ran.
    assert_eq!(meter.sample(4_000_000, 80).unwrap().unwrap().current, 0.0);
}

#[test]
fn completed_fps_handles_counter_wrap_and_rejects_clock_or_count_regressions() {
    let mut meter = CompletionRate::new(10, u32::MAX - 5);
    assert!(meter.sample(9, u32::MAX).is_err());
    assert_eq!(meter.sample(1_000_010, 4).unwrap().unwrap().current, 10.0);
    assert!(meter.sample(2_000_010, 3).is_err());
    assert_eq!(meter.sample(2_000_010, 14).unwrap().unwrap().current, 10.0);
    let mut meter = CompletionRate::new(u64::MAX, 0);
    assert!(meter.sample(u64::MAX, 0).unwrap().is_none());
}
