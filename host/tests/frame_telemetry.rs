use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::telemetry::{CompletionRate, FrameTrace, RenderTrace, TraceRing};
use dashboard_host_validation::input_trace::InputTrace;

#[test]
fn trace_storage_retains_the_64_entry_ram_budget() {
    assert_eq!(core::mem::size_of::<FrameTrace>(), 152);
    assert_eq!(core::mem::size_of::<Option<FrameTrace>>(), 152);
    // Native usize is wider than ARM's; neither target needs more than this.
    assert!(core::mem::size_of::<TraceRing<64>>() <= 9752);
}

#[test]
fn trace_roundtrips_all_optional_fields_without_reserving_any_cycle_value() {
    for at in [0, 37, u64::MAX] {
        for active_cycles in [None, Some(0), Some(1), Some(u64::MAX)] {
            for sample_us in [None, Some(0), Some(at)] {
                for input in [
                    None,
                    Some(InputTrace {
                        first_id: u64::MAX,
                        last_id: u64::MAX,
                        observed_us: 0,
                        accepted_us: at,
                        consumed_us: at,
                        events: u32::MAX,
                    }),
                ] {
                    let render = RenderTrace {
                        active_cycles,
                        sample_us,
                        input,
                        ..RenderTrace::instant(at)
                    };
                    let token = FrameToken {
                        buffer: BufferId::B,
                        sequence: u64::MAX,
                    };
                    let trace = FrameTrace::finish(token, render, at, at, 153600).unwrap();
                    assert_eq!(trace.render(), render);
                    assert_eq!(trace.token(), token);
                    assert_eq!(
                        (trace.started_us(), trace.completed_us(), trace.payload_bytes()),
                        (at, at, 153600)
                    );
                    assert_eq!(trace.sample_age_us(), sample_us.map(|sample| at - sample));
                    assert_eq!(trace.input_age_us(), input.map(|_| at));
                    assert_eq!(trace.input_after_consume_us(), input.map(|_| 0));
                    assert_eq!(
                        (
                            trace.render_us(),
                            trace.queue_us(),
                            trace.transfer_observed_us(),
                            trace.total_us()
                        ),
                        (0, 0, 0, 0)
                    );
                }
            }
        }
    }
}

#[test]
fn full_trace_ring_preserves_missing_zero_and_max_cycles_through_overwrites() {
    let make = |sequence, at| {
        FrameTrace::finish(
            FrameToken {
                buffer: BufferId::A,
                sequence,
            },
            RenderTrace {
                active_cycles: [None, Some(0), Some(u64::MAX)][(sequence % 3) as usize],
                sample_us: Some(0),
                ..RenderTrace::instant(at)
            },
            at,
            at,
            2,
        )
        .unwrap()
    };
    let mut ring = TraceRing::<64>::new();
    for sequence in 1..=130 {
        assert!(ring.push(make(sequence, sequence * 10)));
    }
    assert_eq!((ring.len(), ring.overwritten()), (64, 66));
    for index in 0..64 {
        let sequence = index as u64 + 67;
        assert_eq!(ring.get(index), Some(make(sequence, sequence * 10)));
    }
    assert_eq!(ring.latest(), Some(make(130, 1300)));
    assert!(!ring.push(make(130, 1300)));
    assert!(!ring.push(make(131, 0)));
    assert_eq!((ring.len(), ring.overwritten()), (64, 66));
    assert!(ring.get(64).is_none());
}

#[test]
fn compact_trace_still_rejects_invalid_tokens_payloads_and_input_metadata() {
    let good = frame(1);
    let token = FrameToken {
        sequence: 0,
        ..good.token()
    };
    assert!(FrameTrace::finish(token, good.render(), 180, 220, 2).is_err());
    assert!(FrameTrace::finish(good.token(), good.render(), 180, 220, 3).is_err());
    let input = InputTrace {
        first_id: 1,
        last_id: 2,
        observed_us: 1,
        accepted_us: 2,
        consumed_us: 3,
        events: 1,
    };
    for invalid in [
        InputTrace { first_id: 0, ..input },
        InputTrace { first_id: 3, ..input },
        InputTrace {
            observed_us: 3,
            ..input
        },
        InputTrace {
            accepted_us: 4,
            ..input
        },
        InputTrace {
            consumed_us: 101,
            ..input
        },
        InputTrace { events: 0, ..input },
    ] {
        assert!(
            FrameTrace::finish(
                good.token(),
                RenderTrace {
                    input: Some(invalid),
                    ..good.render()
                },
                180,
                220,
                2
            )
            .is_err()
        );
    }
}

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
            active_cycles: Some(200),
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
