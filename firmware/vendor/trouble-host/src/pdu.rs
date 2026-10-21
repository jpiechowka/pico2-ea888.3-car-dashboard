//! Owned packet wrappers carrying a logical initialized-prefix length.
use crate::Packet;

/// Internal packet owner plus payload length; constructors trust length admission upstream.
pub(crate) struct Pdu<P> {
    /// Pool-backed storage returned/released according to the packet implementation.
    packet: P,
    /// Logical payload byte count; must not exceed packet capacity before byte access.
    len: usize,
}

impl<P> Pdu<P> {
    /// Returns owned `packet` with logical byte `len`; capacity is not checked here.
    pub(crate) fn new(packet: P, len: usize) -> Self {
        Self { packet, len }
    }
    /// Returns the logical payload byte count.
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    /// Returns the full packet owner, discarding only the logical-length wrapper.
    pub(crate) fn into_inner(self) -> P {
        self.packet
    }
}

impl<P: Packet> core::fmt::Debug for Pdu<P> {
    /// Formats the initialized prefix into `f`; returns formatter errors and panics on invalid length.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Pdu").field(&&self.packet.as_ref()[..self.len]).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: Packet> defmt::Format for Pdu<P> {
    /// Emits initialized payload bytes through `fmt`; panics if logical length exceeds capacity.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Pdu({=[u8]:02x})", &self.packet.as_ref()[..self.len])
    }
}

impl<P: Packet> AsRef<[u8]> for Pdu<P> {
    /// Returns the payload prefix; panics if its stored length exceeds packet capacity.
    fn as_ref(&self) -> &[u8] {
        &self.packet.as_ref()[..self.len]
    }
}

impl<P: Packet> AsMut<[u8]> for Pdu<P> {
    /// Returns exclusive payload bytes; panics if stored length exceeds packet capacity.
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.packet.as_mut()[..self.len]
    }
}

/// Service Data Unit
///
/// A unit of payload that can be received or sent over an L2CAP channel.
pub struct Sdu<P> {
    /// Packet owner and logical SDU payload length.
    pdu: Pdu<P>,
}

impl<P> Sdu<P> {
    /// Returns an SDU owning pre-populated `packet` with logical byte `len`.
    ///
    /// `len` must not exceed the packet's byte capacity. Accessing an SDU created
    /// with an excessive length panics, just like indexing beyond a slice.
    pub fn new(packet: P, len: usize) -> Self {
        Self {
            pdu: Pdu::new(packet, len),
        }
    }

    /// Returns an SDU consuming `pdu` without copying or revalidating its length.
    pub(crate) fn from_pdu(pdu: Pdu<P>) -> Self {
        Self { pdu }
    }

    /// Returns the logical payload byte count.
    pub fn len(&self) -> usize {
        self.pdu.len()
    }

    /// Returns whether the logical payload length is zero.
    pub fn is_empty(&self) -> bool {
        self.pdu.len() == 0
    }

    /// Returns the complete packet owner; logical payload length is not carried separately.
    pub fn into_inner(self) -> P {
        self.pdu.into_inner()
    }
}

impl<P: Packet> AsRef<[u8]> for Sdu<P> {
    /// Returns the payload prefix; panics if its stored length exceeds packet capacity.
    fn as_ref(&self) -> &[u8] {
        self.pdu.as_ref()
    }
}

impl<P: Packet> AsMut<[u8]> for Sdu<P> {
    /// Returns exclusive payload bytes; panics if stored length exceeds packet capacity.
    fn as_mut(&mut self) -> &mut [u8] {
        self.pdu.as_mut()
    }
}
