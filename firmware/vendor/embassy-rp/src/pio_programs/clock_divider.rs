//! Helper functions for calculating PIO clock dividers

use fixed::types::extra::U8;

use crate::clocks::clk_sys_freq;

/// Calculate a PIO clock divider value based on the desired target frequency.
///
/// # Arguments
///
/// * `target_hz` - The desired PIO clock frequency in Hz
///
/// # Returns
///
/// A divider truncated to 1/256, so the achieved rate can exceed the requested rate.
/// Panics for zero target or a computed divider outside 1..=65536.
/// The maximum is encoded as zero only when written to the hardware register.
pub fn calculate_pio_clock_divider(target_hz: u32) -> fixed::FixedU32<U8> {
    calculate_pio_clock_divider_value(clk_sys_freq(), target_hz)
}

/// Calculate a PIO clock divider value based on the desired target frequency.
///
/// # Arguments
///
/// * `sys_hz` - The system clock frequency in Hz
/// * `target_hz` - The desired PIO clock frequency in Hz
///
/// # Returns
///
/// A divider truncated to 1/256, so the achieved rate can exceed the requested rate.
/// Panics for zero target or a computed divider outside 1..=65536.
/// The maximum is encoded as zero only when written to the hardware register.
pub const fn calculate_pio_clock_divider_value(
    sys_hz: u32,
    target_hz: u32,
) -> fixed::FixedU32<U8> {
    // Requires a non-zero frequency
    core::assert!(target_hz > 0);

    // Widen before shifting: even ordinary clock remainders can exceed24 bits.
    let result = ((sys_hz as u64) << 8) / target_hz as u64;

    // 65536 is represented by a zero integer field in the hardware register.
    core::assert!(result <= 0x100_0000);
    // The clock divider can't be used to go faster than the system clock.
    core::assert!(result >= 0x0001_00);

    fixed::FixedU32::from_bits(result as u32)
}

#[cfg(test)]
/// Exact fixed-point arithmetic and overflow boundary regressions.
mod tests {
    use fixed::traits::ToFixed;

    use super::*;

    /// Retains large fractional remainders and the 65536 divider endpoint.
    #[test]
    fn large_remainders_do_not_wrap_and_maximum_divider_is_representable() {
        assert_eq!(
            calculate_pio_clock_divider_value(150_000_000, 80_000_000).to_bits(),
            480
        );
        assert_eq!(calculate_pio_clock_divider_value(u32::MAX, 1 << 24).to_bits(), 65535);
        assert_eq!(calculate_pio_clock_divider_value(65_536_000, 1000).to_bits(), 1 << 24);
    }

    /// Rejects out-of-range division before narrowing the wide intermediate.
    #[test]
    #[should_panic]
    fn oversized_divider_is_rejected_before_integer_shift_wraps() {
        let _ = calculate_pio_clock_divider_value(16_777_217, 1);
    }

    /// Checks fractional precision and wide system-clock arithmetic.
    #[test]
    fn clock_divider_math() {
        // A simple divider that must have a fractional part.
        let divider = calculate_pio_clock_divider_value(125_000_000, 40_000_000);
        let expected: fixed::FixedU32<U8> = 3.125.to_fixed();
        assert_eq!(divider, expected);

        // A system clk so high it would overflow a u32 if shifted left.
        let divider = calculate_pio_clock_divider_value(2_000_000_000, 40_000);
        let expected: fixed::FixedU32<U8> = 50000.to_fixed();
        assert_eq!(divider, expected);

        // A divider that requires all 8 fractional bits.
        let divider = calculate_pio_clock_divider_value(134_283_264, 16_777_216);
        let expected: fixed::FixedU32<U8> = 8.00390625.to_fixed();
        assert_eq!(divider, expected);
    }
}
