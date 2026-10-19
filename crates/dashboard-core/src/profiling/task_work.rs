//! Elapsed application poll time; suspended futures do not contribute work.
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

#[derive(Clone, Copy, Default)]
pub struct WorkWindow {
    start_us: Option<u64>,
    last_end_us: u64,
    work_us: u64,
    latest: Option<(u64, u16)>,
}
impl WorkWindow {
    pub const fn new() -> Self {
        Self {
            start_us: None,
            last_end_us: 0,
            work_us: 0,
            latest: None,
        }
    }

    /// One owner per core records disjoint polls. Invalid timing clears validity.
    pub fn record(
        &mut self,
        start_us: u64,
        end_us: u64,
    ) {
        if end_us < start_us || (self.start_us.is_some() && start_us < self.last_end_us) {
            *self = Self::new();
            return;
        }
        let origin = *self.start_us.get_or_insert(start_us);
        let Some(work) = self.work_us.checked_add(end_us - start_us) else {
            *self = Self::new();
            return;
        };
        self.last_end_us = end_us;
        self.work_us = work;
        let elapsed = end_us - origin;
        if elapsed >= 1_000_000 {
            self.latest = work.checked_mul(1000).map(|scaled| (end_us, (scaled / elapsed) as u16));
            self.start_us = Some(end_us);
            self.work_us = 0;
        }
    }

    /// Tenths of a percent. Old or future-dated observations remain unavailable.
    pub fn percent_tenths(
        &self,
        now_us: u64,
    ) -> Option<u16> {
        let (at, value) = self.latest?;
        crate::diagnostic_values::fresh(at, now_us).then_some(value)
    }
}

/// Wrap a top-level task once; nested wrappers would double count its work.
/// Interrupts during a poll are included; scheduler/IRQ work outside polls is not.
pub fn measure<F: Future>(
    future: F,
    now: impl FnMut() -> u64,
    record: impl FnMut(u64, u64),
) -> impl Future<Output = F::Output> {
    Measured { future, now, record }
}

pin_project_lite::pin_project! {
    // A concrete wrapper stores the inner future once. Nested async functions
    // can retain separate pre/post-first-poll copies of large radio futures.
    struct Measured<F, N, R> {
        #[pin]
        future: F,
        now: N,
        record: R,
    }
}
impl<F: Future, N: FnMut() -> u64, R: FnMut(u64, u64)> Future for Measured<F, N, R> {
    type Output = F::Output;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();
        let start = (this.now)();
        let result = this.future.poll(cx);
        (this.record)(start, (this.now)());
        result
    }
}
