use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
use dashboard_host_validation::display::window::{FrameWindow, WINDOW_US};
use dashboard_host_validation::input_trace::{InputCounters, InputTrace};

fn trace(
    id: u64,
    start: u64,
    end: u64,
    tx: u64,
    done: u64,
) -> FrameTrace {
    FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence: id,
        },
        RenderTrace {
            start_us: start,
            end_us: end,
            queued_us: end + 2,
            blocked_us: 2,
            active_cycles: 10,
            sample_us: Some(0),
            input: None,
        },
        tx,
        done,
        153600,
    )
    .unwrap()
}

#[test]
fn thirty_second_windows_preserve_boundary_intervals_and_report_idle() {
    let mut window = FrameWindow::new(0);
    assert!(window.report(WINDOW_US - 1, InputCounters::ZERO).unwrap().is_none());
    // A frame cannot arrive behind the reporting clock.
    assert!(window.record(trace(1, 10, 20, 30, 40)).is_err());
    let empty = window.report(WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!((empty.sequence, empty.frames, empty.rejected), (1, 0, 1));
    assert!(empty.metrics.iter().all(|m| m.samples == 0 && m.p99.is_none()));
    window
        .record(trace(2, WINDOW_US, WINDOW_US + 5, WINDOW_US + 10, WINDOW_US + 100))
        .unwrap();
    let one = window.report(2 * WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!((one.frames, one.payload_bytes, one.active_cycles), (1, 153600, 10));
    assert_eq!(one.metrics[0].samples, 0);
    assert_eq!(one.metrics[8].samples, 0);
    let idle = window.report(3 * WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!(idle.frames, 0);
    window
        .record(trace(
            3,
            3 * WINDOW_US,
            3 * WINDOW_US + 5,
            3 * WINDOW_US + 10,
            3 * WINDOW_US + 100,
        ))
        .unwrap();
    let next = window.report(4 * WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!(next.metrics[0].max, Some(2 * WINDOW_US));
    assert_eq!(next.metrics[8].max, Some(0));
}

#[test]
fn overlapping_and_serial_render_intervals_match_an_independent_sorted_reference() {
    let mut window = FrameWindow::new(0);
    let mut expected: [Vec<u64>; 9] = core::array::from_fn(|_| Vec::new());
    let mut previous = None;
    let mut bytes = 0;
    for id in 1..=1500u64 {
        let tx = id * 10000;
        let render_start = tx - if id % 2 == 0 { 9900 } else { 1000 };
        let t = trace(id, render_start, tx - 10, tx, tx + 5000);
        if let Some((old_tx, old_done)) = previous {
            expected[0].push(tx + 5000 - old_done);
            expected[8].push((tx - 10).min(old_done).saturating_sub(render_start.max(old_tx)));
        }
        expected[1].push(tx - 10 - render_start);
        expected[2].push(8);
        expected[3].push(5000);
        expected[4].push(2);
        expected[5].push(tx + 5000);
        bytes += u64::from(t.payload_bytes());
        previous = Some((tx, tx + 5000));
        window.record(t).unwrap();
    }
    let r = window.report(WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!((r.frames, r.payload_bytes, r.active_cycles), (1500, bytes, 15000));
    for (actual, mut values) in r.metrics.into_iter().zip(expected) {
        values.sort_unstable();
        assert_eq!(actual.samples as usize, values.len());
        assert_eq!(actual.min, values.first().copied());
        assert_eq!(actual.max, values.last().copied());
        for (p, bounds) in [(50, actual.p50), (95, actual.p95), (99, actual.p99)] {
            if values.is_empty() {
                assert_eq!(bounds, None);
                continue;
            }
            let v = values[(values.len() * p).div_ceil(100) - 1];
            let b = bounds.unwrap();
            assert!(b.lower <= v && v <= b.upper);
        }
    }
}

#[test]
fn invalid_frame_or_replayed_input_is_rejected_atomically_across_reports() {
    let mut window = FrameWindow::new(0);
    let with_input = |id, at, input_id| {
        FrameTrace::finish(
            FrameToken {
                buffer: BufferId::B,
                sequence: id,
            },
            RenderTrace {
                input: Some(InputTrace {
                    first_id: input_id,
                    last_id: input_id,
                    observed_us: 1,
                    accepted_us: 2,
                    consumed_us: 3,
                    events: 5,
                }),
                ..RenderTrace::instant(at)
            },
            at,
            at + 10,
            2,
        )
        .unwrap()
    };
    window.record(with_input(1, 10, 1)).unwrap();
    assert!(window.record(with_input(1, 30, 2)).is_err());
    assert!(window.record(with_input(2, 15, 2)).is_err());
    assert!(window.record(with_input(2, 30, 1)).is_err());
    let first = window.report(WINDOW_US, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!((first.frames, first.payload_bytes, first.rejected), (1, 2, 3));
    assert_eq!(
        first.metrics[6].samples, 1,
        "five events represent only one latency batch"
    );
    assert_eq!(first.metrics[6].max, Some(19));
    assert_eq!(first.metrics[7].max, Some(17));
    assert!(window.record(with_input(2, WINDOW_US, 1)).is_err());
    assert!(window.report(WINDOW_US - 1, InputCounters::ZERO).is_err());
    window.record(with_input(3, WINDOW_US, 3)).unwrap();
    let c = InputCounters {
        obscured_events: 12,
        ..InputCounters::ZERO
    };
    let second = window.report(2 * WINDOW_US, c).unwrap().unwrap();
    assert_eq!(
        (second.frames, second.rejected, second.inputs.obscured_events),
        (1, 2, 12)
    );
}

#[test]
fn maximum_clock_and_zero_duration_have_defined_results() {
    let mut window = FrameWindow::new(u64::MAX - WINDOW_US);
    let at = u64::MAX - WINDOW_US;
    let t = FrameTrace::finish(
        FrameToken {
            buffer: BufferId::A,
            sequence: 1,
        },
        RenderTrace::instant(at),
        at,
        at,
        2,
    )
    .unwrap();
    window.record(t).unwrap();
    let r = window.report(u64::MAX, InputCounters::ZERO).unwrap().unwrap();
    assert_eq!(r.metrics[3].max, Some(0));
    assert!(window.report(u64::MAX, InputCounters::ZERO).unwrap().is_none());
    assert!(core::mem::size_of::<FrameWindow>() < 38000);
}

#[test]
fn dropping_an_idle_receive_never_consumes_or_releases_a_queued_frame() {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    let channel = Channel::<CriticalSectionRawMutex, Box<u32>, 1>::new();
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut receiving = pin!(channel.receive());
        assert!(matches!(receiving.as_mut().poll(&mut cx), Poll::Pending));
    }
    channel.try_send(Box::new(42)).unwrap();
    // Even dropping an unpolled receive while data is ready cannot take it.
    drop(channel.receive());
    let mut receiving = pin!(channel.receive());
    assert_eq!(receiving.as_mut().poll(&mut cx), Poll::Ready(Box::new(42)));
    assert!(channel.try_receive().is_err());
}

#[test]
fn fixed_encoder_capacity_covers_all_maximum_width_fields() {
    use dashboard_host_validation::display::window::WindowReport;
    use dashboard_host_validation::metrics::{Bounds, Distribution};
    use dashboard_host_validation::telemetry_protocol;
    let b = Some(Bounds {
        lower: u64::MAX,
        upper: u64::MAX,
    });
    let d = Distribution {
        samples: u32::MAX,
        overflow: u32::MAX,
        rejected: u32::MAX,
        min: Some(u64::MAX),
        max: Some(u64::MAX),
        p50: b,
        p95: b,
        p99: b,
    };
    let r = WindowReport {
        sequence: u64::MAX,
        start_us: u64::MAX,
        end_us: u64::MAX,
        frames: u32::MAX,
        payload_bytes: u64::MAX,
        active_cycles: u64::MAX,
        rejected: u64::MAX,
        metrics: [d; 9],
        inputs: InputCounters {
            ignored_events: u64::MAX,
            no_damage_events: u64::MAX,
            obscured_events: u64::MAX,
            invalid: u64::MAX,
            maintenance_gaps: u64::MAX,
            encoder_dropped: u64::MAX,
            encoder_discarded: u64::MAX,
        },
    };
    let mut lines = 0;
    telemetry_protocol::frames(r, |line| {
        assert!(line.len() <= 320);
        lines += 1;
    })
    .unwrap();
    assert_eq!(lines, 12);
}
