//! Blocking controller types and traits.
use crate::controller::ErrorType;
use crate::{data, ControllerToHostPacket};

/// Trait representing a HCI controller which supports blocking and non-blocking operations.
pub trait Controller: ErrorType {
    /// Type of the buffer
    type Buffer<'a>;

    /// Allocate a buffer
    /// Returns controller-specific initialized receive storage or an allocation error.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error>;

    /// Write ACL data to the controller. Blocks until done.
    /// Sends borrowed `packet`; returns success after submission or a transport error. No deadline is imposed.
    fn write_acl_data(
        &self,
        packet: &data::AclPacket,
    ) -> Result<(), Self::Error>;

    /// Write Sync data to the controller. Blocks until done.
    /// Sends borrowed `packet`; returns success after submission or a transport error. No deadline is imposed.
    fn write_sync_data(
        &self,
        packet: &data::SyncPacket,
    ) -> Result<(), Self::Error>;

    /// Write Iso data to the controller. Blocks until done.
    /// Sends borrowed `packet`; returns success after submission or a transport error. No deadline is imposed.
    fn write_iso_data(
        &self,
        packet: &data::IsoPacket,
    ) -> Result<(), Self::Error>;

    /// Attempt to write ACL data to the controller.
    ///
    /// Returns a TryError if the operation would block.
    /// Borrows `packet` for one attempt; returns success, Busy or the underlying transport error.
    /// Partial effects depend on the transport; Busy must not be interpreted as peer receipt.
    fn try_write_acl_data(
        &self,
        packet: &data::AclPacket,
    ) -> Result<(), TryError<Self::Error>>;

    /// Attempt to write Sync data to the controller.
    ///
    /// Returns a TryError if the operation would block.
    /// Borrows `packet` for one attempt; returns success, Busy or the underlying transport error.
    /// Partial effects depend on the transport; Busy must not be interpreted as peer receipt.
    fn try_write_sync_data(
        &self,
        packet: &data::SyncPacket,
    ) -> Result<(), TryError<Self::Error>>;

    /// Attempt to write Iso data to the controller.
    ///
    /// Returns a TryError if the operation would block.
    /// Borrows `packet` for one attempt; returns success, Busy or the underlying transport error.
    /// Partial effects depend on the transport; Busy must not be interpreted as peer receipt.
    fn try_write_iso_data(
        &self,
        packet: &data::IsoPacket,
    ) -> Result<(), TryError<Self::Error>>;

    /// Read a valid HCI packet from the controller. Blocks until done.
    /// Fills caller `buf` and returns a packet borrowing it, or a transport/decode error; no deadline is imposed.
    fn read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<ControllerToHostPacket<'a>, Self::Error>;

    /// Read a valid HCI packet from the controller.
    ///
    /// Returns a TryError if the operation would block.
    /// Fills caller `buf` and returns a borrowing packet, Busy, or a transport/decode error.
    /// Consumed bytes and command-response dispatch before Busy/error are not rolled back.
    fn try_read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<ControllerToHostPacket<'a>, TryError<Self::Error>>;
}

pub use bt_hci_transport::blocking::TryError;
