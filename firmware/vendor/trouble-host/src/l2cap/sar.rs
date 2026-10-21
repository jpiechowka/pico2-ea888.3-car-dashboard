//! Bounded HCI/L2CAP reassembly owning one pool packet until completion or failure.
//! [Core L2CAP section 3.4.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/logical-link-control-and-adaptation-protocol-specification.html)
//! requires channel disconnection when accumulated K-frame payload exceeds the declared SDU length.
use crate::pdu::Pdu;
use crate::{Error, Packet};

/// Packet storage plus initialized prefix length; construction trusts internal length admission.
pub(crate) struct AssembledPacket<P> {
    /// Owned allocation returned by its implementation on drop.
    packet: P,
    /// Prefix bytes already populated, excluding framing stripped by the caller.
    written: usize,
}

impl<P> core::fmt::Debug for AssembledPacket<P> {
    /// Writes assembly progress metadata to `f`; returns formatting failure or success without payload bytes.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AssembledPacket")
            .field("assembled", &self.written)
            .finish()
    }
}

#[cfg(feature = "defmt")]
impl<P> defmt::Format for AssembledPacket<P> {
    /// Writes initialized-byte count through `f`, omitting packet contents.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "assembled = {}", self.written);
    }
}

impl<P> AssembledPacket<P> {
    /// Returns ownership of `packet` with `initial` populated bytes; internal callers must validate capacity first.
    pub(crate) fn new(packet: P, initial: usize) -> Self {
        Self {
            packet,
            written: initial,
        }
    }

    /// Appends `data`, returning InsufficientSpace without mutation if the allocation would be exceeded.
    /// Relies on admitted written length and a packet exposing equal immutable/mutable capacities.
    pub(crate) fn write(&mut self, data: &[u8]) -> Result<(), Error>
    where
        P: Packet,
    {
        if self.written + data.len() <= self.packet.as_ref().len() {
            self.packet.as_mut()[self.written..self.written + data.len()].copy_from_slice(data);
            self.written += data.len();
            Ok(())
        } else {
            Err(Error::InsufficientSpace)
        }
    }

    /// Returns the initialized prefix length in bytes.
    pub(crate) fn len(&self) -> usize {
        self.written
    }

    /// Returns a PDU when expected `length` equals the written prefix; mismatch returns FailedToFinalize and drops storage.
    pub(crate) fn finalize(self, length: usize) -> Result<Pdu<P>, Error> {
        if length != self.written {
            return Err(Error::FailedToFinalize {
                expected: length,
                actual: self.written,
            });
        }
        Ok(Pdu::new(self.packet, length))
    }
}

/// Single in-progress assembly; does not track connection identity or impose a time limit.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct PacketReassembly<P> {
    /// None while idle; errors from update consume and drop the active assembly.
    state: Option<State<P>>,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Target metadata paired with the uniquely owned partially populated packet.
pub(crate) struct State<P> {
    /// Destination CID and declared final payload length.
    state: AssemblyState,
    /// Retained bytes accumulated for this target.
    packet: AssembledPacket<P>,
}

impl<P> core::fmt::Debug for PacketReassembly<P> {
    /// Writes assembly progress metadata to `f`; returns formatting failure or success without payload bytes.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PacketReassembly")
            .field("state", &self.state.is_some())
            .finish()
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Destination and length carried with a completed reassembly.
pub(crate) struct AssemblyState {
    /// Destination L2CAP channel ID supplied by the initiating frame.
    pub channel: u16,
    /// Declared payload byte count, excluding caller-stripped framing.
    pub length: u16,
}

impl<P> PacketReassembly<P> {
    /// Returns an idle reassembler without allocating a packet.
    pub const fn new() -> Self {
        Self { state: None }
    }

    /// Starts empty `p` for destination `channel` and final byte `length`, returning Ok after admission.
    /// Returns InvalidState if occupied, or InsufficientSpace if length exceeds p capacity; errors drop p.
    pub fn init(&mut self, channel: u16, length: u16, p: P) -> Result<(), Error>
    where
        P: Packet,
    {
        self.init_with_written(channel, length, p, 0)
    }

    /// Starts `p` for `channel`/`length` with an existing `written`-byte prefix, returning Ok on admission.
    /// InvalidState preserves an existing assembly; InvalidValue rejects a prefix beyond length;
    /// InsufficientSpace rejects length beyond capacity. Every error drops the supplied p.
    pub fn init_with_written(&mut self, channel: u16, length: u16, p: P, written: usize) -> Result<(), Error>
    where
        P: Packet,
    {
        if self.state.is_some() {
            return Err(Error::InvalidState);
        }
        if written > usize::from(length) {
            return Err(Error::InvalidValue);
        }
        if usize::from(length) > p.as_ref().len() {
            return Err(Error::InsufficientSpace);
        }
        self.state.replace(State {
            state: AssemblyState { channel, length },
            packet: AssembledPacket::new(p, written),
        });
        //info!(
        //    "[host] initial reassembly on {} starting at {}, target {})",
        //    channel, written, length
        //);
        Ok(())
    }

    /// Drops the active packet, if any, and returns to idle; no channel or connection state is changed here.
    pub fn clear(&mut self) {
        let _ = self.state.take();
    }

    /// Returns whether or not there is a reassembly in progress.
    pub fn in_progress(&self) -> bool {
        self.state.is_some()
    }

    /// Appends `data`; returns Some(metadata, PDU) exactly at the target, None while still incomplete.
    /// NotFound means idle; InvalidValue means declared-length overrun; InsufficientSpace means storage exhaustion.
    /// Every update error drops active storage and returns to idle; the caller owns disconnect/recovery policy.
    pub fn update(&mut self, data: &[u8]) -> Result<Option<(AssemblyState, Pdu<P>)>, Error>
    where
        P: Packet,
    {
        if let Some(mut state) = self.state.take() {
            let target = state.state.length as usize;
            if state.packet.len() + data.len() > target {
                warn!(
                    "[l2cap][cid = {}] segment data would exceed declared SDU length {}",
                    state.state.channel, target
                );
                return Err(Error::InvalidValue);
            }
            state.packet.write(data)?;
            //info!(
            //    "[host] update reassembly on {} written {}, target {})",
            //    state.state.channel, state.packet.written, target
            //);
            if state.packet.len() == target {
                Ok(Some((state.state, state.packet.finalize(target)?)))
            } else {
                self.state.replace(state);
                Ok(None)
            }
        } else {
            Err(Error::NotFound)
        }
    }
}

#[cfg(test)]
/// Shared reassembly admission, ownership preservation and terminal-error regressions.
mod tests {
    use super::*;

    /// Four-byte local packet avoids shared global-pool state in these tests.
    struct Buffer(#[doc = "Initialized backing bytes; no separate logical-length field."] [u8; 4]);
    impl Packet for Buffer {}
    impl AsRef<[u8]> for Buffer {
        /// Returns the full four-byte storage.
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }
    impl AsMut<[u8]> for Buffer {
        /// Returns exclusive four-byte storage.
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }

    /// Invalid initial lengths never publish an assembly; busy admission preserves the original packet.
    #[test]
    fn admission_rejects_overrun_and_capacity_without_replacing_active_packet() {
        let mut assembly = PacketReassembly::new();
        for (length, written, expected) in [
            (1, 2, Error::InvalidValue),
            (4, usize::MAX, Error::InvalidValue),
            (5, 0, Error::InsufficientSpace),
        ] {
            assert_eq!(
                assembly.init_with_written(64, length, Buffer([0; 4]), written),
                Err(expected)
            );
            assert!(!assembly.in_progress());
        }
        assembly.init(64, 4, Buffer([0; 4])).unwrap();
        assert_eq!(assembly.init(65, 1, Buffer([9; 4])), Err(Error::InvalidState));
        let (metadata, pdu) = assembly.update(&[1, 2, 3, 4]).unwrap().unwrap();
        assert_eq!(metadata.channel, 64);
        assert_eq!(pdu.as_ref(), &[1, 2, 3, 4]);
        assert!(!assembly.in_progress());
    }

    /// Exact, empty, partial and excessive continuations preserve the declared boundary and release on failure.
    #[test]
    fn completion_and_failed_continuations_release_the_assembly() {
        let mut assembly = PacketReassembly::new();
        assembly.init(64, 0, Buffer([0; 4])).unwrap();
        assert_eq!(assembly.update(&[]).unwrap().unwrap().1.len(), 0);
        assembly.init_with_written(64, 3, Buffer([1, 0, 0, 0]), 1).unwrap();
        assert!(assembly.update(&[]).unwrap().is_none());
        assert!(assembly.update(&[2]).unwrap().is_none());
        assert_eq!(assembly.update(&[3]).unwrap().unwrap().1.as_ref(), &[1, 2, 3]);
        assembly.init(64, 1, Buffer([0; 4])).unwrap();
        assert!(matches!(assembly.update(&[1, 2]), Err(Error::InvalidValue)));
        assert!(!assembly.in_progress());
        assert!(matches!(assembly.update(&[]), Err(Error::NotFound)));
        assembly.init(64, 1, Buffer([0; 4])).unwrap();
        assembly.clear();
        assert!(!assembly.in_progress());
    }
}
