use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use dashboard_pico2::display::pipeline::{FrameLease, FrameLifecycle, FrameRequest, FrameToken};
use dashboard_pico2::display::telemetry::FrameTrace;
use dashboard_pico2::display::transfer::flush_plan;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant, with_timeout};

use crate::drivers::St7789Flusher;

pub static FLUSH_REQUESTS: Channel<CriticalSectionRawMutex, FrameRequest<'static>, 1> = Channel::new();
static FLUSH_STARTED: Channel<CriticalSectionRawMutex, Started, 1> = Channel::new();
static FLUSH_COMPLETED: Channel<CriticalSectionRawMutex, Completion, 1> = Channel::new();

pub static BUFFER_SWAPS: AtomicU32 = AtomicU32::new(0);
pub static BUFFER_WAITS: AtomicU32 = AtomicU32::new(0);
pub static FLUSH_BUFFER_IDX: AtomicUsize = AtomicUsize::new(0);
pub static COMPLETED_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static INPUT_COUNTS: embassy_sync::blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    core::cell::Cell<dashboard_pico2::input_trace::InputCounters>,
> = embassy_sync::blocking_mutex::Mutex::new(core::cell::Cell::new(dashboard_pico2::input_trace::InputCounters::ZERO));

pub fn publish_input_counts(mut counts: dashboard_pico2::input_trace::InputCounters) {
    let (dropped, discarded) = crate::peripherals::ENCODER_EVENTS.counters();
    counts.encoder_dropped = u64::from(dropped);
    counts.encoder_discarded = u64::from(discarded);
    INPUT_COUNTS.lock(|slot| slot.set(counts));
}

#[cfg(feature = "frame-trace")]
pub static FRAME_TRACES: embassy_sync::blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    core::cell::RefCell<dashboard_pico2::display::telemetry::TraceRing<64>>,
> = embassy_sync::blocking_mutex::Mutex::new(core::cell::RefCell::new(
    dashboard_pico2::display::telemetry::TraceRing::new(),
));

struct Started {
    token: FrameToken,
    armed: bool,
}
struct Completion {
    request: FrameRequest<'static>,
    trace: Option<FrameTrace>,
}

pub struct CompletedFrame {
    pub lease: FrameLease<'static>,
    pub trace: FrameTrace,
}

// Timeout cancels only a channel receive. The flusher retains the actual buffer
// until the HAL write completes; no timeout/drop path returns DMA-owned memory.
const PROGRESS_DEADLINE: Duration = Duration::from_millis(250);

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

pub async fn wait_completed(
    lifecycle: &mut FrameLifecycle,
    expected: FrameToken,
) -> CompletedFrame {
    BUFFER_WAITS.fetch_add(1, Ordering::Relaxed);
    match with_timeout(PROGRESS_DEADLINE, FLUSH_COMPLETED.receive()).await {
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
pub async fn display_flush_task(flusher: &'static mut St7789Flusher<'static>) {
    loop {
        let request = FLUSH_REQUESTS.receive().await;
        let token = request.token();
        FLUSH_BUFFER_IDX.store(token.buffer.index(), Ordering::Relaxed);
        let mut started_us = None;
        let result = flush_plan(flusher, request.pixels(), request.plan(), || {
            started_us = Some(Instant::now().as_micros());
            assert!(
                FLUSH_STARTED.try_send(Started { token, armed: true }).is_ok(),
                "unconsumed frame start"
            );
        })
        .await;
        if started_us.is_none() {
            assert!(
                FLUSH_STARTED.try_send(Started { token, armed: false }).is_ok(),
                "unconsumed failed start"
            );
        }
        let trace = if result.is_ok() {
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
    }
}
