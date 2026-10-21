//! Little-endian primitive prefix decoding and exact-width cursor encoding.
use crate::codec::{Decode, Encode, Error, FixedSize};

//
// Implementations for primitives
//
impl FixedSize for u8 {
    /// Encoded width in bytes.
    const SIZE: usize = 1;
}

impl FixedSize for u16 {
    /// Encoded width in bytes.
    const SIZE: usize = 2;
}

impl FixedSize for u32 {
    /// Encoded width in bytes.
    const SIZE: usize = 4;
}

impl Decode<'_> for u8 {
    /// Returns the little-endian value in the first1 bytes of `src`, ignoring trailing bytes.
    /// Short input returns InvalidValue.
    fn decode(src: &[u8]) -> Result<Self, Error> {
        if src.is_empty() {
            return Err(Error::InvalidValue);
        }
        Ok(src[0])
    }
}

impl Decode<'_> for u16 {
    /// Returns the little-endian value in the first2 bytes of `src`, ignoring trailing bytes.
    /// Short input returns InvalidValue.
    fn decode(src: &[u8]) -> Result<Self, Error> {
        if src.len() < 2 {
            return Err(Error::InvalidValue);
        }
        Ok(u16::from_le_bytes([src[0], src[1]]))
    }
}

impl Decode<'_> for u32 {
    /// Returns the little-endian value in the first4 bytes of `src`, ignoring trailing bytes.
    /// Short input returns InvalidValue.
    fn decode(src: &[u8]) -> Result<Self, Error> {
        if src.len() < 4 {
            return Err(Error::InvalidValue);
        }
        Ok(u32::from_le_bytes([src[0], src[1], src[2], src[3]]))
    }
}

impl Encode for u8 {
    /// Writes the little-endian value into `dest` and returns success.
    /// Panics when `dest` is empty; extra destination bytes remain unchanged.
    fn encode(&self, dest: &mut [u8]) -> Result<(), Error> {
        dest[0] = *self;
        Ok(())
    }
}

impl Encode for u16 {
    /// Writes the little-endian value into `dest` and returns success.
    /// Panics unless `dest` is exactly2 bytes.
    fn encode(&self, dest: &mut [u8]) -> Result<(), Error> {
        dest.copy_from_slice(&self.to_le_bytes()[..]);
        Ok(())
    }
}

impl Encode for u32 {
    /// Writes the little-endian value into `dest` and returns success.
    /// Panics unless `dest` is exactly4 bytes.
    fn encode(&self, dest: &mut [u8]) -> Result<(), Error> {
        dest.copy_from_slice(&self.to_le_bytes()[..]);
        Ok(())
    }
}
