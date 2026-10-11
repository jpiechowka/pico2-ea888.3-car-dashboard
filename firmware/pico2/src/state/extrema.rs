//! Extrema start empty: zero is a measurement, not an initialization value.

pub(super) fn valid_sample(value: f32) -> bool { value.is_finite() && value.abs() <= 1_000_000.0 }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extrema {
    range: Option<(f32, f32)>,
}

impl Extrema {
    pub const fn new() -> Self { Self { range: None } }

    pub fn min(&self) -> Option<f32> { self.range.map(|(min, _)| min) }

    pub fn max(&self) -> Option<f32> { self.range.map(|(_, max)| max) }

    pub fn would_raise(
        &self,
        value: f32,
    ) -> bool {
        valid_sample(value) && self.range.is_some_and(|(_, max)| value > max)
    }

    pub fn would_extend(
        &self,
        value: f32,
    ) -> bool {
        valid_sample(value) && self.range.is_some_and(|(min, max)| value < min || value > max)
    }

    pub fn record(
        &mut self,
        value: f32,
    ) {
        if valid_sample(value) {
            self.range = Some(
                self.range
                    .map_or((value, value), |(min, max)| (min.min(value), max.max(value))),
            );
        }
    }

    pub fn reset_to(
        &mut self,
        value: f32,
    ) {
        self.range = None;
        self.record(value);
    }
}
