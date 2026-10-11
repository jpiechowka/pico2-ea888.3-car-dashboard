//! Minimum observed SP only. Unobserved deeper call frames are not measured.
pub struct StackSamples {
    bottom: u32,
    top: u32,
    observed_used: u32,
}
impl StackSamples {
    pub const fn new(
        bottom: u32,
        top: u32,
    ) -> Option<Self> {
        if bottom < top {
            Some(Self {
                bottom,
                top,
                observed_used: 0,
            })
        } else {
            None
        }
    }

    pub fn observe(
        &mut self,
        sp: u32,
    ) -> Option<u32> {
        if !(self.bottom..=self.top).contains(&sp) {
            return None;
        }
        self.observed_used = self.observed_used.max(self.top - sp);
        Some(self.observed_used)
    }
}
