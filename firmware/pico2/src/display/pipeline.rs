//! The same token checks run on the device and in the host fault-injection tests.
use core::future::{Future, poll_fn};
use core::pin::pin;

use super::FrameBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferId {
    A,
    B,
}

impl BufferId {
    pub const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameToken {
    pub buffer: BufferId,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferState {
    Free,
    Rendering(FrameToken),
    Ready(FrameToken),
    Queued(FrameToken),
    InFlight(FrameToken),
    Quarantined(FrameToken),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    UnexpectedState,
    Busy,
    Exhausted,
}

pub struct FrameLifecycle {
    states: [BufferState; 2],
    last_sequence: u64,
}

impl Default for FrameLifecycle {
    fn default() -> Self { Self::new() }
}

impl FrameLifecycle {
    pub const fn new() -> Self {
        Self {
            states: [BufferState::Free; 2],
            last_sequence: 0,
        }
    }

    pub fn state(
        &self,
        buffer: BufferId,
    ) -> BufferState {
        self.states[buffer.index()]
    }

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

    pub fn ready(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::Rendering(token), BufferState::Ready(token))
    }

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

    pub fn started(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::Queued(token), BufferState::InFlight(token))
    }

    pub fn complete(
        &mut self,
        token: FrameToken,
    ) -> Result<(), LifecycleError> {
        self.transition(token, BufferState::InFlight(token), BufferState::Free)
    }

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
    id: BufferId,
    buffer: &'a mut FrameBuffer,
}

impl<'a> FrameLease<'a> {
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

    pub fn id(&self) -> BufferId { self.id }

    pub fn buffer(&mut self) -> &mut FrameBuffer { self.buffer }

    pub fn submit(
        self,
        token: FrameToken,
        queued_us: u64,
    ) -> FrameRequest<'a> {
        assert_eq!(self.id, token.buffer, "frame token belongs to another buffer");
        FrameRequest {
            lease: self,
            token,
            queued_us,
        }
    }
}

/// Read access only. The DMA future borrows this request, so it must finish or
/// be dropped before the owner can move back to the renderer.
pub struct FrameRequest<'a> {
    lease: FrameLease<'a>,
    token: FrameToken,
    queued_us: u64,
}

impl<'a> FrameRequest<'a> {
    pub fn token(&self) -> FrameToken { self.token }

    pub fn queued_us(&self) -> u64 { self.queued_us }

    pub fn pixels(&self) -> &FrameBuffer { self.lease.buffer }

    pub fn into_lease(self) -> FrameLease<'a> { self.lease }
}

/// Invoke the callback only after polling the actual transfer future once.
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
mod tests {
    use super::*;
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
