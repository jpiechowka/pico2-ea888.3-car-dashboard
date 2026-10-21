//! Frozen legacy input baseline for matched host benchmarks; production uses shared core input code.
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// Maximum signed motion retained per legacy consume batch.
const MAX_DELTA: i32 = 128;
/// Maximum queued legacy presses before additional presses are counted as dropped.
const MAX_PRESSES: u32 = 8;
/// Stable transition qualification duration in milliseconds.
const DEBOUNCE_MS: u64 = 50;

/// Returns whether `hw_id` is SAMD09 0x55 and big-endian `version` embeds product 4991 in its upper half.
pub fn valid_encoder_identity(
    hw_id: u8,
    version: [u8; 4],
) -> bool {
    // Documented SAMD09 board/firmware for PID4991 and preassembled PID5880.
    hw_id == 0x55 && u32::from_be_bytes(version) >> 16 == 4991
}

/// Returns negated big-endian signed encoder `bytes` within -128..=128, or None outside that bound.
pub fn decode_encoder_delta(bytes: [u8; 4]) -> Option<i32> {
    let raw = i32::from_be_bytes(bytes);
    // PID5880's supplied encoder has the opposite sign to the loose encoder.
    (-MAX_DELTA..=MAX_DELTA).contains(&raw).then(|| -raw)
}

/// Legacy independent atomic motion/press accumulators with saturating loss accounting.
pub struct EncoderEvents {
    /// Coalesced motion clamped to -128..=128.
    delta: AtomicI32,
    /// Queued presses bounded to eight.
    presses: AtomicU32,
    /// Saturating invalid/overflow event count, independent of consumption.
    dropped: AtomicU32,
}
impl Default for EncoderEvents {
    /// Returns empty event accumulators and a zero drop counter.
    fn default() -> Self { Self::new() }
}
impl EncoderEvents {
    /// Returns empty motion/press streams with no prior dropped events.
    pub const fn new() -> Self {
        Self {
            delta: AtomicI32::new(0),
            presses: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
        }
    }

    /// Adds bounded `delta`, clamping accumulated motion; invalid input or saturation counts one dropped event.
    pub fn publish_delta(
        &self,
        delta: i32,
    ) {
        if !(-MAX_DELTA..=MAX_DELTA).contains(&delta) {
            self.drop_event();
            return;
        }
        let old = self
            .delta
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some((v + delta).clamp(-MAX_DELTA, MAX_DELTA))
            })
            .unwrap();
        if !(-MAX_DELTA..=MAX_DELTA).contains(&(old + delta)) {
            self.drop_event();
        }
    }

    /// Queues one press up to eight; a full queue increments the saturating dropped-event count.
    pub fn publish_press(&self) {
        if self
            .presses
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                (v < MAX_PRESSES).then_some(v + 1)
            })
            .is_err()
        {
            self.drop_event();
        }
    }

    /// Saturating-increments the dropped-event counter without altering queued motion or presses.
    pub fn drop_event(&self) {
        let _ = self
            .dropped
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_add(1)));
    }

    /// Returns the current saturating count of invalid/overflow events.
    pub fn dropped(&self) -> u32 { self.dropped.load(Ordering::Relaxed) }

    /// Independent event streams, not a sensor snapshot. Each swap consumes once.
    /// Returns signed motion and press count, resetting each accumulator independently to zero.
    pub fn take(&self) -> (i32, u8) {
        (
            self.delta.swap(0, Ordering::Relaxed),
            self.presses.swap(0, Ordering::Relaxed) as u8,
        )
    }

    /// Consumes both queued streams while preserving lifetime drop accounting.
    pub fn clear(&self) { self.take(); }
}

/// Returns percent brightness after clamping `current` to 100 and applying five percentage points per `delta`.
/// Wide arithmetic clamps the result to 0..=100 even for extreme signed deltas.
pub fn brightness_after(
    current: u32,
    delta: i32,
) -> u32 {
    (i64::from(current.min(100)) + i64::from(delta) * 5).clamp(0, 100) as u32
}
/// Returns scroll offset after subtracting `delta` from bounded `current`, clamped to 0..=`maximum`.
/// The effective maximum also clamps to i32::MAX; wide arithmetic prevents signed overflow.
pub fn scroll_after(
    current: i32,
    delta: i32,
    maximum: usize,
) -> i32 {
    let max = maximum.min(i32::MAX as usize) as i64;
    (i64::from(current).clamp(0, max) - i64::from(delta)).clamp(0, max) as i32
}

/// Legacy edge detector qualifying stable transitions without boot-held synthetic presses.
pub struct DebouncedButton {
    /// Last debounced pressed state; None before the initial observation.
    stable: Option<bool>,
    /// Most recently sampled candidate pressed state.
    candidate: bool,
    /// Candidate transition's first observation in monotonic milliseconds.
    since_ms: u64,
    /// Last accepted observation time; earlier samples are ignored.
    last_ms: u64,
}
impl Default for DebouncedButton {
    /// Returns an uninitialized debounce state.
    fn default() -> Self { Self::new() }
}
impl DebouncedButton {
    /// Returns an uninitialized state whose first observation establishes the baseline without an event.
    pub const fn new() -> Self {
        Self {
            stable: None,
            candidate: false,
            since_ms: 0,
            last_ms: 0,
        }
    }

    /// Samples `pressed` at monotonic `now_ms` and returns true only for a newly qualified press.
    /// Requires 50 ms stable candidate time; releases update state silently and backward time is ignored.
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
