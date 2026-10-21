//! Extrema start empty: zero is a measurement, not an initialization value.

/// Returns whether canonical `value` is finite with magnitude <=1,000,000,
/// keeping shared sums/formatting bounded; this is not a physical sensor range.
pub(crate) fn valid_sample(value: f32) -> bool { value.is_finite() && value.abs() <= 1_000_000.0 }

/// Accepted lifetime numeric range; empty until the first valid sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extrema {
    /// Inclusive canonical `(minimum, maximum)`, absent before valid data/reset.
    range: Option<(f32, f32)>,
}

impl Extrema {
    /// Returns empty extrema without inventing a zero measurement.
    pub const fn new() -> Self { Self { range: None } }

    /// Returns the accepted minimum, or `None` when empty.
    pub fn min(&self) -> Option<f32> { self.range.map(|(min, _)| min) }

    /// Returns the accepted maximum, or `None` when empty.
    pub fn max(&self) -> Option<f32> { self.range.map(|(_, max)| max) }

    /// Returns whether valid `value` strictly exceeds an existing maximum;
    /// the first sample and invalid values never count as new-peak events.
    pub fn would_raise(
        &self,
        value: f32,
    ) -> bool {
        valid_sample(value) && self.range.is_some_and(|(_, max)| value > max)
    }

    /// Returns whether valid `value` strictly extends either existing bound;
    /// empty extrema and equality do not produce peak events.
    pub fn would_extend(
        &self,
        value: f32,
    ) -> bool {
        valid_sample(value) && self.range.is_some_and(|(min, max)| value < min || value > max)
    }

    /// Extends both bounds with canonical `value` when valid; invalid input is
    /// ignored and the first valid sample initializes both bounds identically.
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

    /// Clears old extrema and initializes both to `value` if valid; invalid
    /// input leaves the range empty rather than retaining previous measurements.
    pub fn reset_to(
        &mut self,
        value: f32,
    ) {
        self.range = None;
        self.record(value);
    }
}
