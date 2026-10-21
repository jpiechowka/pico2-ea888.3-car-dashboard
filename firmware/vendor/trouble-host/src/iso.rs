//! Isochronous (CIS/BIS) HCI commands and data.

use bt_hci::cmd::{AsyncCmd, SyncCmd};
use bt_hci::controller::{Controller, ControllerCmdAsync, ControllerCmdSync};
use bt_hci::data::IsoPacket;

use crate::host::BleHost;
use crate::{BleHostError, PacketPool};

/// A type for running isochronous-stream HCI commands and data.
pub struct Iso<'stack, C, P: PacketPool> {
    /// Shared controller command/data transport; stream ownership remains with the caller.
    host: BleHost<'stack, C, P>,
}

impl<'stack, C: Controller, P: PacketPool> Iso<'stack, C, P> {
    /// Returns an ISO facade over `host` without issuing controller commands.
    pub(crate) fn new(host: BleHost<'stack, C, P>) -> Self {
        Self { host }
    }

    /// Sends synchronous HCI `cmd` and returns its command-complete response or host/controller error.
    pub async fn command<Cmd>(&self, cmd: Cmd) -> Result<Cmd::Return, BleHostError<C::Error>>
    where
        Cmd: SyncCmd,
        C: ControllerCmdSync<Cmd>,
    {
        self.host.command(cmd).await
    }

    /// Run an asynchronous HCI command (one whose completion arrives as a separate event, e.g.
    /// `LE Accept CIS Request`'s `LE CIS Established`) without waiting for that event.
    /// `cmd` is the command payload; returns command-status success or host/controller error.
    pub async fn command_async<Cmd>(&self, cmd: Cmd) -> Result<(), BleHostError<C::Error>>
    where
        Cmd: AsyncCmd,
        C: ControllerCmdAsync<Cmd>,
    {
        self.host.async_command(cmd).await
    }

    /// Sends raw ISO `packet` and returns the controller transport result.
    /// Does not establish a stream or prove over-air delivery.
    pub async fn send(&self, packet: &IsoPacket<'_>) -> Result<(), C::Error> {
        self.host.write_iso_data(packet).await
    }
}
