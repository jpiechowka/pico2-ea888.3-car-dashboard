use dashboard_pico2::input_model::DebouncedButton;
use dashboard_pico2::input_trace::Observation;
use embassy_time::Instant;

pub struct ButtonState(DebouncedButton);
impl ButtonState {
    pub const fn new() -> Self { Self(DebouncedButton::new()) }

    pub fn observe(
        &mut self,
        is_low: bool,
    ) -> Option<Observation> {
        let now = Instant::now();
        let since = self.0.update_with_origin(is_low, now.as_millis())?;
        Observation::new(since.checked_mul(1000)?, now.as_micros())
    }
}
