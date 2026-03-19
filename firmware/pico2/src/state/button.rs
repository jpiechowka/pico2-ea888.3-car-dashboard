use embassy_time::{Duration, Instant};

pub const DEBOUNCE_MS: u64 = 50;

pub struct ButtonState {
    was_pressed: bool,
    last_change: Option<Instant>,
}

impl ButtonState {
    pub const fn new() -> Self {
        Self {
            was_pressed: false,
            last_change: None,
        }
    }

    pub fn just_pressed(
        &mut self,
        is_low: bool,
    ) -> bool {
        if is_low != self.was_pressed {
            if let Some(last) = self.last_change
                && last.elapsed() < Duration::from_millis(DEBOUNCE_MS)
            {
                return false;
            }

            self.was_pressed = is_low;
            self.last_change = Some(Instant::now());

            return is_low;
        }

        false
    }
}

impl Default for ButtonState {
    fn default() -> Self { Self::new() }
}
