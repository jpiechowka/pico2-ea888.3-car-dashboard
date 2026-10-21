//! Absolute-time animation: repaint frequency cannot change the trajectory.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

use crate::colors::BLACK;
use crate::render::CELL_COUNT;

/// Fixed decorative color fade duration in monotonic milliseconds.
const COLOR_TRANSITION_DURATION_MS: u32 = 300;

/// Returns warning blink phase for `elapsed_ms`: on for the first200ms of each400ms cycle.
pub const fn blink_on(elapsed_ms: u64) -> bool { (elapsed_ms / 200).is_multiple_of(2) }

/// Shared motion preference. Explicit warning blink has an independent clock.
#[derive(Clone, Copy, Debug)]
pub struct MotionPolicy {
    /// True disables decorative transitions and numeric smoothing, not explicit warning blinking.
    reduced: bool,
}
impl MotionPolicy {
    /// Returns policy for the user's `reduced` motion preference.
    pub const fn new(reduced: bool) -> Self { Self { reduced } }

    /// Returns whether decorative trajectories may animate instead of snapping.
    pub const fn decorative(self) -> bool { !self.reduced }

    /// Returns `configured` smoothing milliseconds when motion is enabled, otherwise zero.
    pub const fn smoothing_ms(
        self,
        configured: u16,
    ) -> u16 {
        if self.reduced { 0 } else { configured }
    }
}

/// One RGB565 trajectory, interpolated from a fixed origin to avoid per-frame quantization drift.
#[derive(Clone, Copy)]
struct Transition {
    /// Quantized color at the most recent target change.
    from: Rgb565,
    /// Last evaluated quantized output color.
    current: Rgb565,
    /// Requested destination color.
    target: Rgb565,
    /// Target-change time in monotonic milliseconds.
    started_ms: u64,
}

impl Transition {
    /// Stationary black trajectory at time zero.
    const IDLE: Self = Self {
        from: BLACK,
        current: BLACK,
        target: BLACK,
        started_ms: 0,
    };

    /// Evaluates at `now_ms` from the fixed origin, clamping elapsed to0..=300ms.
    /// Returns whether quantized output changed; RGB channels round once with widened
    /// arithmetic. Caller normally rejects time reversal before reaching this helper.
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

/// Eight bounded decorative fades driven by elapsed time, never rendered frame count.
pub struct ColorTransition {
    /// Independent cell trajectories in visible-slot order.
    cells: [Transition; CELL_COUNT],
    /// Last admitted shared update/target time; None before the first operation.
    last_update_ms: Option<u64>,
}

impl ColorTransition {
    /// Returns eight stationary black cells with no accepted clock observation.
    pub const fn new() -> Self {
        Self {
            cells: [Transition::IDLE; CELL_COUNT],
            last_update_ms: None,
        }
    }

    /// Returns true after accepting nondecreasing `now_ms` milliseconds; reversal
    /// returns false without lowering the retained clock.
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

    /// Retargets visible `cell_idx` to `target` at monotonic `now_ms`. Returns true
    /// only for a changed target with valid slot/time, first evaluating the old path
    /// at this instant. Invalid slot/reversal rejects; unchanged target still admits time.
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

    /// Returns last evaluated color for `cell_idx`, or BLACK for an out-of-range slot;
    /// does not advance the trajectory.
    pub fn get_current(
        &self,
        cell_idx: usize,
    ) -> Rgb565 {
        self.cells.get(cell_idx).map_or(BLACK, |cell| cell.current)
    }

    /// Decorative transitions snap under reduced motion. Warning blink is not
    /// routed through this policy, including configured LED adapters.
    /// `now_ms` is monotonic milliseconds and `policy` selects animate/snap. Returns
    /// the eight-bit changed-cell mask; reversed time yields zero without mutation.
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

    /// Evaluates all fades at monotonic `now_ms` milliseconds and returns a changed-
    /// cell bitmask. Reversal returns zero unchanged; stalls of300ms or more finish trajectories.
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
    /// Returns stationary black cells, as new.
    fn default() -> Self { Self::new() }
}
