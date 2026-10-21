//! Pure PL022 prescaler selection shared by SPI construction and reconfiguration.

/// Returns (even CPSDVSR, SCR) for requested `freq` Hz and peripheral clock `clk_peri` Hz.
/// Requests above the hardware maximum select clk_peri/2. Panics for zero clock/rate or a rate
/// below the achievable minimum. The selected rational frequency never exceeds the request.
pub(crate) fn calc_prescs(
    clk_peri: u32,
    freq: u32,
) -> (u8, u8) {
    assert!(clk_peri != 0, "SPI peripheral clock must be nonzero");
    assert!(freq != 0, "SPI frequency must be nonzero");
    // Widen before doubling/roundup: all accepted u32 rates must preserve the clock ratio.
    let ratio = u64::from(clk_peri).div_ceil(u64::from(freq) * 2);
    if ratio > 127 * 256 {
        panic!("Requested too low SPI frequency");
    }
    let presc = ratio.div_ceil(256);
    let postdiv = if presc == 1 { ratio } else { ratio.div_ceil(presc) };
    ((presc * 2) as u8, (postdiv - 1) as u8)
}

#[cfg(test)]
/// Regression cases for wide inputs and established valid divider selection.
mod tests {
    extern crate std;
    use super::calc_prescs;

    #[test]
    /// Extreme requested rates must select the fastest legal divider without intermediate overflow.
    fn high_requested_rates_are_clamped_without_overflow() {
        for rate in [u32::MAX / 2, u32::MAX / 2 + 1, u32::MAX] {
            assert_eq!(calc_prescs(150_000_000, rate), (2, 0));
        }
        assert_eq!(calc_prescs(u32::MAX, u32::MAX), (2, 0));
    }

    #[test]
    /// Valid rates preserve the existing two-stage rounding, including the slowest accepted case.
    fn existing_valid_dividers_are_preserved() {
        for (clock, requested, expected) in [
            (150_000_000, 75_000_000, (2, 0)),
            (150_000_000, 30_000_000, (2, 2)),
            (150_000_000, 100_000, (6, 249)),
            (150_000_000, 2307, (254, 255)),
            (280_000_000, 70_000_000, (2, 1)),
            (150_000_001, 75_000_000, (2, 1)),
        ] {
            let result = calc_prescs(clock, requested);
            assert_eq!(result, expected);
            assert!(u64::from(clock) <= u64::from(requested) * u64::from(result.0) * (u64::from(result.1) + 1));
        }
    }

    #[test]
    /// Unusable inputs must panic instead of yielding an illegal zero prescaler.
    fn zero_and_below_minimum_rates_are_rejected() {
        for (clock, rate) in [(0, 1), (150_000_000, 0), (150_000_000, 2306)] {
            assert!(std::panic::catch_unwind(|| calc_prescs(clock, rate)).is_err());
        }
    }
}
