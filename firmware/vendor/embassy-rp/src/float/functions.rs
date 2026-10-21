//! ROM elementary functions with argument-domain and coordinate handling.
// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp2040-hal/src/float/functions.rs

use crate::float::{Float, Int};
use crate::rom_data;

/// Raw ROM elementary functions; callers handle unsupported nonfinite inputs.
/// Double precision and atan2 require ROM v2; older ROM calls panic.
trait ROMFunctions {
    /// Returns the ROM square root of `self`; domain and nonfinite checks belong to the caller.
    fn sqrt(self) -> Self;
    /// Returns the ROM natural logarithm of `self`; domain and nonfinite checks belong to the caller.
    fn ln(self) -> Self;
    /// Returns the ROM exponential of `self`, retaining ROM overflow/underflow behavior.
    fn exp(self) -> Self;
    /// Returns the ROM sine of `self` in radians; caller must range-reduce large angles.
    fn sin(self) -> Self;
    /// Returns the ROM cosine of `self` in radians; caller must range-reduce large angles.
    fn cos(self) -> Self;
    /// Returns the ROM tangent of `self` in radians; caller must range-reduce large angles.
    fn tan(self) -> Self;
    /// Returns the angle in radians for vertical coordinate `self` and horizontal coordinate `y`, using ROM v2.
    fn atan2(
        self,
        y: Self,
    ) -> Self;

    /// Returns `self` unchanged within the precision's ROM angle range, otherwise
    /// its remainder modulo the rounded 2*pi constant; large inputs lose phase accuracy.
    fn to_trig_range(self) -> Self;
}

impl ROMFunctions for f32 {
    /// Returns the ROM square root of `self`; domain and nonfinite checks belong to the caller.
    fn sqrt(self) -> Self { rom_data::float_funcs::fsqrt(self) }

    /// Returns the ROM natural logarithm of `self`; domain and nonfinite checks belong to the caller.
    fn ln(self) -> Self { rom_data::float_funcs::fln(self) }

    /// Returns the ROM exponential of `self`, retaining ROM overflow/underflow behavior.
    fn exp(self) -> Self { rom_data::float_funcs::fexp(self) }

    /// Returns the ROM sine of `self` in radians; caller must range-reduce large angles.
    fn sin(self) -> Self { rom_data::float_funcs::fsin(self) }

    /// Returns the ROM cosine of `self` in radians; caller must range-reduce large angles.
    fn cos(self) -> Self { rom_data::float_funcs::fcos(self) }

    /// Returns the ROM tangent of `self` in radians; caller must range-reduce large angles.
    fn tan(self) -> Self { rom_data::float_funcs::ftan(self) }

    /// Returns the angle in radians for vertical coordinate `self` and horizontal coordinate `y`, using ROM v2.
    fn atan2(
        self,
        y: Self,
    ) -> Self {
        rom_data::float_funcs::fatan2(self, y)
    }

    /// Returns `self` for magnitude below 128 radians, otherwise remainder modulo 2*pi.
    fn to_trig_range(self) -> Self {
        // -128 < X < 128, logic from the Pico SDK
        let exponent = (self.repr() & <Self as Float>::EXPONENT_MASK) >> <Self as Float>::SIGNIFICAND_BITS;
        if exponent < 134 {
            self
        } else {
            self % (core::f32::consts::PI * 2.0)
        }
    }
}

impl ROMFunctions for f64 {
    /// Returns the ROM square root of `self`; domain and nonfinite checks belong to the caller.
    fn sqrt(self) -> Self { rom_data::double_funcs::dsqrt(self) }

    /// Returns the ROM natural logarithm of `self`; domain and nonfinite checks belong to the caller.
    fn ln(self) -> Self { rom_data::double_funcs::dln(self) }

    /// Returns the ROM exponential of `self`, retaining ROM overflow/underflow behavior.
    fn exp(self) -> Self { rom_data::double_funcs::dexp(self) }

    /// Returns the ROM sine of `self` in radians; caller must range-reduce large angles.
    fn sin(self) -> Self { rom_data::double_funcs::dsin(self) }

    /// Returns the ROM cosine of `self` in radians; caller must range-reduce large angles.
    fn cos(self) -> Self { rom_data::double_funcs::dcos(self) }

    /// Returns the ROM tangent of `self` in radians; caller must range-reduce large angles.
    fn tan(self) -> Self { rom_data::double_funcs::dtan(self) }

    /// Returns the angle in radians for vertical coordinate `self` and horizontal coordinate `y`, using ROM v2.
    fn atan2(
        self,
        y: Self,
    ) -> Self {
        rom_data::double_funcs::datan2(self, y)
    }

    /// Returns `self` for magnitude below 1024 radians, otherwise remainder modulo 2*pi.
    fn to_trig_range(self) -> Self {
        // -1024 < X < 1024, logic from the Pico SDK
        let exponent = (self.repr() & <Self as Float>::EXPONENT_MASK) >> <Self as Float>::SIGNIFICAND_BITS;
        if exponent < 1033 {
            self
        } else {
            self % (core::f64::consts::PI * 2.0)
        }
    }
}

/// Returns whether `f` is negative and nonzero, or any NaN; negative zero is accepted.
fn is_negative_nonzero_or_nan<F: Float>(f: F) -> bool {
    let repr = f.repr();
    if (repr & F::SIGN_MASK) != F::Int::ZERO {
        // Negative, so anything other than exactly zero
        return (repr & (!F::SIGN_MASK)) != F::Int::ZERO;
    }
    // NaN
    (repr & (F::EXPONENT_MASK | F::SIGNIFICAND_MASK)) > F::EXPONENT_MASK
}

/// Returns the square root of `f`; negative nonzero values and NaNs return NaN, while signed zero reaches ROM.
fn sqrt<F: Float + ROMFunctions>(f: F) -> F {
    if is_negative_nonzero_or_nan(f) {
        F::NAN
    } else {
        f.sqrt()
    }
}

/// Returns the natural logarithm of `f`; negative nonzero values and NaNs return NaN, while zero reaches ROM.
fn ln<F: Float + ROMFunctions>(f: F) -> F { if is_negative_nonzero_or_nan(f) { F::NAN } else { f.ln() } }

/// Returns e raised to `f`; NaN returns NaN without ROM access.
fn exp<F: Float + ROMFunctions>(f: F) -> F { if f.is_nan() { F::NAN } else { f.exp() } }

/// Returns the sine of angle `f` in radians; nonfinite inputs return NaN, large finite angles use remainder reduction.
fn sin<F: Float + ROMFunctions>(f: F) -> F {
    if f.is_not_finite() {
        F::NAN
    } else {
        f.to_trig_range().sin()
    }
}

/// Returns the cosine of angle `f` in radians; nonfinite inputs return NaN, large finite angles use remainder
/// reduction.
fn cos<F: Float + ROMFunctions>(f: F) -> F {
    if f.is_not_finite() {
        F::NAN
    } else {
        f.to_trig_range().cos()
    }
}

/// Returns the tangent of angle `f` in radians; nonfinite inputs return NaN, large finite angles use remainder
/// reduction.
fn tan<F: Float + ROMFunctions>(f: F) -> F {
    if f.is_not_finite() {
        F::NAN
    } else {
        f.to_trig_range().tan()
    }
}

/// Returns the quadrant-aware angle of coordinates `y`, `x` using `rom_atan2`;
/// NaN inputs return NaN without invoking the callback.
fn atan2_with<F: Float>(
    y: F,
    x: F,
    rom_atan2: impl FnOnce(F, F) -> F,
) -> F {
    if y.is_nan() || x.is_nan() {
        F::NAN
    } else {
        rom_atan2(y, x)
    }
}

/// Returns the ROM quadrant-aware angle of vertical `y` and horizontal `x` in radians.
fn atan2<F: Float + ROMFunctions>(
    y: F,
    x: F,
) -> F {
    atan2_with(y, x, ROMFunctions::atan2)
}

/// C symbol wrappers, separated to avoid collisions with generic implementations.
mod intrinsics {
    intrinsics! {
        /// Returns the square root of `f`; negative nonzero inputs/NaNs yield NaN.
        extern "C" fn sqrtf(f: f32) -> f32 {
            super::sqrt(f)
        }

        #[bootrom_v2]
        /// Returns the square root of `f`; negative nonzero inputs/NaNs yield NaN. Requires ROM v2.
        extern "C" fn sqrt(f: f64) -> f64 {
            super::sqrt(f)
        }

        /// Returns the natural logarithm of `f`; negative nonzero inputs/NaNs yield NaN.
        extern "C" fn logf(f: f32) -> f32 {
            super::ln(f)
        }

        #[bootrom_v2]
        /// Returns the natural logarithm of `f`; negative nonzero inputs/NaNs yield NaN. Requires ROM v2.
        extern "C" fn log(f: f64) -> f64 {
            super::ln(f)
        }

        /// Returns e raised to `f`, retaining ROM overflow/underflow behavior.
        extern "C" fn expf(f: f32) -> f32 {
            super::exp(f)
        }

        #[bootrom_v2]
        /// Returns e raised to `f`, retaining ROM overflow/underflow behavior. Requires ROM v2.
        extern "C" fn exp(f: f64) -> f64 {
            super::exp(f)
        }

        #[slower_than_default]
        /// Returns sine of `f` radians; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn sinf(f: f32) -> f32 {
            super::sin(f)
        }

        #[slower_than_default]
        #[bootrom_v2]
        /// Returns sine of `f` radians using ROM v2; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn sin(f: f64) -> f64 {
            super::sin(f)
        }

        #[slower_than_default]
        /// Returns cosine of `f` radians; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn cosf(f: f32) -> f32 {
            super::cos(f)
        }

        #[slower_than_default]
        #[bootrom_v2]
        /// Returns cosine of `f` radians using ROM v2; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn cos(f: f64) -> f64 {
            super::cos(f)
        }

        #[slower_than_default]
        /// Returns tangent of `f` radians; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn tanf(f: f32) -> f32 {
            super::tan(f)
        }

        #[slower_than_default]
        #[bootrom_v2]
        /// Returns tangent of `f` radians using ROM v2; nonfinite inputs yield NaN. This slower override is not exported.
        extern "C" fn tan(f: f64) -> f64 {
            super::tan(f)
        }

        // Questionable gain
        #[bootrom_v2]
        /// Returns the quadrant-aware angle of vertical `a` and horizontal `b`, in radians; NaNs yield NaN. Requires ROM v2.
        extern "C" fn atan2f(a: f32, b: f32) -> f32 {
            super::atan2(a, b)
        }

        // Questionable gain
        #[bootrom_v2]
        /// Returns the quadrant-aware angle of vertical `a` and horizontal `b`, in radians; NaNs yield NaN. Requires ROM v2.
        extern "C" fn atan2(a: f64, b: f64) -> f64 {
            super::atan2(a, b)
        }
    }
}

/// Host checks for the coordinate forwarding contract; no ROM or MMIO is called.
#[cfg(all(test, not(target_arch = "arm")))]
mod tests {
    /// Large coordinates must retain their ratio rather than be treated as angles.
    #[test]
    fn atan2_preserves_coordinates() {
        // The callback observes the exact inputs that would reach ROM.
        for (y, x) in [(128.0_f32, 1.0), (-256.0, 3.0), (0.0, -1.0)] {
            let actual = super::atan2_with(y, x, |actual_y, actual_x| {
                assert_eq!(actual_y.to_bits(), y.to_bits());
                assert_eq!(actual_x.to_bits(), x.to_bits());
                actual_y
            });
            assert_eq!(actual.to_bits(), y.to_bits());
        }
        for (y, x) in [(1024.0_f64, 1.0), (-2048.0, 3.0), (-0.0, -1.0)] {
            let actual = super::atan2_with(y, x, |actual_y, actual_x| {
                assert_eq!(actual_y.to_bits(), y.to_bits());
                assert_eq!(actual_x.to_bits(), x.to_bits());
                actual_y
            });
            assert_eq!(actual.to_bits(), y.to_bits());
        }
    }
    /// NaN in either coordinate bypasses ROM regardless of the other coordinate.
    #[test]
    fn atan2_nan_bypasses_rom() {
        for (y, x) in [(f32::NAN, 1.0), (1.0, f32::NAN), (f32::NAN, f32::NAN)] {
            assert!(super::atan2_with(y, x, |_, _| panic!("ROM must not receive NaN")).is_nan());
        }
        for (y, x) in [(f64::NAN, 1.0), (1.0, f64::NAN), (f64::NAN, f64::NAN)] {
            assert!(super::atan2_with(y, x, |_, _| panic!("ROM must not receive NaN")).is_nan());
        }
    }
}
