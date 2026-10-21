//! Histogram quantile intervals checked against sorted exact samples and compact overflow buckets.
use dashboard_host_validation::metrics::Histogram;

/// All percentiles contain the exact nearest-rank value across power-of-two edges and deterministic random samples.
#[test]
fn quantile_bounds_cover_exact_sorted_reference_and_all_integer_edges() {
    let mut values = vec![0, 1, 15, 16, 31, 32, u64::MAX];
    for bit in 0..64 {
        let value = 1u64 << bit;
        values.extend([value - 1, value, value.saturating_add(1)]);
    }
    let mut seed = 91u64;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.push(seed);
    }
    let mut histogram = Histogram::<976>::new();
    for &value in &values {
        assert!(histogram.record(value));
    }
    values.sort();
    for percentile in 1..=100 {
        let bounds = histogram.quantile(percentile).unwrap();
        let exact = values[(values.len() * percentile as usize).div_ceil(100) - 1];
        assert!(
            bounds.lower <= exact && exact <= bounds.upper,
            "p{percentile}: {bounds:?} vs {exact}"
        );
    }
    let summary = histogram.summary();
    assert_eq!(summary.samples, values.len() as u32);
    assert_eq!(
        (summary.min, summary.max, summary.overflow),
        (Some(0), Some(u64::MAX), 0)
    );
}

/// Empty/invalid quantiles stay absent; compact overflow contributes an explicit unbounded upper bucket.
#[test]
fn compact_histograms_expose_overflow_and_empty_quantiles() {
    let mut histogram = Histogram::<160>::new();
    assert!(histogram.quantile(50).is_none());
    assert!(histogram.quantile(0).is_none());
    assert!(histogram.quantile(101).is_none());
    for value in [0, 8191, 8192, u64::MAX] {
        assert!(histogram.record(value));
    }
    let summary = histogram.summary();
    assert_eq!((summary.samples, summary.overflow), (4, 2));
    assert_eq!(summary.p95.unwrap().lower, 8192);
    assert_eq!(summary.p95.unwrap().upper, u64::MAX);
    assert_eq!(summary.p50.unwrap().upper, 8191);
}
