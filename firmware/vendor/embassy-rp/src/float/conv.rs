//! ROM numeric conversions and nonfinite precision conversion.
//! Signed EABI `z` conversions truncate toward zero, unlike raw ROM flooring:
//! <https://github.com/ARM-software/abi-aa/blob/main/rtabi32/rtabi32.rst>.
//! ROM-v2 operations panic on older silicon; NaN-to-integer behavior is ROM-defined.
// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp2040-hal/src/float/conv.rs

use super::Float;
use crate::rom_data;

// Some of these are also not connected in the Pico SDK.  This is probably
// because the ROM version actually does a fixed point conversion, just with
// the fractional width set to zero.

intrinsics! {
    // Not connected in the Pico SDK
    #[slower_than_default]
    #[aeabi = __aeabi_i2f]
    /// Returns signed `i` rounded to f32 using the ROM (nearest, ties even); this slower override is not exported.
    extern "C" fn __floatsisf(i: i32) -> f32 {
        rom_data::float_funcs::int_to_float(i)
    }

    // Not connected in the Pico SDK
    #[slower_than_default]
    #[aeabi = __aeabi_i2d]
    /// Returns signed `i` converted to f64 using ROM v2; this slower override is not exported.
    extern "C" fn __floatsidf(i: i32) -> f64 {
        rom_data::double_funcs::int_to_double(i)
    }

    // Questionable gain
    #[aeabi = __aeabi_l2f]
    /// Returns signed `i` rounded to f32 (nearest, ties even); requires ROM v2 and panics on older ROM.
    extern "C" fn __floatdisf(i: i64) -> f32 {
        rom_data::float_funcs::int64_to_float(i)
    }

    #[bootrom_v2]
    #[aeabi = __aeabi_l2d]
    /// Returns signed `i` rounded to f64 (nearest, ties even); requires ROM v2.
    extern "C" fn __floatdidf(i: i64) -> f64 {
        rom_data::double_funcs::int64_to_double(i)
    }

    // Not connected in the Pico SDK
    #[slower_than_default]
    #[aeabi = __aeabi_ui2f]
    /// Returns unsigned `i` rounded to f32 (nearest, ties even); this slower override is not exported.
    extern "C" fn __floatunsisf(i: u32) -> f32 {
        rom_data::float_funcs::uint_to_float(i)
    }

    // Questionable gain
    #[bootrom_v2]
    #[aeabi = __aeabi_ui2d]
    /// Returns unsigned `i` converted to f64 using ROM v2.
    extern "C" fn __floatunsidf(i: u32) -> f64 {
        rom_data::double_funcs::uint_to_double(i)
    }

    // Questionable gain
    #[bootrom_v2]
    #[aeabi = __aeabi_ul2f]
    /// Returns unsigned `i` rounded to f32 (nearest, ties even); requires ROM v2.
    extern "C" fn __floatundisf(i: u64) -> f32 {
        rom_data::float_funcs::uint64_to_float(i)
    }

    #[bootrom_v2]
    #[aeabi = __aeabi_ul2d]
    /// Returns unsigned `i` rounded to f64 (nearest, ties even); requires ROM v2.
    extern "C" fn __floatundidf(i: u64) -> f64 {
        rom_data::double_funcs::uint64_to_double(i)
    }


    // ROM signed conversion floors, while EABI z conversions truncate toward zero.
    // Use unsigned positive magnitude for negatives, following the Pico SDK policy.
    #[aeabi = __aeabi_f2iz]
    /// Returns `f` truncated toward zero to i32 with signed saturation; NaN follows ROM behavior.
    extern "C" fn __fixsfsi(f: f32) -> i32 {
        to_i32(f, |value| rom_data::float_funcs::float_to_int(value), |value| rom_data::float_funcs::float_to_uint(value))
    }

    #[bootrom_v2]
    #[aeabi = __aeabi_f2lz]
    /// Returns `f` truncated toward zero to i64 with signed saturation using ROM v2; NaN follows ROM behavior.
    extern "C" fn __fixsfdi(f: f32) -> i64 {
        to_i64(f, |value| rom_data::float_funcs::float_to_int64(value), |value| rom_data::float_funcs::float_to_uint64(value))
    }

    // Not connected in the Pico SDK
    #[slower_than_default]
    #[bootrom_v2]
    #[aeabi = __aeabi_d2iz]
    /// Returns `f` truncated toward zero to i32 with signed saturation using ROM v2; this slower override is not exported.
    extern "C" fn __fixdfsi(f: f64) -> i32 {
        to_i32(f, |value| rom_data::double_funcs::double_to_int(value), |value| rom_data::double_funcs::double_to_uint(value))
    }

    // Double-to-64-bit uses the same signed-magnitude truncation policy.
    #[bootrom_v2]
    #[aeabi = __aeabi_d2lz]
    /// Returns `f` truncated toward zero to i64 with signed saturation using ROM v2; NaN follows ROM behavior.
    extern "C" fn __fixdfdi(f: f64) -> i64 {
        to_i64(f, |value| rom_data::double_funcs::double_to_int64(value), |value| rom_data::double_funcs::double_to_uint64(value))
    }

    #[slower_than_default]
    #[aeabi = __aeabi_f2uiz]
    /// Returns `f` floored/clamped to u32 by ROM (nonnegative finite values truncate toward zero); this slower override is not exported.
    extern "C" fn __fixunssfsi(f: f32) -> u32 {
        rom_data::float_funcs::float_to_uint(f)
    }

    #[slower_than_default]
    #[bootrom_v2]
    #[aeabi = __aeabi_f2ulz]
    /// Returns `f` floored/clamped to u64 by ROM v2; this slower override is not exported.
    extern "C" fn __fixunssfdi(f: f32) -> u64 {
        rom_data::float_funcs::float_to_uint64(f)
    }

    #[slower_than_default]
    #[bootrom_v2]
    #[aeabi = __aeabi_d2uiz]
    /// Returns `f` floored/clamped to u32 by ROM v2; this slower override is not exported.
    extern "C" fn __fixunsdfsi(f: f64) -> u32 {
        rom_data::double_funcs::double_to_uint(f)
    }

    #[slower_than_default]
    #[bootrom_v2]
    #[aeabi = __aeabi_d2ulz]
    /// Returns `f` floored/clamped to u64 by ROM v2; this slower override is not exported.
    extern "C" fn __fixunsdfdi(f: f64) -> u64 {
        rom_data::double_funcs::double_to_uint64(f)
    }

    #[bootrom_v2]
    #[alias = __extendsfdf2vfp]
    #[aeabi = __aeabi_f2d]
    /// Returns widened `f`; nonfinite inputs preserve sign and NaN/infinity category in software, finite values use ROM v2.
    extern "C" fn  __extendsfdf2(f: f32) -> f64 {
        if f.is_not_finite() {
            return f64::from_repr(
                // Not finite
                <f64 as Float>::EXPONENT_MASK |
                // Preserve NaN or inf
                ((f.repr() & <f32 as Float>::SIGNIFICAND_MASK) as u64) |
                // Preserve sign
                ((f.repr() & <f32 as Float>::SIGN_MASK) as u64) << (<f64 as Float>::BITS-<f32 as Float>::BITS)
            );
        }
        rom_data::float_funcs::float_to_double(f)
    }

    #[bootrom_v2]
    #[alias = __truncdfsf2vfp]
    #[aeabi = __aeabi_d2f]
    /// Returns narrowed `f`; nonfinite inputs preserve sign and category but not NaN payload, finite values use ROM v2 rounding.
    extern "C" fn __truncdfsf2(f: f64) -> f32 {
        if f.is_not_finite() {
            let mut repr: u32 =
                // Not finite
                <f32 as Float>::EXPONENT_MASK |
                // Preserve sign
                ((f.repr() & <f64 as Float>::SIGN_MASK) >> (<f64 as Float>::BITS-<f32 as Float>::BITS)) as u32;
            // Set NaN
            if  (f.repr() & <f64 as Float>::SIGNIFICAND_MASK) != 0 {
                repr |= 1;
            }
            return f32::from_repr(repr);
        }
        rom_data::double_funcs::double_to_float(f)
    }
}

/// Returns `f` truncated toward zero with signed saturation: `signed` floors
/// nonnegative inputs; `unsigned` floors the positive magnitude of negative inputs.
/// Callback NaN behavior remains ROM-defined; sign/magnitude avoids negating signed MIN.
fn to_i32<F: Float>(
    f: F,
    signed: impl FnOnce(F) -> i32,
    unsigned: impl FnOnce(F) -> u32,
) -> i32 {
    if f.is_sign_negative() {
        let magnitude = unsigned(f.negate());
        if magnitude > i32::MAX as u32 {
            i32::MIN
        } else {
            -(magnitude as i32)
        }
    } else {
        signed(f)
    }
}

/// Returns `f` truncated toward zero with signed saturation: `signed` floors
/// nonnegative inputs; `unsigned` floors the positive magnitude of negative inputs.
/// Callback NaN behavior remains ROM-defined; sign/magnitude avoids negating signed MIN.
fn to_i64<F: Float>(
    f: F,
    signed: impl FnOnce(F) -> i64,
    unsigned: impl FnOnce(F) -> u64,
) -> i64 {
    if f.is_sign_negative() {
        let magnitude = unsigned(f.negate());
        if magnitude > i64::MAX as u64 {
            i64::MIN
        } else {
            -(magnitude as i64)
        }
    } else {
        signed(f)
    }
}

/// Native policy checks use modeled ROM flooring, never hardware entry points.
#[cfg(all(test, not(target_arch = "arm")))]
mod tests {
    /// Negative fractional inputs must truncate toward zero instead of using ROM floor directly.
    #[test]
    fn signed_abi_fractional_conversion_truncates_toward_zero() {
        for value in [-1.75_f32, -1.25] {
            assert_eq!(
                super::to_i32(
                    value,
                    |_| -2,
                    |v| {
                        assert!(v > 0.0);
                        1
                    }
                ),
                -1
            );
            assert_eq!(
                super::to_i64(
                    value,
                    |_| -2,
                    |v| {
                        assert!(v > 0.0);
                        1
                    }
                ),
                -1
            );
        }
        for value in [-1.75_f64, -1.25] {
            assert_eq!(
                super::to_i32(
                    value,
                    |_| -2,
                    |v| {
                        assert!(v > 0.0);
                        1
                    }
                ),
                -1
            );
            assert_eq!(
                super::to_i64(
                    value,
                    |_| -2,
                    |v| {
                        assert!(v > 0.0);
                        1
                    }
                ),
                -1
            );
        }
    }

    /// Signed minimum, overflow and both zero signs avoid overflowing negation.
    #[test]
    fn signed_abi_magnitude_boundaries_saturate() {
        for magnitude in [1_u32 << 31, u32::MAX] {
            assert_eq!(super::to_i32(-1.0_f32, |_| i32::MIN, |_| magnitude), i32::MIN);
        }
        for magnitude in [1_u64 << 63, u64::MAX] {
            assert_eq!(super::to_i64(-1.0_f64, |_| i64::MIN, |_| magnitude), i64::MIN);
        }
        for zero in [-0.0_f32, 0.0] {
            assert_eq!(super::to_i32(zero, |_| 0, |_| 0), 0);
            assert_eq!(super::to_i64(zero, |_| 0, |_| 0), 0);
        }
        assert_eq!(super::to_i32(-0.75_f32, |_| -1, |_| 0), 0);
        assert_eq!(super::to_i64(-0.75_f64, |_| -1, |_| 0), 0);
        assert_eq!(super::to_i32(-2.0_f32, |_| -2, |_| 2), -2);
        assert_eq!(super::to_i64(-2.0_f64, |_| -2, |_| 2), -2);
        assert_eq!(
            super::to_i32(1.75_f32, |_| 1, |_| panic!("positive input uses signed ROM")),
            1
        );
        assert_eq!(
            super::to_i64(1.75_f64, |_| 1, |_| panic!("positive input uses signed ROM")),
            1
        );
    }
}
