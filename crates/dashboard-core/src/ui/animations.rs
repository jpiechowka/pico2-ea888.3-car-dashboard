//! Absolute-time animation: repaint frequency cannot change the trajectory.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

use crate::colors::BLACK;
use crate::render::CELL_COUNT;

const COLOR_TRANSITION_DURATION_MS: u32 = 300;

pub const fn blink_on(elapsed_ms: u64) -> bool { (elapsed_ms / 200).is_multiple_of(2) }

/// Shared motion preference. Explicit warning blink has an independent clock.
#[derive(Clone, Copy, Debug)]
pub struct MotionPolicy {
    reduced: bool,
}
impl MotionPolicy {
    pub const fn new(reduced: bool) -> Self { Self { reduced } }

    pub const fn decorative(self) -> bool { !self.reduced }

    pub const fn smoothing_ms(
        self,
        configured: u16,
    ) -> u16 {
        if self.reduced { 0 } else { configured }
    }
}

#[derive(Clone, Copy)]
struct Transition {
    from: Rgb565,
    current: Rgb565,
    target: Rgb565,
    started_ms: u64,
}

impl Transition {
    const IDLE: Self = Self {
        from: BLACK,
        current: BLACK,
        target: BLACK,
        started_ms: 0,
    };

    fn advance(
        &mut self,
        now_ms: u64,
    ) -> bool {
        let before = self.current;
        if before != self.target {
            let elapsed = now_ms
                .saturating_sub(self.started_ms)
                .min(u64::from(COLOR_TRANSITION_DURATION_MS)) as u32;
            let channel = |from: u8, to: u8| {
                // Round once from the fixed origin; never accumulate quantized
                // per-frame steps. All intermediates fit comfortably in u32.
                ((u32::from(from) * (COLOR_TRANSITION_DURATION_MS - elapsed)
                    + u32::from(to) * elapsed
                    + COLOR_TRANSITION_DURATION_MS / 2)
                    / COLOR_TRANSITION_DURATION_MS) as u8
            };
            self.current = Rgb565::new(
                channel(self.from.r(), self.target.r()),
                channel(self.from.g(), self.target.g()),
                channel(self.from.b(), self.target.b()),
            );
        }
        before != self.current
    }
}

pub struct ColorTransition {
    cells: [Transition; CELL_COUNT],
    last_update_ms: Option<u64>,
}

impl ColorTransition {
    pub const fn new() -> Self {
        Self {
            cells: [Transition::IDLE; CELL_COUNT],
            last_update_ms: None,
        }
    }

    fn accept_time(
        &mut self,
        now_ms: u64,
    ) -> bool {
        if self.last_update_ms.is_some_and(|last| now_ms < last) {
            return false;
        }
        self.last_update_ms = Some(now_ms);
        true
    }

    pub fn set_target(
        &mut self,
        cell_idx: usize,
        target: Rgb565,
        now_ms: u64,
    ) -> bool {
        if cell_idx >= CELL_COUNT || !self.accept_time(now_ms) {
            return false;
        }
        let cell = &mut self.cells[cell_idx];
        if cell.target == target {
            return false;
        }
        // Retarget from the old trajectory at this instant, even if no frame
        // was painted in between. Idle time never counts toward the new fade.
        cell.advance(now_ms);
        cell.from = cell.current;
        cell.target = target;
        cell.started_ms = now_ms;
        true
    }

    pub fn get_current(
        &self,
        cell_idx: usize,
    ) -> Rgb565 {
        self.cells.get(cell_idx).map_or(BLACK, |cell| cell.current)
    }

    /// Decorative transitions snap under reduced motion. Warning blink is not
    /// routed through this policy, including configured LED adapters.
    pub fn update_with_policy(
        &mut self,
        now_ms: u64,
        policy: MotionPolicy,
    ) -> u8 {
        if policy.decorative() {
            return self.update(now_ms);
        }
        if !self.accept_time(now_ms) {
            return 0;
        }
        let mut changed = 0;
        for (index, cell) in self.cells.iter_mut().enumerate() {
            if cell.current != cell.target {
                changed |= 1 << index;
            }
            cell.current = cell.target;
            cell.from = cell.target;
            cell.started_ms = now_ms;
        }
        changed
    }

    pub fn update(
        &mut self,
        now_ms: u64,
    ) -> u8 {
        if !self.accept_time(now_ms) {
            return 0;
        }
        let mut changed = 0;
        for (index, cell) in self.cells.iter_mut().enumerate() {
            if cell.advance(now_ms) {
                changed |= 1 << index;
            }
        }
        changed
    }
}

impl Default for ColorTransition {
    fn default() -> Self { Self::new() }
}
