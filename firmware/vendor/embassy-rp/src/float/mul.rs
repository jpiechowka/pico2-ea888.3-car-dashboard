//! ROM multiplication with explicit nonfinite product semantics.
// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp2040-hal/src/float/mul.rs

use super::Float;
use crate::rom_data;

/// Precision-specific ROM product entry point; caller supplies the nonfinite policy.
trait ROMMul {
    /// Returns the ROM product of `self` and `b`; retains ROM rounding/subnormal behavior.
    fn rom_mul(
        self,
        b: Self,
    ) -> Self;
}

impl ROMMul for f32 {
    /// Returns the ROM product of `self` and `b`; retains ROM rounding/subnormal behavior.
    fn rom_mul(
        self,
        b: Self,
    ) -> Self {
        rom_data::float_funcs::fmul(self, b)
    }
}

impl ROMMul for f64 {
    /// Returns the ROM product of `self` and `b`; retains ROM rounding/subnormal behavior.
    fn rom_mul(
        self,
        b: Self,
    ) -> Self {
        rom_data::double_funcs::dmul(self, b)
    }
}

/// Returns `a` times `b`, handling NaN, infinity and signed zero before the ROM
/// product. Finite inputs retain the ROM's rounding and subnormal limitations.
fn mul<F: Float + ROMMul>(
    a: F,
    b: F,
) -> F {
    if a.is_nan() || b.is_nan() {
        return F::NAN;
    }
    if a.is_not_finite() {
        if b.is_zero() {
            // [-]inf * 0 = NaN
            return F::NAN;
        }

        return if b.is_sign_negative() {
            // [+/-]inf * (-X) = [-/+]inf
            a.negate()
        } else {
            // [-]inf * X = [-]inf
            a
        };
    }

    if b.is_not_finite() {
        if a.is_zero() {
            // 0 * [-]inf = NaN
            return F::NAN;
        }

        return if a.is_sign_negative() {
            // (-X) * [+/-]inf = [-/+]inf
            b.negate()
        } else {
            // X * [-]inf = [-]inf
            b
        };
    }

    a.rom_mul(b)
}

intrinsics! {
    #[alias = __mulsf3vfp]
    #[aeabi = __aeabi_fmul]
    /// Returns `a * b`; NaN propagates, zero times infinity is NaN, and other infinite products use XOR signs. Finite values use ROM rounding.
    extern "C" fn __mulsf3(a: f32, b: f32) -> f32 {
        mul(a, b)
    }

    #[bootrom_v2]
    #[alias = __muldf3vfp]
    #[aeabi = __aeabi_dmul]
    /// Returns `a * b` with nonfinite handling; finite arithmetic requires ROM v2 or later (older ROM panics).
    extern "C" fn __muldf3(a: f64, b: f64) -> f64 {
        mul(a, b)
    }
}

/// Native checks exercise only software special-value branches, never ROM/MMIO.
#[cfg(all(test, not(target_arch = "arm")))]
mod tests {
    /// Both operand orders preserve product sign for infinities and reject zero times infinity.
    #[test]
    fn infinite_products_match_ieee_signs_in_both_orders() {
        for a in [-2.0_f32, -0.0, 0.0, 2.0] {
            for b in [f32::NEG_INFINITY, f32::INFINITY] {
                let expected = a * b;
                for actual in [super::mul(a, b), super::mul(b, a)] {
                    if expected.is_nan() {
                        assert!(actual.is_nan());
                    } else {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }
                }
            }
        }
        for a in [-2.0_f64, -0.0, 0.0, 2.0] {
            for b in [f64::NEG_INFINITY, f64::INFINITY] {
                let expected = a * b;
                for actual in [super::mul(a, b), super::mul(b, a)] {
                    if expected.is_nan() {
                        assert!(actual.is_nan());
                    } else {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    }
                }
            }
        }
    }

    /// NaN remains NaN even when the other operand is infinity, regardless of signs/order.
    #[test]
    fn nan_products_remain_nan() {
        for a in [f32::NEG_INFINITY, f32::INFINITY, -2.0, 2.0] {
            for nan in [f32::NAN, -f32::NAN] {
                assert!(super::mul(a, nan).is_nan());
                assert!(super::mul(nan, a).is_nan());
            }
        }
        for a in [f64::NEG_INFINITY, f64::INFINITY, -2.0, 2.0] {
            for nan in [f64::NAN, -f64::NAN] {
                assert!(super::mul(a, nan).is_nan());
                assert!(super::mul(nan, a).is_nan());
            }
        }
    }
}
