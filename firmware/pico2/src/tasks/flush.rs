//! Capacity-one ownership handoff between renderer and the sole SPI/DMA owner.
//! Receive deadlines quarantine lifecycle state; they never cancel bus work.
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use dashboard_pico2::display::pipeline::{FrameLease, FrameLifecycle, FrameRequest, FrameToken};
use dashboard_pico2::display::telemetry::FrameTrace;
use dashboard_pico2::display::transfer::{PanelState, flush_oriented_plan};
use dashboard_pico2::view_config::Orientation;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant, with_timeout};

use crate::drivers::St7789Flusher;

/// Affine frame requests; a queued request owns its framebuffer until completion.
pub static FLUSH_REQUESTS: Channel<CriticalSectionRawMutex, FrameRequest<'static>, 1> = Channel::new();
/// Single unconsumed payload-start acknowledgement for the submitted frame token.
static FLUSH_STARTED: Channel<CriticalSectionRawMutex, Started, 1> = Channel::new();
/// Completed request plus optional success trace; failure still retains its lease.
static FLUSH_COMPLETED: Channel<CriticalSectionRawMutex, Completion, 1> = Channel::new();

/// Wrapping count of submissions, updated by the renderer rather than DMA.
pub static BUFFER_SWAPS: AtomicU32 = AtomicU32::new(0);
/// Wrapping count of completion waits that were not immediately ready.
pub static BUFFER_WAITS: AtomicU32 = AtomicU32::new(0);
// No active transfer until a request is received. Failed transfers retain ownership.
/// Active buffer index; `usize::MAX` means no transfer, and failure retains the index.
pub static FLUSH_BUFFER_IDX: AtomicUsize = AtomicUsize::new(usize::MAX);
/// Wrapping count of successful HAL transfer completions, not panel refreshes.
pub static COMPLETED_FRAMES: AtomicU32 = AtomicU32::new(0);
/// Coherent input-counter snapshot read under a short cross-core critical section.
pub static INPUT_COUNTS: embassy_sync::blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    core::cell::Cell<dashboard_pico2::input_trace::InputCounters>,
> = embassy_sync::blocking_mutex::Mutex::new(core::cell::Cell::new(dashboard_pico2::input_trace::InputCounters::ZERO));

/// Publishes `counts` after replacing encoder drop/discard fields with queue totals.
/// The whole snapshot is replaced atomically under the counter mutex.
pub fn publish_input_counts(mut counts: dashboard_pico2::input_trace::InputCounters) {
    let (dropped, discarded) = crate::peripherals::ENCODER_EVENTS.counters();
    counts.encoder_dropped = u64::from(dropped);
    counts.encoder_discarded = u64::from(discarded);
    INPUT_COUNTS.lock(|slot| slot.set(counts));
}

#[cfg(feature = "frame-trace")]
/// Bounded history of successful frame timings, protected against cross-core reads.
pub static FRAME_TRACES: embassy_sync::blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    core::cell::RefCell<dashboard_pico2::display::telemetry::TraceRing<64>>,
> = embassy_sync::blocking_mutex::Mutex::new(core::cell::RefCell::new(
    dashboard_pico2::display::telemetry::TraceRing::new(),
));

#[cfg(feature = "frame-trace")]
/// One-time storage for the flusher's boot-relative aggregate window accumulator.
static FRAME_WINDOW: static_cell::ConstStaticCell<dashboard_pico2::display::window::FrameWindow> =
    static_cell::ConstStaticCell::new(dashboard_pico2::display::window::FrameWindow::new(0));
#[cfg(feature = "frame-trace")]
/// Latest completed telemetry window; the single UI receiver may skip older reports.
pub static FRAME_WINDOWS: embassy_sync::watch::Watch<
    CriticalSectionRawMutex,
    dashboard_pico2::display::window::WindowReport,
    1,
> = embassy_sync::watch::Watch::new();

/// Acknowledgement emitted only after polling the real payload future.
struct Started {
    /// Exact buffer/frame identity to match against the outstanding submission.
    token: FrameToken,
    /// False if panel preparation failed before payload polling.
    armed: bool,
}
/// Return envelope; even failed transfer results retain ownership of `request`.
struct Completion {
    /// Submitted pixels, plan, orientation and affine lease.
    request: FrameRequest<'static>,
    /// Success timing only; `None` prevents recycling the failed request.
    trace: Option<FrameTrace>,
}

/// Successfully completed transfer whose lease may safely return to rendering.
pub struct CompletedFrame {
    /// Unique framebuffer ownership recovered after the HAL returned successfully.
    pub lease: FrameLease<'static>,
    /// Matching token, monotonic timestamps and payload byte count.
    pub trace: FrameTrace,
}

// Timeout cancels only a channel receive. The flusher retains the actual buffer
// until the HAL write completes; no timeout/drop path returns DMA-owned memory.
/// Maximum channel-acknowledgement delay; does not bound synchronous HAL waits.
const PROGRESS_DEADLINE: Duration = Duration::from_millis(250);

/// Waits for an armed start matching `expected` and moves `lifecycle` to InFlight.
/// On timeout, wrong token or failed start, quarantines the expected lease and
/// panics; no buffer is returned and the separately spawned bus task is retained.
pub async fn wait_started(
    lifecycle: &mut FrameLifecycle,
    expected: FrameToken,
) {
    match with_timeout(PROGRESS_DEADLINE, FLUSH_STARTED.receive()).await {
        Ok(Started { token, armed: true }) if token == expected => {
            lifecycle.started(token).expect("invalid frame start");
        }
        _ => {
            lifecycle.quarantine(expected).expect("invalid start quarantine");
            crate::log_fault!("Display start failed");
            panic!("display did not start the requested frame");
        }
    }
}

/// Returns a recycled lease and trace only for matching successful `expected`.
/// Updates `lifecycle` to complete; a wrong/missing/error result quarantines it
/// and panics. A receive timeout cancels no SPI/DMA future.
pub async fn wait_completed(
    lifecycle: &mut FrameLifecycle,
    expected: FrameToken,
) -> CompletedFrame {
    let completion = match FLUSH_COMPLETED.try_receive() {
        Ok(completed) => Ok(completed),
        Err(_) => {
            BUFFER_WAITS.fetch_add(1, Ordering::Relaxed);
            with_timeout(PROGRESS_DEADLINE, FLUSH_COMPLETED.receive()).await
        }
    };
    match completion {
        Ok(Completion {
            request,
            trace: Some(trace),
        }) if request.token() == expected && trace.token() == expected => {
            lifecycle.complete(expected).expect("invalid frame completion");
            CompletedFrame {
                lease: request.into_lease(),
                trace,
            }
        }
        _ => {
            lifecycle.quarantine(expected).expect("invalid completion quarantine");
            crate::log_fault!("Display completion failed");
            panic!("display did not complete the requested frame");
        }
    }
}

#[embassy_executor::task]
/// Runs the sole initialized `flusher` indefinitely, consuming affine requests.
/// Acknowledges the first payload poll and publishes success only after HAL idle.
/// Panics on full acknowledgement channels, invalid timing or stale trace data;
/// callers must never cancel this task while it owns a request or bus operation.
pub async fn display_flush_task(flusher: &'static mut St7789Flusher<'static>) {
    // init() has completed before this sole bus owner starts. Successful HAL
    // writes include SPI idle, so no extra delay or DMA cancellation is needed.
    let mut panel_state = PanelState::initialized(Orientation::Landscape);
    #[cfg(feature = "frame-trace")]
    let window = FRAME_WINDOW.take();
    loop {
        // No bin scan, formatting or await occurs under the counter/Watch lock.
        // The first window begins at boot time zero; idle windows remain visible.
        #[cfg(feature = "frame-trace")]
        if let Ok(Some(report)) = window.report(Instant::now().as_micros(), INPUT_COUNTS.lock(|slot| slot.get())) {
            FRAME_WINDOWS.sender().send(report);
        }
        #[cfg(feature = "frame-trace")]
        let request = match with_timeout(Duration::from_secs(1), FLUSH_REQUESTS.receive()).await {
            Ok(request) => request,
            Err(_) => continue,
        };
        #[cfg(not(feature = "frame-trace"))]
        let request = FLUSH_REQUESTS.receive().await;
        let token = request.token();
        FLUSH_BUFFER_IDX.store(token.buffer.index(), Ordering::Relaxed);
        let mut started_us = None;
        let result = flush_oriented_plan(
            flusher,
            &mut panel_state,
            request.pixels(),
            request.plan(),
            request.orientation(),
            || {
                started_us = Some(Instant::now().as_micros());
                assert!(
                    FLUSH_STARTED.try_send(Started { token, armed: true }).is_ok(),
                    "unconsumed frame start"
                );
            },
        )
        .await;
        if started_us.is_none() {
            assert!(
                FLUSH_STARTED.try_send(Started { token, armed: false }).is_ok(),
                "unconsumed failed start"
            );
        }
        let trace = if result.is_ok() {
            FLUSH_BUFFER_IDX.store(usize::MAX, Ordering::Relaxed);
            let trace = FrameTrace::finish(
                token,
                request.trace(),
                started_us.expect("missing payload start"),
                Instant::now().as_micros(),
                request.plan().bytes() as u32,
            )
            .expect("invalid completed frame timing");
            #[cfg(feature = "frame-trace")]
            FRAME_TRACES.lock(|ring| assert!(ring.borrow_mut().push(trace), "stale frame trace"));
            COMPLETED_FRAMES.fetch_add(1, Ordering::Relaxed);
            Some(trace)
        } else {
            None
        };
        assert!(
            FLUSH_COMPLETED.try_send(Completion { request, trace }).is_ok(),
            "unconsumed frame completion"
        );
        #[cfg(feature = "frame-trace")]
        if let Some(trace) = trace {
            // Telemetry rejection is counted, never a reason to release or
            // cancel DMA. Only successful HAL completions reach this point.
            let _ = window.record(trace);
        }
    }
}
