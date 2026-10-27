//! Portable connection supervision, shared with native lifecycle regressions.
use core::future::Future;

use embassy_futures::select::{Either, select};

/// Awaits `runner`, then invokes synchronous `latch` before returning its output.
/// Wrapping the HCI side of select closes admission before select can drop the
/// operation future and its connection lease. No terminal publication is awaited.
pub async fn latch_completion<T>(
    runner: impl Future<Output = T>,
    latch: impl FnOnce(),
) -> T {
    let result = runner.await;
    latch();
    result
}

/// Runs `startup`, then lends its successful client to `run`, while polling `events`
/// throughout. Returns the work result or the first event error; work wins when
/// both are ready in the same poll. The losing future is dropped before return.
/// Callers must make losing futures cancellation-safe and perform connection
/// cleanup afterward; a returned success does not acknowledge disconnection.
/// The successful startup value stays here across callback suspension and drops
/// after the callback. If that value is a client reference, the caller retains
/// the actual owner and must drop it after this future and before link cleanup.
/// No owned startup value enters the callback future.
pub async fn inspect_with_events<C, T, E>(
    startup: impl Future<Output = Result<C, E>>,
    run: impl AsyncFnOnce(&C) -> Result<T, E>,
    events: impl Future<Output = E>,
) -> Result<T, E> {
    let work = async {
        let client = startup.await?;
        run(&client).await
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
/// Awaits `wait`, samples `now_ms` and `context` afterward, and uses `emit` to
/// publish context before the end event under `revision`. Returns the wait's bool,
/// including false on incomplete cleanup; event publication is awaited reliably.
/// Do not cancel between messages if the model must receive a terminal end event.
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
