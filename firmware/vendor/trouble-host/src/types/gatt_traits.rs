//! Borrowed GATT value encodings and bounded owned decoding.
//! Primitive encodings use native memory byte order; supported RP2350 and host targets are little-endian.
use core::{mem, slice};

use bt_hci::uuid::BluetoothUuid16;
use heapless::{String, Vec};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Error type to signify an issue when converting from GATT bytes to a concrete type
pub enum FromGattError {
    /// Byte array's length did not match what was expected for the converted type
    InvalidLength,
    /// Attempt to encode as string failed due to an invalid character representation in the byte array
    InvalidCharacter,
}

/// Trait to allow conversion of a fixed size type to and from a byte slice
#[allow(private_bounds)]
pub trait FixedGattValue: FromGatt {
    /// Size of the type in bytes
    const SIZE: usize;
}

/// Trait to allow conversion of a type to gatt bytes
pub trait AsGatt {
    /// The minimum size the type might be
    const MIN_SIZE: usize;
    /// The maximum size the type might be
    const MAX_SIZE: usize;
    /// Returns a borrowed encoding with length in MIN_SIZE..=MAX_SIZE; no allocation is required.
    fn as_gatt(&self) -> &[u8];
}

/// Trait to allow conversion of gatt bytes into a type
///
/// Requires that the type implements AsGatt
pub trait FromGatt: AsGatt + Sized {
    /// Returns a value decoded from `data`; reject unsupported lengths with InvalidLength.
    /// String implementations additionally reject malformed UTF-8 with InvalidCharacter.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError>;
}

/// Generates exact-width native-endian codecs for Copy types with every bit pattern valid and no padding.
macro_rules! primitive {
    ($($ty:ty),*) => {
        $(
            impl FixedGattValue for $ty {
                /// Exact in-memory width in bytes.
                const SIZE: usize = mem::size_of::<Self>();
            }

            impl FromGatt for $ty {
                /// Returns an unaligned native-endian copy of `data`; rejects any non-exact length.
                fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
                    if data.len() != Self::SIZE {
                        Err(FromGattError::InvalidLength)
                    } else {
                        // SAFETY
                        // - Pointer is considered "valid" as per the rules outlined for validity in std::ptr v1.82.0
                        // - Pointer was generated from a slice of bytes matching the size of the type, and all primitives are valid for all possible configurations of bits
                        // - Primitives are copy
                        unsafe { Ok((data.as_ptr() as *const Self).read_unaligned()) }
                    }
                }
            }

            impl AsGatt for $ty {
                /// Native value width; shorter encodings are invalid.
                const MIN_SIZE: usize = mem::size_of::<Self>();
                /// Native value width; longer encodings are invalid.
                const MAX_SIZE: usize = mem::size_of::<Self>();

                /// Returns the initialized object bytes borrowed from self, without endian conversion.
                fn as_gatt(&self) -> &[u8] {
                    // SAFETY
                    // - Slice is of type u8 so data is guaranteed valid for reads of any length
                    // - Data and len are tied to the address and size of the type
                    unsafe { slice::from_raw_parts(self as *const Self as *const u8, Self::SIZE) }
                }
            }
        )*
    };
}

primitive!(
    u8,
    u16,
    u32,
    u64,
    u128,
    i8,
    i16,
    i32,
    i64,
    i128,
    f32,
    f64,
    BluetoothUuid16
);

impl FixedGattValue for () {
    /// Zero-byte value.
    const SIZE: usize = 0;
}

impl FromGatt for () {
    /// Returns unit only for empty `data`; otherwise InvalidLength.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        if data.len() != Self::SIZE {
            Err(FromGattError::InvalidLength)
        } else {
            Ok(())
        }
    }
}

impl AsGatt for () {
    /// Unit has no encoded bytes.
    const MIN_SIZE: usize = Self::SIZE;
    /// Unit has no encoded bytes.
    const MAX_SIZE: usize = Self::SIZE;

    /// Returns the empty slice.
    fn as_gatt(&self) -> &[u8] {
        &[]
    }
}

impl FixedGattValue for bool {
    /// Boolean values occupy one encoded byte.
    const SIZE: usize = 1;
}

impl FromGatt for bool {
    /// Returns false for one zero byte, true for any one nonzero byte in `data`; rejects other lengths.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        if data.len() != Self::SIZE {
            Err(FromGattError::InvalidLength)
        } else {
            Ok(data != [0x00])
        }
    }
}

impl AsGatt for bool {
    /// Both Boolean encodings occupy one byte.
    const MIN_SIZE: usize = Self::SIZE;
    /// Both Boolean encodings occupy one byte.
    const MAX_SIZE: usize = Self::SIZE;

    /// Returns the canonical one-byte encoding (zero or one).
    fn as_gatt(&self) -> &[u8] {
        match self {
            true => &[0x01],
            false => &[0x00],
        }
    }
}

impl<const N: usize> FromGatt for Vec<u8, N> {
    /// Returns an owned copy of `data`, or InvalidLength when its byte length exceeds N.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        Self::from_slice(data).map_err(|_| FromGattError::InvalidLength)
    }
}

impl<const N: usize> AsGatt for Vec<u8, N> {
    /// Empty values are admitted.
    const MIN_SIZE: usize = 0;
    /// Maximum capacity in bytes.
    const MAX_SIZE: usize = N;

    /// Returns the occupied byte prefix, excluding unused capacity.
    fn as_gatt(&self) -> &[u8] {
        self
    }
}

impl<const N: usize> FromGatt for [u8; N] {
    /// Returns `data` copied into a zero-filled N-byte array; rejects lengths greater than N.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        if data.len() <= Self::MAX_SIZE {
            let mut actual = [0; N];
            actual[..data.len()].copy_from_slice(data);
            Ok(actual)
        } else {
            data.try_into().map_err(|_| FromGattError::InvalidLength)
        }
    }
}

impl<const N: usize> AsGatt for [u8; N] {
    /// Empty values are admitted.
    const MIN_SIZE: usize = 0;
    /// Maximum capacity in bytes.
    const MAX_SIZE: usize = N;

    /// Returns all N bytes, including any zeros added during decoding.
    fn as_gatt(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<const N: usize> FromGatt for String<N> {
    /// Returns owned UTF-8 decoded from `data`; capacity overflow is InvalidLength, malformed text InvalidCharacter.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        String::from_utf8(Vec::from_slice(data).map_err(|_| FromGattError::InvalidLength)?)
            .map_err(|_| FromGattError::InvalidCharacter)
    }
}

impl<const N: usize> AsGatt for String<N> {
    /// Empty values are admitted.
    const MIN_SIZE: usize = 0;
    /// Maximum capacity in bytes.
    const MAX_SIZE: usize = N;

    /// Returns occupied UTF-8 bytes without a trailing NUL.
    fn as_gatt(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsGatt for str {
    /// Empty values are admitted.
    const MIN_SIZE: usize = 0;

    /// No additional compile-time bound beyond the slice/string length.
    const MAX_SIZE: usize = usize::MAX;

    /// Returns UTF-8 bytes without a trailing NUL.
    fn as_gatt(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsGatt for [u8] {
    /// Empty values are admitted.
    const MIN_SIZE: usize = 0;

    /// No additional compile-time bound beyond the slice/string length.
    const MAX_SIZE: usize = usize::MAX;

    /// Returns the original byte slice with no copy or length prefix.
    fn as_gatt(&self) -> &[u8] {
        self
    }
}

impl AsGatt for crate::types::uuid::Uuid {
    /// Smallest supported UUID width in bytes.
    const MIN_SIZE: usize = 2;
    /// Largest supported UUID width; intermediate lengths other than four remain invalid.
    const MAX_SIZE: usize = 16;

    /// Returns raw little-endian UUID bytes; 32-bit values are not expanded to 128 bits.
    fn as_gatt(&self) -> &[u8] {
        self.as_raw()
    }
}

impl FromGatt for crate::types::uuid::Uuid {
    /// Returns a UUID from little-endian `data` of exactly 2, 4 or 16 bytes; other lengths are InvalidLength.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        Self::try_from(data).map_err(|_| FromGattError::InvalidLength)
    }
}

impl<T: AsGatt + ?Sized> AsGatt for &T {
    /// Same lower encoded-size bound as the referent.
    const MIN_SIZE: usize = T::MIN_SIZE;

    /// Same upper encoded-size bound as the referent.
    const MAX_SIZE: usize = T::MAX_SIZE;

    /// Returns the borrowed referent encoding, preserving its implementation-specific representation.
    fn as_gatt(&self) -> &[u8] {
        <T as AsGatt>::as_gatt(*self)
    }
}

#[cfg(test)]
/// Regressions for UTF-8 validity and fixed-capacity String decoding.
mod bounded_string_tests {
    use super::*;

    /// Empty/exact multibyte strings succeed; overflow and malformed UTF-8 return distinct errors.
    #[test]
    fn capacity_and_utf8_errors_are_results() {
        assert_eq!(String::<0>::from_gatt(b"").unwrap().as_str(), "");
        assert_eq!(String::<4>::from_gatt("éé".as_bytes()).unwrap().as_str(), "éé");
        assert_eq!(String::<4>::from_gatt(b"abcde"), Err(FromGattError::InvalidLength));
        assert_eq!(String::<0>::from_gatt(b"a"), Err(FromGattError::InvalidLength));
        assert_eq!(String::<4>::from_gatt(&[0xff]), Err(FromGattError::InvalidCharacter));
    }
}
