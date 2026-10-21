//! Browser frame handles and queue ownership, testable without a browser scheduler.
use crate::simulator::CallbackGate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Nonwrapping appearance-queue identity; only its originating queue can finish it.
pub struct DrawRequest(
    /// Checked monotonically issued identifier; zero is never issued.
    u64,
);

/// The gate and both RAF slots share one lifetime. Retirement invalidates
/// callbacks and releases queue ownership even before a handle is installed.
pub struct FrameCallbacks<H> {
    /// Generation gate shared with simulation frame completions.
    pub gate: CallbackGate,
    /// Installed simulation-completion callback handle, if any.
    pub completion: Option<H>,
    /// Appearance reservation and its optionally installed scheduler handle.
    appearance: Option<(DrawRequest, Option<H>)>,
    /// Last issued appearance request; exhaustion permanently prevents new requests.
    next_draw: u64,
}

impl<H> Default for FrameCallbacks<H> {
    /// Return an empty callback owner with a fresh completion gate.
    fn default() -> Self { Self::new() }
}

impl<H> FrameCallbacks<H> {
    /// Return empty scheduler slots and an unretired completion gate.
    pub const fn new() -> Self {
        Self {
            gate: CallbackGate::new(),
            completion: None,
            appearance: None,
            next_draw: 0,
        }
    }

    /// Reserve the appearance slot and return its identity before scheduling.
    /// Return `None` without changing state if occupied or the counter is exhausted.
    pub fn queue_appearance(&mut self) -> Option<DrawRequest> {
        if self.appearance.is_some() {
            return None;
        }
        self.next_draw = self.next_draw.checked_add(1)?;
        let request = DrawRequest(self.next_draw);
        self.appearance = Some((request, None));
        Some(request)
    }

    /// Install `handle` only for the current unfilled `request` reservation.
    /// Return `Err(handle)` unchanged on stale/duplicate installation so the caller
    /// can cancel the rejected callback; `Ok(())` transfers queue ownership.
    pub fn set_appearance_handle(
        &mut self,
        request: DrawRequest,
        handle: H,
    ) -> Result<(), H> {
        if let Some((current, slot)) = &mut self.appearance
            && *current == request
            && slot.is_none()
        {
            *slot = Some(handle);
            Ok(())
        } else {
            Err(handle)
        }
    }

    /// Release the slot matching delivered `request`, returning whether it matched.
    /// A stale callback cannot clear a newer reservation. This does not cancel its handle.
    pub fn finish_appearance(
        &mut self,
        request: DrawRequest,
    ) -> bool {
        if self.appearance.as_ref().is_some_and(|(current, _)| *current == request) {
            self.appearance = None;
            true
        } else {
            false
        }
    }

    /// Retire the completion generation and remove both reservations, passing each
    /// installed handle to `cancel`. No handle exists for an unscheduled reservation.
    /// `cancel` must not panic: a panic can leave the later appearance slot uncleared.
    pub fn retire(
        &mut self,
        mut cancel: impl FnMut(H),
    ) {
        self.gate.retire();
        if let Some(handle) = self.completion.take() {
            cancel(handle);
        }
        if let Some((_, Some(handle))) = self.appearance.take() {
            cancel(handle);
        }
    }
}
