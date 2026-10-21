//! Fixed-memory integer distributions. Quantiles are explicit inclusive bounds,
//! never interpolated point estimates. Sixteen bins per power of two, exact
//! through 31; compact instances keep an explicit overflow bucket.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Inclusive bucket interval in the recorded values' units; not an exact quantile.
pub struct Bounds {
    /// Smallest value assigned to this bucket.
    pub lower: u64,
    /// Largest value assigned to this bucket, including `u64::MAX` for overflow.
    pub upper: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Immutable counts, exact extrema and bounded nearest-rank quantiles.
pub struct Distribution {
    /// Accepted observations, including overflow; freezes at `u32::MAX`.
    pub samples: u32,
    /// Accepted observations beyond the configured ordinary bins.
    pub overflow: u32,
    /// Saturating count of observations rejected after sample-count exhaustion.
    pub rejected: u32,
    /// Exact accepted minimum, including overflow values; `None` when empty.
    pub min: Option<u64>,
    /// Exact accepted maximum, including overflow values; `None` when empty.
    pub max: Option<u64>,
    /// Bucket containing rank `ceil(samples * 50 / 100)`, or `None` when empty.
    pub p50: Option<Bounds>,
    /// Bucket containing rank `ceil(samples * 95 / 100)`, or `None` when empty.
    pub p95: Option<Bounds>,
    /// Bucket containing rank `ceil(samples * 99 / 100)`, or `None` when empty.
    pub p99: Option<Bounds>,
}

/// Packed fixed-memory distribution with `N` ordinary bins and one overflow bin.
///
/// `N` must be a multiple of 16 in `32..=976`; 976 covers all `u64` values,
/// while 160 covers `0..=8191` before overflow. Recording never allocates.
pub struct Histogram<const N: usize> {
    /// Least significant 16 bits of each ordinary bin's count.
    low: [u16; N],
    /// Count bits 16 through 23 for each ordinary bin.
    middle: [u8; N],
    // Unary high bytes: one zero terminates each bin's run of ones.
    // sum(high) <= floor(u32::MAX / 2^24) = 255, so N + 255 <= 1231 bits.
    /// Count bits 24 through 31 encoded as unary runs, each terminated by zero.
    high: [u32; 39],
    /// Number of unary one bits, at most 255 under the total-count bound.
    high_total: u8,
    /// Total accepted observations across ordinary and overflow bins.
    count: u32,
    /// Accepted observations whose index is at least `N`.
    overflow: u32,
    /// Saturating rejected-observation count after `count` reaches `u32::MAX`.
    rejected: u32,
    /// Exact minimum across accepted values; absent before the first record.
    min: Option<u64>,
    /// Exact maximum across accepted values; absent before the first record.
    max: Option<u64>,
}

#[derive(Default)]
/// Forward-only decoder reused for nondecreasing nearest-rank queries.
struct Cursor {
    /// First ordinary bin not yet accumulated.
    next_bin: usize,
    /// Bit offset of the next unary high-count run.
    high_bit: usize,
    /// Accepted observations in all previously decoded ordinary bins.
    cumulative: u64,
}

/// Returns the full histogram index `0..=975` containing `value` in caller units.
fn index(value: u64) -> usize {
    if value < 16 {
        return value as usize;
    }
    let exponent = 63 - value.leading_zeros();
    let sub = (value - (1u64 << exponent)) >> (exponent - 4);
    16 + (exponent as usize - 4) * 16 + sub as usize
}

/// Returns the inclusive interval for `index` in `0..976`.
/// Out-of-range indices violate this internal contract and may overflow shifts.
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
    /// Returns an empty histogram; panics for the same invalid `N` as [`Self::new`].
    fn default() -> Self { Self::new() }
}
impl<const N: usize> Histogram<N> {
    /// Returns an empty histogram. Panics unless `N` is a multiple of 16 in `32..=976`.
    pub const fn new() -> Self {
        assert!(N >= 32 && N <= 976 && N.is_multiple_of(16));
        Self {
            low: [0; N],
            middle: [0; N],
            high: [0; 39],
            high_total: 0,
            count: 0,
            overflow: 0,
            rejected: 0,
            min: None,
            max: None,
        }
    }

    /// Records `value` in caller-selected units, updating its bin and exact extrema.
    /// Returns `true` on acceptance, including overflow-bin acceptance. At `u32::MAX`
    /// samples, returns `false` and only saturating-increments the rejection count.
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
            let (low, carry) = self.low[slot].overflowing_add(1);
            self.low[slot] = low;
            if carry {
                let (middle, carry) = self.middle[slot].overflowing_add(1);
                self.middle[slot] = middle;
                if carry {
                    self.increment_high(slot);
                }
            }
        } else {
            self.overflow += 1;
        }
        self.count += 1;
        self.min = Some(self.min.map_or(value, |old| old.min(value)));
        self.max = Some(self.max.map_or(value, |old| old.max(value)));
        true
    }

    // Insert a one before this bin's terminating zero. Only a 24-bit carry
    // reaches here, at most 255 times between clears. Work is bounded by 39
    // word scans, 31 zero selections and 39 word shifts; no allocation.
    /// Inserts one unary high-count unit for `slot` in `0..N` after a 24-bit carry.
    /// Caller must retain room under the total-count limit; at most 255 such
    /// insertions occur between clears. Shifts the fixed suffix without allocation.
    fn increment_high(
        &mut self,
        slot: usize,
    ) {
        let mut remaining = slot;
        let mut word = 0;
        while remaining >= self.high[word].count_zeros() as usize {
            remaining -= self.high[word].count_zeros() as usize;
            word += 1;
        }
        let mut zeros = !self.high[word];
        for _ in 0..remaining {
            zeros &= zeros - 1;
        }
        let bit = zeros.trailing_zeros();
        let mask = (1u32 << bit) - 1;
        let old = self.high[word];
        self.high[word] = (old & mask) | ((old & !mask) << 1) | (1 << bit);
        let mut carry = old >> 31;
        for next in &mut self.high[word + 1..] {
            let old = *next;
            *next = (old << 1) | carry;
            carry = old >> 31;
        }
        self.high_total += 1;
    }

    /// Returns inclusive bounds around nearest rank `ceil(count * percentile / 100)`.
    /// `percentile` outside `1..=100`, or an empty histogram, returns `None`.
    /// Bounds retain bin width and may extend beyond the exact observed extrema.
    pub fn quantile(
        &self,
        percentile: u8,
    ) -> Option<Bounds> {
        if self.count == 0 || !(1..=100).contains(&percentile) {
            return None;
        }
        let rank = (u64::from(self.count) * u64::from(percentile)).div_ceil(100);
        Some(self.bounds_at_rank(rank, &mut Cursor::default()))
    }

    // Reuse a cursor only for nondecreasing, nonzero ranks. Summary decodes
    // each counter once; individual quantile queries start at the first bin.
    /// Returns the bucket containing one-based `rank` in `1..=count`.
    /// Advances `cursor`; reuse requires nondecreasing ranks on this unchanged
    /// histogram. Violating these private preconditions can underflow or misidentify a bin.
    fn bounds_at_rank(
        &self,
        rank: u64,
        cursor: &mut Cursor,
    ) -> Bounds {
        while cursor.cumulative < rank && cursor.next_bin < N {
            let low = self.low[cursor.next_bin];
            let middle = self.middle[cursor.next_bin];
            let mut count = u32::from(low) | (u32::from(middle) << 16);
            if self.high_total != 0 {
                while self.high[cursor.high_bit / 32] & (1 << (cursor.high_bit % 32)) != 0 {
                    count += 1 << 24;
                    cursor.high_bit += 1;
                }
                cursor.high_bit += 1;
            }
            cursor.cumulative += u64::from(count);
            cursor.next_bin += 1;
        }
        if cursor.cumulative >= rank {
            return bounds(cursor.next_bin - 1);
        }
        Bounds {
            lower: bounds(N - 1).upper.saturating_add(1),
            upper: u64::MAX,
        }
    }

    /// Returns current counts, exact extrema and p50/p95/p99 bounds in one forward
    /// decode. Empty histograms retain `None` extrema/quantiles, not valid zero samples.
    pub fn summary(&self) -> Distribution {
        let mut cursor = Cursor::default();
        let [p50, p95, p99] = [50, 95, 99].map(|p| {
            (self.count != 0).then(|| {
                let rank = (u64::from(self.count) * p).div_ceil(100);
                self.bounds_at_rank(rank, &mut cursor)
            })
        });
        Distribution {
            samples: self.count,
            overflow: self.overflow,
            rejected: self.rejected,
            min: self.min,
            max: self.max,
            p50,
            p95,
            p99,
        }
    }

    /// Clears every bin, extrema and rejection counter, restoring empty admission.
    pub fn clear(&mut self) {
        self.low.fill(0);
        self.middle.fill(0);
        self.high.fill(0);
        self.high_total = 0;
        self.count = 0;
        self.overflow = 0;
        self.rejected = 0;
        self.min = None;
        self.max = None;
    }
}

#[cfg(test)]
/// Packed-counter regressions against the independently frozen dense implementation.
mod tests {
    use super::*;

    /// Returns equivalent packed/dense states from `bins`, using each occupied bin's
    /// lower edge as its extrema representative. Panics for invalid `N` or total > `u32::MAX`.
    fn seeded<const N: usize>(bins: [u32; N]) -> (Histogram<N>, dense::Histogram<N>) {
        let mut packed = Histogram::new();
        let mut original = dense::Histogram::new();
        let mut bit = 0;
        for (slot, &count) in bins.iter().enumerate() {
            original.count = original.count.checked_add(count).unwrap();
            if count != 0 {
                let value = bounds(slot).lower;
                original.min.get_or_insert(value);
                original.max = Some(value);
            }
            packed.low[slot] = count as u16;
            packed.middle[slot] = (count >> 16) as u8;
            for _ in 0..count >> 24 {
                packed.high[bit / 32] |= 1 << (bit % 32);
                packed.high_total += 1;
                bit += 1;
            }
            bit += 1;
        }
        original.bins = bins;
        packed.count = original.count;
        packed.min = original.min;
        packed.max = original.max;
        (packed, original)
    }

    /// Asserts `packed` and `original` agree on summaries and all valid/adjacent-invalid
    /// percentile queries; panics on any mismatch without modifying either state.
    fn same<const N: usize>(
        packed: &Histogram<N>,
        original: &dense::Histogram<N>,
    ) {
        assert_eq!(packed.summary(), original.summary());
        for p in 0..=101 {
            assert_eq!(packed.quantile(p), original.quantile(p), "percentile {p}");
        }
    }

    #[test]
    /// Checks both packed carry boundaries at every bin against differently positioned high runs.
    fn every_bin_carries_without_changing_dense_distribution() {
        // A carry before, within or after another run moves a different suffix.
        // Exhaust all 976 positions, both low-counter boundaries and existing
        // high runs crossing word boundaries, without billions of insertions.
        for slot in 0..976 {
            for count in [u16::MAX as u32, (1 << 24) - 1] {
                for occupied in [0, 31, 512, 975] {
                    let mut bins = [0; 976];
                    bins[occupied] = (63 << 24) + 7;
                    bins[slot] = count;
                    let (mut packed, mut original) = seeded(bins);
                    let value = bounds(slot).lower;
                    for _ in 0..2 {
                        assert_eq!(packed.record(value), original.record(value));
                        assert_eq!(packed.summary(), original.summary());
                    }
                }
            }
        }
    }

    #[test]
    /// Checks high-run exhaustion, rejection saturation and clear/reuse against dense counts.
    fn exhausted_high_capacity_and_rejection_saturation_match_dense() {
        for slot in 0..976 {
            let mut carry_bins = [0; 976];
            carry_bins[slot] = u32::MAX - (1 << 24);
            let (mut carry_packed, mut carry_original) = seeded(carry_bins);
            assert!(carry_packed.record(bounds(slot).lower));
            assert!(carry_original.record(bounds(slot).lower));
            assert_eq!(carry_packed.summary(), carry_original.summary());
            let mut bins = [0; 976];
            bins[slot] = u32::MAX - 1;
            let (mut packed, mut original) = seeded(bins);
            let value = bounds(slot).lower;
            assert!(packed.record(value));
            assert!(original.record(value));
            packed.rejected = u32::MAX - 1;
            original.rejected = u32::MAX - 1;
            for value in [0, 1, u64::MAX] {
                assert!(!packed.record(value));
                assert!(!original.record(value));
            }
            assert_eq!(packed.summary(), original.summary());
            packed.clear();
            original.clear();
            assert!(packed.record(u64::MAX));
            assert!(original.record(u64::MAX));
            assert_eq!(packed.summary(), original.summary());
        }
        // All 255 high units already exist, spread over separate bins. A
        // final low carry must not overwrite their delimiters or overflow.
        let mut bins = [0; 976];
        for count in &mut bins[700..955] {
            *count = 1 << 24;
        }
        bins[975] = (1 << 24) - 2;
        let (mut packed, mut original) = seeded(bins);
        assert!(packed.record(u64::MAX));
        assert!(original.record(u64::MAX));
        same(&packed, &original);
        assert!(!packed.record(0));
        assert!(!original.record(0));
        same(&packed, &original);
    }

    #[test]
    /// Verifies the last accepted overflow sample freezes compact counts and exact extrema coherently.
    fn compact_overflow_exhaustion_preserves_all_bins_and_extrema() {
        let mut bins = [0; 160];
        bins[159] = u32::MAX - 1;
        let (mut packed, mut original) = seeded(bins);
        assert!(packed.record(u64::MAX));
        assert!(original.record(u64::MAX));
        same(&packed, &original);
        assert!(!packed.record(0));
        assert!(!original.record(0));
        same(&packed, &original);
    }

    /// Checks 96 deterministic distributions at valid capacity `N`, including boundary
    /// inserts and every percentile; panics on any packed/dense disagreement.
    fn random_distributions<const N: usize>() {
        let mut seed = 9181u64;
        for round in 0..96 {
            let mut bins = [0u32; N];
            let mut remaining = u32::MAX;
            for _ in 0..512 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let slot = (seed >> 32) as usize % N;
                let count = (seed as u32 % (1 << (round % 25))).min(remaining);
                bins[slot] += count;
                remaining -= count;
            }
            let (mut packed, mut original) = seeded(bins);
            same(&packed, &original);
            for value in [0, 31, 32, 8191, 8192, u64::MAX] {
                assert_eq!(packed.record(value), original.record(value));
                same(&packed, &original);
            }
        }
    }

    #[test]
    /// Exercises exact-only, compact-overflow and full-range capacities with sparse/dense seeds.
    fn seeded_sparse_dense_and_overflow_distributions_match_all_percentiles() {
        random_distributions::<32>();
        random_distributions::<160>();
        random_distributions::<976>();
    }

    #[test]
    /// Checks every full-range bucket edge and 20,000 deterministic streaming samples against the oracle.
    fn all_value_bin_edges_and_random_streams_match_frozen_dense() {
        let mut packed = Histogram::<976>::new();
        let mut original = dense::Histogram::<976>::new();
        for slot in 0..976 {
            let b = bounds(slot);
            for value in [b.lower.saturating_sub(1), b.lower, b.upper, b.upper.saturating_add(1)] {
                assert_eq!(packed.record(value), original.record(value));
            }
            assert_eq!(packed.summary(), original.summary());
        }
        let mut seed = 19u64;
        for i in 0..20_000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            assert_eq!(packed.record(seed), original.record(seed));
            if i % 100 == 0 {
                same(&packed, &original);
            }
        }
        same(&packed, &original);
    }
    // Frozen dense implementation from 168cb0d7. Only output types and test
    // field visibility differ; keep its arithmetic independent of production.
    /// Frozen dense arithmetic from commit `168cb0d7`; intentionally independent of packed helpers.
    mod dense {
        use super::{Bounds, Distribution};
        /// Reference histogram with one full-width counter per ordinary bin.
        pub struct Histogram<const N: usize> {
            /// Ordinary counts, exposed only for test seeding.
            pub bins: [u32; N],
            /// Accepted samples, including overflow.
            pub count: u32,
            /// Samples outside the first `N` bins.
            pub overflow: u32,
            /// Saturating rejected count after sample exhaustion.
            pub rejected: u32,
            /// Exact minimum or `None` when empty.
            pub min: Option<u64>,
            /// Exact maximum or `None` when empty.
            pub max: Option<u64>,
        }

        /// Returns the independently computed full-range bucket `0..=975` for `value`.
        fn index(value: u64) -> usize {
            if value < 16 {
                return value as usize;
            }
            let exponent = 63 - value.leading_zeros();
            let sub = (value - (1u64 << exponent)) >> (exponent - 4);
            16 + (exponent as usize - 4) * 16 + sub as usize
        }

        /// Returns independently computed inclusive bounds for valid `index` in `0..976`.
        /// Invalid indices may overflow shift arithmetic.
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
            /// Returns an empty reference histogram; panics for invalid capacity like [`Self::new`].
            fn default() -> Self { Self::new() }
        }
        impl<const N: usize> Histogram<N> {
            /// Returns empty reference state; panics unless `N` is a multiple of 16 in `32..=976`.
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

            /// Records `value` and its exact extrema. Returns `true` when accepted,
            /// including overflow; returns `false` at count exhaustion and only increments rejections.
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

            /// Returns the dense nearest-rank bin for `percentile` in `1..=100`;
            /// invalid percentiles or empty state return `None`. Overflow extends to `u64::MAX`.
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

            /// Returns counts, exact extrema and three independently scanned quantiles.
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

            /// Resets all reference bins, counters and extrema to empty admission state.
            pub fn clear(&mut self) {
                self.bins.fill(0);
                self.count = 0;
                self.overflow = 0;
                self.rejected = 0;
                self.min = None;
                self.max = None;
            }
        }
    }
    #[test]
    /// Verifies exhausted packed counts reject new extremes without corrupting quantile decoding.
    fn exhausted_counts_do_not_corrupt_quantiles() {
        let mut histogram = Histogram::<32>::new();
        histogram.low[0] = u16::MAX;
        histogram.middle[0] = u8::MAX;
        histogram.high[..7].fill(u32::MAX);
        histogram.high[7] = u32::MAX >> 1;
        histogram.high_total = 255;
        histogram.count = u32::MAX;
        histogram.min = Some(0);
        histogram.max = Some(0);
        assert!(!histogram.record(u64::MAX));
        assert_eq!(histogram.summary().rejected, 1);
        assert_eq!(histogram.summary().max, Some(0));
        assert_eq!(histogram.quantile(99), Some(Bounds { lower: 0, upper: 0 }));
    }
}
