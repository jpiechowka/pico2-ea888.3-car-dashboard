//! Portable connection supervision, shared with native lifecycle regressions.
use core::future::Future;

use embassy_futures::select::{Either, select};

pub async fn inspect_with_events<C, T, E, R: Future<Output = Result<T, E>>>(
    startup: impl Future<Output = Result<C, E>>,
    run: impl FnOnce(C) -> R,
    events: impl Future<Output = E>,
) -> Result<T, E> {
    let work = async {
        let client = startup.await?;
        run(client).await
    };
    // Work wins simultaneous readiness, retaining the operation's stage/range.
    // The event consumer is already active while MTU startup is suspended.
    match select(work, events).await {
        Either::First(result) => result,
        Either::Second(error) => Err(error),
    }
}

/// Publish terminal evidence only after the controller cleanup wait resolves.
pub async fn finish_cleanup(
    revision: u32,
    wait: impl Future<Output = bool>,
    now_ms: impl FnOnce() -> u64,
    reason: impl FnOnce() -> Option<u8>,
    emit: impl AsyncFn(dashboard_core::discovery::Event),
) -> bool {
    let complete = wait.await;
    emit(dashboard_core::discovery::Event::InspectionEnd {
        revision,
        at_ms: now_ms(),
        cleanup_complete: complete,
        disconnect_reason: reason(),
    })
    .await;
    complete
}
