//! Type-complete inert controller for host state-machine tests; transport/command execution intentionally panics.
use core::convert::Infallible;

use bt_hci::cmd::{self, AsyncCmd, SyncCmd};
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};

/// No device state; only fixed-size buffer allocation is implemented.
pub struct MockController {}

impl MockController {
    /// Returns an inert controller without opening a transport.
    pub fn new() -> Self {
        Self {}
    }
}

impl embedded_io::ErrorType for MockController {
    /// Buffer allocation cannot fail; unimplemented operations panic instead of reporting this error.
    type Error = Infallible;
}

impl bt_hci::controller::blocking::Controller for MockController {
    /// Owned 259-byte buffer used by host-side decoding fixtures.
    type Buffer<'a> = [u8; 259];

    /// Returns a fresh zeroed 259-byte receive buffer.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0u8; 259])
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn write_acl_data(&self, packet: &bt_hci::data::AclPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn write_sync_data(&self, packet: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn write_iso_data(&self, packet: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn try_write_acl_data(
        &self,
        packet: &bt_hci::data::AclPacket,
    ) -> Result<(), bt_hci::controller::blocking::TryError<Self::Error>> {
        todo!()
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn try_write_sync_data(
        &self,
        packet: &bt_hci::data::SyncPacket,
    ) -> Result<(), bt_hci::controller::blocking::TryError<Self::Error>> {
        todo!()
    }

    /// Panics immediately; `packet` transmission is intentionally unsupported, so no Result is returned.
    fn try_write_iso_data(
        &self,
        packet: &bt_hci::data::IsoPacket,
    ) -> Result<(), bt_hci::controller::blocking::TryError<Self::Error>> {
        todo!()
    }

    /// Panics immediately; `buf` is not populated and no packet result is returned.
    fn read<'a>(&self, buf: &'a mut Self::Buffer<'_>) -> Result<bt_hci::ControllerToHostPacket<'a>, Self::Error> {
        todo!()
    }

    /// Panics immediately; `buf` is not populated and no packet result is returned.
    fn try_read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<bt_hci::ControllerToHostPacket<'a>, bt_hci::controller::blocking::TryError<Self::Error>> {
        todo!()
    }
}

impl bt_hci::controller::Controller for MockController {
    /// Owned 259-byte buffer used by host-side decoding fixtures.
    type Buffer<'a> = [u8; 259];

    /// Returns a fresh zeroed 259-byte receive buffer.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0u8; 259])
    }

    /// Returns a future that panics when polled; `packet` is never transmitted.
    async fn write_acl_data(&self, packet: &bt_hci::data::AclPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Returns a future that panics when polled; `packet` is never transmitted.
    async fn write_sync_data(&self, packet: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Returns a future that panics when polled; `packet` is never transmitted.
    async fn write_iso_data(&self, packet: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    /// Returns a future that panics when polled; `buf` is never populated.
    async fn read<'a>(&self, buf: &'a mut Self::Buffer<'_>) -> Result<bt_hci::ControllerToHostPacket<'a>, Self::Error> {
        todo!()
    }
}

impl<C: SyncCmd> ControllerCmdSync<C> for MockController {
    /// Returns a future that panics when polled; `cmd` is not sent or interpreted.
    async fn exec(&self, cmd: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        todo!()
    }
}

impl<C: AsyncCmd> ControllerCmdAsync<C> for MockController {
    /// Returns a future that panics when polled; `cmd` is not sent or interpreted.
    async fn exec(&self, cmd: &C) -> Result<(), cmd::Error<Self::Error>> {
        todo!()
    }
}
