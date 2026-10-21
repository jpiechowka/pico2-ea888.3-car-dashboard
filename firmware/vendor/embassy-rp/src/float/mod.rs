//! RP2040 ROM-backed arithmetic overrides; excluded from RP235x builds.
//! Bit classification handles NaN/infinity outside the ROM; ordinary arithmetic
//! retains ROM rounding and subnormal limitations, not full IEEE exception state.

// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp2040-hal/src/float/mod.rs

use core::ops;

/// Unsigned representation operations needed to classify IEEE binary32/binary64 bits.
pub(crate) trait Int:
    Copy
    + core::fmt::Debug
    + PartialEq
    + PartialOrd
    + ops::AddAssign
    + ops::SubAssign
    + ops::BitAndAssign
    + ops::BitOrAssign
    + ops::BitXorAssign
    + ops::ShlAssign<i32>
    + ops::ShrAssign<u32>
    + ops::Add<Output = Self>
    + ops::Sub<Output = Self>
    + ops::Div<Output = Self>
    + ops::Shl<u32, Output = Self>
    + ops::Shr<u32, Output = Self>
    + ops::BitOr<Output = Self>
    + ops::BitXor<Output = Self>
    + ops::BitAnd<Output = Self>
    + ops::Not<Output = Self>
{
    /// All-zero bit pattern used by masks and comparisons.
    const ZERO: Self;
}

/// Implements representation zero for the supplied unsigned integer type.
macro_rules! int_impl {
    ($ty:ty) => {
        impl Int for $ty {
            /// All bits clear in this representation width.
            const ZERO: Self = 0;
        }
    };
}

int_impl!(u32);
int_impl!(u64);

/// Float bit-layout contract used without invoking arithmetic classification intrinsics.
pub(crate) trait Float:
    Copy
    + core::fmt::Debug
    + PartialEq
    + PartialOrd
    + ops::AddAssign
    + ops::MulAssign
    + ops::Add<Output = Self>
    + ops::Sub<Output = Self>
    + ops::Div<Output = Self>
    + ops::Rem<Output = Self>
{
    /// Unsigned integer with the same bit width as this float
    type Int: Int;

    /// NaN representation for the float
    const NAN: Self;

    /// The bitwidth of the float type
    const BITS: u32;

    /// Number of stored fraction bits, excluding the implicit leading bit
    const SIGNIFICAND_BITS: u32;

    /// A mask for the sign bit
    const SIGN_MASK: Self::Int;

    /// A mask for the significand
    const SIGNIFICAND_MASK: Self::Int;

    /// A mask for the exponent
    const EXPONENT_MASK: Self::Int;

    /// Returns the exact bit representation of `self`, including sign and NaN payload
    fn repr(self) -> Self::Int;

    /// Returns the float with bit pattern `a`; every pattern is accepted
    fn from_repr(a: Self::Int) -> Self;

    /// Returns `self` with its arithmetic sign negated
    fn negate(self) -> Self;

    /// Returns true if `self` is either NaN or infinity
    fn is_not_finite(self) -> bool { (self.repr() & <Self as Float>::EXPONENT_MASK) == <Self as Float>::EXPONENT_MASK }

    /// Returns true if `self` is infinity
    #[allow(unused)]
    fn is_infinity(self) -> bool {
        (self.repr() & (<Self as Float>::EXPONENT_MASK | <Self as Float>::SIGNIFICAND_MASK))
            == <Self as Float>::EXPONENT_MASK
    }

    /// Returns true if `self` is NaN
    fn is_nan(self) -> bool {
        (self.repr() & (<Self as Float>::EXPONENT_MASK | <Self as Float>::SIGNIFICAND_MASK))
            > <Self as Float>::EXPONENT_MASK
    }

    /// Returns the sign bit of `self`, including negative zero and signed NaNs
    fn is_sign_negative(self) -> bool { (self.repr() & <Self as Float>::SIGN_MASK) != Self::Int::ZERO }

    /// Returns true if `self` is zero (either sign)
    fn is_zero(self) -> bool {
        (self.repr() & (<Self as Float>::SIGNIFICAND_MASK | <Self as Float>::EXPONENT_MASK)) == Self::Int::ZERO
    }
}

/// Implements binary layout for `$ty`/`$ity` using total `$bits` and stored `$significand_bits`.
macro_rules! float_impl {
    ($ty:ident, $ity:ident, $bits:expr, $significand_bits:expr) => {
        impl Float for $ty {
            /// Unsigned bit representation with identical width.
            type Int = $ity;

            /// Total representation width in bits.
            const BITS: u32 = $bits;
            /// Mask selecting the biased exponent bits.
            const EXPONENT_MASK: Self::Int = !(<Self as Float>::SIGN_MASK | <Self as Float>::SIGNIFICAND_MASK);
            /// Canonical quiet NaN supplied by the primitive type.
            const NAN: Self = <$ty>::NAN;
            /// Stored fraction width, excluding the implicit leading bit.
            const SIGNIFICAND_BITS: u32 = $significand_bits;
            /// Mask selecting stored fraction bits.
            const SIGNIFICAND_MASK: Self::Int = (1 << <Self as Float>::SIGNIFICAND_BITS) - 1;
            /// Mask selecting the most significant sign bit.
            const SIGN_MASK: Self::Int = 1 << (<Self as Float>::BITS - 1);

            /// Returns the exact bits of `self` without numeric conversion.
            fn repr(self) -> Self::Int { self.to_bits() }

            /// Returns the float with representation `a`, preserving every input bit.
            fn from_repr(a: Self::Int) -> Self { Self::from_bits(a) }

            /// Returns arithmetic negation of `self`, including zero and infinity signs.
            fn negate(self) -> Self { -self }
        }
    };
}

float_impl!(f32, u32, 32, 23);
float_impl!(f64, u64, 64, 52);

/// Nonfinite classification and ROM addition/subtraction ABI wrappers.
mod add_sub;
/// Ordered/unordered comparison results for compiler and ARM EABI callers.
mod cmp;
/// ROM numeric conversions and explicit nonfinite precision conversions.
mod conv;
/// Floating division with per-core SIO divider state preservation.
mod div;
/// ROM elementary functions and argument-domain handling.
mod functions;
/// ROM multiplication with nonfinite operand handling.
mod mul;
