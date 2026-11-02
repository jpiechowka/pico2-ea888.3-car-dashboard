//! Primitive validity and length-prefix rules shared by command and event codecs.

use crate::{ByteAlignedValue, FixedSizeValue, FromHciBytes, FromHciBytesError, WriteHci};

unsafe impl FixedSizeValue for () {
    #[inline(always)]
    /// Returns true: unit has no representation bytes; `_data` is ignored under the exact-size trait contract.
    fn is_valid(_data: &[u8]) -> bool { true }
}

unsafe impl ByteAlignedValue for () {}

impl<'de> FromHciBytes<'de> for &'static () {
    #[inline(always)]
    /// Returns a static unit and all of `data` unchanged; consumes zero bytes and cannot fail.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> { Ok((&(), data)) }
}

unsafe impl FixedSizeValue for bool {
    #[inline(always)]
    /// Returns whether `data` contains a valid boolean byte (0 or 1); the trait supplies exactly one byte.
    fn is_valid(data: &[u8]) -> bool { !data.is_empty() && data[0] < 2 }
}

unsafe impl ByteAlignedValue for bool {}

impl<'de> FromHciBytes<'de> for &'de bool {
    #[inline(always)]
    /// Validates the first byte of `data`; returns a borrowed boolean and tail, or size/value error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <bool as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

impl WriteHci for &[u8] {
    #[inline(always)]
    /// Returns one count byte plus payload length; panics for more than 255 payload bytes.
    fn size(&self) -> usize { usize::from(crate::checked_hci_u8_len(self.len())) + 1 }

    #[inline(always)]
    /// Writes count and payload to `writer`; returns completion or I/O error after possible partial output.
    /// Panics above 255 payload bytes before this serializer writes anything.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let length = crate::checked_hci_u8_len(self.len());
        writer.write_all(&[length])?;
        writer.write_all(self)
    }

    #[inline(always)]
    /// Writes count and payload to `writer`; returns completion or I/O error; cancellation can leave partial output.
    /// Panics above 255 payload bytes before this serializer writes anything.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let length = crate::checked_hci_u8_len(self.len());
        writer.write_all(&[length]).await?;
        writer.write_all(self).await
    }
}

unsafe impl<T: FixedSizeValue, const N: usize> FixedSizeValue for [T; N] {
    /// Returns whether all `N` element representations in `data` are valid; nested arrays retain their element checks.
    /// Truncated input fails; trailing bytes are excluded from the exact-sized element callbacks.
    #[inline(always)]
    fn is_valid(data: &[u8]) -> bool { crate::validated_element_prefix::<T>(data, N).is_ok() }
}

unsafe impl<T: ByteAlignedValue, const N: usize> ByteAlignedValue for [T; N] {}

impl<'de, T: ByteAlignedValue, const N: usize> FromHciBytes<'de> for &'de [T; N] {
    #[inline(always)]
    /// Validates a fixed `N`-element prefix of `data`; returns borrowed array and tail or size/value error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <[T; N] as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

impl<T: WriteHci> WriteHci for Option<T> {
    #[inline(always)]
    /// Returns contained value's serialized size, or zero for `None`; there is no presence byte.
    fn size(&self) -> usize { self.as_ref().map(|x| x.size()).unwrap_or_default() }

    #[inline(always)]
    /// Writes the present value to `writer`; returns completion/error, or no-op success for `None`.
    fn write_hci<W: embedded_io::Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error> {
        match self {
            Some(val) => val.write_hci(writer),
            None => Ok(()),
        }
    }

    #[inline(always)]
    /// Writes the present value to `writer`; returns completion/error, or no-op success for `None`.
    /// Cancellation inherits the contained serializer's partial-output behavior.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error> {
        match self {
            Some(val) => val.write_hci_async(writer).await,
            None => Ok(()),
        }
    }
}

impl<'de, T: FromHciBytes<'de>> FromHciBytes<'de> for Option<T> {
    /// Returns `None` only for empty `data`; otherwise returns parsed `Some` and tail or propagates parse failure.
    /// No discriminator is consumed; callers must place this optional representation where omission is unambiguous.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        if data.is_empty() {
            Ok((None, data))
        } else {
            T::from_hci_bytes(data).map(|(x, y)| (Some(x), y))
        }
    }
}
