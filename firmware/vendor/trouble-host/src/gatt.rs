//! Fixed-bearer GATT server events, client transactions, discovery and bounded notification fan-out.
//! Retaining additional connection owners can panic at the bounded connection reference limit.
//!
//! Client requests require a concurrent receive task; cancellation after request admission closes the
//! bearer because ATT has no transaction identity. Server event drop processes once and queues replies
//! best-effort. See [Bluetooth ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/attribute-protocol--att-.html)
//! and [GATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/generic-attribute-profile--gatt-.html).
use core::cell::RefCell;
use core::future::Future;
use core::marker::PhantomData;
use core::ops::ControlFlow;

use bt_hci::controller::Controller;
use bt_hci::param::{ConnHandle, FrameSpaceInitiator, PhyKind, PhyMask, SpacingTypes, Status};
use bt_hci::uuid::declarations::{CHARACTERISTIC, PRIMARY_SERVICE};
use bt_hci::uuid::descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::channel::Channel;
use embassy_sync::pubsub::{self, PubSubChannel, WaitResult};
use embassy_time::{with_timeout, Duration};
use heapless::Vec;

use crate::att::{
    self, Att, AttCfm, AttClient, AttCmd, AttErrorCode, AttReq, AttRsp, AttServer, AttUns, ATT_HANDLE_VALUE_IND,
    ATT_HANDLE_VALUE_NTF,
};
use crate::attribute::{AttributeHandle, Characteristic, CharacteristicProps, Descriptor, Uuid};
use crate::attribute_server::{AttributeServer, DynamicAttributeServer};
use crate::connection::Connection;
#[cfg(feature = "security")]
use crate::connection::SecurityLevel;
use crate::cursor::{ReadCursor, WriteCursor};
use crate::pdu::Pdu;
use crate::prelude::{CharacteristicDeclaration, ConnectionEvent, ConnectionParamsRequest};
#[cfg(feature = "security")]
use crate::security_manager::PassKey;
use crate::types::gatt_traits::{AsGatt, FromGatt, FromGattError};
use crate::types::l2cap::L2capHeader;
#[cfg(feature = "security")]
use crate::BondInformation;
use crate::{config, BleHostError, Error, PacketPool, Stack, MAX_INVALID_DATA_LEN};

/// A GATT connection event.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GattConnectionEvent<'stack, 'server, P: PacketPool> {
    /// Connection disconnected.
    Disconnected {
        /// The reason (status code) for the disconnect.
        reason: Status,
    },
    /// The phy settings was updated for this connection.
    PhyUpdated {
        /// The TX phy.
        tx_phy: PhyKind,
        /// The RX phy.
        rx_phy: PhyKind,
    },
    /// Connection timing was updated for this connection.
    ConnectionParamsUpdated {
        /// Connection interval.
        conn_interval: Duration,
        /// Allowed skipped connection events.
        peripheral_latency: u16,
        /// Supervision timeout.
        supervision_timeout: Duration,
    },
    /// The subrating was updated for this connection.
    SubratingParamsUpdated {
        /// Subrate factor: only every `subrate_factor`-th connection event is used.
        subrate_factor: u16,
        /// Peripheral latency, in subrated connection events.
        peripheral_latency: u16,
        /// Number of underlying connection events to stay awake for after a non-empty packet.
        continuation_number: u16,
        /// Supervision timeout.
        supervision_timeout: Duration,
    },
    /// A request to change the connection parameters.
    ///
    /// Respond through [`ConnectionParamsRequest`]; dropping the token sends no reply.
    /// Acceptance requires `central` or `connection-params-update`; rejection is always available.
    RequestConnectionParams(
        #[doc = "Pending controller parameter request; caller responds through its token."] ConnectionParamsRequest,
    ),
    /// The data length was changed for this connection.
    DataLengthUpdated {
        /// Max TX octets.
        max_tx_octets: u16,
        /// Maximum TX packet transmission time in microseconds.
        max_tx_time: u16,
        /// Max RX octets.
        max_rx_octets: u16,
        /// Maximum RX packet transmission time in microseconds.
        max_rx_time: u16,
    },
    /// The frame space was updated for this connection.
    FrameSpaceUpdated {
        /// The negotiated frame space value.
        frame_space: Duration,
        /// Who initiated the frame space update.
        initiator: FrameSpaceInitiator,
        /// PHYs affected.
        phys: PhyMask,
        /// Spacing types affected.
        spacing_types: SpacingTypes,
    },
    /// Connection rate has been changed.
    ConnectionRateChanged {
        /// Connection interval.
        conn_interval: Duration,
        /// Subrate factor.
        subrate_factor: u16,
        /// Allowed skipped connection events.
        peripheral_latency: u16,
        /// Underlying connection events retained after a nonempty packet.
        continuation_number: u16,
        /// Supervision timeout.
        supervision_timeout: Duration,
    },
    /// GATT event.
    Gatt {
        /// The event that was returned
        event: GattEvent<'stack, 'server, P>,
    },

    #[cfg(feature = "security")]
    /// Display pass key
    PassKeyDisplay(#[doc = "Pairing-generated display value."] PassKey),
    #[cfg(feature = "security")]
    /// Confirm pass key
    PassKeyConfirm(#[doc = "Peer comparison value awaiting user confirmation."] PassKey),
    #[cfg(feature = "security")]
    /// Input the pass key
    PassKeyInput,
    #[cfg(feature = "security")]
    /// Pairing completed
    PairingComplete {
        /// Security level of this pairing
        security_level: SecurityLevel,
        /// Bond information if the devices create a bond with this pairing.
        bond: Option<BondInformation>,
    },
    #[cfg(feature = "security")]
    /// Pairing failed
    PairingFailed(#[doc = "Pairing failure classified by the security state machine."] Error),
    #[cfg(feature = "security")]
    /// The peer has lost its bond.
    BondLost,
    #[cfg(feature = "security")]
    /// The link is now encrypted.
    Encrypted {
        /// Security level achieved by the encryption.
        security_level: SecurityLevel,
        /// Bond information if encryption was achieved using a stored bond.
        bond: Option<BondInformation>,
    },
    #[cfg(feature = "security")]
    /// OOB data is requested during pairing. Respond with [`GattConnection::provide_oob_data()`].
    OobRequest,
}

/// Used to manage a GATT connection with a client.
pub struct GattConnection<'stack, 'server, P: PacketPool> {
    /// Retained link owner; its final drop requests disconnection.
    connection: Connection<'stack, P>,
    /// Borrowed local attribute/CCCD server used for permission checks and processing.
    pub(crate) server: &'server dyn DynamicAttributeServer<P>,
}

impl<P: PacketPool> Drop for GattConnection<'_, '_, P> {
    /// Unregisters this connection from the attribute server; releasing the final underlying connection owner schedules link closure.
    fn drop(&mut self) {
        trace!("[gatt {}] disconnecting from server", self.connection.handle().raw());
        self.server.disconnect(&self.connection);
    }
}

impl<'stack, 'server, P: PacketPool> GattConnection<'stack, 'server, P> {
    /// Creates a GATT connection from the given BLE connection and `AttributeServer`:
    /// this will register the client within the server's CCCD table.
    /// Returns a wrapper consuming `connection` and registering its identity with `server`; propagates client-table capacity/state errors without a wrapper.
    pub(crate) fn try_new<'values, M: RawMutex, const AT: usize, const CN: usize>(
        connection: Connection<'stack, P>,
        server: &'server AttributeServer<'values, M, P, AT, CN>,
    ) -> Result<Self, Error> {
        trace!("[gatt {}] connecting to server", connection.handle().raw());
        server.connect(&connection)?;
        Ok(Self { connection, server })
    }

    /// Confirm that the displayed pass key matches the one displayed on the other party
    /// Returns the underlying pairing confirmation result; unsupported without security and invalid outside the expected pairing state.
    pub fn pass_key_confirm(&self) -> Result<(), Error> {
        self.connection.pass_key_confirm()
    }

    /// The displayed pass key does not match the one displayed on the other party
    /// Returns the underlying pairing rejection result; unsupported without security and invalid outside the expected pairing state.
    pub fn pass_key_cancel(&self) -> Result<(), Error> {
        self.connection.pass_key_cancel()
    }

    /// Input the pairing pass key
    /// Forwards `pass_key` to pairing and returns its state/admission result; caller supplies 0..=999999, which is not range-checked here.
    pub fn pass_key_input(&self, pass_key: u32) -> Result<(), Error> {
        self.connection.pass_key_input(pass_key)
    }

    /// Provide OOB data during pairing.
    ///
    /// Call this when [`GattConnectionEvent::OobRequest`] is received.
    #[cfg(feature = "security")]
    /// Returns pairing admission/verification for `local_oob` and `peer_oob`; see Connection::provide_oob_data for asymmetric use, missing-material rejection and confirm verification.
    pub fn provide_oob_data(
        &self,
        local_oob: crate::security_manager::OobData,
        peer_oob: crate::security_manager::OobData,
    ) -> Result<(), Error> {
        self.connection.provide_oob_data(local_oob, peer_oob)
    }

    /// Wait for the next GATT connection event.
    ///
    /// Uses the attribute server to handle the protocol.
    /// Returns the first connection-control or queued ATT event, preferring a ready control event; cancellation while waiting consumes neither. Dropping a returned GATT event performs its default processing.
    pub async fn next(&self) -> GattConnectionEvent<'stack, 'server, P> {
        match select(self.connection.next(), self.connection.next_gatt()).await {
            Either::First(event) => match event {
                ConnectionEvent::Disconnected { reason } => GattConnectionEvent::Disconnected { reason },
                ConnectionEvent::ConnectionParamsUpdated {
                    conn_interval,
                    peripheral_latency,
                    supervision_timeout,
                } => GattConnectionEvent::ConnectionParamsUpdated {
                    conn_interval,
                    peripheral_latency,
                    supervision_timeout,
                },
                ConnectionEvent::SubratingParamsUpdated {
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout,
                } => GattConnectionEvent::SubratingParamsUpdated {
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout,
                },
                ConnectionEvent::RequestConnectionParams(req) => GattConnectionEvent::RequestConnectionParams(req),
                ConnectionEvent::PhyUpdated { tx_phy, rx_phy } => GattConnectionEvent::PhyUpdated { tx_phy, rx_phy },
                ConnectionEvent::DataLengthUpdated {
                    max_tx_octets,
                    max_tx_time,
                    max_rx_octets,
                    max_rx_time,
                } => GattConnectionEvent::DataLengthUpdated {
                    max_tx_octets,
                    max_tx_time,
                    max_rx_octets,
                    max_rx_time,
                },
                ConnectionEvent::FrameSpaceUpdated {
                    frame_space,
                    initiator,
                    phys,
                    spacing_types,
                } => GattConnectionEvent::FrameSpaceUpdated {
                    frame_space,
                    initiator,
                    phys,
                    spacing_types,
                },
                ConnectionEvent::ConnectionRateChanged {
                    conn_interval,
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout,
                } => GattConnectionEvent::ConnectionRateChanged {
                    conn_interval,
                    subrate_factor,
                    peripheral_latency,
                    continuation_number,
                    supervision_timeout,
                },

                #[cfg(feature = "security")]
                ConnectionEvent::PassKeyDisplay(key) => GattConnectionEvent::PassKeyDisplay(key),

                #[cfg(feature = "security")]
                ConnectionEvent::PassKeyConfirm(key) => GattConnectionEvent::PassKeyConfirm(key),

                #[cfg(feature = "security")]
                ConnectionEvent::PassKeyInput => GattConnectionEvent::PassKeyInput,

                #[cfg(feature = "security")]
                ConnectionEvent::PairingComplete { security_level, bond } => {
                    GattConnectionEvent::PairingComplete { security_level, bond }
                }

                #[cfg(feature = "security")]
                ConnectionEvent::PairingFailed(err) => GattConnectionEvent::PairingFailed(err),

                #[cfg(feature = "security")]
                ConnectionEvent::BondLost => GattConnectionEvent::BondLost,

                #[cfg(feature = "security")]
                ConnectionEvent::Encrypted { security_level, bond } => {
                    GattConnectionEvent::Encrypted { security_level, bond }
                }

                #[cfg(feature = "security")]
                ConnectionEvent::OobRequest => GattConnectionEvent::OobRequest,
            },
            Either::Second(data) => GattConnectionEvent::Gatt {
                event: GattEvent::new(GattData::new(data, self.connection.clone()), self.server),
            },
        }
    }

    /// Get a reference to the underlying BLE connection.
    /// Returns the retained connection reference; does not create another reference-counted owner.
    pub fn raw(&self) -> &Connection<'stack, P> {
        &self.connection
    }

    /// Set the value of an attribute on the local GATT server for this connection.
    /// Returns the result of storing `input` bytes at `attribute_handle` in this local server; no notification or peer write is sent.
    pub fn set<T: AttributeHandle>(&self, attribute_handle: &T, input: &T::Value) -> Result<(), Error> {
        let gatt_value = input.as_gatt();
        self.server.set(&self.connection, attribute_handle.handle(), gatt_value)
    }

    /// Get the value of an attribute from the local GATT server for this connection.
    /// Returns the decoded local value at `attribute_handle`, or table/decoder error; uses a 512-byte stack buffer and preserves only a bounded byte prefix on decode failure.
    pub fn get<T: AttributeHandle>(&self, attribute_handle: &T) -> Result<T::Value, Error>
    where
        T::Value: FromGatt,
    {
        let mut buf = [0; 512];
        let len = self.server.get(&self.connection, attribute_handle.handle(), &mut buf)?;
        let value_slice = &buf[..len];
        T::Value::from_gatt(value_slice).map_err(|_| {
            let mut invalid_data = [0u8; MAX_INVALID_DATA_LEN];
            let len_to_copy = value_slice.len().min(MAX_INVALID_DATA_LEN);
            invalid_data[..len_to_copy].copy_from_slice(&value_slice[..len_to_copy]);

            Error::CannotConstructGattValue(invalid_data)
        })
    }
}

/// A GATT payload ready for processing.
pub struct GattData<'stack, P: PacketPool> {
    /// Owned packet awaiting consumption; None means already processed or no response required.
    pdu: Option<Pdu<P::Packet>>,
    /// Retained link owner; its final drop requests disconnection.
    connection: Connection<'stack, P>,
}

impl<P: PacketPool> core::fmt::Debug for GattData<'_, P> {
    /// Formats the owned ATT packet representation into `f`; returns formatter failure. Payload bytes are not redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("GattData").field(&self.pdu).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for GattData<'_, P> {
    /// Formats the owned ATT packet representation into `fmt`; payload bytes are not redacted.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "GattData({})", &self.pdu)
    }
}

impl<'stack, P: PacketPool> GattData<'stack, P> {
    /// Returns a request owner consuming `pdu` and `connection`; ingress must already have validated a client ATT PDU.
    pub(crate) const fn new(pdu: Pdu<P::Packet>, connection: Connection<'stack, P>) -> Self {
        Self {
            pdu: Some(pdu),
            connection,
        }
    }

    /// Return the characteristic handle that this GATT request is related to, if applicable.
    ///
    /// Returns `None` if the request is not related to a characteristic handle (e.g. a service discovery request).
    pub fn handle(&self) -> Option<u16> {
        match self.incoming() {
            AttClient::Request(AttReq::Write { handle, .. }) => Some(handle),
            AttClient::Command(AttCmd::Write { handle, .. }) => Some(handle),
            AttClient::Request(AttReq::Read { handle }) => Some(handle),
            AttClient::Request(AttReq::ReadBlob { handle, .. }) => Some(handle),
            _ => None,
        }
    }

    /// Get the raw incoming ATT PDU.
    /// Returns the borrowed decoded client PDU. Panics if internal ingress invariants are broken: missing, malformed or non-client packet.
    pub fn incoming(&self) -> AttClient<'_> {
        // We know that:
        // - The PDU is decodable, as it was already decoded once before adding it to the connection queue
        // - The PDU is of type `Att::Client` because only those types of PDUs are added to the connection queue
        let att = unwrap!(Att::decode(self.pdu.as_ref().unwrap().as_ref()));
        let Att::Client(client) = att else {
            unreachable!("Expected Att::Client, got {:?}", att)
        };

        client
    }

    /// Respond directly to request.
    /// Returns after encoding `rsp` and queue admission; allocation, codec, disconnected or retention-exhaustion errors propagate. Cancellation drops unqueued bytes; admission is not delivery.
    pub async fn reply(self, rsp: AttRsp<'_>) -> Result<(), Error> {
        let pdu = assemble(&self.connection, AttServer::Response(rsp))?;
        self.connection.send(pdu).await?;
        Ok(())
    }

    /// Send an unsolicited ATT PDU without having a request (e.g. notification or indication)
    /// Returns after encoding `uns` for `connection` and queue admission; allocation, codec or link errors propagate. Does not await indication confirmation; cancellation drops unqueued bytes.
    pub async fn send_unsolicited(connection: &Connection<'_, P>, uns: AttUns<'_>) -> Result<(), Error> {
        let pdu = assemble(connection, AttServer::Unsolicited(uns))?;
        connection.send(pdu).await?;
        Ok(())
    }
}

/// An event returned while processing GATT requests.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GattEvent<'stack, 'server, P: PacketPool> {
    /// A characteristic was read.
    Read(#[doc = "Owned read operation with automatic default processing."] ReadEvent<'stack, 'server, P>),
    /// A characteristic was written.
    Write(#[doc = "Owned write operation with automatic default processing."] WriteEvent<'stack, 'server, P>),
    /// Other event.
    Other(#[doc = "Owned remaining client operation with automatic processing."] OtherEvent<'stack, 'server, P>),
    /// A request was made that was not allowed by the permissions of the attribute.
    NotAllowed(
        #[doc = "Denied operation retaining permission error and exact handle."] NotAllowedEvent<'stack, 'server, P>,
    ),
}

impl<'stack, 'server, P: PacketPool> GattEvent<'stack, 'server, P> {
    /// Create a new GATT event from the provided `GattData` and `DynamicAttributeServer`.
    /// Returns an event consuming `data` and borrowing `server`; permission failures retain the exact denied handle. Dropping an unprocessed event applies its default accept/reject action.
    pub fn new(data: GattData<'stack, P>, server: &'server dyn DynamicAttributeServer<P>) -> Self {
        let att = data.incoming();

        let allowed = match &att {
            AttClient::Command(AttCmd::Write { handle, .. }) => server
                .can_write(&data.connection, *handle)
                .map_err(|err| (*handle, err)),
            AttClient::Request(req) => match req {
                AttReq::Write { handle, .. } => server
                    .can_write(&data.connection, *handle)
                    .map_err(|err| (*handle, err)),
                #[cfg(feature = "att-queued-writes")]
                AttReq::PrepareWrite { handle, .. } => server
                    .can_write(&data.connection, *handle)
                    .map_err(|err| (*handle, err)),
                AttReq::Read { handle } | AttReq::ReadBlob { handle, .. } => {
                    server.can_read(&data.connection, *handle).map_err(|err| (*handle, err))
                }
                AttReq::ReadMultiple { handles } => handles.as_chunks::<2>().0.iter().try_for_each(|handle| {
                    let handle = u16::from_le_bytes(*handle);
                    server.can_read(&data.connection, handle).map_err(|err| (handle, err))
                }),
                _ => Ok(()),
            },
            _ => Ok(()),
        };

        if let Err((handle, err)) = allowed {
            return GattEvent::NotAllowed(NotAllowedEvent {
                data,
                handle,
                err,
                server,
            });
        }

        match att {
            AttClient::Request(AttReq::Write { .. }) | AttClient::Command(AttCmd::Write { .. }) => {
                GattEvent::Write(WriteEvent { data, server })
            }
            AttClient::Request(AttReq::Read { .. }) | AttClient::Request(AttReq::ReadBlob { .. }) => {
                GattEvent::Read(ReadEvent { data, server })
            }
            #[cfg(feature = "att-queued-writes")]
            AttClient::Request(AttReq::ExecuteWrite { flags, .. })
                if flags == 0x01 && data.connection.with_prepare_write(|pw| pw.handle != 0) =>
            {
                GattEvent::Write(WriteEvent { data, server })
            }
            _ => GattEvent::Other(OtherEvent { data, server }),
        }
    }

    /// Accept the event, making it processed by the server.
    /// Returns an owned reply after consuming the request once; server/packet-allocation/codec errors propagate. Dropping the reply only attempts immediate enqueue.
    pub fn accept(self) -> Result<Reply<'stack, P>, Error> {
        match self {
            Self::Read(e) => e.accept(),
            Self::Write(e) => e.accept(),
            Self::Other(e) => e.accept(),
            Self::NotAllowed(e) => e.accept(),
        }
    }

    /// Reject the event with the provided error code, it will not be processed by the attribute server.
    /// Returns an owned error reply using `err`; commands/confirmations remain silent. Consumes the request once and propagates packet/codec errors.
    pub fn reject(self, err: AttErrorCode) -> Result<Reply<'stack, P>, Error> {
        match self {
            Self::Read(e) => e.reject(err),
            Self::Write(e) => e.reject(err),
            Self::Other(e) => e.reject(err),
            Self::NotAllowed(e) => e.reject(err),
        }
    }

    /// Get a reference to the underlying `GattData` payload that this event is enclosing
    /// Returns the borrowed request without processing it; dropping the enclosing event still performs its default action.
    pub fn payload(&self) -> &GattData<'stack, P> {
        match self {
            Self::Read(e) => e.payload(),
            Self::Write(e) => e.payload(),
            Self::Other(e) => e.payload(),
            Self::NotAllowed(e) => e.payload(),
        }
    }

    /// Convert the event back into the `GattData` payload it is enclosing
    ///
    /// Allows for custom processing of the enclosed data, as in handling payloads
    /// which are not supported yet by the enclosed attribute server.
    /// Note that this will consume the event, so it would be up to the caller to respond
    /// to the incoming payload if needed and however they see fit.
    /// Returns ownership of the request and disables this event's automatic processing; caller becomes responsible for any required response.
    pub fn into_payload(self) -> GattData<'stack, P> {
        match self {
            Self::Read(e) => e.into_payload(),
            Self::Write(e) => e.into_payload(),
            Self::Other(e) => e.into_payload(),
            Self::NotAllowed(e) => e.into_payload(),
        }
    }
}

/// A characteristic read event returned while processing GATT requests.
pub struct ReadEvent<'stack, 'server, P: PacketPool> {
    /// Owned request bytes and retained connection; taking its PDU disables automatic processing.
    data: GattData<'stack, P>,
    /// Borrowed local attribute/CCCD server used for permission checks and processing.
    server: &'server dyn DynamicAttributeServer<P>,
}

impl<P: PacketPool> core::fmt::Debug for ReadEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `f`; returns formatter failure. Payload bytes are not redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ReadEvent").field(&self.data).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for ReadEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `fmt`; payload bytes are not redacted.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "ReadEvent({})", &self.data)
    }
}

impl<'stack, P: PacketPool> ReadEvent<'stack, '_, P> {
    /// Characteristic handle that was read
    /// Returns the requested Read/ReadBlob handle; internal event classification guarantees it exists.
    pub fn handle(&self) -> u16 {
        // We know that the unwrap cannot fail, because `ReadEvent` wraps
        // ATT payloads that always do have a handle
        unwrap!(self.data.handle())
    }

    /// Accept the event, making it processed by the server.
    ///
    /// Automatically called if drop() is invoked.
    /// Returns an owned reply after consuming the request once; server/packet-allocation/codec errors propagate. Dropping the reply only attempts immediate enqueue.
    pub fn accept(mut self) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Ok(()), None)
    }

    /// Accept the event without server processing.
    /// Returns a reply containing `data` from the requested blob offset, bounded by ATT MTU and physical packet space. Invalid offset creates an ATT error; allocation/codec errors propagate after consuming the request. Does not mutate the table.
    pub fn accept_unprocessed<T: AsGatt + ?Sized>(mut self, data: &T) -> Result<Reply<'stack, P>, Error> {
        let (rsp, offset) = match self.data.incoming() {
            AttClient::Request(AttReq::Read { .. }) => (att::ATT_READ_RSP, 0),
            AttClient::Request(AttReq::ReadBlob { offset, .. }) => (att::ATT_READ_BLOB_RSP, offset as usize),
            _ => unreachable!(),
        };
        let Some(value) = data.as_gatt().get(offset..) else {
            return self.reject(AttErrorCode::INVALID_OFFSET);
        };
        self.data.pdu = None;

        let mut tx = P::allocate().ok_or(Error::OutOfMemory)?;
        let mut w = WriteCursor::new(tx.as_mut());
        let (mut header, mut payload) = w.split(4)?;
        // The value starts at the requested offset and shares the ATT MTU with
        // one opcode byte. Also respect the physical packet buffer capacity.
        let mtu = self.data.connection.get_att_mtu() as usize;
        payload.write(rsp)?;
        let len = value.len().min(mtu.saturating_sub(1)).min(payload.available());
        payload.append(&value[..len])?;
        header.write(payload.len() as u16)?;
        header.write(4_u16)?;

        let len = header.len() + payload.len();
        let pdu = Pdu::new(tx, len);
        Ok(Reply::new(self.data.connection.clone(), Some(pdu)))
    }

    /// Reject the event with the provided error code, it will not be processed by the attribute server.
    /// Returns an owned error reply using `err`; commands/confirmations remain silent. Consumes the request once and propagates packet/codec errors.
    pub fn reject(mut self, err: AttErrorCode) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Err(err), None)
    }

    /// Get a reference to the underlying `GattData` payload that this event is enclosing
    /// Returns the borrowed request without processing it; dropping the enclosing event still performs its default action.
    pub fn payload(&self) -> &GattData<'stack, P> {
        &self.data
    }

    /// Convert the event back into the `GattData` payload it is enclosing
    ///
    /// Allows for custom processing of the enclosed data, as in handling payloads
    /// which are not supported yet by the enclosed attribute server.
    /// Note that this will consume the event, so it would be up to the caller to respond
    /// to the incoming payload if needed and however they see fit.
    /// Returns ownership of the request and disables this event's automatic processing; caller becomes responsible for any required response.
    pub fn into_payload(mut self) -> GattData<'stack, P> {
        GattData {
            pdu: self.data.pdu.take(),
            connection: self.data.connection.clone(),
        }
    }
}

impl<P: PacketPool> Drop for ReadEvent<'_, '_, P> {
    /// Processes a still-owned Read request once; reply enqueue is best-effort and errors are discarded.
    fn drop(&mut self) {
        let _ = process(&mut self.data, self.server, Ok(()), None);
    }
}

/// A characteristic write event returned while processing GATT requests.
pub struct WriteEvent<'stack, 'server, P: PacketPool> {
    /// Owned request bytes and retained connection; taking its PDU disables automatic processing.
    data: GattData<'stack, P>,
    /// Borrowed local attribute/CCCD server used for permission checks and processing.
    server: &'server dyn DynamicAttributeServer<P>,
}

impl<P: PacketPool> core::fmt::Debug for WriteEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `f`; returns formatter failure. Payload bytes are not redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("WriteEvent").field(&self.data).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for WriteEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `fmt`; payload bytes are not redacted.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "WriteEvent({})", &self.data)
    }
}

impl<'stack, P: PacketPool> WriteEvent<'stack, '_, P> {
    /// Characteristic handle that was written
    /// Returns the direct write handle or retained ExecuteWrite prepare handle; internal event classification guarantees the variant.
    pub fn handle(&self) -> u16 {
        match self.data.incoming() {
            AttClient::Request(AttReq::Write { handle, .. }) | AttClient::Command(AttCmd::Write { handle, .. }) => {
                handle
            }
            #[cfg(feature = "att-queued-writes")]
            AttClient::Request(AttReq::ExecuteWrite { .. }) => self.data.connection.with_prepare_write(|pw| pw.handle),
            _ => unreachable!(),
        }
    }

    /// Raw data to be written
    /// Returns `f` applied synchronously to byte offset and borrowed write bytes; ordinary writes use offset zero, ExecuteWrite uses retained prepare storage. The closure must not reborrow that storage.
    pub fn with_data<R>(&self, f: impl FnOnce(usize, &[u8]) -> R) -> R {
        match self.data.incoming() {
            AttClient::Request(AttReq::Write { data, .. }) | AttClient::Command(AttCmd::Write { data, .. }) => {
                f(0, data)
            }
            #[cfg(feature = "att-queued-writes")]
            AttClient::Request(AttReq::ExecuteWrite { .. }) => self.data.connection.with_prepare_write(|pw| {
                let data = &pw.buf[..pw.len as usize];
                f(usize::from(pw.offset), data)
            }),
            _ => unreachable!(),
        }
    }

    /// Characteristic data to be written
    /// Returns decoded pending bytes using `_c` only to select T; does not verify its handle. Nonzero prepared offset returns InvalidLength.
    pub fn value<T: FromGatt>(&self, _c: &Characteristic<T>) -> Result<T, FromGattError> {
        self.with_data(|offset, data| {
            if offset == 0 {
                T::from_gatt(data)
            } else {
                Err(FromGattError::InvalidLength)
            }
        })
    }

    /// Validate the offset and length of the write request against the attribute's `len` and `capacity`
    /// Returns Ok when the pending byte offset is within `len` and its end fits `capacity`; otherwise the precise ATT offset/length error. Does not mutate storage.
    pub fn validate(&self, len: usize, capacity: usize) -> Result<(), AttErrorCode> {
        self.with_data(|offset, data| {
            if offset > len {
                Err(AttErrorCode::INVALID_OFFSET)
            } else if offset + data.len() > capacity {
                Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH)
            } else {
                Ok(())
            }
        })
    }

    /// Accept the event, making it processed by the server.
    ///
    /// Automatically called if drop() is invoked.
    /// Returns an owned reply after consuming the request once; server/packet-allocation/codec errors propagate. Dropping the reply only attempts immediate enqueue.
    pub fn accept(mut self) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Ok(()), None)
    }

    /// Accept the event without server processing.
    /// Returns a silent command reply or success response without updating the table; ExecuteWrite clears prepared bytes. Allocation/codec failures propagate after consuming the request.
    pub fn accept_unprocessed(mut self) -> Result<Reply<'stack, P>, Error> {
        let rsp = match self.data.incoming() {
            AttClient::Request(AttReq::Write { .. }) => Some(att::ATT_WRITE_RSP),
            AttClient::Command(AttCmd::Write { .. }) => None,
            #[cfg(feature = "att-queued-writes")]
            AttClient::Request(AttReq::ExecuteWrite { .. }) => {
                self.data.connection.clear_prepare_write();
                Some(att::ATT_EXECUTE_WRITE_RSP)
            }
            _ => unreachable!(),
        };

        self.data.pdu = None;

        if let Some(rsp) = rsp {
            let mut tx = P::allocate().ok_or(Error::OutOfMemory)?;
            let mut w = WriteCursor::new(tx.as_mut());
            w.write(1_u16)?;
            w.write(4_u16)?;
            w.write(rsp)?;
            let len = w.len();
            let pdu = Pdu::new(tx, len);
            Ok(Reply::new(self.data.connection.clone(), Some(pdu)))
        } else {
            Ok(Reply::new(self.data.connection.clone(), None))
        }
    }

    /// Reject the event with the provided error code, it will not be processed by the attribute server.
    /// Returns an owned error reply using `err`; commands/confirmations remain silent. Consumes the request once and propagates packet/codec errors.
    pub fn reject(mut self, err: AttErrorCode) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Err(err), None)
    }

    /// Get a reference to the underlying `GattData` payload that this event is enclosing
    /// Returns the borrowed request without processing it; dropping the enclosing event still performs its default action.
    pub fn payload(&self) -> &GattData<'stack, P> {
        &self.data
    }

    /// Convert the event back into the `GattData` payload it is enclosing
    ///
    /// Allows for custom processing of the enclosed data, as in handling payloads
    /// which are not supported yet by the enclosed attribute server.
    /// Note that this will consume the event, so it would be up to the caller to respond
    /// to the incoming payload if needed and however they see fit.
    /// Returns ownership of the request and disables this event's automatic processing; caller becomes responsible for any required response.
    pub fn into_payload(mut self) -> GattData<'stack, P> {
        GattData {
            pdu: self.data.pdu.take(),
            connection: self.data.connection.clone(),
        }
    }
}

impl<P: PacketPool> Drop for WriteEvent<'_, '_, P> {
    /// Applies a still-owned Write request once; reply enqueue is best-effort and errors are discarded.
    fn drop(&mut self) {
        let _ = process(&mut self.data, self.server, Ok(()), None);
    }
}

/// Other event returned while processing GATT requests (neither read, nor write).
pub struct OtherEvent<'stack, 'server, P: PacketPool> {
    /// Owned request bytes and retained connection; taking its PDU disables automatic processing.
    data: GattData<'stack, P>,
    /// Borrowed local attribute/CCCD server used for permission checks and processing.
    server: &'server dyn DynamicAttributeServer<P>,
}

impl<P: PacketPool> core::fmt::Debug for OtherEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `f`; returns formatter failure. Payload bytes are not redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("OtherEvent").field(&self.data).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for OtherEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `fmt`; payload bytes are not redacted.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "OtherEvent({})", &self.data)
    }
}

impl<'stack, P: PacketPool> OtherEvent<'stack, '_, P> {
    /// Accept the event, making it processed by the server.
    ///
    /// Automatically called if drop() is invoked.
    /// Returns an owned reply after consuming the request once; server/packet-allocation/codec errors propagate. Dropping the reply only attempts immediate enqueue.
    pub fn accept(mut self) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Ok(()), None)
    }

    /// Reject the event with the provided error code, it will not be processed by the attribute server.
    /// Returns an owned error reply using `err`; commands/confirmations remain silent. Consumes the request once and propagates packet/codec errors.
    pub fn reject(mut self, err: AttErrorCode) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Err(err), None)
    }

    /// Get a reference to the underlying `GattData` payload that this event is enclosing
    /// Returns the borrowed request without processing it; dropping the enclosing event still performs its default action.
    pub fn payload(&self) -> &GattData<'stack, P> {
        &self.data
    }

    /// Convert the event back into the `GattData` payload it is enclosing
    ///
    /// Allows for custom processing of the enclosed data, as in handling payloads
    /// which are not supported yet by the enclosed attribute server.
    /// Note that this will consume the event, so it would be up to the caller to respond
    /// to the incoming payload if needed and however they see fit.
    /// Returns ownership of the request and disables this event's automatic processing; caller becomes responsible for any required response.
    pub fn into_payload(mut self) -> GattData<'stack, P> {
        GattData {
            pdu: self.data.pdu.take(),
            connection: self.data.connection.clone(),
        }
    }
}

impl<P: PacketPool> Drop for OtherEvent<'_, '_, P> {
    /// Processes a still-owned remaining request once; errors and failed immediate reply admission are discarded.
    fn drop(&mut self) {
        let _ = process(&mut self.data, self.server, Ok(()), None);
    }
}

/// Permission-denied operation; default drop rejects using the captured attribute and error.
pub struct NotAllowedEvent<'stack, 'server, P: PacketPool> {
    /// Exact attribute whose permission check failed, including ReadMultiple members.
    handle: u16,
    /// Owned request bytes and retained connection; taking its PDU disables automatic processing.
    data: GattData<'stack, P>,
    /// Captured ATT permission failure used by default rejection.
    err: AttErrorCode,
    /// Borrowed local attribute/CCCD server used for permission checks and processing.
    server: &'server dyn DynamicAttributeServer<P>,
}

impl<P: PacketPool> core::fmt::Debug for NotAllowedEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `f`; returns formatter failure. Payload bytes are not redacted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("NotAllowedEvent").field(&self.data).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for NotAllowedEvent<'_, '_, P> {
    /// Formats the owned ATT packet representation into `fmt`; payload bytes are not redacted.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "NotAllowedEvent({})", &self.data)
    }
}

impl<'stack, P: PacketPool> NotAllowedEvent<'stack, '_, P> {
    /// Characteristic handle that was requested
    /// Returns the exact attribute whose permission check failed, including a denied member of ReadMultiple.
    pub fn handle(&self) -> u16 {
        self.handle
    }

    /// Rejects the denied event using its captured permission error; also performed by drop.
    /// Returns an ATT error reply using the captured permission failure and handle; does not apply the denied operation.
    pub fn accept(mut self) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Err(self.err), Some(self.handle))
    }

    /// Reject the event with the provided error code, it will not be processed by the attribute server.
    /// Returns an owned error reply using `err`; commands/confirmations remain silent. Consumes the request once and propagates packet/codec errors.
    pub fn reject(mut self, err: AttErrorCode) -> Result<Reply<'stack, P>, Error> {
        process(&mut self.data, self.server, Err(err), Some(self.handle))
    }

    /// Get a reference to the underlying `GattData` payload that this event is enclosing
    /// Returns the borrowed request without processing it; dropping the enclosing event still performs its default action.
    pub fn payload(&self) -> &GattData<'stack, P> {
        &self.data
    }

    /// Convert the event back into the `GattData` payload it is enclosing
    ///
    /// Allows for custom processing of the enclosed data, as in handling payloads
    /// which are not supported yet by the enclosed attribute server.
    /// Note that this will consume the event, so it would be up to the caller to respond
    /// to the incoming payload if needed and however they see fit.
    /// Returns ownership of the request and disables this event's automatic processing; caller becomes responsible for any required response.
    pub fn into_payload(mut self) -> GattData<'stack, P> {
        GattData {
            pdu: self.data.pdu.take(),
            connection: self.data.connection.clone(),
        }
    }
}

impl<P: PacketPool> Drop for NotAllowedEvent<'_, '_, P> {
    /// Rejects a still-owned request with captured permission error/handle; errors and failed immediate reply admission are discarded.
    fn drop(&mut self) {
        let _ = process(&mut self.data, self.server, Err(self.err), Some(self.handle));
    }
}

/// Consumes `data` once, applying `server` for Ok `result` or encoding its error with optional `error_handle` override. Returns an owned reply or allocation/server/codec error; already-consumed data returns a silent reply.
fn process<'stack, P>(
    data: &mut GattData<'stack, P>,
    server: &dyn DynamicAttributeServer<P>,
    result: Result<(), AttErrorCode>,
    error_handle: Option<u16>,
) -> Result<Reply<'stack, P>, Error>
where
    P: PacketPool,
{
    if let Some(pdu) = data.pdu.take() {
        let res = match result {
            Ok(_) => process_accept(&pdu, &data.connection, server),
            Err(code) => process_reject(&pdu, &data.connection, code, error_handle),
        };
        res
    } else {
        Ok(Reply::new(data.connection.clone(), None))
    }
}

/// Returns a reply after processing validated client `pdu` for `connection` through `server`; bounds response storage by negotiated ATT MTU. Pool exhaustion/codec/server errors propagate; table effects may precede later encoding errors.
fn process_accept<'stack, P>(
    pdu: &Pdu<P::Packet>,
    connection: &Connection<'stack, P>,
    server: &dyn DynamicAttributeServer<P>,
) -> Result<Reply<'stack, P>, Error>
where
    P: PacketPool,
{
    // - The PDU is decodable, as it was already decoded once before adding it to the connection queue
    // - The PDU is of type `Att::Client` because only those types of PDUs are added to the connection queue
    let att = unwrap!(Att::decode(pdu.as_ref()));
    let Att::Client(att) = att else {
        unreachable!("Expected Att::Client, got {:?}", att)
    };
    let mut tx = P::allocate().ok_or(Error::OutOfMemory)?;
    let mut w = WriteCursor::new(tx.as_mut());
    let (mut header, mut data) = w.split(4)?;
    // Limit the buffer given to process() so that multi-entry ATT responses
    // (ReadByType, ReadByGroupType, FindInformation) are bounded by the
    // negotiated ATT MTU. Without this, entries are written into the full
    // packet-pool buffer and then post-hoc truncated, which can split an
    // entry in half and produce a malformed PDU.
    let mtu = connection.get_att_mtu() as usize;
    let written = {
        let buf = data.write_buf();
        let limit = buf.len().min(mtu);
        server.process(connection, &att, &mut buf[..limit])?
    };
    if let Some(written) = written {
        data.commit(written)?;
        header.write(data.len() as u16)?;
        header.write(4_u16)?;
        let len = header.len() + data.len();
        let pdu = Pdu::new(tx, len);
        Ok(Reply::new(connection.clone(), Some(pdu)))
    } else {
        Ok(Reply::new(connection.clone(), None))
    }
}

/// Returns an error reply to request `pdu` on `connection`, using `code` and optional exact `error_handle`; commands/confirmations return silent replies. ExecuteWrite clears prepared bytes; allocation/codec errors propagate.
fn process_reject<'stack, P: PacketPool>(
    pdu: &Pdu<P::Packet>,
    connection: &Connection<'stack, P>,
    code: AttErrorCode,
    error_handle: Option<u16>,
) -> Result<Reply<'stack, P>, Error> {
    // - The PDU is decodable, as it was already decoded once before adding it to the connection queue
    // - The PDU is of type `Att::Client` because only those types of PDUs are added to the connection queue
    let att = unwrap!(Att::decode(pdu.as_ref()));
    let Att::Client(att) = att else {
        unreachable!("Expected Att::Client, got {:?}", att)
    };
    // Commands and confirmations never receive an ATT Error Response, even
    // when application or attribute permission checks reject them.
    if !matches!(att, AttClient::Request(_)) {
        return Ok(Reply::new(connection.clone(), None));
    }
    // An Execute Write empties the prepare queue regardless of outcome (Core
    // Spec Vol 3, Part F, §3.4.6.3). `process_accept` clears it via the attribute
    // server (`handle_execute_write`), and `accept_unprocessed` clears it too, but
    // the reject path did not — leaving a stale queue that makes the *next*
    // client's Prepare Write fail with `PREPARE_QUEUE_FULL` on every other
    // rejected long write.
    #[cfg(feature = "att-queued-writes")]
    if let AttClient::Request(AttReq::ExecuteWrite { .. }) = &att {
        connection.clear_prepare_write();
    }
    let handle = error_handle.unwrap_or(match att {
        #[cfg(feature = "att-queued-writes")]
        AttClient::Request(AttReq::PrepareWrite { handle, .. }) => handle,
        AttClient::Request(AttReq::Write { handle, .. }) => handle,
        AttClient::Request(AttReq::Read { handle }) => handle,
        AttClient::Request(AttReq::ReadBlob { handle, .. }) => handle,
        _ => 0, // No application-selected failing handle is available.
    });
    // We know it has been checked, therefore this cannot fail
    let request = pdu.as_ref()[0];
    let rsp = AttRsp::Error { request, handle, code };
    let pdu = assemble(connection, AttServer::Response(rsp))?;
    Ok(Reply::new(connection.clone(), Some(pdu)))
}

/// Returns an owned L2CAP-framed server `att` for `conn`; allocation/codec errors propagate. Encodes into physical packet space first, then truncates ATT bytes to negotiated MTU; callers must avoid splitting structured responses.
pub(crate) fn assemble<'stack, P: PacketPool>(
    conn: &Connection<'stack, P>,
    att: AttServer<'_>,
) -> Result<Pdu<P::Packet>, Error> {
    let mut tx = P::allocate().ok_or(Error::OutOfMemory)?;
    let mut w = WriteCursor::new(tx.as_mut());
    let (mut header, mut data) = w.split(4)?;
    data.write(Att::Server(att))?;

    let mtu = conn.get_att_mtu();
    data.truncate(mtu as usize);
    header.write(data.len() as u16)?;
    header.write(4_u16)?;
    let len = header.len() + data.len();
    Ok(Pdu::new(tx, len))
}

/// A reply to a gatt request.
///
/// Drop attempts immediate queue admission; send waits for capacity. Neither guarantees peer delivery,
/// and disconnected or exhausted retention can still reject admission.
pub struct Reply<'stack, P: PacketPool> {
    /// Retained link owner; its final drop requests disconnection.
    connection: Connection<'stack, P>,
    /// Owned packet awaiting consumption; None means already processed or no response required.
    pdu: Option<Pdu<P::Packet>>,
}

impl<'stack, P: PacketPool> Reply<'stack, P> {
    /// Returns a reply owning `connection` and optional framed `pdu`; None represents an operation requiring no response.
    fn new(connection: Connection<'stack, P>, pdu: Option<Pdu<P::Packet>>) -> Self {
        Self { connection, pdu }
    }

    /// Send the reply.
    ///
    /// May fail if the outbound queue is full.
    /// Returns immediate queue admission or disconnected, retention-exhaustion or full-queue error; consumes the response even on failure. A silent reply succeeds without queue access.
    pub fn try_send(mut self) -> Result<(), Error> {
        if let Some(pdu) = self.pdu.take() {
            self.connection.try_send(pdu)
        } else {
            Ok(())
        }
    }

    /// Queues the reply, waiting for capacity; returns without transmission acknowledgment. This unit-returning API logs and discards admission failures; use try_send for an immediate reported result. Cancellation drops unqueued bytes and retention.
    pub async fn send(mut self) {
        if let Some(pdu) = self.pdu.take() {
            if self.connection.send(pdu).await.is_err() {
                warn!("[gatt] reply enqueue failed");
            }
        }
    }
}

#[cfg(test)]
impl<'stack, P: PacketPool> Reply<'stack, P> {
    /// Extract the ATT payload from the response PDU (skipping 4-byte L2CAP header).
    /// Returns None if the reply carried no PDU.
    fn att_payload(&self) -> Option<&[u8]> {
        self.pdu.as_ref().map(|pdu| &pdu.as_ref()[4..])
    }
}

impl<P: PacketPool> Drop for Reply<'_, P> {
    /// Attempts immediate queue admission for remaining reply bytes; failure discards them and logs only metadata.
    fn drop(&mut self) {
        if let Some(pdu) = self.pdu.take() {
            if self.connection.try_send(pdu).is_err() {
                warn!("[gatt] reply enqueue failed");
            }
        }
    }
}

/// Notification listener for GATT client.
pub struct NotificationListener<'lst, const MTU: usize> {
    /// Optional value-handle filter; None receives all published handles.
    handle: Option<u16>,
    /// Bounded pubsub subscription; drop releases subscriber capacity without changing peer CCCD.
    listener: pubsub::DynSubscriber<'lst, Notification<MTU>>,
}

impl<'lst, const MTU: usize> NotificationListener<'lst, MTU> {
    /// Receive a notification, reporting queue loss instead of joining fragments
    /// across it. Stream protocols must disconnect/invalidate on `Err`.
    /// Returns the next matching notification, or Err(number of globally lost queue entries). Other handles are consumed; cancellation preserves unread messages but already skipped entries stay consumed.
    pub async fn next_checked(&mut self) -> Result<Notification<MTU>, u64> {
        loop {
            match self.listener.next_message().await {
                WaitResult::Lagged(count) => return Err(count),
                WaitResult::Message(m) if self.handle.is_none() || self.handle == Some(m.handle) => return Ok(m),
                WaitResult::Message(_) => {}
            }
        }
    }

    #[allow(clippy::should_implement_trait)]
    /// Wait for the next matching notification, ignoring queue-loss markers.
    /// Returns the next matching value while silently discarding lag notifications; use next_checked for stream protocols. Cancellation leaves unread messages queued.
    pub async fn next(&mut self) -> Notification<MTU> {
        loop {
            if let WaitResult::Message(m) = self.listener.next_message().await {
                if self.handle.is_none() || self.handle == Some(m.handle) {
                    return m;
                }
            }
        }
    }
}

/// Maximum simultaneous client notification subscriptions from generated configuration.
const MAX_NOTIF: usize = config::GATT_CLIENT_NOTIFICATION_MAX_SUBSCRIBERS;
/// Retained broadcast notification entries before oldest-message eviction.
const NOTIF_QSIZE: usize = config::GATT_CLIENT_NOTIFICATION_QUEUE_SIZE;
/// Maximum copied notification value bytes, excluding opcode and handle.
const NOTIF_MTU: usize = config::GATT_CLIENT_NOTIFICATION_MTU;

/// BT Core Spec Vol 3, Part F, Section 3.3.3: ATT transaction timeout.
pub(crate) const ATT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// A GATT client capable of using the GATT protocol.
///
/// One live client owns the connection's fixed ATT bearer. Concurrent requests
/// return `Error::Busy`. Dropping an unfinished request closes the connection:
/// ATT responses have no transaction identifier that permits safe reuse.
pub struct GattClient<'reference, T: Controller, P: PacketPool, const MAX_SERVICES: usize> {
    /// Distinct fully discovered service records, bounded by MAX_SERVICES; incomplete discovery is not cached.
    known_services: RefCell<Vec<ServiceHandle, MAX_SERVICES>>,
    /// Controller type association; stores no controller instance.
    _phantom: PhantomData<T>,
    /// Exclusive fixed-ATT client claim and retained link.
    connection: ClientConnection<'reference, P>,
    /// One outstanding server response; pump waits when full.
    response_channel: Channel<NoopRawMutex, (ConnHandle, Pdu<P::Packet>), 1>,

    // TODO: Wait for something like https://github.com/rust-lang/rust/issues/132980 (min_generic_const_args) to allow using P::MTU
    /// Fixed-capacity broadcast values; immediate publication evicts oldest messages and exposes lag to checked listeners.
    notifications: PubSubChannel<NoopRawMutex, Notification<NOTIF_MTU>, NOTIF_QSIZE, MAX_NOTIF, 1>,
}

/// Exclusive GATT-client claim coupled to a retained connection owner.
struct ClientConnection<'d, P: PacketPool>(
    #[doc = "Retained link coupled to the exclusive client flag."] Connection<'d, P>,
);

impl<'d, P: PacketPool> ClientConnection<'d, P> {
    /// Returns an exclusive fixed-ATT client owner cloned from `connection`; Busy if already claimed, or the underlying link/state error. Drop releases that claim.
    fn acquire(connection: &Connection<'d, P>) -> Result<Self, Error> {
        connection.manager().acquire_gatt_client(connection.index())?;
        Ok(Self(connection.clone()))
    }
}

impl<'d, P: PacketPool> core::ops::Deref for ClientConnection<'d, P> {
    /// Retained BLE connection exposed through the exclusive client owner.
    type Target = Connection<'d, P>;

    /// Returns the retained BLE connection without cloning or releasing ATT ownership.
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<P: PacketPool> Drop for ClientConnection<'_, P> {
    /// Releases this link's exclusive GATT client claim; retained connection drops afterward.
    fn drop(&mut self) {
        self.manager().release_gatt_client(self.index());
    }
}

/// Cancellation guard for one admitted fixed-bearer ATT transaction.
struct AttRequestGuard<'a, 'd, P: PacketPool> {
    /// Borrowed link whose transaction slot is reserved.
    connection: &'a Connection<'d, P>,
    /// False until validated response consumption; unfinished drop requests disconnect.
    complete: bool,
}

impl<'a, 'd, P: PacketPool> AttRequestGuard<'a, 'd, P> {
    /// Returns a guard reserving the request opcode `opcode` on `connection`; Busy/disconnected errors leave no guard. Unfinished drop requests disconnect.
    fn begin(connection: &'a Connection<'d, P>, opcode: u8) -> Result<Self, Error> {
        connection.manager().begin_att_request(connection.index(), opcode)?;
        Ok(Self {
            connection,
            complete: false,
        })
    }

    /// Releases the ATT transaction slot and marks this guard complete, preventing its cancellation disconnect.
    fn finish(mut self) {
        self.connection.manager().finish_att_request(self.connection.index());
        self.complete = true;
    }
}

impl<P: PacketPool> Drop for AttRequestGuard<'_, '_, P> {
    /// Requests disconnect if no validated response completed this ATT transaction; cannot prove canceled sends never reached the peer.
    fn drop(&mut self) {
        if !self.complete {
            // Even cancellation while enqueueing cannot prove that no request
            // reached the bearer. Quarantine until controller disconnect.
            self.connection.disconnect();
        }
    }
}

/// A notification payload.
#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Notification<const MTU: usize> {
    /// Received characteristic value handle.
    handle: u16,
    /// Fixed-capacity copied value bytes; unused tail is zero-filled.
    data: [u8; MTU],
    /// Initialized value prefix length in bytes, at most MTU.
    len: usize,
    /// True for ATT indication, false for notification.
    indication: bool,
}

impl<const MTU: usize> Notification<MTU> {
    /// The characteristic value handle this notification is for.
    /// Returns the received characteristic value handle, without revalidating a cached descriptor.
    pub fn handle(&self) -> u16 {
        self.handle
    }

    /// Whether this notification was received as an indication.
    /// Returns true for an indication, false for an unacknowledged notification; ingress handles confirmation separately.
    pub fn is_indication(&self) -> bool {
        self.indication
    }
}

impl<const MTU: usize> AsRef<[u8]> for Notification<MTU> {
    /// Returns only initialized value bytes, excluding handle/opcode and unused fixed-array tail.
    fn as_ref(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Handle for a GATT service.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ServiceHandle {
    /// Inclusive service declaration handle.
    start: u16,
    /// Inclusive last handle in the service group.
    end: u16,
    /// Discovered service UUID in its decoded width.
    uuid: Uuid,
}

impl ServiceHandle {
    /// Get the attribute handles that belong to this service
    /// Returns the inclusive discovered service handle range, including declaration and attributes.
    pub fn handle_range(&self) -> core::ops::RangeInclusive<u16> {
        self.start..=self.end
    }

    /// Get the UUID of this service
    /// Returns the recorded service UUID without normalization or another discovery request.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}

/// Encoded ATT opcode and owned framed packet, or host/controller-typed encoding failure.
type EncodedAtt<P, E> = Result<(u8, Pdu<P>), BleHostError<E>>;

/// Owned response packet plus the receive pump's controller link identity.
pub(crate) struct Response<P> {
    /// Owned ATT response bytes, without L2CAP header.
    pdu: Pdu<P>,
    /// Controller connection handle captured by the receive pump.
    handle: ConnHandle,
}

// ATT 3.4.1.1 defines a fixed five-byte Error Response. Check its request
// without decoding the full response enum in the async transaction frames.
/// Returns Ok only for exact five-byte `data` with Error Response opcode matching `request`; wrong request yields UnexpectedGattResponse, other shape InvalidValue.
fn validate_error_response(data: &[u8], request: u8) -> Result<(), Error> {
    match data {
        [att::ATT_ERROR_RSP, opcode, _, _, _] if *opcode == request => Ok(()),
        [att::ATT_ERROR_RSP, _, _, _, _] => Err(Error::UnexpectedGattResponse),
        _ => Err(Error::InvalidValue),
    }
}

/// Trait with behavior for a gatt client.
pub(crate) trait Client<'d, E, P: PacketPool> {
    /// Perform a gatt request and return the response.
    /// Returns the response to `req` after exclusive request admission and enqueue; starts a 30-second response timeout after enqueue. Busy, allocation, codec, link, timeout and mismatched-error-opcode failures propagate; cancellation after admission closes the bearer.
    fn request(&self, req: AttReq<'_>) -> impl Future<Output = Result<Response<P::Packet>, BleHostError<E>>>;
    /// Returns after encoding and enqueueing `cmd`; no ATT acknowledgment is expected. Cancellation drops unqueued bytes; prior accepted queue entries remain owned by the runner.
    fn command(&self, cmd: AttCmd<'_>) -> impl Future<Output = Result<(), BleHostError<E>>>;
}

impl<'reference, T: Controller, P: PacketPool, const MAX_SERVICES: usize> Client<'reference, T::Error, P>
    for GattClient<'reference, T, P, MAX_SERVICES>
{
    /// Returns the response to `req` after exclusive request admission and enqueue; starts a 30-second response timeout after enqueue. Busy, allocation, codec, link, timeout and mismatched-error-opcode failures propagate; cancellation after admission closes the bearer.
    async fn request(&self, req: AttReq<'_>) -> Result<Response<P::Packet>, BleHostError<T::Error>> {
        let data = Att::Client(AttClient::Request(req));

        let (request_opcode, pdu) = Self::encode_att_data(data)?;
        let transaction = AttRequestGuard::begin(&self.connection, request_opcode)?;
        self.connection.send(pdu).await?;

        // BT Core Spec Vol 3, Part F, Section 3.3.3: 30-second ATT transaction timeout.
        // If the server does not respond within 30 seconds, the client shall close the
        // ATT bearer (disconnect).
        let (h, pdu) = with_timeout(ATT_TRANSACTION_TIMEOUT, self.response_channel.receive())
            .await
            .map_err(|_| {
                warn!("[gatt] ATT transaction timeout (30s), disconnecting");
                self.connection.disconnect();
                BleHostError::BleHost(Error::Timeout)
            })?;

        if !self.connection.is_connected() {
            return Err(Error::Disconnected.into());
        }
        if h != self.connection.handle() {
            return Err(Error::UnexpectedGattResponse.into());
        }
        if pdu.as_ref().first() == Some(&att::ATT_ERROR_RSP) {
            validate_error_response(pdu.as_ref(), request_opcode)?;
        }
        transaction.finish();
        Ok(Response { handle: h, pdu })
    }

    /// Returns after encoding and enqueueing `cmd`; no ATT acknowledgment is expected. Cancellation drops unqueued bytes; prior accepted queue entries remain owned by the runner.
    async fn command(&self, cmd: AttCmd<'_>) -> Result<(), BleHostError<T::Error>> {
        let data = Att::Client(AttClient::Command(cmd));

        self.send_att_data(data).await?;

        Ok(())
    }
}

impl<'reference, T: Controller, P: PacketPool, const MAX_SERVICES: usize> GattClient<'reference, T, P, MAX_SERVICES> {
    // Keep encoding temporaries out of the async send future.
    /// Returns opcode and owned L2CAP-framed `data`; packet exhaustion or encoding capacity failure leaves no queued packet. Packet-pool size must fit the u16 L2CAP length domain.
    fn encode_att_data(data: Att<'_>) -> EncodedAtt<P::Packet, T::Error> {
        let header = L2capHeader {
            channel: crate::types::l2cap::L2CAP_CID_ATT,
            length: data.size() as u16,
        };

        let mut buf = P::allocate().ok_or(Error::OutOfMemory)?;
        let mut w = WriteCursor::new(buf.as_mut());
        w.write_hci(&header)?;
        let opcode_offset = w.len();
        w.write(data)?;
        let len = w.len();
        let opcode = buf.as_ref()[opcode_offset];

        Ok((opcode, Pdu::new(buf, len)))
    }

    /// Returns the opcode after encoding and queueing `data`; allocation/codec/link/retention failures propagate. Cancellation discards only unqueued bytes.
    async fn send_att_data(&self, data: Att<'_>) -> Result<u8, BleHostError<T::Error>> {
        let (opcode, pdu) = Self::encode_att_data(data)?;
        self.connection.send(pdu).await?;
        Ok(opcode)
    }
}

impl<'reference, C: Controller, P: PacketPool, const MAX_SERVICES: usize> GattClient<'reference, C, P, MAX_SERVICES> {
    /// Creates the sole client on a connection's fixed ATT bearer.
    /// Returns an exclusive client for `connection`; `_stack` supplies the controller type only. Performs once-only MTU exchange, then best-effort bonded encryption. Allocation, busy, malformed response, timeout and disconnected errors propagate; cancellation during ATT exchange closes the bearer. Early notifications are consumed before listeners exist.
    pub async fn new(
        _stack: &Stack<'_, C, P>,
        connection: &Connection<'reference, P>,
    ) -> Result<GattClient<'reference, C, P, MAX_SERVICES>, BleHostError<C::Error>> {
        let mut output = None;
        Self::new_into(_stack, connection, &mut output).await?;
        output.ok_or(Error::InvalidState.into())
    }

    /// Initializes the sole client for `connection` in empty caller-owned `output`,
    /// returning a reference to that final owner; `_stack` supplies the controller type.
    /// An occupied output returns Busy unchanged. MTU/security startup and errors
    /// match [`Self::new`]; error/cancellation leaves an initially empty slot empty
    /// and releases the client claim. Canceling an admitted ATT exchange closes the
    /// bearer. Only successful startup publishes a client; no owned client crosses
    /// the async return. Drop the output after all borrowed work and before link cleanup.
    pub async fn new_into<'slot>(
        _stack: &Stack<'_, C, P>,
        connection: &Connection<'reference, P>,
        output: &'slot mut Option<Self>,
    ) -> Result<&'slot Self, BleHostError<C::Error>> {
        if output.is_some() {
            return Err(Error::Busy.into());
        }
        let connection = ClientConnection::acquire(connection)?;
        if !connection.is_att_mtu_exchanged() {
            let l2cap = L2capHeader { channel: 4, length: 3 };
            let mut buf = P::allocate().ok_or(Error::OutOfMemory)?;
            let mut w = WriteCursor::new(buf.as_mut());
            w.write_hci(&l2cap)?;
            w.write(att::Att::Client(att::AttClient::Request(att::AttReq::ExchangeMtu {
                mtu: P::MTU as u16 - 4,
            })))?;

            let len = w.len();
            let transaction = AttRequestGuard::begin(&connection, att::ATT_EXCHANGE_MTU_REQ)?;
            connection.send(Pdu::new(buf, len)).await?;

            // Await MTU exchange completion (BT Core Spec requires sequential ATT requests)
            with_timeout(ATT_TRANSACTION_TIMEOUT, async {
                loop {
                    let pdu = connection.next_gatt_client().await.ok_or(Error::Disconnected)?;
                    let Some(&opcode) = pdu.as_ref().first() else {
                        connection.disconnect();
                        return Err(Error::InvalidValue.into());
                    };
                    match opcode {
                        att::ATT_EXCHANGE_MTU_RSP if pdu.as_ref().len() == 3 => {
                            return Ok::<_, BleHostError<C::Error>>(());
                        }
                        att::ATT_ERROR_RSP
                            if validate_error_response(pdu.as_ref(), att::ATT_EXCHANGE_MTU_REQ).is_ok() =>
                        {
                            let code = att::AttErrorCode::new(pdu.as_ref()[4]);
                            if code != att::AttErrorCode::REQUEST_NOT_SUPPORTED {
                                return Err(Error::Att(code).into());
                            }
                            // The fallback completes this connection's once-only exchange.
                            connection.set_att_mtu(crate::connection_manager::DEFAULT_ATT_MTU);
                            return Ok(());
                        }
                        ATT_HANDLE_VALUE_NTF | ATT_HANDLE_VALUE_IND if pdu.as_ref().len() >= 3 => {
                            // No subscriber exists before construction completes. Consume early
                            // values; the host ingress already confirms indications exactly once.
                        }
                        _ => {
                            connection.disconnect();
                            return Err(Error::UnexpectedGattResponse.into());
                        }
                    }
                }
            })
            .await
            .map_err(|_| {
                warn!("[gatt] MTU exchange timeout (30s), disconnecting");
                connection.disconnect();
                BleHostError::BleHost(Error::Timeout)
            })??;
            transaction.finish();
        }

        // Enable encryption with bonded peers before starting GATT operations
        // (BT Core Spec Vol 3, Part C, Section 10.3.2: client "should" enable encryption on reconnection)
        #[cfg(feature = "security")]
        if connection.is_bonded_peer() {
            match connection.try_enable_encryption().await {
                Ok(_) => {}
                Err(Error::Disconnected) => return Err(Error::Disconnected.into()),
                Err(e) => {
                    warn!("[gatt] failed to enable encryption for bonded peer: {:?}", e);
                }
            }
        }

        Ok(output.insert(Self {
            known_services: RefCell::new(heapless::Vec::new()),
            _phantom: PhantomData,
            connection,

            response_channel: Channel::new(),

            notifications: PubSubChannel::new(),
        }))
    }

    /// Discover all primary services in ascending handle order.
    /// Returns discovered primary services in increasing nonoverlapping ranges and remembers them atomically; capacity, malformed response and ATT errors propagate. Cancellation may leave peer transactions completed but does not publish a partial cache.
    pub async fn services(&self) -> Result<Vec<ServiceHandle, MAX_SERVICES>, BleHostError<C::Error>> {
        let mut result = Vec::new();
        self.services_into(&mut result).await?;
        Ok(result)
    }

    /// Discovers primary services into caller-owned `output`, returning unit after
    /// complete discovery and atomic cache merge. Clears `output` when first polled;
    /// errors or cancellation may leave an unpublished ordered prefix there, while
    /// the cache remains unchanged. Rows are bounded by MAX_SERVICES; ATT, malformed
    /// ordering and capacity errors match [`Self::services`]. Canceling an admitted
    /// pending ATT transaction closes its bearer. No service-vector result is copied.
    pub async fn services_into(
        &self,
        output: &mut Vec<ServiceHandle, MAX_SERVICES>,
    ) -> Result<(), BleHostError<C::Error>> {
        output.clear();
        let mut start = 1u16;

        loop {
            let data = att::AttReq::ReadByGroupType {
                start,
                end: u16::MAX,
                group_type: PRIMARY_SERVICE.into(),
            };

            let response = self.request(data).await?;
            let res = Self::response(response.pdu.as_ref())?;
            match res {
                AttRsp::Error { request, handle, code } => {
                    if code == att::AttErrorCode::ATTRIBUTE_NOT_FOUND {
                        break;
                    }
                    return Err(Error::Att(code).into());
                }
                AttRsp::ReadByGroupType { mut it } => {
                    let mut end = start - 1;
                    while let Some(res) = it.next() {
                        let (handle, data) = res?;

                        if handle < start || handle <= end {
                            return Err(Error::UnexpectedGattResponse.into());
                        }

                        let mut r = ReadCursor::new(data);
                        end = r.read()?;
                        if end < handle {
                            return Err(Error::UnexpectedGattResponse.into());
                        }
                        let uuid = Uuid::try_from(r.remaining())?;

                        let svc = ServiceHandle {
                            start: handle,
                            end,
                            uuid,
                        };

                        output.push(svc).map_err(|_| Error::InsufficientSpace)?;
                    }
                    if end < start {
                        return Err(Error::UnexpectedGattResponse.into());
                    }
                    if end == u16::MAX {
                        break;
                    }
                    // GATT 4.4.1: continue after the last group, including when
                    // a response ends at a UUID-width change. Gaps are legal.
                    start = end + 1;
                }
                res => {
                    trace!("[gatt client] response: {:?}", res);
                    return Err(Error::UnexpectedGattResponse.into());
                }
            }
        }

        self.remember_services(output)?;
        Ok(())
    }

    /// Discover primary services associated with a UUID.
    /// Returns primary service ranges matching `uuid` (UUID32 expands to UUID128) and remembers them after complete discovery; capacity, malformed ordering and ATT errors propagate.
    pub async fn services_by_uuid(
        &self,
        uuid: &Uuid,
    ) -> Result<Vec<ServiceHandle, MAX_SERVICES>, BleHostError<C::Error>> {
        let mut start: u16 = 0x0001;
        let mut result = Vec::new();

        let (wire_uuid, wire_len) = uuid.att_bytes();
        loop {
            let data = att::AttReq::FindByTypeValue {
                start_handle: start,
                end_handle: 0xFFFF,
                att_type: PRIMARY_SERVICE.into(),
                att_value: &wire_uuid[..wire_len],
            };

            let response = self.request(data).await?;
            let res = Self::response(response.pdu.as_ref())?;
            match res {
                AttRsp::Error { request, handle, code } => {
                    if code == att::AttErrorCode::ATTRIBUTE_NOT_FOUND {
                        break;
                    }
                    return Err(Error::Att(code).into());
                }
                AttRsp::FindByTypeValue { mut it } => {
                    let mut end = start - 1;
                    while let Some(res) = it.next() {
                        let (handle, e) = res?;
                        if handle < start || handle <= end || e < handle {
                            return Err(Error::UnexpectedGattResponse.into());
                        }
                        end = e;
                        let svc = ServiceHandle {
                            start: handle,
                            end,
                            uuid: *uuid,
                        };
                        result.push(svc.clone()).map_err(|_| Error::InsufficientSpace)?;
                    }
                    if end < start {
                        return Err(Error::UnexpectedGattResponse.into());
                    }
                    if end == 0xFFFF {
                        break;
                    }
                    start = end + 1;
                }
                res => {
                    trace!("[gatt client] response: {:?}", res);
                    return Err(Error::UnexpectedGattResponse.into());
                }
            }
        }

        self.remember_services(&result)?;
        Ok(result)
    }

    /// Returns Ok after remembering previously unknown entries from `services`; preflights capacity to avoid partial cache publication. Internal discovery supplies distinct rows.
    fn remember_services(&self, services: &[ServiceHandle]) -> Result<(), Error> {
        let mut known = self.known_services.borrow_mut();
        let additional = services.iter().filter(|s| !known.contains(s)).count();
        if additional > known.capacity() - known.len() {
            return Err(Error::InsufficientSpace);
        }
        for service in services {
            if !known.contains(service) {
                known.push(service.clone()).map_err(|_| Error::InsufficientSpace)?;
            }
        }
        Ok(())
    }

    /// Returns a decoded 5- or 19-byte `item` only when declaration `handle` follows `previous_value` within `service` and precedes its value handle; malformed lengths/order return InvalidCharacteristicDeclarationData.
    fn checked_declaration(
        service: &ServiceHandle,
        previous_value: u16,
        handle: u16,
        item: &[u8],
    ) -> Result<CharacteristicDeclaration, Error> {
        // GATT 3.3: declaration precedes value, then optional descriptors.
        // Handles need not be consecutive, but definitions must not overlap.
        if !matches!(item.len(), 5 | 19) {
            return Err(Error::InvalidCharacteristicDeclarationData);
        }
        let declaration = CharacteristicDeclaration::try_from(item)?;
        if service.start == 0
            || handle <= service.start
            || handle <= previous_value
            || handle >= service.end
            || declaration.value_handle <= handle
            || declaration.value_handle > service.end
        {
            return Err(Error::InvalidCharacteristicDeclarationData);
        }
        Ok(declaration)
    }

    /// Discover all characteristics in a given service
    /// Returns at most N characteristics in `service`, validating declaration/value order and resolving CCCDs inside each characteristic range; missing CCCD is None, other ATT/capacity errors propagate.
    pub async fn characteristics<const N: usize>(
        &self,
        service: &ServiceHandle,
    ) -> Result<Vec<Characteristic<[u8]>, N>, BleHostError<C::Error>> {
        let mut characteristics: Vec<Characteristic<[u8]>, N> = Vec::new();
        let mut err: Option<BleHostError<C::Error>> = None;

        self.read_by_type(
            service.start,
            service.end,
            &CHARACTERISTIC.into(),
            |declaration_handle, item| {
                let previous = characteristics.last().map_or(service.start, |row| row.handle);
                match Self::checked_declaration(service, previous, declaration_handle, item) {
                    Ok(decl) => {
                        if let Some(previous) = characteristics.last_mut() {
                            previous.end_handle = declaration_handle - 1;
                        }
                        if characteristics
                            .push(Characteristic {
                                handle: decl.value_handle,
                                end_handle: service.end,
                                props: decl.props,
                                cccd_handle: None,
                                uuid: decl.uuid,
                                phantom: PhantomData,
                            })
                            .is_err()
                        {
                            err = Some(Error::InsufficientSpace.into());
                            return ControlFlow::Break(());
                        }
                        ControlFlow::Continue(())
                    }
                    Err(e) => {
                        err = Some(e.into());
                        ControlFlow::Break(())
                    }
                }
            },
        )
        .await?;

        if let Some(e) = err {
            return Err(e);
        }

        for characteristic in &mut characteristics {
            if characteristic.props.has_cccd() && characteristic.handle < characteristic.end_handle {
                characteristic.cccd_handle = match self
                    .get_characteristic_cccd(characteristic.handle + 1, characteristic.end_handle)
                    .await
                {
                    Ok(handle) => Some(handle),
                    Err(BleHostError::BleHost(Error::NotFound)) => None,
                    Err(err) => return Err(err),
                };
            }
        }

        Ok(characteristics)
    }

    /// Discover characteristics in a given service using a UUID.
    /// Returns the first declaration in `service` exactly matching `uuid`, with its bounded descriptor range and required CCCD; NotFound covers absent characteristic/required CCCD. T selects interpretation only.
    pub async fn characteristic_by_uuid<T: AsGatt + ?Sized>(
        &self,
        service: &ServiceHandle,
        uuid: &Uuid,
    ) -> Result<Characteristic<T>, BleHostError<C::Error>> {
        let mut found: Option<(u16, CharacteristicProps)> = None;
        let mut previous_value = service.start;

        trace!(
            "[characteristic_by_uuid] service start={}, end={}, uuid={:?}",
            service.start,
            service.end,
            uuid
        );

        // Iterate through characteristic declarations. When we find the matching UUID,
        // we store (value_handle, props) and continue to find the next declaration
        // to determine end_handle.
        let end = self
            .read_by_type(
                service.start,
                service.end,
                &CHARACTERISTIC.into(),
                |declaration_handle, item| {
                    match Self::checked_declaration(service, previous_value, declaration_handle, item) {
                        Ok(decl) => {
                            previous_value = decl.value_handle;
                            if found.is_some() {
                                // We already found our match; this is the next declaration,
                                // so we can determine end_handle.
                                return ControlFlow::Break(Ok(declaration_handle - 1));
                            }

                            if *uuid == decl.uuid {
                                found = Some((decl.value_handle, decl.props));
                            }

                            if decl.value_handle == 0xFFFF && found.is_some() {
                                return ControlFlow::Break(Ok(service.end));
                            }

                            ControlFlow::Continue(())
                        }
                        Err(e) => ControlFlow::Break(Err(e)),
                    }
                },
            )
            .await?
            .unwrap_or(Ok(service.end))?;

        match found {
            Some((handle, props)) => {
                // If we broke early, the next declaration_handle gives us the end.
                // If we exhausted the range, use service.end.
                let cccd_handle: Option<u16> = if props.has_cccd() {
                    if handle >= end {
                        return Err(Error::NotFound.into());
                    }
                    Some(self.get_characteristic_cccd(handle + 1, end).await?)
                } else {
                    None
                };
                Ok(Characteristic {
                    handle,
                    end_handle: end,
                    cccd_handle,
                    props,
                    uuid: *uuid,
                    phantom: PhantomData,
                })
            }
            None => Err(Error::NotFound.into()),
        }
    }

    /// Returns callback break output or None after inclusive `start`..=`end` exhaustion/ATTRIBUTE_NOT_FOUND. `make_req` builds each range request; `handle_rsp` must advance a widened next handle, otherwise UnexpectedGattResponse. Earlier callbacks remain applied on later failure/cancellation.
    async fn paginated_request<R>(
        &self,
        start: u16,
        end: u16,
        mut make_req: impl FnMut(u16, u16) -> AttReq<'static>,
        mut handle_rsp: impl for<'a> FnMut(AttRsp<'a>) -> Result<ControlFlow<R, u32>, Error>,
    ) -> Result<Option<R>, BleHostError<C::Error>> {
        let mut start_handle = start;
        while start_handle <= end {
            let data = make_req(start_handle, end);
            let response = self.request(data).await?;
            match Self::response(response.pdu.as_ref())? {
                AttRsp::Error { code, .. } if code == att::AttErrorCode::ATTRIBUTE_NOT_FOUND => {
                    return Ok(None);
                }
                AttRsp::Error { code, .. } => return Err(Error::Att(code).into()),
                rsp => match handle_rsp(rsp)? {
                    ControlFlow::Break(val) => return Ok(Some(val)),
                    ControlFlow::Continue(next) => {
                        if next <= u32::from(start_handle) {
                            return Err(Error::UnexpectedGattResponse.into());
                        }
                        if next > u32::from(end) {
                            return Ok(None);
                        }
                        start_handle = next as u16;
                    }
                },
            }
        }
        Ok(None)
    }

    /// Discover descriptors in a handle range, calling `callback` for each discovered handle/UUID pair.
    ///
    /// Returns `Ok(Some(val))` if `callback` returns `ControlFlow::Break(val)`, or `Ok(None)` if
    /// the entire range was iterated without breaking.
    /// Queries inclusive `start`..=`end`; validates nonzero increasing handles before invoking `callback`. Empty/reversed ranges finish without requests; earlier callback effects survive later failures.
    pub async fn find_information<R>(
        &self,
        start: u16,
        end: u16,
        mut callback: impl FnMut(u16, Uuid) -> ControlFlow<R>,
    ) -> Result<Option<R>, BleHostError<C::Error>> {
        // Widen the cursor so a valid final handle of 0xffff can finish normally.
        let mut minimum = u32::from(start);
        self.paginated_request(
            start,
            end,
            |start_handle, end_handle| AttReq::FindInformation {
                start_handle,
                end_handle,
            },
            |rsp| match rsp {
                AttRsp::FindInformation { mut it } => {
                    let mut next_handle = None;
                    while let Some(item) = it.next() {
                        let (handle, uuid) = item?;
                        if u32::from(handle) < minimum || handle == 0 || handle > end {
                            return Err(Error::UnexpectedGattResponse);
                        }
                        minimum = u32::from(handle) + 1;
                        next_handle = Some(minimum);
                        if let ControlFlow::Break(val) = callback(handle, uuid) {
                            return Ok(ControlFlow::Break(val));
                        }
                    }
                    next_handle
                        .map(ControlFlow::Continue)
                        .ok_or(Error::UnexpectedGattResponse)
                }
                _ => Err(Error::UnexpectedGattResponse),
            },
        )
        .await
    }

    /// Returns the first CCCD in inclusive `char_start_handle`..=`char_end_handle`, or NotFound; propagates request/codec failures.
    async fn get_characteristic_cccd(
        &self,
        char_start_handle: u16,
        char_end_handle: u16,
    ) -> Result<u16, BleHostError<C::Error>> {
        self.find_information(char_start_handle, char_end_handle, |handle, uuid| {
            if uuid == CLIENT_CHARACTERISTIC_CONFIGURATION.into() {
                ControlFlow::Break(handle)
            } else {
                ControlFlow::Continue(())
            }
        })
        .await?
        .ok_or(Error::NotFound.into())
    }

    /// Discover all descriptors for a characteristic.
    ///
    /// Returns a list of descriptors found in the handle range belonging to the characteristic.
    /// Searches only the descriptor range of `characteristic`; returns empty for no range and InsufficientSpace when more than N entries are found. Does not validate a cached characteristic against peer identity.
    pub async fn descriptors<T: AsGatt + ?Sized, const N: usize>(
        &self,
        characteristic: &Characteristic<T>,
    ) -> Result<Vec<Descriptor<[u8]>, N>, BleHostError<C::Error>> {
        let end = characteristic.end_handle;
        if characteristic.handle >= end {
            return Ok(Vec::new());
        }
        let start = characteristic.handle + 1;
        let mut result = Vec::new();
        let overflow = self
            .find_information(start, end, |handle, uuid| {
                let desc = Descriptor {
                    handle,
                    uuid,
                    phantom: PhantomData,
                };
                if result.push(desc).is_err() {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .await?;
        if overflow.is_some() {
            return Err(Error::InsufficientSpace.into());
        }
        Ok(result)
    }

    /// Find a specific descriptor by UUID for a characteristic.
    ///
    /// Returns the first descriptor matching the given UUID.
    /// Searches the descriptor range of `characteristic` for exact `uuid`; returns NotFound when absent. DT selects byte interpretation only.
    pub async fn descriptor_by_uuid<T: AsGatt + ?Sized, DT: AsGatt + ?Sized>(
        &self,
        characteristic: &Characteristic<T>,
        uuid: &Uuid,
    ) -> Result<Descriptor<DT>, BleHostError<C::Error>> {
        let end = characteristic.end_handle;
        if characteristic.handle >= end {
            return Err(Error::NotFound.into());
        }
        let start = characteristic.handle + 1;
        self.find_information(start, end, |handle, desc_uuid| {
            if desc_uuid == *uuid {
                ControlFlow::Break(Descriptor {
                    handle,
                    uuid: desc_uuid,
                    phantom: PhantomData,
                })
            } else {
                ControlFlow::Continue(())
            }
        })
        .await?
        .ok_or(Error::NotFound.into())
    }

    /// Read attributes by type in a handle range, calling `callback` for each discovered handle/data pair.
    ///
    /// Paginates automatically using successive ATT ReadByType requests.
    /// Returns `Ok(Some(val))` if `callback` returns `ControlFlow::Break(val)`, or `Ok(None)` if
    /// the entire range was iterated without breaking (i.e. ATTRIBUTE_NOT_FOUND was received).
    /// Queries inclusive `start`..=`end` for `attribute_type`, validating increasing in-range handles before `callback`. Callback bytes expire after the call; prior effects survive later errors/cancellation.
    pub async fn read_by_type<R>(
        &self,
        start: u16,
        end: u16,
        attribute_type: &Uuid,
        mut callback: impl FnMut(u16, &[u8]) -> ControlFlow<R>,
    ) -> Result<Option<R>, BleHostError<C::Error>> {
        // Widen the cursor so a valid final handle of 0xffff can finish normally.
        let mut minimum = u32::from(start);
        self.paginated_request(
            start,
            end,
            |start, end| AttReq::ReadByType {
                start,
                end,
                attribute_type: *attribute_type,
            },
            |rsp| match rsp {
                AttRsp::ReadByType { mut it } => {
                    let mut next_handle = None;
                    while let Some(res) = it.next() {
                        let (handle, data) = res?;
                        if u32::from(handle) < minimum || handle == 0 || handle > end {
                            return Err(Error::UnexpectedGattResponse);
                        }
                        minimum = u32::from(handle) + 1;
                        next_handle = Some(minimum);
                        if let ControlFlow::Break(val) = callback(handle, data) {
                            return Ok(ControlFlow::Break(val));
                        }
                    }
                    next_handle
                        .map(ControlFlow::Continue)
                        .ok_or(Error::UnexpectedGattResponse)
                }
                _ => Err(Error::UnexpectedGattResponse),
            },
        )
        .await
    }

    /// Read a characteristic described by a handle.
    ///
    /// The number of bytes copied into the provided buffer is returned.
    /// Reads `characteristic` into `dest` through read_handle; returns bytes copied, silently limited by destination capacity, with partial bytes preserved on later failure.
    pub async fn read_characteristic<T: AsGatt + ?Sized>(
        &self,
        characteristic: &Characteristic<T>,
        dest: &mut [u8],
    ) -> Result<usize, BleHostError<C::Error>> {
        self.read_handle(characteristic.handle, dest).await
    }

    /// Read a characteristic described by a UUID.
    ///
    /// The number of bytes copied into the provided buffer is returned.
    /// Reads the first matching `uuid` in `service` into `dest`; returns the copied prefix length or NotFound. Does not follow ReadBlob for a longer value.
    pub async fn read_characteristic_by_uuid(
        &self,
        service: &ServiceHandle,
        uuid: &Uuid,
        dest: &mut [u8],
    ) -> Result<usize, BleHostError<C::Error>> {
        let result = self
            .read_by_type(service.start, service.end, uuid, |_handle, data| {
                let to_copy = data.len().min(dest.len());
                dest[..to_copy].copy_from_slice(&data[..to_copy]);
                ControlFlow::Break(to_copy)
            })
            .await?;
        result.ok_or(Error::NotFound.into())
    }

    /// Write to a characteristic described by a handle.
    /// Writes raw `buf` to the value identified by `handle`; returns after the ATT Write response or propagates ATT/transaction/codec errors. No prepared-write splitting is performed.
    pub async fn write_characteristic<T: AsGatt + ?Sized>(
        &self,
        handle: &Characteristic<T>,
        buf: &[u8],
    ) -> Result<(), BleHostError<C::Error>> {
        self.write_handle(handle.handle, buf).await
    }

    /// Write without waiting for a response to a characteristic described by a handle.
    /// Queues raw `buf` for the value identified by `handle`; returns admission status without peer acknowledgment. No prepared-write splitting or typed-value validation is performed.
    pub async fn write_characteristic_without_response<T: AsGatt + ?Sized>(
        &self,
        handle: &Characteristic<T>,
        buf: &[u8],
    ) -> Result<(), BleHostError<C::Error>> {
        let data = att::AttCmd::Write {
            handle: handle.handle,
            data: buf,
        };

        self.command(data).await?;

        Ok(())
    }

    /// Read an attribute by raw handle.
    ///
    /// The number of bytes copied into the provided buffer is returned.
    /// Reads raw attribute `handle` into `dest`, following full-length responses with ReadBlob while space remains. Returns copied bytes; destination truncation is successful and later errors/cancellation leave the copied prefix.
    pub async fn read_handle(&self, handle: u16, dest: &mut [u8]) -> Result<usize, BleHostError<C::Error>> {
        let response = self.request(att::AttReq::Read { handle }).await?;

        match Self::response(response.pdu.as_ref())? {
            AttRsp::Read { data } => {
                if data.len() >= usize::from(self.connection.att_mtu()) {
                    return Err(Error::UnexpectedGattResponse.into());
                }
                let to_copy = data.len().min(dest.len());
                dest[..to_copy].copy_from_slice(&data[..to_copy]);

                let att_mtu = self.connection.att_mtu() as usize;
                if data.len() == att_mtu - 1 && dest.len() > to_copy {
                    let remaining = self
                        .read_handle_blob(handle, to_copy as u16, &mut dest[to_copy..])
                        .await?;
                    Ok(to_copy + remaining)
                } else {
                    Ok(to_copy)
                }
            }
            AttRsp::Error { code, .. } => Err(Error::Att(code).into()),
            _ => Err(Error::UnexpectedGattResponse.into()),
        }
    }

    /// Read an attribute value by raw handle using Read Blob requests.
    ///
    /// The `offset` parameter specifies the starting offset within the attribute value.
    /// Reads raw `handle` starting at byte `offset` into `dest`; returns copied bytes, treating INVALID_OFFSET/ATTRIBUTE_NOT_LONG as end-of-value. Rejects offset overflow before another request; failures preserve prior writes. Even empty `dest` sends one request.
    pub async fn read_handle_blob(
        &self,
        handle: u16,
        offset: u16,
        dest: &mut [u8],
    ) -> Result<usize, BleHostError<C::Error>> {
        let att_mtu = self.connection.att_mtu() as usize;
        let mut pos = 0;
        let mut blob_offset = offset as usize;

        loop {
            let response = self
                .request(att::AttReq::ReadBlob {
                    handle,
                    offset: blob_offset as u16,
                })
                .await?;

            match Self::response(response.pdu.as_ref())? {
                AttRsp::ReadBlob { data } => {
                    if data.len() >= att_mtu {
                        return Err(Error::UnexpectedGattResponse.into());
                    }
                    if data.is_empty() {
                        break;
                    }

                    let blob_read_len = data.len();
                    let len_to_copy = blob_read_len.min(dest.len() - pos);
                    dest[pos..pos + len_to_copy].copy_from_slice(&data[..len_to_copy]);
                    pos += len_to_copy;
                    blob_offset += blob_read_len;

                    if blob_read_len < att_mtu - 1 || pos == dest.len() {
                        break;
                    }
                    if blob_offset > usize::from(u16::MAX) {
                        return Err(Error::InvalidValue.into());
                    }
                }
                AttRsp::Error { code, .. } if code == att::AttErrorCode::INVALID_OFFSET => {
                    break;
                }
                AttRsp::Error { code, .. } if code == att::AttErrorCode::ATTRIBUTE_NOT_LONG => {
                    break;
                }
                AttRsp::Error { code, .. } => {
                    return Err(Error::Att(code).into());
                }
                _ => return Err(Error::UnexpectedGattResponse.into()),
            }
        }
        Ok(pos)
    }

    /// Write an attribute by raw handle.
    /// Writes raw `buf` to `handle`; returns success only for ATT Write response, otherwise ATT/unexpected-response/transaction failure. Pool capacity bounds encoding; no long-write splitting.
    pub async fn write_handle(&self, handle: u16, buf: &[u8]) -> Result<(), BleHostError<C::Error>> {
        let response = self.request(att::AttReq::Write { handle, data: buf }).await?;

        match Self::response(response.pdu.as_ref())? {
            AttRsp::Write => Ok(()),
            AttRsp::Error { code, .. } => Err(Error::Att(code).into()),
            _ => Err(Error::UnexpectedGattResponse.into()),
        }
    }

    /// Write an attribute by raw handle without waiting for a response.
    /// Queues raw `buf` to `handle` as Write Command; returns encoding/admission result without peer acknowledgment. Cancellation drops unqueued bytes.
    pub async fn write_handle_without_response(&self, handle: u16, buf: &[u8]) -> Result<(), BleHostError<C::Error>> {
        self.command(att::AttCmd::Write { handle, data: buf }).await
    }

    /// Read a descriptor value.
    ///
    /// The number of bytes copied into the provided buffer is returned.
    /// Reads `descriptor` into `dest`; returns copied length with the same truncation/partial-write semantics as read_handle.
    pub async fn read_descriptor<T: AsGatt + ?Sized>(
        &self,
        descriptor: &Descriptor<T>,
        dest: &mut [u8],
    ) -> Result<usize, BleHostError<C::Error>> {
        self.read_handle(descriptor.handle, dest).await
    }

    /// Write a descriptor value.
    /// Writes raw `buf` to `descriptor`; returns the ATT write result and does not validate bytes against its generic type.
    pub async fn write_descriptor<T: AsGatt + ?Sized>(
        &self,
        descriptor: &Descriptor<T>,
        buf: &[u8],
    ) -> Result<(), BleHostError<C::Error>> {
        self.write_handle(descriptor.handle, buf).await
    }

    /// Subscribe to indication/notification of a given Characteristic
    ///
    /// A listener is returned, which has a `next()` method
    /// Reserves a listener for `characteristic` before writing CCCD: `indication` selects 0x0002, false selects 0x0001. Returns the listener on write acknowledgment; missing CCCD, subscriber limit or write failure propagates. Cancellation drops the listener and quarantines an unfinished request.
    pub async fn subscribe<T: AsGatt + ?Sized>(
        &self,
        characteristic: &Characteristic<T>,
        indication: bool,
    ) -> Result<NotificationListener<'_, NOTIF_MTU>, BleHostError<C::Error>> {
        let cccd = characteristic.cccd_handle.ok_or(Error::NotSupported)?;
        // The peer can notify immediately after applying CCCD, before its write
        // response. Reserve the subscriber first, and drop it on write failure.
        let listener = self.listen(characteristic)?;
        let properties = u16::to_le_bytes(if indication { 0x02 } else { 0x01 });
        self.write_handle(cccd, &properties).await?;
        Ok(listener)
    }

    /// Listen for notifications/indications on a given Characteristic without writing the CCCD.
    ///
    /// Use this for reconnection scenarios where the server remembers the subscription
    /// (per BLE spec 10.3.2.2, CCCD values persist across disconnections for bonded devices).
    /// Returns a new local listener filtered to `characteristic` or subscriber-capacity error; does not check properties, write CCCD or validate cached handles against peer identity.
    pub fn listen<T: AsGatt + ?Sized>(
        &self,
        characteristic: &Characteristic<T>,
    ) -> Result<NotificationListener<'_, NOTIF_MTU>, BleHostError<C::Error>> {
        match self.notifications.dyn_subscriber() {
            Ok(listener) => Ok(NotificationListener {
                listener,
                handle: Some(characteristic.handle),
            }),
            Err(embassy_sync::pubsub::Error::MaximumSubscribersReached) => {
                Err(Error::GattSubscriberLimitReached.into())
            }
            Err(_) => Err(Error::Other.into()),
        }
    }

    /// Listen for notifications/indications on all characteristics without writing the CCCD.
    ///
    /// Returns a catch-all listener that receives notifications for ALL handles.
    /// Use [`Notification::handle()`] to determine which characteristic the notification is for.
    pub fn listen_all(&self) -> Result<NotificationListener<'_, NOTIF_MTU>, BleHostError<C::Error>> {
        match self.notifications.dyn_subscriber() {
            Ok(listener) => Ok(NotificationListener { listener, handle: None }),
            Err(embassy_sync::pubsub::Error::MaximumSubscribersReached) => {
                Err(Error::GattSubscriberLimitReached.into())
            }
            Err(_) => Err(Error::Other.into()),
        }
    }

    /// Unsubscribe from a given Characteristic
    /// Writes zero to the CCCD of `characteristic`; returns ATT/admission result or NotSupported without a CCCD. Existing local listeners remain allocated.
    pub async fn unsubscribe<T: AsGatt + ?Sized>(
        &self,
        characteristic: &Characteristic<T>,
    ) -> Result<(), BleHostError<C::Error>> {
        self.write_handle(characteristic.cccd_handle.ok_or(Error::NotSupported)?, &[0, 0])
            .await
    }

    /// Confirm an indication that was received.
    /// Returns queue admission for an ATT confirmation. Normal host ingress already confirms indications, so calling this additionally sends a duplicate confirmation.
    pub async fn confirm_indication(&self) -> Result<(), BleHostError<C::Error>> {
        self.send_att_data(Att::Client(AttClient::Confirmation(AttCfm::ConfirmIndication)))
            .await
            .map(|_| ())
    }

    /// Handle a notification or indication that was received.
    /// Parses `data` excluding opcode synchronously; `indication` records its wire kind. Returns after immediate publication, evicting oldest messages when full; malformed/oversized values request disconnect and return an error.
    fn handle_notification_packet(&self, data: &[u8], indication: bool) -> Result<(), BleHostError<C::Error>> {
        let mut r = ReadCursor::new(data);
        let value_handle: u16 = r.read().inspect_err(|_| {
            self.connection.disconnect();
        })?;
        let value_attr = r.remaining();

        let handle = value_handle;

        // TODO: make it `P::MTU` in the future — see the note on `GattClient::notifications`.
        if value_attr.len() > NOTIF_MTU {
            self.connection.disconnect();
            return Err(Error::InvalidValue.into());
        }
        let to_copy = value_attr.len();
        let mut n = Notification {
            handle,
            data: [0u8; NOTIF_MTU],
            len: to_copy,
            indication,
        };
        n.data[..to_copy].copy_from_slice(value_attr);
        self.notifications.immediate_publisher().publish_immediate(n);
        Ok(())
    }

    /// Task which handles GATT rx data (needed for notifications to work)
    /// Runs the sole client receive pump; returns Disconnected or malformed-notification error. Must run concurrently with client requests. Cancellation may drop a dequeued response blocked on its one-entry queue; outstanding transactions then time out.
    pub async fn task(&self) -> Result<(), BleHostError<C::Error>> {
        loop {
            let handle = self.connection.handle();
            let pdu = self.connection.next_gatt_client().await.ok_or(Error::Disconnected)?;
            // handle notifications
            let Some(&opcode) = pdu.as_ref().first() else {
                self.connection.disconnect();
                return Err(Error::InvalidValue.into());
            };
            if matches!(opcode, ATT_HANDLE_VALUE_IND | ATT_HANDLE_VALUE_NTF) {
                let indication = pdu.as_ref()[0] == ATT_HANDLE_VALUE_IND;
                self.handle_notification_packet(&pdu.as_ref()[1..], indication)?;
            } else {
                self.response_channel.send((handle, pdu)).await;
            }
        }
    }

    /// Returns the borrowed server response decoded from `data`; rejects other ATT directions/opcodes and propagates codec errors.
    fn response<'a>(data: &'a [u8]) -> Result<AttRsp<'a>, BleHostError<C::Error>> {
        let att = Att::decode(data)?;
        match att {
            Att::Server(AttServer::Response(rsp)) => Ok(rsp),
            _ => Err(Error::UnexpectedGattResponse.into()),
        }
    }
}

#[cfg(test)]
/// Actual GATT event/encoding/transaction regressions with bounded packet pools and synthetic peers.
mod tests {
    extern crate std;

    use core::task::Poll;

    use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;

    use super::*;
    use crate::att::{self, Att, AttClient, AttReq};
    use crate::attribute::Service;
    use crate::attribute_server::AttributeServer;
    use crate::connection::ConnParams;
    use crate::connection_manager::tests::{setup, ADDR_1};
    use crate::cursor::WriteCursor;
    use crate::pdu::Pdu;
    use crate::prelude::*;
    use crate::Address;

    /// Returns a client claiming `connection` with an empty one-service cache and bounded queues; panics if already claimed. Does not perform MTU exchange.
    pub(super) fn test_client(
        connection: &Connection<'static, DefaultPacketPool>,
    ) -> GattClient<'static, crate::mock_controller::MockController, DefaultPacketPool, 1> {
        GattClient {
            known_services: RefCell::new(heapless::Vec::new()),
            _phantom: PhantomData,
            connection: ClientConnection::acquire(connection).unwrap(),
            response_channel: Channel::new(),
            notifications: PubSubChannel::new(),
        }
    }

    /// Returns an accepted central connection backed by leaked test-only manager storage; panics on fixture admission failure.
    fn test_connection() -> Connection<'static, DefaultPacketPool> {
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        conn
    }

    /// Returns encoded ATT response bytes for `value`, optional blob `offset`, and negotiated `mtu`; asserts one response and no automatic duplicate.
    fn custom_read(value: &[u8], offset: Option<u16>, mtu: u16) -> std::vec::Vec<u8> {
        use core::pin::pin;
        use core::task::{Context, Waker};

        let mut table: AttributeTable<'_, NoopRawMutex, 8> = AttributeTable::new();
        let characteristic = table
            .add_service(Service {
                uuid: Uuid::new_short(0x180a),
            })
            .add_characteristic_ro::<u8, _>(Uuid::new_short(0x2a29), &0)
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let conn = test_connection();
        conn.set_att_mtu(mtu);
        let mut packet = DefaultPacketPool::allocate().unwrap();
        let bytes = packet.as_mut();
        bytes[0] = if offset.is_some() {
            att::ATT_READ_BLOB_REQ
        } else {
            att::ATT_READ_REQ
        };
        bytes[1..3].copy_from_slice(&characteristic.handle.to_le_bytes());
        let len = if let Some(offset) = offset {
            bytes[3..5].copy_from_slice(&offset.to_le_bytes());
            5
        } else {
            3
        };
        let GattEvent::Read(event) = GattEvent::new(GattData::new(Pdu::new(packet, len), conn.clone()), &server) else {
            panic!("read should pass attribute permission checks")
        };
        let mut reply = event.accept_unprocessed(value).unwrap();
        let pdu = reply.pdu.take().expect("read response");
        let bytes = pdu.as_ref();
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) as usize, bytes.len() - 4);
        assert_eq!(&bytes[2..4], &[4, 0]);
        let result = bytes[4..].to_vec();
        drop(reply);
        let mut outbound = pin!(conn.manager().outbound());
        assert!(outbound
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        result
    }

    /// Permission failures preserve the first denied ReadMultiple member or PrepareWrite handle and the original request opcode.
    #[test]
    fn denied_multi_and_prepared_requests_keep_failing_handle() {
        for prepared in [false, true] {
            if prepared && !cfg!(feature = "att-queued-writes") {
                continue;
            }
            for reject in [false, true] {
                let mut storage = [0u8];
                let mut table: AttributeTable<'_, NoopRawMutex, 8> = AttributeTable::new();
                let mut service = table.add_service(Service {
                    uuid: Uuid::new_short(0x180a),
                });
                let readable = service
                    .add_characteristic_ro::<u8, _>(Uuid::new_short(0x2a29), &0)
                    .build();
                let denied = service
                    .add_characteristic(Uuid::new_short(0x2a24), [CharacteristicProp::Write], 0u8, &mut storage)
                    .build();
                let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
                let conn = test_connection();
                let handle = if prepared { readable.handle } else { denied.handle };
                let opcode = if prepared { 0x16 } else { 0x20 };
                let mut packet = DefaultPacketPool::allocate().unwrap();
                let bytes = packet.as_mut();
                bytes[0] = opcode;
                bytes[1..3].copy_from_slice(&readable.handle.to_le_bytes());
                bytes[3..5].copy_from_slice(&if prepared { 0u16 } else { denied.handle }.to_le_bytes());
                let GattEvent::NotAllowed(event) = GattEvent::new(GattData::new(Pdu::new(packet, 5), conn), &server)
                else {
                    panic!("expected permission rejection");
                };
                assert_eq!(event.handle(), handle);
                let expected_code = if reject {
                    AttErrorCode::UNLIKELY_ERROR
                } else if prepared {
                    AttErrorCode::WRITE_NOT_PERMITTED
                } else {
                    AttErrorCode::READ_NOT_PERMITTED
                };
                let mut reply = if reject {
                    event.reject(expected_code)
                } else {
                    event.accept()
                }
                .unwrap();
                let packet = reply.pdu.take().unwrap();
                assert_eq!(
                    &packet.as_ref()[4..8],
                    &[att::ATT_ERROR_RSP, opcode, handle as u8, (handle >> 8) as u8]
                );
                assert_eq!(
                    packet.as_ref()[8],
                    if reject {
                        0x0e
                    } else if prepared {
                        0x03
                    } else {
                        0x02
                    }
                );
            }
        }
    }

    #[test]
    /// Checks ordinary reads, valid suffixes and empty/exact-end blob responses.
    fn custom_read_returns_values_and_valid_blob_suffixes() {
        assert_eq!(custom_read(&[10, 20, 30], None, 23), [att::ATT_READ_RSP, 10, 20, 30]);
        assert_eq!(
            custom_read(&[10, 20, 30], Some(1), 23),
            [att::ATT_READ_BLOB_RSP, 20, 30]
        );
        assert_eq!(custom_read(&[], None, 23), [att::ATT_READ_RSP]);
        assert_eq!(custom_read(&[], Some(0), 23), [att::ATT_READ_BLOB_RSP]);
        assert_eq!(custom_read(&[10, 20, 30], Some(3), 23), [att::ATT_READ_BLOB_RSP]);
    }

    #[test]
    /// Checks both negotiated ATT and physical packet limits without slicing beyond supplied value.
    fn custom_read_respects_negotiated_mtu_and_packet_capacity() {
        let value = [0x42; DefaultPacketPool::MTU + 10];
        let read = custom_read(&value, None, 23);
        assert_eq!(read.len(), 23);
        assert_eq!(read[0], att::ATT_READ_RSP);
        assert_eq!(&read[1..], &[0x42; 22]);
        let blob = custom_read(&value, Some(1), u16::MAX);
        assert_eq!(blob.len(), DefaultPacketPool::MTU - 4);
        assert_eq!(blob[0], att::ATT_READ_BLOB_RSP);
        assert!(blob[1..].iter().all(|byte| *byte == 0x42));
    }

    #[test]
    /// Checks oversized blob offsets produce one correctly addressed ATT error, including empty values.
    fn custom_read_rejects_excess_offset_without_panicking() {
        for offset in [4, u16::MAX] {
            // The table's first value handle is 3; the error must name it and
            // the actual Blob request, without an automatic second response.
            assert_eq!(custom_read(&[10, 20, 30], Some(offset), 23), [0x01, 0x0c, 3, 0, 0x07]);
        }
        assert_eq!(custom_read(&[], Some(1), 23), [0x01, 0x0c, 3, 0, 0x07]);
    }

    #[test]
    /// Exercises permission/application rejection and automatic drop: commands stay silent, requests receive exactly one error, storage stays unchanged.
    fn rejected_write_commands_are_silent_and_requests_keep_their_error() {
        use core::pin::pin;
        use core::task::{Context, Waker};

        for command in [true, false] {
            for permission_denied in [true, false] {
                for automatic_drop in [true, false] {
                    if automatic_drop && !permission_denied {
                        continue;
                    }
                    let mut storage = [0x11];
                    let mut table: AttributeTable<'_, NoopRawMutex, 8> = AttributeTable::new();
                    let props = if permission_denied {
                        [CharacteristicProp::Read, CharacteristicProp::Read]
                    } else {
                        [CharacteristicProp::Write, CharacteristicProp::WriteWithoutResponse]
                    };
                    let characteristic = table
                        .add_service(Service {
                            uuid: Uuid::new_short(0x180a),
                        })
                        .add_characteristic(Uuid::new_short(0x2a29), props, 0x11u8, &mut storage)
                        .build();
                    let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
                    let conn = test_connection();
                    let (mut packet, len) = build_write_cmd_pdu(characteristic.handle, &[0x22]);
                    if !command {
                        packet.as_mut()[0] = att::ATT_WRITE_REQ;
                    }
                    let event = GattEvent::new(GattData::new(Pdu::new(packet, len), conn.clone()), &server);
                    if permission_denied {
                        assert!(matches!(event, GattEvent::NotAllowed(_)));
                    } else {
                        assert!(matches!(event, GattEvent::Write(_)));
                    }
                    if automatic_drop {
                        drop(event);
                    } else if permission_denied {
                        drop(event.accept().unwrap());
                    } else {
                        drop(event.reject(AttErrorCode::WRITE_NOT_PERMITTED).unwrap());
                    }
                    let mut outbound = pin!(conn.manager().outbound());
                    let result = outbound.as_mut().poll(&mut Context::from_waker(Waker::noop()));
                    if command {
                        assert!(
                            result.is_pending(),
                            "rejected Write Command must not receive a response"
                        );
                    } else {
                        let Poll::Ready((_, packet)) = result else {
                            panic!("Write Request error response")
                        };
                        assert_eq!(&packet.as_ref()[4..], &[0x01, 0x12, 3, 0, 0x03]);
                        let mut next = pin!(conn.manager().outbound());
                        assert!(next.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
                    }
                    assert_eq!(server.table().get(&characteristic), Ok(0x11));
                }
            }
        }
    }

    #[test]
    /// Confirms rejecting an ATT confirmation never constructs an Error Response.
    fn rejected_confirmation_does_not_create_a_response() {
        let conn = test_connection();
        let mut packet = DefaultPacketPool::allocate().unwrap();
        packet.as_mut()[0] = att::ATT_HANDLE_VALUE_CFM;
        let mut reply = process_reject(&Pdu::new(packet, 1), &conn, AttErrorCode::REQUEST_NOT_SUPPORTED, None).unwrap();
        let response = reply.pdu.take();
        assert!(response.is_none(), "only requests can receive an Error Response");
    }

    #[test]
    /// Publishes a value before the CCCD write response and verifies the pre-reserved listener retains it.
    fn subscription_captures_notification_before_cccd_response() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::{Context, Waker};

        let conn = test_connection();
        let client = test_client(&conn);
        let characteristic = Characteristic::<u8> {
            cccd_handle: Some(2),
            handle: 1,
            end_handle: 2,
            props: [CharacteristicProp::Notify].into(),
            uuid: Uuid::new_short(42),
            phantom: PhantomData,
        };
        let mut subscribe = pin!(client.subscribe(&characteristic, false));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(subscribe.as_mut().poll(&mut cx).is_pending());
        client.handle_notification_packet(&[1, 0, 42], false).unwrap();
        let mut packet = DefaultPacketPool::allocate().unwrap();
        packet.as_mut()[0] = att::ATT_WRITE_RSP;
        client
            .response_channel
            .try_send((conn.handle(), Pdu::new(packet, 1)))
            .unwrap();
        let Poll::Ready(Ok(mut listener)) = subscribe.as_mut().poll(&mut cx) else {
            panic!("CCCD response")
        };
        let mut next = pin!(listener.next_checked());
        let Poll::Ready(Ok(value)) = next.as_mut().poll(&mut cx) else {
            panic!("early notification lost")
        };
        assert_eq!(value.as_ref(), &[42]);
    }

    #[test]
    /// Overfills pubsub to verify lag reporting, then submits a truncated value and verifies link quarantine.
    fn notification_lag_is_visible_and_malformed_values_disconnect() {
        use embassy_futures::block_on;
        let conn = test_connection();
        let client = test_client(&conn);
        let mut listener = client.listen_all().unwrap();
        for _ in 0..crate::config::GATT_CLIENT_NOTIFICATION_QUEUE_SIZE + 1 {
            client.handle_notification_packet(&[1, 0, 42], false).unwrap();
        }
        assert!(block_on(listener.next_checked()).is_err());
        assert!(client.handle_notification_packet(&[1], false).is_err());
        assert!(block_on(conn.next_gatt_client()).is_none());
    }

    #[test]
    /// Empty and maximum notification/indication payloads publish exact bytes; oversized/missing handles publish nothing and close the bearer.
    fn notification_payload_boundaries_preserve_publication() {
        use core::future::Future;
        use core::pin::pin;
        use core::task::{Context, Waker};
        use embassy_futures::block_on;
        for indication in [false, true] {
            let conn = test_connection();
            let client = test_client(&conn);
            let mut listener = client.listen_all().unwrap();
            for length in [0, NOTIF_MTU] {
                let mut bytes = std::vec![0xA5; length + 2];
                bytes[..2].copy_from_slice(&0x1234u16.to_le_bytes());
                client.handle_notification_packet(&bytes, indication).unwrap();
                let value = block_on(listener.next_checked()).unwrap();
                assert_eq!(value.handle(), 0x1234);
                assert_eq!(value.is_indication(), indication);
                assert_eq!(value.as_ref(), &bytes[2..]);
                assert!(value.data[length..].iter().all(|&byte| byte == 0));
            }
            assert!(conn.is_connected());
        }
        for bytes in [std::vec![], std::vec![0xA5; NOTIF_MTU + 3]] {
            let conn = test_connection();
            let client = test_client(&conn);
            let mut listener = client.listen_all().unwrap();
            assert!(client.handle_notification_packet(&bytes, false).is_err());
            assert!(!conn.is_connected());
            assert!(pin!(listener.next_checked())
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        }
    }

    #[test]
    /// Exercises actual constructor exchange against an empty inbound PDU and expects InvalidValue.
    fn mtu_exchange_rejects_empty_packet_without_panicking() {
        use embassy_futures::block_on;
        let mut resources = crate::HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(crate::mock_controller::MockController::new(), &mut resources).build();
        let mgr = &stack.host_state.connections;
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        mgr.post_gatt_client(conn.handle(), Pdu::new(DefaultPacketPool::allocate().unwrap(), 0))
            .unwrap();
        assert!(matches!(
            block_on(GattClient::<_, _, 1>::new(&stack, &conn)),
            Err(BleHostError::BleHost(Error::InvalidValue))
        ));
    }

    /// Build a ReadByType ATT request PDU (ATT payload only, no L2CAP header).
    /// Returns packet and initialized length for inclusive `start`..=`end` and `uuid`; panics on pool/codec failure in the fixture.
    fn build_read_by_type_pdu(start: u16, end: u16, uuid: &Uuid) -> (<DefaultPacketPool as PacketPool>::Packet, usize) {
        let att = Att::Client(AttClient::Request(AttReq::ReadByType {
            start,
            end,
            attribute_type: *uuid,
        }));

        let mut packet = DefaultPacketPool::allocate().unwrap();
        let mut w = WriteCursor::new(packet.as_mut());
        w.write(att).unwrap();
        let len = w.len();
        (packet, len)
    }

    /// Build a Write Command ATT PDU (ATT payload only, no L2CAP header).
    /// Returns packet and initialized length encoding Write Command to `handle` with `data`; panics on fixture allocation/codec failure.
    fn build_write_cmd_pdu(handle: u16, data: &[u8]) -> (<DefaultPacketPool as PacketPool>::Packet, usize) {
        let att = Att::Client(AttClient::Command(AttCmd::Write { handle, data }));

        let mut packet = DefaultPacketPool::allocate().unwrap();
        let mut w = WriteCursor::new(packet.as_mut());
        w.write(att).unwrap();
        let len = w.len();
        (packet, len)
    }

    /// Regression test: process_accept must not produce ReadByType responses
    /// with partial entries when the ATT MTU is smaller than the packet pool
    /// buffer.
    ///
    /// Before the fix, process_accept wrote ATT responses into the full
    /// packet-pool buffer (P::MTU - 4 = 247 bytes) and then post-hoc
    /// truncated to the negotiated ATT MTU. For 128-bit UUID characteristic
    /// declarations, each ReadByType entry is 21 bytes. When the ATT MTU
    /// doesn't align to entry boundaries, truncation splits an entry in half,
    /// producing a malformed PDU.
    ///
    /// 9 characteristics is the minimum to trigger at ATT MTU 185:
    ///   floor((185 - 2) / 21) = 8 entries fit, so 9 overflows.
    #[test]
    fn test_process_accept_read_by_type_no_partial_entries() {
        let _ = env_logger::try_init();

        /// Fixed fixture attribute capacity, including declarations and values.
        const MAX_ATTRIBUTES: usize = 64;
        /// Fixture client-table/link capacity.
        const CONNECTIONS_MAX: usize = 3;
        /// Nine wide declarations force pagination at the fixture ATT MTU.
        const NUM_CHARACTERISTICS: u8 = 9;
        /// Negotiated ATT bytes per response in this pagination fixture.
        const ATT_MTU: u16 = 185;

        // Each ReadByType entry for a 128-bit UUID declaration:
        //   2 (handle) + 1 (props) + 2 (value handle) + 16 (UUID) = 21 bytes
        /// Bytes per handle plus 128-bit characteristic declaration entry.
        const ENTRY_SIZE: usize = 21;
        /// ATT response opcode and entry-width byte count.
        const RESPONSE_HEADER_SIZE: usize = 2;

        // Characteristic declaration UUID (0x2803)
        let char_decl_uuid = Uuid::new_short(0x2803);

        // Create attribute table with 9 characteristics (128-bit UUIDs)
        let mut table: AttributeTable<'_, NoopRawMutex, MAX_ATTRIBUTES> = AttributeTable::new();
        {
            let mut svc = table.add_service(Service {
                uuid: Uuid::new_long([0x32; 16]),
            });
            for i in 0..NUM_CHARACTERISTICS {
                let mut uuid_bytes = [0x32u8; 16];
                uuid_bytes[0] = i;
                let _char = svc
                    .add_characteristic_ro::<[u8; 2], _>(Uuid::new_long(uuid_bytes), &[0, 0])
                    .build();
            }
        }

        let server = AttributeServer::<_, DefaultPacketPool, MAX_ATTRIBUTES, CONNECTIONS_MAX>::new(table);

        // Set up a connection with ATT MTU = 185 (typical iOS value)
        let mgr = setup();
        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());
        unwrap!(mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };
        conn.set_att_mtu(ATT_MTU);

        // Walk through characteristic discovery via process_accept, just as a
        // real BLE client would.
        let mut start: u16 = 1;
        let mut total_chars_found: usize = 0;

        loop {
            let (packet, len) = build_read_by_type_pdu(start, u16::MAX, &char_decl_uuid);
            let pdu = Pdu::new(packet, len);
            let reply = process_accept::<DefaultPacketPool>(&pdu, &conn, &server).unwrap();

            let att_bytes = reply
                .att_payload()
                .expect("process_accept should produce a response PDU");

            if att_bytes[0] == att::ATT_ERROR_RSP {
                break;
            }

            assert_eq!(att_bytes[0], att::ATT_READ_BY_TYPE_RSP);
            let entry_len = att_bytes[1] as usize;
            assert_eq!(entry_len, ENTRY_SIZE);

            let payload = &att_bytes[RESPONSE_HEADER_SIZE..];

            // The payload must be an exact multiple of the entry size.
            // Before the fix, this assertion failed: 183 % 21 = 15.
            assert_eq!(
                payload.len() % entry_len,
                0,
                "ReadByType payload length {} is not a multiple of entry size {} — partial entry detected (ATT MTU \
                 truncation bug)",
                payload.len(),
                entry_len,
            );

            let num_entries = payload.len() / entry_len;
            assert!(num_entries > 0);
            total_chars_found += num_entries;

            let last_entry = &payload[(num_entries - 1) * entry_len..];
            let last_handle = u16::from_le_bytes([last_entry[0], last_entry[1]]);
            start = last_handle + 1;

            // Forget the reply without trying to send (no outbound queue in test)
            core::mem::forget(reply);
        }

        assert_eq!(
            total_chars_found, NUM_CHARACTERISTICS as usize,
            "should discover all {} characteristics",
            NUM_CHARACTERISTICS,
        );
    }

    #[test]
    /// Checks accepted Write Command updates local storage and surfaces application bytes without generating an ATT response.
    fn test_write_command_surfaces_gatt_write_event_without_att_response() {
        let _ = env_logger::try_init();

        /// Fixed fixture attribute capacity, including declarations and values.
        const MAX_ATTRIBUTES: usize = 16;
        /// Fixture client-table/link capacity.
        const CONNECTIONS_MAX: usize = 3;
        let mut table: AttributeTable<'_, NoopRawMutex, MAX_ATTRIBUTES> = AttributeTable::new();
        let mut storage = [0u8; 1];
        let characteristic: Characteristic<u8> = table
            .add_service(Service {
                uuid: Uuid::new_long([0x44; 16]),
            })
            .add_characteristic(
                Uuid::new_long([0x45; 16]),
                [CharacteristicProp::Read, CharacteristicProp::WriteWithoutResponse],
                0u8,
                &mut storage[..],
            )
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, MAX_ATTRIBUTES, CONNECTIONS_MAX>::new(table);

        let mgr = setup();
        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());
        unwrap!(mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        let payload = [0x2A];
        let (packet, len) = build_write_cmd_pdu(characteristic.handle, &payload);
        let pdu = Pdu::new(packet, len);

        let event = GattEvent::new(GattData::new(pdu, conn.clone()), &server);
        match event {
            GattEvent::Write(write) => {
                assert_eq!(write.handle(), characteristic.handle);
                write.with_data(|_, data| assert_eq!(data, &payload));
                let reply = write.accept().unwrap();
                assert!(
                    reply.att_payload().is_none(),
                    "write command must not generate an ATT response"
                );
                core::mem::forget(reply);
            }
            _ => panic!("expected write event for write command"),
        }

        let stored: u8 = server.table().get(&characteristic).unwrap();
        assert_eq!(stored, payload[0]);
    }

    /// Regression test: a *rejected* Execute Write must empty the prepare queue.
    ///
    /// `process_accept` clears the queue via the attribute server, and
    /// `accept_unprocessed` clears it explicitly, but `process_reject` did not.
    /// A leftover queue makes the *next* client's Prepare Write fail with
    /// `PREPARE_QUEUE_FULL` on every other rejected long write.
    #[cfg(feature = "att-queued-writes")]
    #[test]
    fn test_execute_write_reject_clears_prepare_queue() {
        let _ = env_logger::try_init();

        /// Fixed fixture attribute capacity, including declarations and values.
        const MAX_ATTRIBUTES: usize = 16;
        /// Fixture client-table/link capacity.
        const CONNECTIONS_MAX: usize = 3;
        let mut table: AttributeTable<'_, NoopRawMutex, MAX_ATTRIBUTES> = AttributeTable::new();
        let mut storage = [0u8; 1];
        let characteristic: Characteristic<u8> = table
            .add_service(Service {
                uuid: Uuid::new_long([0x44; 16]),
            })
            .add_characteristic(
                Uuid::new_long([0x45; 16]),
                [CharacteristicProp::Read, CharacteristicProp::Write],
                0u8,
                &mut storage[..],
            )
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, MAX_ATTRIBUTES, CONNECTIONS_MAX>::new(table);

        let mgr = setup();
        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());
        unwrap!(mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        // Populate the prepare queue, then reject the Execute Write that flushes it.
        conn.prepare_write(characteristic.handle, 0, &[1, 2, 3, 4]).unwrap();
        assert_ne!(
            conn.with_prepare_write(|pw| pw.handle),
            0,
            "prepare queue should be populated after a Prepare Write",
        );

        // Build the Execute Write request PDU by hand (opcode + flags): the ATT
        // encoder does not implement client-request encoding.
        let mut packet = DefaultPacketPool::allocate().unwrap();
        packet.as_mut()[0] = att::ATT_EXECUTE_WRITE_REQ;
        packet.as_mut()[1] = 0x01; // flags = execute (not cancel)
        let pdu = Pdu::new(packet, 2);

        let event = GattEvent::new(GattData::new(pdu, conn.clone()), &server);
        let GattEvent::Write(write) = event else {
            panic!("expected write event for execute write with a populated queue");
        };
        let reply = write.reject(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH).unwrap();
        core::mem::forget(reply);

        // The fix: a rejected Execute Write empties the prepare queue, so the
        // next Prepare Write starts fresh instead of hitting PREPARE_QUEUE_FULL.
        assert_eq!(
            conn.with_prepare_write(|pw| pw.handle),
            0,
            "rejected Execute Write must clear the prepare queue",
        );
    }
}

#[cfg(test)]
#[path = "gatt_discovery_tests.rs"]
mod discovery_tests;
