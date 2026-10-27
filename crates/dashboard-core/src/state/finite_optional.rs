//! Private lossless storage for optional finite snapshot values.

/// One float's finite bits or a reserved NaN bit pattern for absence.
/// Only the checked constructor admits values; numeric equality preserves signed-zero semantics.
#[derive(Clone, Copy)]
pub(super) struct FiniteOptional {
    /// Original finite IEEE-754 bits, or `NONE.bits`; never another nonfinite pattern.
    bits: u32,
}

/// Checked conversion rejected a NaN or infinity instead of substituting absence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NonFinite;

impl FiniteOptional {
    /// Absent measurement, distinct from every finite value including both zeros.
    pub const NONE: Self = Self { bits: 0x7FC0_0000 };

    /// Returns lossless storage for optional `value`, preserving finite bits and absence.
    /// Returns `NonFinite` for either infinity or any NaN, without producing a replacement value.
    pub fn try_from_option(value: Option<f32>) -> Result<Self, NonFinite> {
        match value {
            None => Ok(Self::NONE),
            Some(value) if value.is_finite() => Ok(Self { bits: value.to_bits() }),
            Some(_) => Err(NonFinite),
        }
    }

    /// Returns the original finite bits as a value, or `None` for absence.
    pub fn get(self) -> Option<f32> {
        if self.bits == Self::NONE.bits {
            None
        } else {
            Some(f32::from_bits(self.bits))
        }
    }
}

impl PartialEq for FiniteOptional {
    /// Returns the same equality as `Option<f32>` with `other`, including equal signed zeros.
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.get() == other.get()
    }
}

impl core::fmt::Debug for FiniteOptional {
    /// Writes the public optional-value representation to `formatter`, returning its formatting result.
    fn fmt(
        &self,
        formatter: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        self.get().fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    //! Bit-boundary and representative-pattern checks independent of scalar sensor admission.
    use super::FiniteOptional;

    /// Detects loss of absence, finite endpoints, subnormals, either zero, or the four-byte storage bound.
    #[test]
    fn compact_storage_preserves_absence_and_finite_boundary_bits() {
        assert_eq!(core::mem::size_of::<FiniteOptional>(), 4);
        assert_eq!(FiniteOptional::NONE.get(), None);
        assert_eq!(FiniteOptional::try_from_option(None).unwrap().get(), None);
        for bits in [
            0,
            0x8000_0000,
            1,
            0x8000_0001,
            0x007F_FFFF,
            0x807F_FFFF,
            0x0080_0000,
            0x8080_0000,
            0x7F7F_FFFF,
            0xFF7F_FFFF,
            1_000_000.0_f32.to_bits(),
            (-1_000_000.0_f32).to_bits(),
        ] {
            let stored = FiniteOptional::try_from_option(Some(f32::from_bits(bits))).unwrap();
            assert_eq!(stored.get().unwrap().to_bits(), bits);
            assert_ne!(stored, FiniteOptional::NONE);
        }
    }

    /// Both infinities and quiet/signalling NaNs of either sign fail instead of becoming absent.
    #[test]
    fn nonfinite_patterns_are_rejected() {
        for bits in [
            0x7F80_0000,
            0xFF80_0000,
            0x7FC0_0000,
            0xFFC0_0000,
            0x7F80_0001,
            0xFF80_0001,
            0x7FFF_FFFF,
            0xFFFF_FFFF,
        ] {
            assert!(FiniteOptional::try_from_option(Some(f32::from_bits(bits))).is_err());
        }
    }

    /// Samples every sign/exponent with deterministic mantissas, comparing exact recovery and Option equality.
    #[test]
    fn representative_patterns_match_optional_float_semantics() {
        let zero = FiniteOptional::try_from_option(Some(0.0)).unwrap();
        assert_eq!(zero, FiniteOptional::try_from_option(Some(-0.0)).unwrap());
        for sign_exponent in 0..512_u32 {
            for mantissa in [0, 1, 0x0001_2345, 0x0040_0000, 0x007F_FFFF] {
                let bits = (sign_exponent << 23) | mantissa;
                let value = f32::from_bits(bits);
                let stored = FiniteOptional::try_from_option(Some(value));
                if !value.is_finite() {
                    assert!(stored.is_err());
                    continue;
                }
                let stored = stored.unwrap();
                assert_eq!(stored.get().unwrap().to_bits(), bits);
                for other in [None, Some(0.0), Some(-0.0), Some(-value), Some(value), Some(1.0)] {
                    let packed_other = FiniteOptional::try_from_option(other).unwrap();
                    assert_eq!(stored == packed_other, Some(value) == other);
                }
            }
        }
    }
}
