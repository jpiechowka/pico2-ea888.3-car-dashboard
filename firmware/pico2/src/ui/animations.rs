use embassy_time::Instant;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::IntoStorage;

use crate::render::CELL_COUNT;

#[allow(dead_code)]
const SHAKE_AMPLITUDE: f32 = 3.0;

#[allow(dead_code)]
const SHAKE_FREQUENCY: f32 = 0.5;

const COLOR_TRANSITION_DURATION_MS: u32 = 300;

const COLOR_SNAP_THRESHOLD: i32 = 2;

#[inline]
#[allow(dead_code)]
pub fn calculate_shake_offset(
    frame: u32,
    is_critical: bool,
) -> i32 {
    if !is_critical {
        return 0;
    }

    let phase = frame as f32 * SHAKE_FREQUENCY;
    let offset = micromath::F32(phase).sin().0 * SHAKE_AMPLITUDE;
    offset as i32
}

pub struct ColorTransition {
    current_colors: [Rgb565; CELL_COUNT],
    target_colors: [Rgb565; CELL_COUNT],
    transitioning: [bool; CELL_COUNT],
    last_update: Option<Instant>,
}

impl ColorTransition {
    pub const fn new() -> Self {
        use super::colors::BLACK;
        Self {
            current_colors: [BLACK; CELL_COUNT],
            target_colors: [BLACK; CELL_COUNT],
            transitioning: [false; CELL_COUNT],
            last_update: None,
        }
    }

    pub fn set_target(
        &mut self,
        cell_idx: usize,
        target: Rgb565,
    ) -> bool {
        if self.target_colors[cell_idx] == target {
            false
        } else {
            self.target_colors[cell_idx] = target;
            self.transitioning[cell_idx] = true;
            true
        }
    }

    #[inline]
    pub const fn get_current(
        &self,
        cell_idx: usize,
    ) -> Rgb565 {
        self.current_colors[cell_idx]
    }

    pub fn update(
        &mut self,
        now: Instant,
    ) -> u8 {
        let delta_ms = if let Some(last) = self.last_update {
            now.duration_since(last).as_millis() as u32
        } else {
            16
        };
        self.last_update = Some(now);

        let t = (delta_ms as f32 / COLOR_TRANSITION_DURATION_MS as f32).min(1.0);

        let mut changed: u8 = 0;

        for i in 0..CELL_COUNT {
            if self.transitioning[i] {
                let current = self.current_colors[i];
                let target = self.target_colors[i];

                if current == target {
                    self.transitioning[i] = false;
                    continue;
                }

                let new_color = lerp_rgb565(current, target, t);

                if colors_close_enough(new_color, target) {
                    self.current_colors[i] = target;
                    self.transitioning[i] = false;
                } else {
                    self.current_colors[i] = new_color;
                }

                changed |= 1 << i;
            }
        }

        changed
    }
}

impl Default for ColorTransition {
    fn default() -> Self { Self::new() }
}

fn lerp_rgb565(
    from: Rgb565,
    to: Rgb565,
    t: f32,
) -> Rgb565 {
    let from_raw = from.into_storage();
    let to_raw = to.into_storage();

    let from_r = i32::from((from_raw >> 11) & 0x1F);
    let from_g = i32::from((from_raw >> 5) & 0x3F);
    let from_b = i32::from(from_raw & 0x1F);

    let to_r = i32::from((to_raw >> 11) & 0x1F);
    let to_g = i32::from((to_raw >> 5) & 0x3F);
    let to_b = i32::from(to_raw & 0x1F);

    let t_fixed = (t * 256.0) as i32;

    let compute_step = |delta: i32| -> i32 {
        if delta == 0 || t_fixed == 0 {
            0
        } else {
            let step = (delta * t_fixed) >> 8;
            if step == 0 {
                if delta > 0 { 1 } else { -1 }
            } else {
                step
            }
        }
    };

    let new_r = from_r + compute_step(to_r - from_r);
    let new_g = from_g + compute_step(to_g - from_g);
    let new_b = from_b + compute_step(to_b - from_b);

    let r = new_r.clamp(0, 31) as u16;
    let g = new_g.clamp(0, 63) as u16;
    let b = new_b.clamp(0, 31) as u16;

    Rgb565::new(r as u8, g as u8, b as u8)
}

fn colors_close_enough(
    a: Rgb565,
    b: Rgb565,
) -> bool {
    let a_raw = a.into_storage();
    let b_raw = b.into_storage();

    let a_r = i32::from((a_raw >> 11) & 0x1F);
    let a_g = i32::from((a_raw >> 5) & 0x3F);
    let a_b = i32::from(a_raw & 0x1F);

    let b_r = i32::from((b_raw >> 11) & 0x1F);
    let b_g = i32::from((b_raw >> 5) & 0x3F);
    let b_b = i32::from(b_raw & 0x1F);

    let diff = (a_r - b_r).abs() + (a_g - b_g).abs() + (a_b - b_b).abs();
    diff <= COLOR_SNAP_THRESHOLD
}
