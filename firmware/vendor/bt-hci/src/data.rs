//! HCI data packets [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-c8fdfc58-ec59-1a87-6e78-0a01de2e0846)

use core::future::Future;

use bt_hci_transport::PacketToController;
use embedded_io::Write;
use embedded_io_async::Write as AsyncWrite;

use crate::param::{ConnHandle, param};
use crate::{FromHciBytes, FromHciBytesError, PacketKind, ReadHci, ReadHciError, WriteHci};

/// HCI ACL Data packet `Packet_Boundary_Flag` [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-49cf6aaa-b2f3-30b0-e737-5b515d3b3168)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum AclPacketBoundary {
    /// First non-automatically-flushable packet of a higher layer message (start of a non-automatically-flushable
    /// L2CAP PDU) from Host to Controller.
    FirstNonFlushable = 0x00,
    /// Continuing fragment of a higher layer message
    Continuing = 0x01,
    /// First automatically flushable packet of a higher layer message (start of an automatically-flushable L2CAP PDU).
    FirstFlushable = 0x02,
    /// A complete L2CAP PDU. Automatically flushable.
    Complete = 0x03,
}

/// HCI ACL Data packet `Broadcast_Flag` [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-49cf6aaa-b2f3-30b0-e737-5b515d3b3168)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum AclBroadcastFlag {
    /// Point-to-point (ACL-U or LE-U)
    PointToPoint = 0x00,
    /// BR/EDR broadcast (APB-U)
    BrEdrBroadcast = 0x01,
    /// Reserved for future use.
    Reserved = 0x02,
}

param! {
    /// HCI ACL Data Packet header [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-49cf6aaa-b2f3-30b0-e737-5b515d3b3168)
    struct AclPacketHeader {
        handle: u16,
        data_len: u16,
    }
}

impl AclPacketHeader {
    /// Returns the low 12-bit connection handle, excluding boundary and broadcast flags.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the two-bit packet-boundary classification; all encodings are represented.
    pub fn boundary_flag(&self) -> AclPacketBoundary {
        match (self.handle >> 12) & 0x03 {
            0 => AclPacketBoundary::FirstNonFlushable,
            1 => AclPacketBoundary::Continuing,
            2 => AclPacketBoundary::FirstFlushable,
            3 => AclPacketBoundary::Complete,
            _ => unreachable!(),
        }
    }

    /// Returns the broadcast classification; encodings 2 and 3 map to `Reserved`.
    pub fn broadcast_flag(&self) -> AclBroadcastFlag {
        match (self.handle >> 14) & 0x03 {
            0 => AclBroadcastFlag::PointToPoint,
            1 => AclBroadcastFlag::BrEdrBroadcast,
            2 => AclBroadcastFlag::Reserved,
            3 => AclBroadcastFlag::Reserved,
            _ => unreachable!(),
        }
    }

    /// Returns payload length in bytes, 0..=65,535, excluding this four-byte header.
    pub fn data_len(&self) -> usize { usize::from(self.data_len) }
}

/// HCI ACL Data Packet [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-49cf6aaa-b2f3-30b0-e737-5b515d3b3168)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AclPacket<'a> {
    /// Packed connection handle, boundary bits and broadcast bits, preserved from the wire.
    handle: u16,
    /// Borrowed payload, limited to the 16-bit header length by every constructor.
    data: &'a [u8],
}

impl<'a> AclPacket<'a> {
    /// Borrows `data` for `handle`, with packet-boundary `pbf` and broadcast `bf` flags.
    /// Returns a packet without copying or transmitting; callers still enforce negotiated limits.
    ///
    /// # Panics
    /// Panics if `data` exceeds the 16-bit length field (65,535 bytes), before any output.
    pub fn new(
        handle: ConnHandle,
        pbf: AclPacketBoundary,
        bf: AclBroadcastFlag,
        data: &'a [u8],
    ) -> Self {
        assert!(
            data.len() <= usize::from(u16::MAX),
            "HCI ACL payload exceeds 65535 bytes"
        );
        let handle: u16 = handle.into_inner() | ((pbf as u16) << 12) | ((bf as u16) << 14);
        Self { handle, data }
    }

    /// Splits `data` using the byte count in `header`; returns the borrowed packet and unused tail.
    /// Returns `InvalidSize` for a truncated payload; does not enforce negotiated controller limits.
    pub fn from_header_hci_bytes(
        header: AclPacketHeader,
        data: &'a [u8],
    ) -> Result<(Self, &'a [u8]), FromHciBytesError> {
        let data_len = usize::from(header.data_len);
        if data.len() < data_len {
            Err(FromHciBytesError::InvalidSize)
        } else {
            let (data, rest) = data.split_at(data_len);
            Ok((
                Self {
                    handle: header.handle,
                    data: &data[..data_len],
                },
                rest,
            ))
        }
    }

    /// Returns the low 12-bit connection handle, excluding boundary and broadcast flags.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the packed handle/flags and the exact admitted payload byte length.
    pub fn header(&self) -> AclPacketHeader {
        AclPacketHeader {
            handle: self.handle,
            data_len: self.data.len() as u16,
        }
    }

    /// Returns the two-bit packet-boundary classification; all encodings are represented.
    pub fn boundary_flag(&self) -> AclPacketBoundary {
        match (self.handle >> 12) & 0x03 {
            0 => AclPacketBoundary::FirstNonFlushable,
            1 => AclPacketBoundary::Continuing,
            2 => AclPacketBoundary::FirstFlushable,
            3 => AclPacketBoundary::Complete,
            _ => unreachable!(),
        }
    }

    /// Returns the broadcast classification; encodings 2 and 3 map to `Reserved`.
    pub fn broadcast_flag(&self) -> AclBroadcastFlag {
        match (self.handle >> 14) & 0x03 {
            0 => AclBroadcastFlag::PointToPoint,
            1 => AclBroadcastFlag::BrEdrBroadcast,
            2 => AclBroadcastFlag::Reserved,
            3 => AclBroadcastFlag::Reserved,
            _ => unreachable!(),
        }
    }

    /// Returns the borrowed payload without the four-byte ACL header.
    pub fn data(&self) -> &'a [u8] { self.data }
}

impl<'de> FromHciBytes<'de> for AclPacket<'de> {
    /// Decodes one header and length-delimited payload from `data`; returns packet and unused tail, or truncation/field
    /// error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (header, data) = AclPacketHeader::from_hci_bytes(data)?;
        Self::from_header_hci_bytes(header, data)
    }
}

impl<'de> ReadHci<'de> for AclPacket<'de> {
    /// Maximum packet bytes including its header, excluding the H4 indicator; no buffer is allocated here.
    const MAX_LEN: usize = 4 + u16::MAX as usize;

    /// Reads one packet from `reader` into payload storage `buf`; returns a borrowing packet or I/O/size error.
    /// The header is consumed before buffer admission; errors can leave the transport between packet boundaries.
    fn read_hci<R: embedded_io::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 4];
        reader.read_exact(&mut header)?;
        let (header, _) = AclPacketHeader::from_hci_bytes(&header)?;
        let data_len = header.data_len();
        if buf.len() < data_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_len);
            reader.read_exact(buf)?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }

    /// Reads one packet from `reader` into `buf`; returns a borrowing packet or I/O/size error.
    /// Cancellation or failure can consume only part of a packet; restore framing before the next read.
    async fn read_hci_async<R: embedded_io_async::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 4];
        reader.read_exact(&mut header).await?;
        let (header, _) = AclPacketHeader::from_hci_bytes(&header)?;
        let data_len = header.data_len();
        if buf.len() < data_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_len);
            reader.read_exact(buf).await?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }
}

impl WriteHci for AclPacket<'_> {
    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { 4 + self.data.len() }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = AclPacketHeader {
            handle: self.handle,
            data_len: self.data.len() as u16,
        };
        header.write_hci(&mut writer)?;
        writer.write_all(self.data)
    }

    #[inline(always)]
    /// Writes to `writer`; returns completion or its I/O error. Cancellation/error can leave partial output.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = AclPacketHeader {
            handle: self.handle,
            data_len: self.data.len() as u16,
        };
        header.write_hci_async(&mut writer).await?;
        writer.write_all(self.data).await
    }
}

impl PacketToController for AclPacket<'_> {
    /// H4 discriminator emitted separately by the transport, not by this packet serializer.
    const KIND: PacketKind = PacketKind::AclData;

    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { <Self as WriteHci>::size(self) }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error> {
        <Self as WriteHci>::write_hci(self, writer)
    }

    #[inline(always)]
    /// Returns a future writing to `writer`; cancellation/error can leave partial output; success means all bytes
    /// accepted.
    fn write_hci_async<W: AsyncWrite>(
        &self,
        writer: W,
    ) -> impl Future<Output = Result<(), W::Error>> {
        <Self as WriteHci>::write_hci_async(self, writer)
    }
}

/// HCI Synchronous Data packet `Packet_Status_Flag` [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-ea780739-1980-92c8-1a0a-96fcf9bec7b7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SyncPacketStatus {
    /// Correctly received data. The payload data belongs to received eSCO or SCO packets that the Baseband marked as
    /// “good data”.
    Correct,
    /// Possibly invalid data. At least one eSCO packet has been marked by the Baseband as “data with possible errors”
    /// and all others have been marked as “good data” in the eSCO interval(s) corresponding to the HCI Synchronous
    /// Data packet.
    PossiblyInvalid,
    /// No data received. All data from the Baseband received during the (e)SCO interval(s) corresponding to the HCI
    /// Synchronous Data packet have been marked as "lost data" by the Baseband. The Payload data octets shall be set
    /// to 0.
    NoData,
    /// Data partially lost. Not all, but at least one (e)SCO packet has been marked as “lost data” by the Baseband in
    /// the (e)SCO intervals corresponding to the HCI Synchronous Data packet. The payload data octets corresponding to
    /// the missing (e)SCO packets shall be set to 0.
    PartiallyLost,
}

param! {
    /// HCI Synchronous Data packet header [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-ea780739-1980-92c8-1a0a-96fcf9bec7b7)
    struct SyncPacketHeader {
        handle: u16,
        data_len: u8,
    }
}

impl SyncPacketHeader {
    /// Returns the low 12-bit synchronous connection handle, excluding status bits.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the two-bit receive status; this does not validate the payload contents.
    pub fn status(&self) -> SyncPacketStatus {
        match (self.handle >> 12) & 0x03 {
            0 => SyncPacketStatus::Correct,
            1 => SyncPacketStatus::PossiblyInvalid,
            2 => SyncPacketStatus::NoData,
            3 => SyncPacketStatus::PartiallyLost,
            _ => unreachable!(),
        }
    }

    /// Returns payload length in bytes, 0..=255, excluding this three-byte header.
    pub fn data_len(&self) -> usize { usize::from(self.data_len) }
}

/// HCI Synchronous Data packet [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-ea780739-1980-92c8-1a0a-96fcf9bec7b7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SyncPacket<'a> {
    /// Packed connection handle and receive-status bits copied from the wire.
    handle: u16,
    /// Borrowed payload, limited to 255 bytes by the decoded header.
    data: &'a [u8],
}

impl<'a> SyncPacket<'a> {
    /// Splits `data` using the byte count in `header`; returns the borrowed packet and unused tail.
    /// Returns `InvalidSize` when the advertised payload is truncated.
    pub fn from_header_hci_bytes(
        header: SyncPacketHeader,
        data: &'a [u8],
    ) -> Result<(Self, &'a [u8]), FromHciBytesError> {
        let data_len = usize::from(header.data_len);
        if data.len() < data_len {
            Err(FromHciBytesError::InvalidSize)
        } else {
            let (data, rest) = data.split_at(data_len);
            Ok((
                Self {
                    handle: header.handle,
                    data: &data[..data_len],
                },
                rest,
            ))
        }
    }

    /// Returns the low 12-bit synchronous connection handle, excluding status bits.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the two-bit receive status; this does not validate the payload contents.
    pub fn status(&self) -> SyncPacketStatus {
        match (self.handle >> 12) & 0x03 {
            0 => SyncPacketStatus::Correct,
            1 => SyncPacketStatus::PossiblyInvalid,
            2 => SyncPacketStatus::NoData,
            3 => SyncPacketStatus::PartiallyLost,
            _ => unreachable!(),
        }
    }

    /// Returns the borrowed synchronous payload without its three-byte header.
    pub fn data(&self) -> &[u8] { self.data }
}

impl<'de> FromHciBytes<'de> for SyncPacket<'de> {
    /// Decodes one header and length-delimited payload from `data`; returns packet and unused tail, or truncation/field
    /// error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (header, data) = SyncPacketHeader::from_hci_bytes(data)?;
        Self::from_header_hci_bytes(header, data)
    }
}

impl<'de> ReadHci<'de> for SyncPacket<'de> {
    /// Maximum packet bytes including its header, excluding the H4 indicator; no buffer is allocated here.
    const MAX_LEN: usize = 258;

    /// Reads one packet from `reader` into payload storage `buf`; returns a borrowing packet or I/O/size error.
    /// The header is consumed before buffer admission; errors can leave the transport between packet boundaries.
    fn read_hci<R: embedded_io::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 3];
        reader.read_exact(&mut header)?;
        let (header, _) = SyncPacketHeader::from_hci_bytes(&header)?;
        let data_len = header.data_len();
        if buf.len() < data_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_len);
            reader.read_exact(buf)?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }

    /// Reads one packet from `reader` into `buf`; returns a borrowing packet or I/O/size error.
    /// Cancellation or failure can consume only part of a packet; restore framing before the next read.
    async fn read_hci_async<R: embedded_io_async::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 3];
        reader.read_exact(&mut header).await?;
        let (header, _) = SyncPacketHeader::from_hci_bytes(&header)?;
        let data_len = header.data_len();
        if buf.len() < data_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_len);
            reader.read_exact(buf).await?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }
}

impl WriteHci for SyncPacket<'_> {
    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { 3 + self.data.len() }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = SyncPacketHeader {
            handle: self.handle,
            data_len: self.data.len() as u8,
        };
        header.write_hci(&mut writer)?;
        writer.write_all(self.data)
    }

    #[inline(always)]
    /// Writes to `writer`; returns completion or its I/O error. Cancellation/error can leave partial output.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = SyncPacketHeader {
            handle: self.handle,
            data_len: self.data.len() as u8,
        };
        header.write_hci_async(&mut writer).await?;
        writer.write_all(self.data).await
    }
}

impl PacketToController for SyncPacket<'_> {
    /// H4 discriminator emitted separately by the transport, not by this packet serializer.
    const KIND: PacketKind = PacketKind::SyncData;

    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { <Self as WriteHci>::size(self) }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error> {
        <Self as WriteHci>::write_hci(self, writer)
    }

    #[inline(always)]
    /// Returns a future writing to `writer`; cancellation/error can leave partial output; success means all bytes
    /// accepted.
    fn write_hci_async<W: AsyncWrite>(
        &self,
        writer: W,
    ) -> impl Future<Output = Result<(), W::Error>> {
        <Self as WriteHci>::write_hci_async(self, writer)
    }
}

/// HCI ISO Data packet `PB_Flag` [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-9b5fb085-278b-5084-ac33-bee2839abe6b)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IsoPacketBoundary {
    /// The `ISO_Data_Load` field contains a header and the first fragment of a fragmented SDU.
    FirstFragment,
    /// The `ISO_Data_Load` field contains a continuation fragment of an SDU.
    ContinuationFragment,
    /// The `ISO_Data_Load` field contains a header and a complete SDU.
    Complete,
    /// The `ISO_Data_Load` field contains the last fragment of an SDU.
    LastFragment,
}

/// HCI ISO Data packet `Packet_Status_Flag` [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-9b5fb085-278b-5084-ac33-bee2839abe6b)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IsoPacketStatus {
    /// Valid data. The complete SDU was received correctly.
    Correct,
    /// Possibly invalid data. The contents of the ISO_SDU_Fragment may contain errors or part of the SDU may be
    /// missing. This is reported as "data with possible errors".
    PossiblyInvalid,
    /// Part(s) of the SDU were not received correctly. This is reported as "lost data".
    PartiallyLost,
}

param! {
    /// HCI ISO Data packet header [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-9b5fb085-278b-5084-ac33-bee2839abe6b)
    struct IsoPacketHeader {
        handle: u16,
        data_load_len: u16,
    }
}

impl IsoPacketHeader {
    /// Returns the low 12-bit ISO logical-channel handle, excluding boundary/timestamp flags.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the ISO fragment classification from the two boundary bits.
    pub fn boundary_flag(&self) -> IsoPacketBoundary {
        match (self.handle >> 12) & 0x03 {
            0 => IsoPacketBoundary::FirstFragment,
            1 => IsoPacketBoundary::ContinuationFragment,
            2 => IsoPacketBoundary::Complete,
            3 => IsoPacketBoundary::LastFragment,
            _ => unreachable!(),
        }
    }

    /// Returns whether the timestamp flag is set; continuation packets have no parsed load header.
    pub fn has_timestamp(&self) -> bool { ((self.handle >> 14) & 1) != 0 }

    /// Returns the 14-bit `ISO_Data_Load` length in octets (0..=16,383); ignores two reserved high bits.
    ///
    ///  In the Host to Controller direction, `ISO_Data_Load_Length` shall be less than or equal to the size of the
    /// buffer supported by the Controller (which is returned using the `ISO_Data_Packet_Length` return parameter of
    /// the LE Read Buffer Size command).
    pub fn data_load_len(&self) -> usize { usize::from(self.data_load_len & 0x3FFF) }
}

/// HCI ISO Data conditional header values [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-9b5fb085-278b-5084-ac33-bee2839abe6b)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct IsoDataLoadHeader {
    /// A time in microseconds.
    ///
    /// See Bluetooth Core Specification Vol 6, Part G, §3
    pub timestamp: Option<u32>,
    /// The sequence number of the SDU.
    ///
    /// See Bluetooth Core Specification Vol 6, Part G, §1
    pub sequence_num: u16,
    /// Raw packed word: SDU byte length in bits0..11, reserved bits12..13, packet status in bits14..15.
    /// Kept verbatim for roundtrip compatibility; use [`Self::sdu_len`] instead of treating it as a length.
    pub iso_sdu_len: u16,
}

impl IsoDataLoadHeader {
    /// Returns total SDU length (not fragment length) in octets,0..=4095, excluding status/reserved bits.
    pub fn sdu_len(&self) -> u16 { self.iso_sdu_len & 0x0FFF }

    /// Returns the controller-reported status; reserved encoding3 is `None`, never successful data.
    /// Host-to-controller writers must supply zero status; the raw field is preserved, not normalized.
    pub fn packet_status(&self) -> Option<IsoPacketStatus> {
        match self.iso_sdu_len >> 14 {
            0 => Some(IsoPacketStatus::Correct),
            1 => Some(IsoPacketStatus::PossiblyInvalid),
            2 => Some(IsoPacketStatus::PartiallyLost),
            _ => None,
        }
    }

    /// Create an `IsoDataLoadHeader` from `data`.
    ///
    /// `timestamp` indicates whether the data load header includes a timestamp field.
    /// Returns the decoded raw header and unused tail; a truncated field returns `InvalidSize`.
    pub fn from_hci_bytes(
        timestamp: bool,
        data: &[u8],
    ) -> Result<(Self, &[u8]), FromHciBytesError> {
        let (timestamp, data) = if timestamp {
            u32::from_hci_bytes(data).map(|(x, y)| (Some(x), y))?
        } else {
            (None, data)
        };

        let (sequence_num, data) = u16::from_hci_bytes(data)?;
        let (iso_sdu_len, data) = u16::from_hci_bytes(data)?;

        Ok((
            Self {
                timestamp,
                sequence_num,
                iso_sdu_len,
            },
            data,
        ))
    }
}

impl WriteHci for IsoDataLoadHeader {
    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { if self.timestamp.is_some() { 8 } else { 4 } }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        if let Some(timestamp) = self.timestamp {
            timestamp.write_hci(&mut writer)?;
        }
        self.sequence_num.write_hci(&mut writer)?;
        self.iso_sdu_len.write_hci(writer)
    }

    #[inline(always)]
    /// Writes to `writer`; returns completion or its I/O error. Cancellation/error can leave partial output.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        if let Some(timestamp) = self.timestamp {
            timestamp.write_hci_async(&mut writer).await?;
        }
        self.sequence_num.write_hci_async(&mut writer).await?;
        self.iso_sdu_len.write_hci_async(writer).await
    }
}

/// HCI ISO Data packet [📖](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html#UUID-9b5fb085-278b-5084-ac33-bee2839abe6b)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct IsoPacket<'a> {
    /// Packed channel handle, boundary and timestamp flags preserved from the wire.
    handle: u16,
    /// Parsed SDU metadata for first/complete packets; absent for continuation/last fragments.
    data_load_header: Option<IsoDataLoadHeader>,
    /// Borrowed fragment bytes after the conditional load header; not necessarily a complete SDU.
    data: &'a [u8],
}

impl<'a> IsoPacket<'a> {
    /// Splits `data` at the masked load length in `header`; returns packet and unused tail.
    /// Returns `InvalidSize` for a truncated load or required conditional header.
    /// Parses SDU metadata only on first/complete boundaries; does not reassemble or validate sequence order.
    pub fn from_header_hci_bytes(
        header: IsoPacketHeader,
        data: &'a [u8],
    ) -> Result<(Self, &'a [u8]), FromHciBytesError> {
        let data_load_len = header.data_load_len();
        if data.len() < data_load_len {
            Err(FromHciBytesError::InvalidSize)
        } else {
            let (data, rest) = data.split_at(data_load_len);
            let (data_load_header, data) = match header.boundary_flag() {
                IsoPacketBoundary::FirstFragment | IsoPacketBoundary::Complete => {
                    IsoDataLoadHeader::from_hci_bytes(header.has_timestamp(), &data[..data_load_len])
                        .map(|(x, y)| (Some(x), y))?
                }
                IsoPacketBoundary::ContinuationFragment | IsoPacketBoundary::LastFragment => (None, data),
            };

            Ok((
                Self {
                    handle: header.handle,
                    data_load_header,
                    data,
                },
                rest,
            ))
        }
    }

    /// Returns the low 12-bit ISO logical-channel handle, excluding boundary/timestamp flags.
    pub fn handle(&self) -> ConnHandle { ConnHandle::new(self.handle & 0xFFF) }

    /// Returns the ISO fragment classification from the two boundary bits.
    pub fn boundary_flag(&self) -> IsoPacketBoundary {
        match (self.handle >> 12) & 0x03 {
            0 => IsoPacketBoundary::FirstFragment,
            1 => IsoPacketBoundary::ContinuationFragment,
            2 => IsoPacketBoundary::Complete,
            3 => IsoPacketBoundary::LastFragment,
            _ => unreachable!(),
        }
    }

    /// Returns copied SDU metadata, or `None` for continuation/last fragments.
    pub fn data_load_header(&self) -> Option<IsoDataLoadHeader> { self.data_load_header }

    /// Returns the load byte count including conditional metadata, excluding the four-byte ISO header.
    pub fn data_load_len(&self) -> usize {
        self.data_load_header.as_ref().map(|x| x.size()).unwrap_or_default() + self.data.len()
    }

    /// Returns fragment bytes without conditional metadata; callers must check status and reassemble SDUs.
    pub fn data(&self) -> &[u8] { self.data }
}

impl<'de> FromHciBytes<'de> for IsoPacket<'de> {
    /// Decodes one header and length-delimited payload from `data`; returns packet and unused tail, or truncation/field
    /// error.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (header, data) = IsoPacketHeader::from_hci_bytes(data)?;
        Self::from_header_hci_bytes(header, data)
    }
}

impl<'de> ReadHci<'de> for IsoPacket<'de> {
    /// Maximum packet bytes including its header, excluding the H4 indicator; no buffer is allocated here.
    const MAX_LEN: usize = 4 + 0x3FFF;

    /// Reads one packet from `reader` into payload storage `buf`; returns a borrowing packet or I/O/size error.
    /// The header is consumed before buffer admission; errors can leave the transport between packet boundaries.
    fn read_hci<R: embedded_io::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 4];
        reader.read_exact(&mut header)?;
        let (header, _) = IsoPacketHeader::from_hci_bytes(&header)?;
        let data_load_len = header.data_load_len();
        if buf.len() < data_load_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_load_len);
            reader.read_exact(buf)?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }

    /// Reads one packet from `reader` into `buf`; returns a borrowing packet or I/O/size error.
    /// Cancellation or failure can consume only part of a packet; restore framing before the next read.
    async fn read_hci_async<R: embedded_io_async::Read>(
        mut reader: R,
        buf: &'de mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        let mut header = [0; 4];
        reader.read_exact(&mut header).await?;
        let (header, _) = IsoPacketHeader::from_hci_bytes(&header)?;
        let data_load_len = header.data_load_len();
        if buf.len() < data_load_len {
            Err(ReadHciError::BufferTooSmall)
        } else {
            let (buf, _) = buf.split_at_mut(data_load_len);
            reader.read_exact(buf).await?;
            let (pkt, _) = Self::from_header_hci_bytes(header, buf)?;
            Ok(pkt)
        }
    }
}

impl WriteHci for IsoPacket<'_> {
    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { 4 + self.data_load_len() }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: embedded_io::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = IsoPacketHeader {
            handle: self.handle,
            data_load_len: self.data_load_len() as u16,
        };
        header.write_hci(&mut writer)?;
        if let Some(data_load_header) = &self.data_load_header {
            data_load_header.write_hci(&mut writer)?;
        }
        writer.write_all(self.data)
    }

    #[inline(always)]
    /// Writes to `writer`; returns completion or its I/O error. Cancellation/error can leave partial output.
    async fn write_hci_async<W: embedded_io_async::Write>(
        &self,
        mut writer: W,
    ) -> Result<(), W::Error> {
        let header = IsoPacketHeader {
            handle: self.handle,
            data_load_len: self.data_load_len() as u16,
        };
        header.write_hci_async(&mut writer).await?;
        if let Some(data_load_header) = &self.data_load_header {
            data_load_header.write_hci_async(&mut writer).await?;
        }
        writer.write_all(self.data).await
    }
}

impl PacketToController for IsoPacket<'_> {
    /// H4 discriminator emitted separately by the transport, not by this packet serializer.
    const KIND: PacketKind = PacketKind::IsoData;

    #[inline(always)]
    /// Returns serialized bytes excluding the H4 indicator; includes this value's own wire header.
    fn size(&self) -> usize { <Self as WriteHci>::size(self) }

    #[inline(always)]
    /// Writes header and payload to `writer`; returns completion or its I/O error, possibly after partial output.
    fn write_hci<W: Write>(
        &self,
        writer: W,
    ) -> Result<(), W::Error> {
        <Self as WriteHci>::write_hci(self, writer)
    }

    #[inline(always)]
    /// Returns a future writing to `writer`; cancellation/error can leave partial output; success means all bytes
    /// accepted.
    fn write_hci_async<W: AsyncWrite>(
        &self,
        writer: W,
    ) -> impl Future<Output = Result<(), W::Error>> {
        <Self as WriteHci>::write_hci_async(self, writer)
    }
}

#[cfg(test)]
/// Header fixtures exercise packed handle masking independently of packet payload decoding.
mod tests {
    use super::AclPacketHeader;
    use crate::FromHciBytes;
    use crate::param::ConnHandle;

    #[test]
    /// Confirms little-endian handle decoding removes boundary flags without losing handle bits.
    fn test_decode_acl_handle() {
        let input = &[32, 32, 0, 0];
        let header = AclPacketHeader::from_hci_bytes_complete(input).unwrap();
        assert_eq!(header.handle(), ConnHandle::new(32));

        let input = &[0, 33, 0, 0];
        let header = AclPacketHeader::from_hci_bytes_complete(input).unwrap();
        assert_eq!(header.handle(), ConnHandle::new(256));
    }
}
