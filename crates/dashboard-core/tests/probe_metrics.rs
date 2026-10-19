use std::cell::Cell;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use dashboard_core::display::telemetry::CompletionRate;
use dashboard_core::task_work::{WorkWindow, measure};

#[test]
fn measuring_large_pinned_futures_does_not_duplicate_their_storage() {
    struct LargeFuture {
        data: [u8; 4096],
        address: Cell<usize>,
        _pinned: core::marker::PhantomPinned,
    }
    impl Future for LargeFuture {
        type Output = u8;

        fn poll(
            self: core::pin::Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<u8> {
            let this = self.as_ref().get_ref();
            let address = this as *const Self as usize;
            let previous = this.address.replace(address);
            if previous == 0 {
                Poll::Pending
            } else {
                assert_eq!(previous, address);
                Poll::Ready(this.data[4095])
            }
        }
    }
    let measured = measure(
        LargeFuture {
            data: [7; 4096],
            address: Cell::new(0),
            _pinned: core::marker::PhantomPinned,
        },
        || 0,
        |_, _| {},
    );
    assert!(core::mem::size_of_val(&measured) <= core::mem::size_of::<LargeFuture>() + 32);
    let mut measured = pin!(measured);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(measured.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!(measured.as_mut().poll(&mut cx), Poll::Ready(7));
}

#[test]
fn polling_preserves_errors() {
    let clock = Cell::new(0u64);
    let mut polls = 0;
    let work = core::future::poll_fn(|_| {
        polls += 1;
        clock.set(clock.get() + if polls == 1 { 100 } else { 200 });
        if polls == 1 {
            Poll::Pending
        } else {
            Poll::Ready(Err::<(), _>("peer left"))
        }
    });
    let mut measured = pin!(measure(work, || clock.get(), |_, _| {}));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(measured.as_mut().poll(&mut cx).is_pending());
    clock.set(1_000_100);
    assert_eq!(measured.as_mut().poll(&mut cx), Poll::Ready(Err("peer left")));
}

#[test]
fn poll_observer_records_only_running_spans() {
    let clock = Cell::new(0u64);
    let mut spans = Vec::new();
    {
        let mut polls = 0;
        let work = core::future::poll_fn(|_| {
            polls += 1;
            clock.set(clock.get() + if polls == 1 { 100 } else { 200 });
            if polls == 1 { Poll::Pending } else { Poll::Ready(7) }
        });
        let mut measured = pin!(measure(work, || clock.get(), |a, b| spans.push((a, b))));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(measured.as_mut().poll(&mut cx).is_pending());
        clock.set(1_000_100);
        assert_eq!(measured.as_mut().poll(&mut cx), Poll::Ready(7));
    }
    assert_eq!(spans, [(0, 100), (1_000_100, 1_000_300)]);
    {
        let mut measured = pin!(measure(
            core::future::poll_fn(|_| {
                clock.set(clock.get() + 50);
                Poll::<()>::Pending
            }),
            || clock.get(),
            |a, b| spans.push((a, b)),
        ));
        assert!(
            measured
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        clock.set(9_000_000);
    }
    assert_eq!(spans, [(0, 100), (1_000_100, 1_000_300), (1_000_300, 1_000_350)]);
}

#[test]
fn work_percentage_uses_actual_window_and_excludes_idle() {
    let mut load = WorkWindow::new();
    load.record(0, 100_000);
    assert_eq!(load.percent_tenths(999_999), None);
    load.record(900_000, 1_000_000);
    assert_eq!(load.percent_tenths(1_000_000), Some(200));
    load.record(2_000_000, 3_000_000);
    assert_eq!(load.percent_tenths(3_000_000), Some(500));
    load.record(4_000_000, 4_000_000);
    assert_eq!(load.percent_tenths(4_000_000), Some(0));
    assert_eq!(load.percent_tenths(6_000_001), None);
    assert_eq!(load.percent_tenths(3_999_999), None);
}

#[test]
fn invalid_work_spans_invalidate_then_recover_without_fake_zero_load() {
    for (start, end) in [(999_999, 1_000_001), (2_000_000, 1_999_999)] {
        let mut load = WorkWindow::new();
        load.record(0, 1_000_000);
        assert_eq!(load.percent_tenths(1_000_000), Some(1000));
        load.record(start, end);
        assert_eq!(load.percent_tenths(2_000_000), None);
        load.record(3_000_000, 3_100_000);
        load.record(3_900_000, 4_000_000);
        assert_eq!(load.percent_tenths(4_000_000), Some(200));
    }
    let mut load = WorkWindow::new();
    load.record(0, u64::MAX);
    assert_eq!(load.percent_tenths(u64::MAX), None);
}

#[test]
fn probe_frame_rate_accounts_for_idle_variable_windows_and_counter_wrap() {
    let mut rate = CompletionRate::new(500_000, u32::MAX - 9);
    assert_eq!(rate.sample(500_000, u32::MAX - 9).unwrap(), None);
    assert_eq!(rate.sample(1_499_999, 9).unwrap(), None);
    let first = rate.sample(1_500_000, 10).unwrap().unwrap();
    assert_eq!((first.current, first.average), (20.0, 20.0));
    let delayed = rate.sample(3_500_000, 30).unwrap().unwrap();
    assert_eq!(delayed.current, 10.0);
    assert!((delayed.average - 40.0 / 3.0).abs() < 0.00001);
    let idle = rate.sample(4_500_000, 30).unwrap().unwrap();
    assert_eq!((idle.current, idle.average), (0.0, 10.0));
    assert!(rate.sample(4_499_999, 30).is_err());
    assert!(rate.sample(5_500_000, 29).is_err());
    let resumed = rate.sample(5_500_000, 50).unwrap().unwrap();
    assert_eq!((resumed.current, resumed.average), (20.0, 12.0));
}
