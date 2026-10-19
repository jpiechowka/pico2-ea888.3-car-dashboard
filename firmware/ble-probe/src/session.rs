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

/// Sample local/terminal evidence after cleanup, even when inventory succeeded.
/// Return acknowledgment only; the model separately classifies termination quality.
pub async fn finish_cleanup(
    revision: u32,
    wait: impl Future<Output = bool>,
    now_ms: impl FnOnce() -> u64,
    context: impl FnOnce() -> Option<dashboard_core::discovery::ConnectionFailureContext>,
    emit: impl AsyncFn(dashboard_core::discovery::Event),
) -> bool {
    let complete = wait.await;
    let at_ms = now_ms();
    let context = context();
    if let Some(context) = context {
        emit(dashboard_core::discovery::Event::FailureContext { revision, context }).await;
    }
    emit(dashboard_core::discovery::Event::InspectionEnd {
        revision,
        at_ms,
        cleanup_complete: complete,
        disconnect_reason: context.and_then(|context| context.disconnect_reason),
    })
    .await;
    complete
}
