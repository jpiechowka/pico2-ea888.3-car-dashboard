//! Browser frame handles and queue ownership, testable without a browser scheduler.
use crate::simulator::CallbackGate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawRequest(u64);

/// The gate and both RAF slots share one lifetime. Retirement invalidates
/// callbacks and releases queue ownership even before a handle is installed.
pub struct FrameCallbacks<H> {
    pub gate: CallbackGate,
    pub completion: Option<H>,
    appearance: Option<(DrawRequest, Option<H>)>,
    next_draw: u64,
}

impl<H> Default for FrameCallbacks<H> {
    fn default() -> Self { Self::new() }
}

impl<H> FrameCallbacks<H> {
    pub const fn new() -> Self {
        Self {
            gate: CallbackGate::new(),
            completion: None,
            appearance: None,
            next_draw: 0,
        }
    }

    pub fn queue_appearance(&mut self) -> Option<DrawRequest> {
        if self.appearance.is_some() {
            return None;
        }
        self.next_draw = self.next_draw.checked_add(1)?;
        let request = DrawRequest(self.next_draw);
        self.appearance = Some((request, None));
        Some(request)
    }

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

    /// A delivered callback may release only the slot it originally requested.
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
