//! Elapsed application poll time; suspended futures do not contribute work.
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Single-core accumulator of disjoint poll wall times, with >=1-second windows.
#[derive(Clone, Copy, Default)]
pub struct WorkWindow {
    /// Start of current accumulation window; absent before first valid poll.
    start_us: Option<u64>,
    /// End of the preceding accepted poll in monotonic microseconds.
    last_end_us: u64,
    /// Checked sum of poll durations in the current window, excluding suspension.
    work_us: u64,
    /// Last completed window's timestamp and work tenths-percent, if valid.
    latest: Option<(u64, u16)>,
}
impl WorkWindow {
    /// Returns an empty accumulator with no fresh work observation.
    pub const fn new() -> Self {
        Self {
            start_us: None,
            last_end_us: 0,
            work_us: 0,
            latest: None,
        }
    }

    /// One owner per core records disjoint polls. Invalid timing clears validity.
    /// Adds the poll interval `start_us`..`end_us` in monotonic microseconds.
    /// Reversal/overlap or sum overflow resets all history. At >=1 second, publishes
    /// integer tenths-percent and starts a new window at end; scale overflow makes
    /// that result unavailable. Idle gaps contribute elapsed time but no work.
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
    /// Returns the last completed fraction at monotonic `now_us`, within the
    /// shared two-second freshness limit, or `None` before/after validity.
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
/// Returns a future yielding `future`'s unchanged output. `now` supplies monotonic
/// microseconds before/after each poll and `record` receives that pair even for
/// a ready result. Dropping the wrapper drops the inner future; its cancellation
/// guarantees are unchanged. Panics in polling/callbacks are not intercepted.
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
    /// One pinned inner task with clock/recorder callbacks; avoids copying large
    /// task state into separate pre/post-await storage in another async wrapper.
    /// `future` is structurally pinned. `now` supplies monotonic microseconds;
    /// `record` consumes start/end times after every normally returning poll.
    /// The pinned-field macro accepts only its pin marker on fields, so their
    /// contracts reside here rather than on rejected field doc attributes.
    struct Measured<F, N, R> {
        // Task future, structurally pinned through the generated projection.
        #[pin]
        future: F,
        // Monotonic microsecond clock callback invoked around each inner poll.
        now: N,
        // Callback accepting start/end observations after each completed poll.
        record: R,
    }
}
impl<F: Future, N: FnMut() -> u64, R: FnMut(u64, u64)> Future for Measured<F, N, R> {
    /// Unchanged completion value of the wrapped task.
    type Output = F::Output;

    /// Polls the pinned inner task with `cx`, records its elapsed interval and
    /// returns the same pending/ready result; callback execution is outside timing.
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
