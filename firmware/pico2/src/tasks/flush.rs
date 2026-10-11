use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use dashboard_pico2::display::pipeline::{FrameLease, FrameLifecycle, FrameRequest, FrameToken};
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
pub static LAST_FLUSH_TIME_US: AtomicU32 = AtomicU32::new(0);
pub static COMPLETED_FRAMES: AtomicU32 = AtomicU32::new(0);

struct Started {
    token: FrameToken,
    armed: bool,
}
struct Completion {
    request: FrameRequest<'static>,
    success: bool,
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
            panic!("display did not start the requested frame");
        }
    }
}

pub async fn wait_completed(
    lifecycle: &mut FrameLifecycle,
    expected: FrameToken,
) -> FrameLease<'static> {
    BUFFER_WAITS.fetch_add(1, Ordering::Relaxed);
    match with_timeout(PROGRESS_DEADLINE, FLUSH_COMPLETED.receive()).await {
        Ok(completion) if completion.request.token() == expected && completion.success => {
            lifecycle.complete(expected).expect("invalid frame completion");
            completion.request.into_lease()
        }
        _ => {
            lifecycle.quarantine(expected).expect("invalid completion quarantine");
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
        let result = flusher
            .flush_buffer(request.pixels(), || {
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
        if result.is_ok() {
            let elapsed = Instant::now()
                .as_micros()
                .saturating_sub(started_us.unwrap_or(request.queued_us()));
            LAST_FLUSH_TIME_US.store(elapsed.min(u32::MAX as u64) as u32, Ordering::Relaxed);
            COMPLETED_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        assert!(
            FLUSH_COMPLETED
                .try_send(Completion {
                    request,
                    success: result.is_ok()
                })
                .is_ok(),
            "unconsumed frame completion"
        );
    }
}
