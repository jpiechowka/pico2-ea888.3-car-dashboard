//! Bounded input contracts shared by physical buttons and the optional encoder.
//! [Adafruit's encoder guide](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/arduino)
//! documents product4991 firmware identity and the assembled PID5880 direction reversal.
use core::cell::RefCell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::input_trace::Observation;

/// Maximum absolute admitted/pending encoder detents per batch.
const MAX_DELTA: i32 = 128;
/// Maximum queued debounced clicks before subsequent presses are counted dropped.
const MAX_PRESSES: u32 = 8;
/// Stable candidate interval in milliseconds for both press and release.
const DEBOUNCE_MS: u64 = 50;
/// DeferredButton hold threshold in milliseconds from accepted press; configurable controls use their own policy.
pub const BUTTON_HOLD_MS: u64 = 2000;

/// Returns whether `hw_id` is SAMD09 0x55 and big-endian `version` has product4991
/// in its upper16bits, the seesaw firmware identity used by assembled PID5880.
/// Low version bits are not constrained; identity alone does not prove wiring/operation.
pub fn valid_encoder_identity(
    hw_id: u8,
    version: [u8; 4],
) -> bool {
    // Documented SAMD09 board/firmware for PID4991 and preassembled PID5880.
    hw_id == 0x55 && u32::from_be_bytes(version) >> 16 == 4991
}

/// Returns sign-corrected detents from signed big-endian `bytes` for the supplied
/// PID5880 encoder, or None outside -128..=128. Bounds precede negation, including i32::MIN.
pub fn decode_encoder_delta(bytes: [u8; 4]) -> Option<i32> {
    let raw = i32::from_be_bytes(bytes);
    // PID5880's supplied encoder has the opposite sign to the loose encoder.
    (-MAX_DELTA..=MAX_DELTA).contains(&raw).then(|| -raw)
}

/// Coherent bounded encoder values plus software observations, consumed atomically once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncoderBatch {
    /// Net detents clamped to -128..=128; opposing observed events may sum to zero.
    pub delta: i32,
    /// Queued debounced clicks in0..=8.
    pub presses: u8,
    /// Earliest/latest accepted motion timestamps and event count, including cancellation to zero.
    pub motion: Option<Observation>,
    /// Coalesced click observations; event count equals presses for publisher-created batches.
    pub button: Option<Observation>,
}
/// All publication state protected by one short critical-section lock.
struct EncoderState {
    /// Values and observations pending one consumer take.
    batch: EncoderBatch,
    /// Saturating rejected/clipped publication count, not number of clipped detents.
    dropped: u32,
    /// Saturating observations intentionally cleared without consumer delivery.
    discarded: u32,
    /// Latest admitted producer acceptance time in monotonic microseconds.
    last_us: u64,
}
/// Cross-context bounded mailbox; platform critical-section implementation supplies synchronization.
pub struct EncoderEvents {
    /// Short critical-section/RefCell access; no await or hardware I/O occurs while held.
    state: Mutex<CriticalSectionRawMutex, RefCell<EncoderState>>,
}
impl Default for EncoderEvents {
    /// Returns an empty mailbox with zero counters/time, as new.
    fn default() -> Self { Self::new() }
}
impl EncoderEvents {
    /// Returns a const-initializable mailbox with no pending motion/clicks.
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

    /// Runs bounded non-reentrant `f` on locked state and returns its result. A
    /// reentrant mutable borrow panics; callers must not block or await inside the closure.
    fn mutate<R>(
        &self,
        f: impl FnOnce(&mut EncoderState) -> R,
    ) -> R {
        self.state.lock(|state| f(&mut state.borrow_mut()))
    }

    /// Publishes `delta` detents at timestamp zero for untimed callers; using this
    /// after timed events may reject it as stale. Bounds/drop semantics match publish_delta_at.
    pub fn publish_delta(
        &self,
        delta: i32,
    ) {
        self.publish_delta_at(delta, 0);
    }

    /// Adds `delta` in-128..=128 at monotonic `now_us` microseconds. Invalid/stale
    /// input is counted dropped; valid sums clamp to the same range and count clipping
    /// once. Nonzero effective changes append one observation atomically; zero only advances time.
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

    /// Publishes one untimed click at zero; may be rejected after a timed publication.
    pub fn publish_press(&self) { self.publish_press_at(Observation::new(0, 0).unwrap()); }

    /// Queues one debounced `observation`, requiring events==1, valid time ordering,
    /// nondecreasing acceptance and fewer than eight queued presses. Rejections saturating-
    /// increment dropped; admission updates count, timestamps and observations atomically.
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

    /// Counts one externally rejected hardware/publisher event without adding pending input.
    pub fn drop_event(&self) { self.mutate(|state| state.dropped = state.dropped.saturating_add(1)); }

    /// Returns the saturating lifetime rejected/clipped publication count under the lock.
    pub fn dropped(&self) -> u32 { self.mutate(|state| state.dropped) }

    /// Returns the saturating observation count explicitly removed by clear.
    pub fn discarded(&self) -> u32 { self.mutate(|state| state.discarded) }

    /// Returns one coherent (dropped, discarded) counter snapshot.
    pub fn counters(&self) -> (u32, u32) { self.mutate(|state| (state.dropped, state.discarded)) }

    /// Returns and atomically empties values/observations; counters and producer time persist.
    pub fn take_observed(&self) -> EncoderBatch { self.mutate(|state| core::mem::take(&mut state.batch)) }

    /// Returns and consumes net(detents, clicks), intentionally omitting timing observations.
    pub fn take(&self) -> (i32, u8) {
        let batch = self.take_observed();
        (batch.delta, batch.presses)
    }

    /// Discards pending values/observations and counts their events, preserving producer
    /// clock and drop count; opposing motion still contributes its observation count.
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

/// Returns slider percent0..=100 after clamping `current` then adding five points
/// per signed `delta`; widened arithmetic handles every u32/i32 input without overflow.
pub fn brightness_after(
    current: u32,
    delta: i32,
) -> u32 {
    (i64::from(current.min(100)) + i64::from(delta) * 5).clamp(0, 100) as u32
}
/// Returns log offset after clamping `current`, subtracting signed `delta`, then
/// clamping to0..=min(`maximum`,i32::MAX). Positive rotation moves toward live/newer entries.
pub fn scroll_after(
    current: i32,
    delta: i32,
    maximum: usize,
) -> i32 {
    let max = maximum.min(i32::MAX as usize) as i64;
    (i64::from(current).clamp(0, max) - i64::from(delta)).clamp(0, max) as i32
}

/// Sampled-level debounce with no boot/reconnect press event and no clock-regression effects.
pub struct DebouncedButton {
    /// Last accepted pressed level; None before the initial sample.
    stable: Option<bool>,
    /// Most recent raw candidate level.
    candidate: bool,
    /// Monotonic milliseconds when the candidate was first observed.
    since_ms: u64,
    /// Last nonregressing sample time in milliseconds.
    last_ms: u64,
}
impl Default for DebouncedButton {
    /// Returns an unsampled debouncer, as new.
    fn default() -> Self { Self::new() }
}
impl DebouncedButton {
    /// Returns an unsampled button; the first sample establishes state without emitting a press.
    pub const fn new() -> Self {
        Self {
            stable: None,
            candidate: false,
            since_ms: 0,
            last_ms: 0,
        }
    }

    /// Last accepted level, not the raw/candidate level. None before first
    /// update; a held boot/reconnect level still does not emit a press edge.
    /// Arm release gestures only after update() actually reports a press.
    /// Returns the stable pressed level or None before initial sampling.
    pub fn stable_pressed(&self) -> Option<bool> { self.stable }

    /// Samples raw `pressed` at monotonic `now_ms` milliseconds. Returns the first
    /// candidate observation time only when a new press passes50ms debounce; release,
    /// initial held state, bounce and reversed time return None.
    pub fn update_with_origin(
        &mut self,
        pressed: bool,
        now_ms: u64,
    ) -> Option<u64> {
        self.update(pressed, now_ms).then_some(self.since_ms)
    }

    /// Samples raw `pressed` at monotonic `now_ms` milliseconds. Returns true once
    /// per newly accepted press after50ms stable candidate time; release updates state
    /// but returns false. Initial state emits no edge; reversal makes no mutation.
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

/// Independent edge flags: a delayed poll can qualify hold and release together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonEdges {
    /// A new debounced press was accepted in this update.
    pub pressed: bool,
    /// An armed press reached debounced release.
    pub released: bool,
    /// Release occurred without a qualified hold; caller may commit a short action.
    pub short: bool,
    /// Fires once per accepted press, never again on its later release.
    pub held: bool,
}

/// Stable press/release with a deferred short action and a two-second hold.
/// Initial held/reconnected buttons are suppressed until a new accepted press.
pub struct DeferredButton {
    /// Shared50ms press/release debounce and monotonic sample guard.
    button: DebouncedButton,
    /// Accepted-press timestamp in milliseconds, absent for initial-held/unarmed state.
    pressed_at: Option<u64>,
    /// Hold action already emitted for this armed press, suppressing later short release.
    held: bool,
}
impl Default for DeferredButton {
    /// Returns an unarmed button with no initial level, as new.
    fn default() -> Self { Self::new() }
}
impl DeferredButton {
    /// Returns an unarmed deferred button; boot-held input cannot emit gestures.
    pub const fn new() -> Self {
        Self {
            button: DebouncedButton::new(),
            pressed_at: None,
            held: false,
        }
    }

    /// Samples raw `pressed` at monotonic `now_ms` milliseconds and returns independent
    /// press/release/short/hold flags. Hold starts at accepted press and fires once after
    /// 2000ms; a delayed qualified release may also emit hold, but uses the observed
    /// release origin so an earlier short release cannot become long after a stall.
    /// Reversed time returns empty edges without state changes.
    pub fn update(
        &mut self,
        pressed: bool,
        now_ms: u64,
    ) -> ButtonEdges {
        if now_ms < self.button.last_ms {
            return ButtonEdges::default();
        }
        if self.button.update(pressed, now_ms) {
            self.pressed_at = Some(now_ms);
            self.held = false;
            return ButtonEdges {
                pressed: true,
                ..ButtonEdges::default()
            };
        }
        let Some(start) = self.pressed_at else {
            return ButtonEdges::default();
        };
        let released = self.button.stable_pressed() == Some(false);
        // A known early release must not become a long hold after a poll stall.
        let end = if released { self.button.since_ms } else { now_ms };
        let held = !self.held
            && (pressed || released)
            && end.checked_sub(start).is_some_and(|elapsed| elapsed >= BUTTON_HOLD_MS);
        self.held |= held;
        let short = released && !self.held;
        if released {
            self.pressed_at = None;
        }
        ButtonEdges {
            pressed: false,
            released,
            short,
            held,
        }
    }
}
