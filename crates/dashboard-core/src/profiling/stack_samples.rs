//! Minimum observed SP only. Unobserved deeper call frames are not measured.
/// Maximum observed usage of one downward-growing reserved stack range.
pub struct StackSamples {
    /// Lowest accepted SP address, inclusive.
    bottom: u32,
    /// Empty-stack SP address, inclusive; greater than bottom.
    top: u32,
    /// Greatest `top - sp` among accepted samples, in bytes.
    observed_used: u32,
}
impl StackSamples {
    /// Returns an initially unused stack sampler for address range `bottom`
    /// through `top` inclusive, or `None` unless bottom < top. Does not inspect
    /// memory, alignment or linker ownership of either address.
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

    /// Samples raw stack pointer `sp`; returns the updated lifetime maximum byte
    /// usage, or `None` outside the reserved range without altering history.
    /// A valid zero means sampled empty-stack usage, not absence of measurement.
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
