#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![no_std]

use core::future::Future;

use bt_hci_transport::PacketToHost;
use bt_hci_transport::blocking::TryError;
use embedded_io::ReadExactError;

mod fmt;

pub mod cmd;
pub mod controller;
pub mod data;
pub mod event;
pub mod param;
pub mod transport;
pub use bt_hci_transport::{PacketKind, ReadHciError};
pub use btuuid as uuid;

#[cfg(test)]
mod param_regression_tests;

#[cfg(test)]
mod packet_regression_tests;

/// Returns byte-sized HCI length/count `len` without truncation.
/// Panics above 255; serializers call this before output because their generic I/O
/// error type cannot represent caller-invalid packet construction.
pub(crate) fn checked_hci_u8_len(len: usize) -> u8 { u8::try_from(len).expect("HCI length/count exceeds 255") }

/// Errors from parsing HCI data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FromHciBytesError {
    /// Size of input did not match valid size.
    InvalidSize,
    /// Value of input did not match valid values.
    InvalidValue,
}

impl<E: From<FromHciBytesError>> From<FromHciBytesError> for TryError<E> {
    /// Returns terminal `Error` carrying converted parse failure `value`, never `Busy`.
    fn from(value: FromHciBytesError) -> Self { TryError::Error(E::from(value)) }
}

impl<E: embedded_io::Error> From<FromHciBytesError> for ReadHciError<E> {
    /// Converts `value`; returns unexpected EOF for size errors and invalid-value for rejected bit patterns.
    fn from(value: FromHciBytesError) -> Self {
        match value {
            FromHciBytesError::InvalidSize => ReadHciError::Read(ReadExactError::UnexpectedEof),
            FromHciBytesError::InvalidValue => ReadHciError::InvalidValue,
        }
    }
}

/// A HCI type which can be represented as bytes.
pub trait AsHciBytes {
    /// Returns a borrowed wire representation; the slice cannot outlive the value.
    fn as_hci_bytes(&self) -> &[u8];
}

/// An owned or borrowing HCI value decoded from a bounded input slice.
pub trait FromHciBytes<'de>: Sized {
    /// Decodes a prefix of `data`; returns the value and unused tail, or invalid-size/value error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError>;

    /// Decodes `data`; returns the value only if all bytes were consumed, otherwise `InvalidSize`.
    /// Propagates field validation failures; does not silently accept appended records.
    fn from_hci_bytes_complete(data: &'de [u8]) -> Result<Self, FromHciBytesError> {
        let (val, buf) = Self::from_hci_bytes(data)?;
        if buf.is_empty() {
            Ok(val)
        } else {
            Err(FromHciBytesError::InvalidSize)
        }
    }
}

impl<'de> FromHciBytes<'de> for PacketKind {
    /// Reads the first H4 indicator from `data`; returns kind and tail, rejecting empty/unknown indicators.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        if data.is_empty() {
            Err(FromHciBytesError::InvalidSize)
        } else {
            let (data, rest) = data.split_at(1);
            match data[0] {
                1 => Ok((PacketKind::Cmd, rest)),
                2 => Ok((PacketKind::AclData, rest)),
                3 => Ok((PacketKind::SyncData, rest)),
                4 => Ok((PacketKind::Event, rest)),
                5 => Ok((PacketKind::IsoData, rest)),
                _ => Err(FromHciBytesError::InvalidValue),
            }
        }
    }
}

/// Adapter trait for deserializing HCI types from embedded-io implementations.
pub trait ReadHci<'de>: FromHciBytes<'de> {
    /// Maximum serialized bytes read by this type, including its header and any H4 indicator
    /// at the packet-wrapper level. This is a protocol bound, not a required embedded allocation;
    /// readers may use smaller negotiated buffers and return [`ReadHciError::BufferTooSmall`].
    const MAX_LEN: usize;

    /// Reads from `reader`, borrowing variable payload bytes from `buf` for the returned value.
    /// Errors may consume a header or partial payload; `BufferTooSmall` does not rewind the reader.
    /// A streaming caller must restore framing before retrying, rather than parse the remaining body as a header.
    fn read_hci<R: embedded_io::Read>(
        reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>>;

    /// Returns a future reading from `reader` into borrowing storage `buf`, with the same result as [`Self::read_hci`].
    /// Cancellation can likewise leave a partial frame consumed.
    fn read_hci_async<R: embedded_io_async::Read>(
        reader: R,
        buf: &'de mut [u8],
    ) -> impl Future<Output = Result<Self, ReadHciError<R::Error>>>;
}

/// Adapter trait for serializing HCI types to embedded-io implementations.
///
/// Length-prefixed parameters and commands panic for locally constructed lengths/counts outside
/// their wire fields; generic `W::Error` cannot represent a parameter error. Their `size()` performs
/// the same admission. Validation precedes that value's own output, but an enclosing transport may
/// already have emitted a prefix. I/O errors and async cancellation may leave partial output.
pub trait WriteHci {
    /// Returns serialized byte count; invalid local length/count fields may panic as described above.
    fn size(&self) -> usize;

    /// Writes to `writer`; returns completion or its I/O error, potentially after partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error>;

    /// Returns a future writing to `writer`; completion means all bytes accepted, not transport delivery.
    /// I/O error or cancellation may leave partial output.
    fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        writer: W,
    ) -> impl Future<Output = Result<(), W::Error>>;
}

/// Marker trait for HCI values that have a known, fixed size
///
/// # Safety
/// - Must not contain any padding (uninitialized) bytes (recursively)
/// - structs must be `#[repr(C)]` or `#[repr(transparent)]`
/// - enums must be `#[repr(<int>)]`
/// - Must not contain any references, pointers, atomics, or interior mutability
/// - `is_valid()` must return true only if `data` is a valid bit representation of `Self`
pub unsafe trait FixedSizeValue: Copy {
    /// Returns whether exact-sized `data` is a valid Rust representation of `Self`.
    ///
    /// May panic if `data.len() != core::mem::size_of::<Self>()`
    fn is_valid(data: &[u8]) -> bool;
}

/// Marker trait for [`FixedSizeValue`]s that have byte alignment.
///
/// # Safety
/// - Must have `core::mem::align_of::<T>() == 1`
pub unsafe trait ByteAlignedValue: FixedSizeValue {
    /// Validates a fixed-sized prefix of `data`; returns a borrowed reference and unused tail.
    /// Returns `InvalidSize` for truncation or `InvalidValue` for invalid bit patterns.
    ///
    /// # Safety
    /// - Must have `core::mem::align_of::<T>() == 1`
    fn ref_from_hci_bytes(data: &[u8]) -> Result<(&Self, &[u8]), FromHciBytesError> {
        if data.len() < core::mem::size_of::<Self>() {
            return Err(FromHciBytesError::InvalidSize);
        }
        let (data, rest) = data.split_at(core::mem::size_of::<Self>());
        if !Self::is_valid(data) {
            return Err(FromHciBytesError::InvalidValue);
        }
        Ok((unsafe { &*(data.as_ptr() as *const Self) }, rest))
    }
}

impl<T: FixedSizeValue> AsHciBytes for T {
    /// Returns the padding-free in-memory bytes promised by `FixedSizeValue`; no allocation or endian conversion.
    fn as_hci_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, core::mem::size_of::<Self>()) }
    }
}

impl<'de, T: FixedSizeValue> FromHciBytes<'de> for T {
    /// Validates and copies a fixed-sized prefix of `data`; returns value and tail or size/value error.
    /// Unaligned input is supported; the unsafe trait implementation must validate every Rust bit-pattern restriction.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        if data.len() < core::mem::size_of::<Self>() {
            return Err(FromHciBytesError::InvalidSize);
        }
        let (data, rest) = data.split_at(core::mem::size_of::<Self>());
        if !Self::is_valid(data) {
            return Err(FromHciBytesError::InvalidValue);
        }
        Ok((unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Self) }, rest))
    }
}

/// Splits `count` contiguous `T` representations from `data`, validating each exact-sized element.
/// Returns the validated prefix and untouched tail; overflow/truncation is InvalidSize and invalid
/// element bits are InvalidValue. Empty arrays need no element validity; nonempty zero-sized arrays
/// validate the empty representation once, without zero-sized chunk iteration. No typed value is made.
fn validated_element_prefix<T: FixedSizeValue>(
    data: &[u8],
    count: usize,
) -> Result<(&[u8], &[u8]), FromHciBytesError> {
    let size = core::mem::size_of::<T>();
    let byte_len = count.checked_mul(size).ok_or(FromHciBytesError::InvalidSize)?;
    let (prefix, rest) = data.split_at_checked(byte_len).ok_or(FromHciBytesError::InvalidSize)?;
    let valid = if count == 0 {
        true
    } else if size == 0 {
        T::is_valid(&[])
    } else {
        prefix.chunks_exact(size).all(T::is_valid)
    };
    if !valid {
        return Err(FromHciBytesError::InvalidValue);
    }
    Ok((prefix, rest))
}

impl<'de, T: ByteAlignedValue> FromHciBytes<'de> for &'de [T] {
    /// Decodes a one-byte element count and validated records from `data`; returns borrowed array and tail.
    /// Rejects truncation or invalid elements before constructing typed references; zero-sized elements are supported.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let Some((len, data)) = data.split_first() else {
            return Err(FromHciBytesError::InvalidSize);
        };

        let len = usize::from(*len);
        let (data, rest) = validated_element_prefix::<T>(data, len)?;

        Ok((
            unsafe { core::slice::from_raw_parts(data.as_ptr() as *const T, len) },
            rest,
        ))
    }
}

impl<'de, T: FixedSizeValue> ReadHci<'de> for T {
    /// Exact wire size in bytes, without an outer transport indicator.
    const MAX_LEN: usize = core::mem::size_of::<Self>();

    /// Reads from `reader` through scratch `buf`; returns the copied value or I/O/validation error.
    /// An undersized buffer fails before reading; an I/O failure may consume a partial value.
    fn read_hci<R: embedded_io::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        if buf.len() < core::mem::size_of::<Self>() {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(core::mem::size_of::<Self>());
            reader.read_exact(buf)?;
            Self::from_hci_bytes(buf).map(|(x, _)| x).map_err(Into::into)
        }
    }

    /// Reads from `reader` through scratch `buf`; returns the copied value or I/O/validation error.
    /// An undersized buffer fails before reading; cancellation may consume a partial value.
    async fn read_hci_async<R: embedded_io_async::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        if buf.len() < core::mem::size_of::<Self>() {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(core::mem::size_of::<Self>());
            reader.read_exact(buf).await?;
            Self::from_hci_bytes(buf).map(|(x, _)| x).map_err(Into::into)
        }
    }
}

impl<T: FixedSizeValue> WriteHci for T {
    #[inline(always)]
    /// Returns the exact padding-free representation size in bytes.
    fn size(&self) -> usize { core::mem::size_of::<Self>() }

    /// Writes representation bytes to `writer`; returns completion or an I/O error after possible partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        writer.write_all(self.as_hci_bytes())
    }

    /// Writes representation bytes to `writer`; returns completion or I/O error; cancellation may leave partial output.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        writer.write_all(self.as_hci_bytes()).await
    }
}

/// Type representing valid deserialized HCI packets.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ControllerToHostPacket<'a> {
    /// ACL packet.
    Acl(
        /// Borrowed ACL header/payload view.
        data::AclPacket<'a>,
    ),
    /// Sync packet.
    Sync(
        /// Borrowed synchronous header/payload view.
        data::SyncPacket<'a>,
    ),
    /// Event packet.
    Event(
        /// Borrowed event payload with its raw event code.
        event::EventPacket<'a>,
    ),
    /// Isochronous packet.
    Iso(
        /// Borrowed ISO fragment and conditional metadata.
        data::IsoPacket<'a>,
    ),
}

impl<'a> ControllerToHostPacket<'a> {
    /// Returns the H4 discriminator for this decoded variant.
    pub fn kind(&self) -> PacketKind {
        match self {
            Self::Acl(_) => PacketKind::AclData,
            Self::Sync(_) => PacketKind::SyncData,
            Self::Event(_) => PacketKind::Event,
            Self::Iso(_) => PacketKind::IsoData,
        }
    }

    /// Decodes `data` without an H4 prefix using `kind`; returns packet and unused tail.
    /// Rejects host-to-controller commands and propagates packet size/value errors.
    pub fn from_hci_bytes_with_kind(
        kind: PacketKind,
        data: &'a [u8],
    ) -> Result<(ControllerToHostPacket<'a>, &'a [u8]), FromHciBytesError> {
        match kind {
            PacketKind::Cmd => Err(FromHciBytesError::InvalidValue),
            PacketKind::AclData => data::AclPacket::from_hci_bytes(data).map(|(x, y)| (Self::Acl(x), y)),
            PacketKind::SyncData => data::SyncPacket::from_hci_bytes(data).map(|(x, y)| (Self::Sync(x), y)),
            PacketKind::Event => event::EventPacket::from_hci_bytes(data).map(|(x, y)| (Self::Event(x), y)),
            PacketKind::IsoData => data::IsoPacket::from_hci_bytes(data).map(|(x, y)| (Self::Iso(x), y)),
        }
    }
}

impl<'de> FromHciBytes<'de> for ControllerToHostPacket<'de> {
    /// Decodes H4 kind and packet from `data`; returns packet and tail, rejecting commands, unknown kinds or invalid
    /// packets.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (kind, data) = PacketKind::from_hci_bytes(data)?;
        match kind {
            PacketKind::Cmd => Err(FromHciBytesError::InvalidValue),
            PacketKind::AclData => data::AclPacket::from_hci_bytes(data).map(|(x, y)| (Self::Acl(x), y)),
            PacketKind::SyncData => data::SyncPacket::from_hci_bytes(data).map(|(x, y)| (Self::Sync(x), y)),
            PacketKind::Event => event::EventPacket::from_hci_bytes(data).map(|(x, y)| (Self::Event(x), y)),
            PacketKind::IsoData => data::IsoPacket::from_hci_bytes(data).map(|(x, y)| (Self::Iso(x), y)),
        }
    }
}

impl<'de> ReadHci<'de> for ControllerToHostPacket<'de> {
    /// Largest supported protocol packet including its H4 indicator; not a requested allocation.
    const MAX_LEN: usize = 1 + <data::AclPacket<'de> as ReadHci<'de>>::MAX_LEN;

    /// Reads H4 kind and packet from `reader` using `buf`; returns a borrowing packet or I/O/size/value error.
    /// Errors may consume kind/header bytes; framing recovery belongs to the caller.
    fn read_hci<R: embedded_io::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut kind = [0];
        reader.read_exact(&mut kind)?;
        match PacketKind::from_hci_bytes(&kind)?.0 {
            PacketKind::Cmd => Err(ReadHciError::InvalidValue),
            PacketKind::AclData => data::AclPacket::read_hci(reader, buf).map(Self::Acl),
            PacketKind::SyncData => data::SyncPacket::read_hci(reader, buf).map(Self::Sync),
            PacketKind::Event => event::EventPacket::read_hci(reader, buf).map(Self::Event),
            PacketKind::IsoData => data::IsoPacket::read_hci(reader, buf).map(Self::Iso),
        }
    }

    /// Reads H4 kind and packet from `reader` using `buf`; returns a borrowing packet or I/O/size/value error.
    /// Cancellation may consume a partial packet; framing recovery belongs to the caller.
    async fn read_hci_async<R: embedded_io_async::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut kind = [0u8];
        reader.read_exact(&mut kind).await?;
        match PacketKind::from_hci_bytes(&kind)?.0 {
            PacketKind::Cmd => Err(ReadHciError::InvalidValue),
            PacketKind::AclData => data::AclPacket::read_hci_async(reader, buf).await.map(Self::Acl),
            PacketKind::SyncData => data::SyncPacket::read_hci_async(reader, buf).await.map(Self::Sync),
            PacketKind::Event => event::EventPacket::read_hci_async(reader, buf).await.map(Self::Event),
            PacketKind::IsoData => data::IsoPacket::read_hci_async(reader, buf).await.map(Self::Iso),
        }
    }
}

impl<'de> PacketToHost<'de> for ControllerToHostPacket<'de> {
    /// Reads the already-selected `kind` from `reader` into `buf`; returns a borrowing packet or I/O/validation error.
    /// Commands are rejected before reading; other errors may leave partial input consumed.
    fn read_hci<R: embedded_io::Read>(
        kind: PacketKind,
        reader: &mut R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        match kind {
            PacketKind::Cmd => Err(ReadHciError::InvalidValue),
            PacketKind::AclData => data::AclPacket::read_hci(reader, buf).map(Self::Acl),
            PacketKind::SyncData => data::SyncPacket::read_hci(reader, buf).map(Self::Sync),
            PacketKind::Event => event::EventPacket::read_hci(reader, buf).map(Self::Event),
            PacketKind::IsoData => data::IsoPacket::read_hci(reader, buf).map(Self::Iso),
        }
    }

    /// Reads the already-selected `kind` from `reader` into `buf`; returns a borrowing packet or I/O/validation error.
    /// Commands are rejected before reading; cancellation/errors may leave partial input consumed.
    async fn read_hci_async<R: embedded_io_async::Read>(
        kind: PacketKind,
        reader: &mut R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        match kind {
            PacketKind::Cmd => Err(ReadHciError::InvalidValue),
            PacketKind::AclData => data::AclPacket::read_hci_async(reader, buf).await.map(Self::Acl),
            PacketKind::SyncData => data::SyncPacket::read_hci_async(reader, buf).await.map(Self::Sync),
            PacketKind::Event => event::EventPacket::read_hci_async(reader, buf).await.map(Self::Event),
            PacketKind::IsoData => data::IsoPacket::read_hci_async(reader, buf).await.map(Self::Iso),
        }
    }
}
