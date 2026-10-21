//! Affine framebuffer leases and generation-checked ownership transitions.
//! The same token checks run on the device and in the host fault-injection tests.
use core::future::{Future, poll_fn};
use core::pin::pin;

use dashboard_config::Orientation;

use super::FrameBuffer;
use super::damage::DamagePlan;
use super::telemetry::RenderTrace;

/// Stable index of one of the two physical framebuffers; not a generation ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferId {
    /// First exclusively borrowed allocation, array index zero.
    A,
    /// Second exclusively borrowed allocation, array index one.
    B,
}

impl BufferId {
    /// Returns the slot index in every two-buffer ownership table.
    pub const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

/// Identity of one render, preserved through queue, transfer and completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameToken {
    /// Allocation carrying this generation's pixels.
    pub buffer: BufferId,
    /// Globally increasing nonzero render ID; lifecycle allocation never wraps.
    pub sequence: u64,
}

/// Logical ownership; token equality rejects stale events after buffer reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferState {
    /// Available to begin a new render; pixel history is tracked separately.
    Free,
    /// Renderer may mutate this generation's lease.
    Rendering(
        /// Generation being produced.
        FrameToken,
    ),
    /// Render finished; may submit or discard without transfer.
    Ready(
        /// Completed render awaiting a queue decision.
        FrameToken,
    ),
    /// Submitted and no longer available to the renderer.
    Queued(
        /// Generation awaiting its payload-start observation.
        FrameToken,
    ),
    /// Payload future has been polled; lease remains held until completion.
    InFlight(
        /// Generation owned by the flusher.
        FrameToken,
    ),
    /// Faulted ownership retained indefinitely; blocks every further submission.
    Quarantined(
        /// Generation that cannot be safely reclaimed.
        FrameToken,
    ),
}

/// Rejected lifecycle operation; all failures leave ownership unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    /// State or exact token does not match the requested transition.
    UnexpectedState,
    /// Another queued, in-flight or quarantined generation occupies the transfer lane.
    Busy,
    /// No unused nonzero `u64` sequence remains.
    Exhausted,
}

/// Single-owner protocol checker; it does not move leases or perform hardware I/O.
pub struct FrameLifecycle {
    /// Current logical owner of A and B respectively.
    states: [BufferState; 2],
    /// Last issued ID, including discarded frames; zero before first render.
    last_sequence: u64,
}

impl Default for FrameLifecycle {
    /// Returns two free buffers with no issued generations.
    fn default() -> Self { Self::new() }
}

impl FrameLifecycle {
    /// Returns the initial ownership table; first successful begin receives ID one.
    pub const fn new() -> Self {
        Self {
            states: [BufferState::Free; 2],
            last_sequence: 0,
        }
    }

    /// Returns the current logical state of `buffer` without changing ownership.
    pub fn state(
        &self,
        buffer: BufferId,
    ) -> BufferState {
        self.states[buffer.index()]
    }

    /// Claims free `buffer` for rendering and returns a fresh token. Returns
    /// `UnexpectedState` if occupied or `Exhausted` before sequence overflow.
    pub fn begin(
        &mut self,
        buffer: BufferId,
    ) -> Result<FrameToken, LifecycleError> {
        if self.state(buffer) != BufferState::Free {
            return Err(LifecycleError::UnexpectedState);
        }
        let sequence = self.last_sequence.checked_add(1).ok_or(LifecycleError::Exhausted)?;
        let token = FrameToken { buffer, sequence };
        self.states[buffer.index()] = BufferState::Rendering(token);
        self.last_sequence = sequence;
        Ok(token)
    }

    /// Marks exactly the rendering `token` ready. Returns success or
    /// `UnexpectedState` without mutation for a stale/wrong-state token.
    pub fn ready(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::Rendering(token), BufferState::Ready(token))
    }

    /// Queues ready `token`. Returns `Busy` while any transfer or quarantine
    /// exists, otherwise success or `UnexpectedState` for a mismatched token.
    pub fn queue(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        if self.states.iter().any(|state| {
            matches!(
                state,
                BufferState::Queued(_) | BufferState::InFlight(_) | BufferState::Quarantined(_)
            )
        }) {
            return Err(LifecycleError::Busy);
        }
        self.transition(token, BufferState::Ready(token), BufferState::Queued(token))
    }

    /// Releases ready `token` without transfer. Returns success or
    /// `UnexpectedState`; queued/in-flight storage cannot be discarded.
    pub fn discard(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::Ready(token), BufferState::Free)
    }

    /// Advances queued `token` after the payload-start observation. Returns
    /// success or `UnexpectedState` for stale, duplicate or out-of-order events.
    pub fn started(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::Queued(token), BufferState::InFlight(token))
    }

    /// Releases in-flight `token` after validated completion. Returns success
    /// or `UnexpectedState`; quarantine is deliberately not recoverable here.
    pub fn complete(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::InFlight(token), BufferState::Free)
    }

    /// Permanently retains queued/in-flight `token` after a fault. Returns
    /// success or `UnexpectedState`; never cancels DMA or releases a lease.
    pub fn quarantine(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        match self.state(token.buffer) {
            BufferState::Queued(active) | BufferState::InFlight(active) if active == token => {
                self.states[token.buffer.index()] = BufferState::Quarantined(token);
                Ok(())
            }
            _ => Err(LifecycleError::UnexpectedState),
        }
    }

    /// Replaces the slot selected by `token` only if it equals `from`, with `to`.
    /// Returns success or `UnexpectedState`; callers supply token-bearing states.
    fn transition(
        &mut self,
        token: FrameToken,
        from: BufferState,
        to: BufferState,
    ) -> Result<(), LifecycleError> {
        let state = &mut self.states[token.buffer.index()];
        if *state != from {
            return Err(LifecycleError::UnexpectedState);
        }
        *state = to;
        Ok(())
    }
}

/// An affine owner. Moving it into a request ends all renderer access.
pub struct FrameLease<'a> {
    /// Allocation label assigned once when creating the pair.
    id: BufferId,
    /// Unique mutable borrow; moving the lease preserves this exclusivity.
    buffer: &'a mut FrameBuffer,
}

impl<'a> FrameLease<'a> {
    /// Borrows distinct `a` and `b` allocations exclusively and returns their
    /// A/B leases in that order; borrow checking excludes overlapping storage.
    pub fn pair(
        a: &'a mut FrameBuffer,
        b: &'a mut FrameBuffer,
    ) -> [Self; 2] {
        [
            Self {
                id: BufferId::A,
                buffer: a,
            },
            Self {
                id: BufferId::B,
                buffer: b,
            },
        ]
    }

    /// Returns this lease's stable allocation label.
    pub fn id(&self) -> BufferId { self.id }

    /// Returns the exclusively borrowed pixels for drawing before submission.
    pub fn buffer(&mut self) -> &mut FrameBuffer { self.buffer }

    /// Returns read-only pixels, for example to compare a completed generation.
    pub fn pixels(&self) -> &FrameBuffer { self.buffer }

    /// Consumes the lease into a full landscape request for `token` with `trace`.
    /// Returns the immutable request; panics on token or timing mismatch as in
    /// [`Self::submit_plan_in`]. Does not itself enqueue or validate lifecycle state.
    pub fn submit(
        self,
        token: FrameToken,
        trace: RenderTrace,
    ) -> FrameRequest<'a> {
        self.submit_plan(token, trace, DamagePlan::full())
    }

    /// Consumes the lease with `token`, `trace` and landscape `plan`. Returns the
    /// request; panics on empty/wrong-geometry plans or invalid token/timing.
    pub fn submit_plan(
        self,
        token: FrameToken,
        trace: RenderTrace,
        plan: DamagePlan,
    ) -> FrameRequest<'a> {
        self.submit_plan_in(token, trace, plan, Orientation::Landscape)
    }

    /// Returns an immutable request owning this lease, identified by `token`,
    /// timed by `trace`, with RGB565 regions in `plan` and fixed `orientation`.
    /// Panics if geometry differs, the token names another allocation, the plan
    /// is empty, or timing is invalid. Lifecycle/sequence admission is separate.
    pub fn submit_plan_in(
        self,
        token: FrameToken,
        trace: RenderTrace,
        plan: DamagePlan,
        orientation: Orientation,
    ) -> FrameRequest<'a> {
        assert_eq!(
            plan.geometry(),
            orientation.geometry(),
            "frame orientation and plan geometry differ"
        );
        assert_eq!(self.id, token.buffer, "frame token belongs to another buffer");
        assert!(!plan.is_empty(), "empty plans must not be queued");
        trace.validate().expect("invalid render timing");
        FrameRequest {
            lease: self,
            token,
            trace,
            plan,
            orientation,
        }
    }
}

/// Read-only transfer ownership. Borrow checking prevents reclaiming while a
/// future borrows pixels, but does not prove DMA stopped after dropping a future.
/// The device retains the request on timeout/failure instead of canceling it.
pub struct FrameRequest<'a> {
    /// Unique buffer owner, unavailable for mutation while this request exists.
    lease: FrameLease<'a>,
    /// Exact generation carried by start/completion messages.
    token: FrameToken,
    /// Validated render timing captured before enqueue.
    trace: RenderTrace,
    /// Nonempty immutable regions and geometry.
    plan: DamagePlan,
    /// Address mode fixed at submission, independent of subsequent UI changes.
    orientation: Orientation,
}

impl<'a> FrameRequest<'a> {
    /// Returns the immutable buffer/generation identity.
    pub fn token(&self) -> FrameToken { self.token }

    /// Returns the submitted address mode, including mounting inversion.
    pub const fn orientation(&self) -> Orientation { self.orientation }

    /// Returns the render's timing and optional input/sample observations.
    pub fn trace(&self) -> RenderTrace { self.trace }

    /// Returns the immutable transfer plan borrowed from this request.
    pub fn plan(&self) -> &DamagePlan { &self.plan }

    /// Returns read-only pixel bytes, retaining ownership for the borrow's lifetime.
    pub fn pixels(&self) -> &FrameBuffer { self.lease.buffer }

    /// Consumes the request and returns mutable ownership. The caller must first
    /// establish transfer completion; this method performs no DMA/lifecycle check.
    pub fn into_lease(self) -> FrameLease<'a> { self.lease }
}

/// Polls `future` and invokes `callback` once immediately after its first poll,
/// including an immediately ready result; returns the unchanged future output.
/// Dropping this wrapper drops `future`; cancellation safety is inherited from it.
/// The RP HAL 0.10.0 SPI write arms DMA before its first Pending; this wrapper
/// alone makes no such guarantee for arbitrary futures. Recheck HAL upgrades.
pub async fn after_first_poll<F: Future>(
    future: F,
    callback: impl FnOnce(),
) -> F::Output {
    let mut future = pin!(future);
    let mut callback = Some(callback);
    poll_fn(|cx| {
        let result = future.as_mut().poll(cx);
        if let Some(callback) = callback.take() {
            callback();
        }
        result
    })
    .await
}

#[cfg(test)]
/// Sequence-boundary regression for non-reusable generation identities.
mod tests {
    use super::*;
    /// Returns unit after checking the final ID is usable and overflow preserves
    /// the other free slot; panics if ownership or identity aliases.
    #[test]
    fn sequence_exhaustion_does_not_alias_an_old_frame() {
        let mut lifecycle = FrameLifecycle {
            states: [BufferState::Free; 2],
            last_sequence: u64::MAX - 1,
        };
        assert_eq!(lifecycle.begin(BufferId::A).unwrap().sequence, u64::MAX);
        assert_eq!(lifecycle.begin(BufferId::B), Err(LifecycleError::Exhausted));
        assert_eq!(lifecycle.state(BufferId::B), BufferState::Free);
    }
}
