use dashboard_pico2::input_model::DebouncedButton;
use embassy_time::Instant;

pub struct ButtonState(DebouncedButton);
impl ButtonState {
    pub const fn new() -> Self { Self(DebouncedButton::new()) }

    pub fn just_pressed(
        &mut self,
        is_low: bool,
    ) -> bool {
        self.0.update(is_low, Instant::now().as_millis())
    }
}
