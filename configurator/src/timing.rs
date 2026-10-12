//! Synthetic preview time advances by elapsed monotonic milliseconds, never frames.
#[derive(Clone, Copy, Debug)]
pub struct Playback {
    sample: u64,
    anchor: Option<(f64, u64)>,
}

impl Playback {
    pub const fn new(sample: u64) -> Self { Self { sample, anchor: None } }

    pub fn start(
        &mut self,
        now_ms: f64,
    ) {
        if self.anchor.is_none() && now_ms.is_finite() && now_ms >= 0.0 {
            self.anchor = Some((now_ms, self.sample));
        }
    }

    pub fn sample(
        &mut self,
        now_ms: f64,
    ) -> u64 {
        if let Some((anchor, base)) = self.anchor
            && now_ms.is_finite()
            && now_ms >= anchor
        {
            self.sample = self.sample.max(base.saturating_add((now_ms - anchor) as u64));
        }
        self.sample
    }

    pub fn pause(
        &mut self,
        now_ms: f64,
    ) -> u64 {
        let sample = self.sample(now_ms);
        self.anchor = None;
        sample
    }
}
