//! Bounded input contracts shared by physical buttons and the optional encoder.
use core::cell::RefCell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::input_trace::Observation;

const MAX_DELTA: i32 = 128;
const MAX_PRESSES: u32 = 8;
const DEBOUNCE_MS: u64 = 50;

pub fn valid_encoder_identity(
    hw_id: u8,
    version: [u8; 4],
) -> bool {
    // Documented SAMD09 board/firmware for PID4991 and preassembled PID5880.
    hw_id == 0x55 && u32::from_be_bytes(version) >> 16 == 4991
}

pub fn decode_encoder_delta(bytes: [u8; 4]) -> Option<i32> {
    let raw = i32::from_be_bytes(bytes);
    // PID5880's supplied encoder has the opposite sign to the loose encoder.
    (-MAX_DELTA..=MAX_DELTA).contains(&raw).then(|| -raw)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncoderBatch {
    pub delta: i32,
    pub presses: u8,
    pub motion: Option<Observation>,
    pub button: Option<Observation>,
}
struct EncoderState {
    batch: EncoderBatch,
    dropped: u32,
    discarded: u32,
    last_us: u64,
}
pub struct EncoderEvents {
    state: Mutex<CriticalSectionRawMutex, RefCell<EncoderState>>,
}
impl Default for EncoderEvents {
    fn default() -> Self { Self::new() }
}
impl EncoderEvents {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(RefCell::new(EncoderState {
                batch: EncoderBatch {
                    delta: 0,
                    presses: 0,
                    motion: None,
                    button: None,
                },
                dropped: 0,
                discarded: 0,
                last_us: 0,
            })),
        }
    }

    fn mutate<R>(
        &self,
        f: impl FnOnce(&mut EncoderState) -> R,
    ) -> R {
        self.state.lock(|state| f(&mut state.borrow_mut()))
    }

    pub fn publish_delta(
        &self,
        delta: i32,
    ) {
        self.publish_delta_at(delta, 0);
    }

    pub fn publish_delta_at(
        &self,
        delta: i32,
        now_us: u64,
    ) {
        self.mutate(|state| {
            if !(-MAX_DELTA..=MAX_DELTA).contains(&delta) || now_us < state.last_us {
                state.dropped = state.dropped.saturating_add(1);
                return;
            }
            state.last_us = now_us;
            if delta == 0 {
                return;
            }
            let sum = state.batch.delta + delta;
            let next = sum.clamp(-MAX_DELTA, MAX_DELTA);
            if sum != next {
                state.dropped = state.dropped.saturating_add(1);
            }
            if next == state.batch.delta {
                return;
            }
            if Observation::append(&mut state.batch.motion, Observation::new(now_us, now_us).unwrap()) {
                state.batch.delta = next;
            } else {
                state.dropped = state.dropped.saturating_add(1);
            }
        });
    }

    pub fn publish_press(&self) { self.publish_press_at(Observation::new(0, 0).unwrap()); }

    pub fn publish_press_at(
        &self,
        observation: Observation,
    ) {
        self.mutate(|state| {
            if observation.events != 1
                || observation.observed_us > observation.accepted_us
                || observation.accepted_us < state.last_us
                || u32::from(state.batch.presses) == MAX_PRESSES
            {
                state.dropped = state.dropped.saturating_add(1);
                return;
            }
            if Observation::append(&mut state.batch.button, observation) {
                state.batch.presses += 1;
                state.last_us = observation.accepted_us;
            } else {
                state.dropped = state.dropped.saturating_add(1);
            }
        });
    }

    pub fn drop_event(&self) { self.mutate(|state| state.dropped = state.dropped.saturating_add(1)); }

    pub fn dropped(&self) -> u32 { self.mutate(|state| state.dropped) }

    pub fn discarded(&self) -> u32 { self.mutate(|state| state.discarded) }

    pub fn counters(&self) -> (u32, u32) { self.mutate(|state| (state.dropped, state.discarded)) }

    pub fn take_observed(&self) -> EncoderBatch { self.mutate(|state| core::mem::take(&mut state.batch)) }

    pub fn take(&self) -> (i32, u8) {
        let batch = self.take_observed();
        (batch.delta, batch.presses)
    }

    pub fn clear(&self) {
        self.mutate(|state| {
            let batch = core::mem::take(&mut state.batch);
            state.discarded = state
                .discarded
                .saturating_add(batch.motion.map_or(0, |v| v.events))
                .saturating_add(batch.button.map_or(0, |v| v.events));
        });
    }
}

pub fn brightness_after(
    current: u32,
    delta: i32,
) -> u32 {
    (i64::from(current.min(100)) + i64::from(delta) * 5).clamp(0, 100) as u32
}
pub fn scroll_after(
    current: i32,
    delta: i32,
    maximum: usize,
) -> i32 {
    let max = maximum.min(i32::MAX as usize) as i64;
    (i64::from(current).clamp(0, max) - i64::from(delta)).clamp(0, max) as i32
}

pub struct DebouncedButton {
    stable: Option<bool>,
    candidate: bool,
    since_ms: u64,
    last_ms: u64,
}
impl Default for DebouncedButton {
    fn default() -> Self { Self::new() }
}
impl DebouncedButton {
    pub const fn new() -> Self {
        Self {
            stable: None,
            candidate: false,
            since_ms: 0,
            last_ms: 0,
        }
    }

    pub fn update_with_origin(
        &mut self,
        pressed: bool,
        now_ms: u64,
    ) -> Option<u64> {
        self.update(pressed, now_ms).then_some(self.since_ms)
    }

    pub fn update(
        &mut self,
        pressed: bool,
        now_ms: u64,
    ) -> bool {
        if now_ms < self.last_ms {
            return false;
        }
        self.last_ms = now_ms;
        if self.stable.is_none() {
            self.stable = Some(pressed);
            self.candidate = pressed;
            self.since_ms = now_ms;
            return false;
        }
        if pressed != self.candidate {
            self.candidate = pressed;
            self.since_ms = now_ms;
            return false;
        }
        if self.stable != Some(pressed) && now_ms - self.since_ms >= DEBOUNCE_MS {
            self.stable = Some(pressed);
            return pressed;
        }
        false
    }
}
