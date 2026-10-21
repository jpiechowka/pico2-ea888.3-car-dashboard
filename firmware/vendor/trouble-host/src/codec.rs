//! Opinionated BLE codec
//!
//! Assumes little endian for all types

/// Values whose encoded width is independent of their contents.
pub trait FixedSize: Sized {
    /// Exact encoded byte width, used to reserve cursor space.
    const SIZE: usize;
}

/// Encoded size contract used to advance read/write cursors.
pub trait Type: Sized {
    /// Returns encoded byte length; must match bytes emitted/consumed by codec implementations.
    fn size(&self) -> usize;
}

/// Encodes one value into an exactly sized destination supplied by the write cursor.
pub trait Encode: Type {
    /// Encodes into `dest` and returns success or codec error. Callers must provide `size()`
    /// bytes; implementations may panic for other lengths and may partially write before error.
    fn encode(&self, dest: &mut [u8]) -> Result<(), Error>;
}

/// Decodes a prefix, potentially borrowing its storage for lifetime d.
pub trait Decode<'d>: Type {
    /// Returns the value at the start of `src`, or a codec error. Successful value size must
    /// fit within `src`; trailing bytes belong to subsequent values and need not be consumed.
    fn decode(src: &'d [u8]) -> Result<Self, Error>;
}

impl<T: FixedSize> Type for T {
    /// Returns the fixed encoded width declared by this type.
    fn size(&self) -> usize {
        Self::SIZE
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq)]
/// Buffer admission or malformed-value failure in the internal BLE codec.
pub enum Error {
    /// Required read/write bytes exceed the supplied buffer.
    InsufficientSpace,
    /// Input does not encode a supported value, including short primitive prefixes.
    InvalidValue,
}
