//! Fixed-memory integer distributions. Quantiles are explicit inclusive bounds,
//! never interpolated point estimates. Sixteen bins per power of two, exact
//! through 31; compact instances keep an explicit overflow bucket.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub lower: u64,
    pub upper: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Distribution {
    pub samples: u32,
    pub overflow: u32,
    pub rejected: u32,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub p50: Option<Bounds>,
    pub p95: Option<Bounds>,
    pub p99: Option<Bounds>,
}

pub struct Histogram<const N: usize> {
    bins: [u32; N],
    count: u32,
    overflow: u32,
    rejected: u32,
    min: Option<u64>,
    max: Option<u64>,
}

fn index(value: u64) -> usize {
    if value < 16 {
        return value as usize;
    }
    let exponent = 63 - value.leading_zeros();
    let sub = (value - (1u64 << exponent)) >> (exponent - 4);
    16 + (exponent as usize - 4) * 16 + sub as usize
}

fn bounds(index: usize) -> Bounds {
    if index < 16 {
        return Bounds {
            lower: index as u64,
            upper: index as u64,
        };
    }
    let exponent = 4 + (index - 16) / 16;
    let width = 1u64 << (exponent - 4);
    let lower = (1u64 << exponent) + ((index - 16) % 16) as u64 * width;
    Bounds {
        lower,
        upper: lower + (width - 1),
    }
}

impl<const N: usize> Default for Histogram<N> {
    fn default() -> Self { Self::new() }
}
impl<const N: usize> Histogram<N> {
    pub const fn new() -> Self {
        assert!(N >= 32 && N <= 976 && N.is_multiple_of(16));
        Self {
            bins: [0; N],
            count: 0,
            overflow: 0,
            rejected: 0,
            min: None,
            max: None,
        }
    }

    /// Freeze the distribution at counter exhaustion, with explicit rejection.
    pub fn record(
        &mut self,
        value: u64,
    ) -> bool {
        if self.count == u32::MAX {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        let slot = index(value);
        if slot < N {
            self.bins[slot] += 1;
        } else {
            self.overflow += 1;
        }
        self.count += 1;
        self.min = Some(self.min.map_or(value, |old| old.min(value)));
        self.max = Some(self.max.map_or(value, |old| old.max(value)));
        true
    }

    pub fn quantile(
        &self,
        percentile: u8,
    ) -> Option<Bounds> {
        if self.count == 0 || !(1..=100).contains(&percentile) {
            return None;
        }
        let rank = (u64::from(self.count) * u64::from(percentile)).div_ceil(100);
        let mut cumulative = 0u64;
        for (index, &count) in self.bins.iter().enumerate() {
            cumulative += u64::from(count);
            if cumulative >= rank {
                return Some(bounds(index));
            }
        }
        Some(Bounds {
            lower: bounds(N - 1).upper.saturating_add(1),
            upper: u64::MAX,
        })
    }

    pub fn summary(&self) -> Distribution {
        Distribution {
            samples: self.count,
            overflow: self.overflow,
            rejected: self.rejected,
            min: self.min,
            max: self.max,
            p50: self.quantile(50),
            p95: self.quantile(95),
            p99: self.quantile(99),
        }
    }

    pub fn clear(&mut self) {
        self.bins.fill(0);
        self.count = 0;
        self.overflow = 0;
        self.rejected = 0;
        self.min = None;
        self.max = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_counts_do_not_corrupt_quantiles() {
        let mut histogram = Histogram::<32>::new();
        histogram.bins[0] = u32::MAX;
        histogram.count = u32::MAX;
        histogram.min = Some(0);
        histogram.max = Some(0);
        assert!(!histogram.record(u64::MAX));
        assert_eq!(histogram.summary().rejected, 1);
        assert_eq!(histogram.summary().max, Some(0));
        assert_eq!(histogram.quantile(99), Some(Bounds { lower: 0, upper: 0 }));
    }
}
