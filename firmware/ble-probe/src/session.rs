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
