//! Bounded input contracts shared by physical buttons and the optional encoder.
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

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

pub struct EncoderEvents {
    delta: AtomicI32,
    presses: AtomicU32,
    dropped: AtomicU32,
}
impl Default for EncoderEvents {
    fn default() -> Self { Self::new() }
}
impl EncoderEvents {
    pub const fn new() -> Self {
        Self {
            delta: AtomicI32::new(0),
            presses: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
        }
    }

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

    pub fn drop_event(&self) {
        let _ = self
            .dropped
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_add(1)));
    }

    pub fn dropped(&self) -> u32 { self.dropped.load(Ordering::Relaxed) }

    /// Independent event streams, not a sensor snapshot. Each swap consumes once.
    pub fn take(&self) -> (i32, u8) {
        (
            self.delta.swap(0, Ordering::Relaxed),
            self.presses.swap(0, Ordering::Relaxed) as u8,
        )
    }

    pub fn clear(&self) { self.take(); }
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
