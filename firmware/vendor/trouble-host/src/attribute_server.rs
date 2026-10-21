//! Bounded GATT attribute dispatch and per-peer client values.
//! Discovery follows [Core Vol 3 Part G, section 2.5](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html);
//! wire procedures follow [Part F, section 3.4](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html).

use core::marker::PhantomData;

use embassy_sync::blocking_mutex::raw::RawMutex;

use self::client_att_table::ClientAttTables;
pub use self::client_att_table::{ClientAttTable, ClientAttTableBuilder, ClientAttTableView};
use crate::att::{self, AttClient, AttCmd, AttErrorCode, AttReq};
use crate::attribute::{Attribute, AttributeData, AttributeTable, CCCD};
use crate::connection::Connection;
use crate::cursor::WriteCursor;
#[cfg(feature = "gatt")]
use crate::types::gatt_traits::AsGatt;
use crate::types::uuid::Uuid;
use crate::{codec, Error, Identity, PacketPool};

mod client_att_table;
#[cfg(all(test, feature = "gatt", feature = "default-packet-pool"))]
#[path = "attribute_notify_tests.rs"]
mod notify_tests;

/// Returns the canonical Bluetooth-base128 bytes for `uuid` when comparing ATT type identifiers.
/// Keeps the general UUID type's representation-sensitive equality unchanged.
fn canonical_att_uuid(uuid: &Uuid) -> [u8; 16] {
    match uuid {
        Uuid::Uuid16(bytes) => bt_hci::uuid::BluetoothUuid128::base()
            .set_initial_group(u32::from(u16::from_le_bytes(*bytes)))
            .to_le_bytes(),
        _ => uuid.att_bytes().0,
    }
}

#[derive(Default)]
/// Per-slot identity and liveness; bonded peers retain values while disconnected.
struct Client {
    /// Address and optional IRK used for retained-value lookup.
    identity: Identity,
    /// True while the peer occupies a live connection; false permits slot eviction.
    is_connected: bool,
}

impl Client {
    /// Replaces the stored `identity` without changing liveness or any associated value bytes.
    fn set_identity(&mut self, identity: Identity) {
        self.identity = identity;
    }
}

/// Bounded GATT attribute server with shared metadata/value storage and per-peer client-specific values. ATT byte
/// decoding and bearer transport are handled separately.
pub struct AttributeServer<'values, M: RawMutex, P: PacketPool, const ATT_MAX: usize, const CONN_MAX: usize> {
    /// Owned shared attribute layout and locally accessible values.
    att_table: AttributeTable<'values, M, ATT_MAX>,
    /// Per-identity values, including CCCDs retained for bonded disconnected peers.
    client_att_tables: ClientAttTables<M, CONN_MAX>,
    /// Packet-pool type association; no packet allocation is retained here.
    _p: PhantomData<P>,
}

/// Internal object-safe dispatch used by connection and GATT code.
pub(crate) mod sealed {
    use super::*;

    /// Erased server operations; local get/set bypass peer permissions, while protocol reads/writes check them.
    pub trait DynamicAttributeServer<P: PacketPool> {
        /// Admits `connection`'s identity to per-client storage. Returns success preserving retained values, or
        /// ConnectionLimitReached when all slots are live.
        fn connect(&self, connection: &Connection<'_, P>) -> Result<(), Error>;
        /// Releases `connection`'s client slot; retains values for bonded peers and clears unbonded peers.
        fn disconnect(&self, connection: &Connection<'_, P>);
        /// Processes decoded `packet` for `connection` into response buffer `rx`. Returns response byte count in `Some`,
        /// `None` for no response, or a codec-derived error; writes may already have taken effect.
        fn process(
            &self,
            connection: &Connection<'_, P>,
            packet: &AttClient,
            rx: &mut [u8],
        ) -> Result<Option<usize>, Error>;
        /// Returns whether `connection`'s two-byte `cccd_handle` value enables notifications; missing/malformed values
        /// return false. Does not check value permissions.
        fn should_notify(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool;
        /// Returns whether `connection`'s two-byte `cccd_handle` value enables indications; missing/malformed values
        /// return false. Does not check value permissions.
        fn should_indicate(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool;
        /// Locally writes `input` to `att_handle`, selecting client values by `connection`. Returns
        /// missing/length/storage errors; bypasses peer permissions and requires exact fixed-value length.
        fn set(&self, connection: &Connection<'_, P>, att_handle: u16, input: &[u8]) -> Result<(), Error>;
        /// Locally copies `att_handle` into `buf`, selecting client values by `connection`. Returns bytes copied or
        /// missing/storage errors; short buffers truncate and peer permissions are bypassed.
        fn get(&self, connection: &Connection<'_, P>, att_handle: u16, buf: &mut [u8]) -> Result<usize, Error>;
        /// Updates retained peer lookup to `identity` without changing values/liveness. Returns NotFound when no stored
        /// identity matches.
        fn update_identity(&self, identity: Identity) -> Result<(), Error>;

        /// Checks peer read access to `handle` for `connection`. Returns permission/security errors or INVALID_HANDLE for
        /// a missing attribute or unavailable connection state.
        fn can_read(&self, connection: &Connection<'_, P>, handle: u16) -> Result<(), AttErrorCode>;
        /// Checks peer write access to `handle` for `connection`. Returns permission/security errors or INVALID_HANDLE
        /// for a missing attribute or unavailable connection state.
        fn can_write(&self, connection: &Connection<'_, P>, handle: u16) -> Result<(), AttErrorCode>;
    }
}

/// Type-erased server marker backed by the sealed dispatch contract; implementations are supplied by
/// AttributeServer.
pub trait DynamicAttributeServer<P: PacketPool>: sealed::DynamicAttributeServer<P> {}

impl<M: RawMutex, P: PacketPool, const ATT_MAX: usize, const CONN_MAX: usize> DynamicAttributeServer<P>
    for AttributeServer<'_, M, P, ATT_MAX, CONN_MAX>
{
}
impl<M: RawMutex, P: PacketPool, const ATT_MAX: usize, const CONN_MAX: usize> sealed::DynamicAttributeServer<P>
    for AttributeServer<'_, M, P, ATT_MAX, CONN_MAX>
{
    /// Admits `connection`'s identity to per-client storage. Returns success preserving retained values, or
    /// ConnectionLimitReached when all slots are live.
    fn connect(&self, connection: &Connection<'_, P>) -> Result<(), Error> {
        AttributeServer::connect(self, connection)
    }

    /// Releases `connection`'s client slot; retains values for bonded peers and clears unbonded peers.
    fn disconnect(&self, connection: &Connection<'_, P>) {
        #[cfg(feature = "security")]
        let bonded = connection.is_bonded_peer();
        #[cfg(not(feature = "security"))]
        let bonded = false;

        self.client_att_tables.disconnect(&connection.peer_identity(), bonded);
    }

    /// Processes decoded `packet` for `connection` into response buffer `rx`. Returns response byte count in `Some`,
    /// `None` for no response, or a codec-derived error; writes may already have taken effect.
    fn process(
        &self,
        connection: &Connection<'_, P>,
        packet: &AttClient,
        rx: &mut [u8],
    ) -> Result<Option<usize>, Error> {
        let res = AttributeServer::process(self, connection, packet, rx)?;
        Ok(res)
    }

    /// Returns whether `connection`'s two-byte `cccd_handle` value enables notifications; missing/malformed values
    /// return false. Does not check value permissions.
    fn should_notify(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool {
        AttributeServer::should_notify(self, connection, cccd_handle)
    }
    /// Returns whether `connection`'s two-byte `cccd_handle` value enables indications; missing/malformed values
    /// return false. Does not check value permissions.
    fn should_indicate(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool {
        AttributeServer::should_indicate(self, connection, cccd_handle)
    }

    /// Locally copies `att_handle` into `buf`, selecting client values by `connection`. Returns bytes copied or
    /// missing/storage errors; short buffers truncate and peer permissions are bypassed.
    fn get(&self, connection: &Connection<'_, P>, att_handle: u16, buf: &mut [u8]) -> Result<usize, Error> {
        self.att_table
            .with_attribute(att_handle, |att| {
                if matches!(att.data, AttributeData::ClientSpecific { .. }) {
                    self.client_att_tables
                        .read(&connection.peer_identity(), att_handle, 0, buf)
                } else {
                    att.read(0, buf)
                }
                .map_err(Into::into)
            })
            .unwrap_or(Err(Error::NotFound))
    }

    /// Locally writes `input` to `att_handle`, selecting client values by `connection`. Returns
    /// missing/length/storage errors; bypasses peer permissions and requires exact fixed-value length.
    fn set(&self, connection: &Connection<'_, P>, att_handle: u16, input: &[u8]) -> Result<(), Error> {
        self.att_table
            .with_attribute_mut(att_handle, |att| {
                if !att.data.is_variable_len() && att.data.capacity() != input.len() {
                    Err(Error::UnexpectedDataLength {
                        expected: att.data.capacity(),
                        actual: input.len(),
                    })
                } else if matches!(att.data, AttributeData::ClientSpecific { .. }) {
                    self.client_att_tables
                        .write(&connection.peer_identity(), att_handle, 0, input)
                        .map_err(Into::into)
                } else {
                    att.write(0, input).map_err(Into::into)
                }
            })
            .unwrap_or(Err(Error::NotFound))
    }

    /// Updates retained peer lookup to `identity` without changing values/liveness. Returns NotFound when no stored
    /// identity matches.
    fn update_identity(&self, identity: Identity) -> Result<(), Error> {
        self.client_att_tables.update_identity(identity)
    }

    /// Checks peer read access to `handle` for `connection`. Returns permission/security errors or INVALID_HANDLE for
    /// a missing attribute or unavailable connection state.
    fn can_read(&self, connection: &Connection<'_, P>, handle: u16) -> Result<(), AttErrorCode> {
        self.att_table
            .with_attribute(handle, |att| self.can_read(connection, att))
            .unwrap_or(Err(AttErrorCode::INVALID_HANDLE))
    }

    /// Checks peer write access to `handle` for `connection`. Returns permission/security errors or INVALID_HANDLE
    /// for a missing attribute or unavailable connection state.
    fn can_write(&self, connection: &Connection<'_, P>, handle: u16) -> Result<(), AttErrorCode> {
        self.att_table
            .with_attribute(handle, |att| self.can_write(connection, att))
            .unwrap_or(Err(AttErrorCode::INVALID_HANDLE))
    }
}

impl<'values, M: RawMutex, P: PacketPool, const ATT_MAX: usize, const CONN_MAX: usize>
    AttributeServer<'values, M, P, ATT_MAX, CONN_MAX>
{
    /// Returns a server owning `att_table`, with a zeroed per-client layout in each of CONN_MAX slots. Panics if
    /// client-specific entries exceed the serialized storage/handle/capacity limits.
    pub fn new(att_table: AttributeTable<'values, M, ATT_MAX>) -> AttributeServer<'values, M, P, ATT_MAX, CONN_MAX> {
        let cccd_tables = ClientAttTables::new(&att_table);
        AttributeServer {
            att_table,
            client_att_tables: cccd_tables,
            _p: PhantomData,
        }
    }

    /// Reads `handle` for `connection` after permission checks, copying from byte `offset` into `data`. Returns
    /// copied bytes (possibly truncated or zero at end), ATTRIBUTE_NOT_FOUND for a missing attribute, or
    /// permission/offset/client-value errors.
    pub fn read(
        &self,
        connection: &Connection<'_, P>,
        handle: u16,
        offset: usize,
        data: &mut [u8],
    ) -> Result<usize, AttErrorCode> {
        self.att_table
            .with_attribute(handle, |att| {
                self.read_attribute_data(connection, handle, offset, att, data)
            })
            .unwrap_or(Err(AttErrorCode::ATTRIBUTE_NOT_FOUND))
    }

    /// Admits `connection`'s peer identity, retaining any matching bonded values. Returns ConnectionLimitReached if
    /// no matching, vacant or disconnected slot exists.
    pub(crate) fn connect(&self, connection: &Connection<'_, P>) -> Result<(), Error> {
        self.client_att_tables.connect(&connection.peer_identity())
    }

    /// Returns the notification bit of `connection`'s `cccd_handle`; absent peer/handle or non-two-byte values return
    /// false. Value security permissions are checked separately when sending.
    pub(crate) fn should_notify(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool {
        self.client_att_tables
            .with_value(&connection.peer_identity(), cccd_handle, |value| {
                if let Ok(value) = value.try_into() {
                    CCCD(u16::from_le_bytes(value)).should_notify()
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Returns the indication bit of `connection`'s `cccd_handle`; absent peer/handle or non-two-byte values return
    /// false. Does not itself authorize reading the associated value.
    pub(crate) fn should_indicate(&self, connection: &Connection<'_, P>, cccd_handle: u16) -> bool {
        self.client_att_tables
            .with_value(&connection.peer_identity(), cccd_handle, |value| {
                if let Ok(value) = value.try_into() {
                    CCCD(u16::from_le_bytes(value)).should_indicate()
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Checks read permissions of `att` against `connection` security. Returns the permission error, or
    /// INVALID_HANDLE if connection security is unavailable; missing legacy key length is treated as zero bytes.
    fn can_read(&self, connection: &Connection<'_, P>, att: &Attribute<'_>) -> Result<(), AttErrorCode> {
        match connection.security_level() {
            Ok(level) => att.permissions().can_read(
                level,
                #[cfg(feature = "legacy-pairing")]
                connection.encryption_key_len().unwrap_or(0),
            ),
            Err(_) => Err(AttErrorCode::INVALID_HANDLE),
        }
    }

    /// Checks write permissions of `att` against `connection` security. Returns the permission error, or
    /// INVALID_HANDLE if connection security is unavailable; missing legacy key length is treated as zero bytes.
    fn can_write(&self, connection: &Connection<'_, P>, att: &Attribute<'_>) -> Result<(), AttErrorCode> {
        match connection.security_level() {
            Ok(level) => att.permissions().can_write(
                level,
                #[cfg(feature = "legacy-pairing")]
                connection.encryption_key_len().unwrap_or(0),
            ),
            Err(_) => Err(AttErrorCode::INVALID_HANDLE),
        }
    }

    /// Reads `att` at `handle` for `connection`, from byte `offset` into `data`, after authorization. Returns copied
    /// bytes or permission/offset/missing-client errors; client-specific data comes from the peer table, all other
    /// data from the supplied attribute.
    fn read_attribute_data(
        &self,
        connection: &Connection<'_, P>,
        handle: u16,
        offset: usize,
        att: &Attribute<'values>,
        data: &mut [u8],
    ) -> Result<usize, AttErrorCode> {
        self.can_read(connection, att)?;
        if matches!(att.data, AttributeData::ClientSpecific { .. }) {
            return self
                .client_att_tables
                .read(&connection.peer_identity(), handle, offset, data);
        }
        att.read(offset, data)
    }

    /// Writes `data` at byte `offset` to `att` identified by `handle` for `connection`, after authorization. Returns
    /// permission/offset/capacity/missing-client errors; client-specific and shared storage follow their respective
    /// write-length rules.
    fn write_attribute_data(
        &self,
        connection: &Connection<'_, P>,
        handle: u16,
        offset: usize,
        att: &mut Attribute<'values>,
        data: &[u8],
    ) -> Result<(), AttErrorCode> {
        self.can_write(connection, att)?;
        if matches!(att.data, AttributeData::ClientSpecific { .. }) {
            return self
                .client_att_tables
                .write(&connection.peer_identity(), handle, offset, data);
        }
        att.write(offset, data)
    }

    /// Handles `attribute_type` over inclusive `start`..=`end` for `connection`, writing a response into `buf`.
    /// Returns encoded bytes or codec space errors. Canonical UUID comparison accepts equivalent widths; values are
    /// capped at 253 bytes and equal tuple widths. First access failure becomes ATT error; later failure/size change
    /// ends the successful prefix. Invalid ranges become INVALID_HANDLE.
    fn handle_read_by_type_req(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        start: u16,
        end: u16,
        attribute_type: &Uuid,
    ) -> Result<usize, codec::Error> {
        let mut handle = start;
        let mut data = WriteCursor::new(buf);

        if start > end || start == 0 {
            return Self::error_response(data, att::ATT_READ_BY_TYPE_REQ, start, AttErrorCode::INVALID_HANDLE);
        }

        let (mut header, mut body) = data.split(2)?;
        let attribute_type = canonical_att_uuid(attribute_type);
        let err = self.att_table.iterate_from(start, |it| {
            let mut ret = Err(AttErrorCode::ATTRIBUTE_NOT_FOUND);
            for (att_handle, att) in it {
                // trace!("[read_by_type] Check attribute {:?} {}", att.uuid, att.handle);
                if canonical_att_uuid(&att.uuid) == attribute_type && att_handle <= end {
                    body.write(att_handle)?;
                    handle = att_handle;

                    // The response's one-byte tuple length includes the two-byte handle.
                    let value_limit = body.available().min(253);
                    let new_ret =
                        self.read_attribute_data(connection, att_handle, 0, att, &mut body.write_buf()[..value_limit]);
                    match (new_ret, ret) {
                        (Ok(first_length), Err(_)) => {
                            // First successful read, store this length, all subsequent ones must match it.
                            // debug!("[read_by_type] found first entry {:x?}, handle {}", att.uuid, handle);
                            ret = new_ret;
                            body.commit(first_length)?;
                        }
                        (Ok(new_length), Ok(old_length)) => {
                            // Any matching attribute after the first, verify the lengths are identical, if not break.
                            if new_length == old_length {
                                // debug!("[read_by_type] found equal length {}, handle {}", new_length, handle);
                                body.commit(new_length)?;
                            } else {
                                // We encountered a different length,  unwind the handle.
                                // debug!("[read_by_type] different length: {}, old: {}", new_length, old_length);
                                body.truncate(body.len() - 2);
                                // And then break to ensure we respond with the previously found entries.
                                break;
                            }
                        }
                        (Err(error_code), Ok(_old_length)) => {
                            // New read failed, but we had a previous value, return what we had thus far, truncate to
                            // remove the previously written handle.
                            body.truncate(body.len() - 2);
                            // We do silently drop the error here.
                            // debug!("[read_by_group] new error: {:?}, returning result thus far", error_code);
                            break;
                        }
                        (Err(_), Err(_)) => {
                            // Error on the first possible read, return this error.
                            ret = new_ret;
                            break;
                        }
                    }
                    // If we get here, we always have had a successful read, and we can check that we still have space
                    // left in the buffer to write the next entry if it exists.
                    if let Ok(expected_length) = ret {
                        if body.available() < expected_length + 2 {
                            break;
                        }
                    }
                }
            }
            ret
        });

        match err {
            Ok(len) => {
                header.write(att::ATT_READ_BY_TYPE_RSP)?;
                header.write(2 + len as u8)?;
                Ok(header.len() + body.len())
            }
            Err(e) => Ok(Self::error_response(data, att::ATT_READ_BY_TYPE_REQ, handle, e)?),
        }
    }

    /// Handles service `group_type` over inclusive `start`..=`end` for `connection`, writing into `buf`. Returns
    /// encoded bytes or codec space errors. Only canonical primary/secondary service types are supported; invalid
    /// ranges take precedence. Values are capped at 251 bytes so handle/end/value tuples fit one-byte lengths; later
    /// size/access changes end the successful prefix.
    fn handle_read_by_group_type_req(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        start: u16,
        end: u16,
        group_type: &Uuid,
    ) -> Result<usize, codec::Error> {
        let mut handle = start;
        let mut data = WriteCursor::new(buf);

        if start > end || start == 0 {
            return Self::error_response(
                data,
                att::ATT_READ_BY_GROUP_TYPE_REQ,
                start,
                AttErrorCode::INVALID_HANDLE,
            );
        }

        let group_type = canonical_att_uuid(group_type);
        if group_type != canonical_att_uuid(&bt_hci::uuid::declarations::PRIMARY_SERVICE.into())
            && group_type != canonical_att_uuid(&bt_hci::uuid::declarations::SECONDARY_SERVICE.into())
        {
            return Self::error_response(
                data,
                att::ATT_READ_BY_GROUP_TYPE_REQ,
                start,
                AttErrorCode::UNSUPPORTED_GROUP_TYPE,
            );
        }
        let (mut header, mut body) = data.split(2)?;
        // Multiple entries can be returned in the response as long as they are of equal length.
        let err = self.att_table.iterate_from(start, |mut it| {
            // ret either holds the length of the attribute, or the error code encountered.
            let mut ret: Result<usize, AttErrorCode> = Err(AttErrorCode::ATTRIBUTE_NOT_FOUND);
            while let Some((att_handle, att)) = it.next() {
                // trace!("[read_by_group] Check attribute {:x?} {}", att.uuid, att.handle);
                if canonical_att_uuid(&att.uuid) == group_type && att_handle <= end {
                    // debug!("[read_by_group] found! {:x?} handle: {}", att.uuid, att.handle);
                    handle = att_handle;
                    let last_handle_in_group = it.service_group_end();

                    body.write(att_handle)?;
                    body.write(last_handle_in_group)?;
                    let value_limit = body.available().min(251);
                    let new_ret =
                        self.read_attribute_data(connection, att_handle, 0, att, &mut body.write_buf()[..value_limit]);
                    match (new_ret, ret) {
                        (Ok(first_length), Err(_)) => {
                            // First successful read, store this length, all subsequent ones must match it.
                            // debug!("[read_by_group] found first entry {:x?}, handle {}", att.uuid, handle);
                            ret = new_ret;
                            body.commit(first_length)?;
                        }
                        (Ok(new_length), Ok(old_length)) => {
                            // Any matching attribute after the first, verify the lengths are identical, if not break.
                            if new_length == old_length {
                                // debug!("[read_by_group] found equal length {}, handle {}", new_length, handle);
                                body.commit(new_length)?;
                            } else {
                                // We encountered a different length,  unwind the handle and last_handle written.
                                // debug!("[read_by_group] different length: {}, old: {}", new_length, old_length);
                                body.truncate(body.len() - 4);
                                // And then break to ensure we respond with the previously found entries.
                                break;
                            }
                        }
                        (Err(error_code), Ok(_old_length)) => {
                            // New read failed, but we had a previous value, return what we had thus far, truncate to
                            // remove the previously written handle and last handle.
                            body.truncate(body.len() - 4);
                            // We do silently drop the error here.
                            // debug!("[read_by_group] new error: {:?}, returning result thus far", error_code);
                            break;
                        }
                        (Err(_), Err(_)) => {
                            // Error on the first possible read, return this error.
                            ret = new_ret;
                            break;
                        }
                    }
                    // If we get here, we always have had a successful read, and we can check that we still have space
                    // left in the buffer to write the next entry if it exists.
                    if let Ok(expected_length) = ret {
                        if body.available() < expected_length + 4 {
                            break;
                        }
                    }
                }
            }
            ret
        });

        match err {
            Ok(len) => {
                header.write(att::ATT_READ_BY_GROUP_TYPE_RSP)?;
                header.write(4 + len as u8)?;
                Ok(header.len() + body.len())
            }
            Err(e) => Ok(Self::error_response(data, att::ATT_READ_BY_GROUP_TYPE_REQ, handle, e)?),
        }
    }

    /// Reads `handle` at offset zero for `connection` into ATT response `buf`. Returns response byte count, including
    /// an encoded access/not-found error, or codec space failure. Value bytes truncate to remaining response
    /// capacity.
    fn handle_read_req(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
    ) -> Result<usize, codec::Error> {
        let mut data = WriteCursor::new(buf);

        data.write(att::ATT_READ_RSP)?;

        let err = self
            .att_table
            .with_attribute(handle, |att| {
                let len = self.read_attribute_data(connection, handle, 0, att, data.write_buf())?;
                data.commit(len)?;
                Ok(len)
            })
            .unwrap_or(Err(AttErrorCode::ATTRIBUTE_NOT_FOUND));

        match err {
            Ok(_) => Ok(data.len()),
            Err(e) => Ok(Self::error_response(data, att::ATT_READ_REQ, handle, e)?),
        }
    }

    /// Attempts the command write of `data` to `handle` for `connection`; `buf` is unused. Returns zero because
    /// commands never respond, including when permission, handle or value admission fails.
    fn handle_write_cmd(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
        data: &[u8],
    ) -> Result<usize, codec::Error> {
        self.att_table.with_attribute_mut(handle, |att| {
            // Write commands can't respond with an error.
            let _ = self.write_attribute_data(connection, handle, 0, att, data);
        });
        Ok(0)
    }

    /// Writes `data` to `handle` for `connection` at offset zero, then encodes success/error into `buf`. Returns
    /// response byte count or codec space failure; a successful value mutation is not rolled back if response
    /// encoding fails.
    fn handle_write_req(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
        data: &[u8],
    ) -> Result<usize, codec::Error> {
        let err = self
            .att_table
            .with_attribute_mut(handle, |att| {
                self.write_attribute_data(connection, handle, 0, att, data)
            })
            .unwrap_or(Err(AttErrorCode::ATTRIBUTE_NOT_FOUND));

        let mut w = WriteCursor::new(buf);
        match err {
            Ok(()) => {
                w.write(att::ATT_WRITE_RSP)?;
                Ok(w.len())
            }
            Err(e) => Ok(Self::error_response(w, att::ATT_WRITE_REQ, handle, e)?),
        }
    }

    /// Finds inclusive `start`..=`end` entries matching canonical 16-bit `attr_type` and exact raw `attr_value`,
    /// writing handle/group-end pairs into `buf`. Returns response bytes or codec space errors; invalid ranges/no
    /// match become ATT errors. This discovery path reads available shared value bytes without peer permission
    /// checks; client-specific values do not match.
    fn handle_find_type_value(
        &self,
        buf: &mut [u8],
        start: u16,
        end: u16,
        attr_type: u16,
        attr_value: &[u8],
    ) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(buf);
        let attr_type = canonical_att_uuid(&Uuid::new_short(attr_type));

        if start > end || start == 0 {
            return Self::error_response(w, att::ATT_FIND_BY_TYPE_VALUE_REQ, start, AttErrorCode::INVALID_HANDLE);
        }

        w.write(att::ATT_FIND_BY_TYPE_VALUE_RSP)?;
        self.att_table.iterate_from(start, |mut it| {
            while let Some((handle, att)) = it.next() {
                if handle <= end && canonical_att_uuid(&att.uuid) == attr_type {
                    let value = att.data.value();
                    if value == Some(attr_value) {
                        if w.available() < 4 {
                            break;
                        }
                        w.write(handle)?;
                        w.write(it.service_group_end())?;
                    }
                }
            }
            Ok::<(), codec::Error>(())
        })?;
        if w.len() > 1 {
            Ok(w.len())
        } else {
            Ok(Self::error_response(
                w,
                att::ATT_FIND_BY_TYPE_VALUE_REQ,
                start,
                AttErrorCode::ATTRIBUTE_NOT_FOUND,
            )?)
        }
    }

    /// Discovers handles/types in inclusive `start`..=`end`, writing into `buf`. Returns response bytes or codec
    /// space errors; invalid/empty ranges become ATT errors. Batches one UUID width, expands UUID32 to 128 bits, and stops
    /// before a width change or incomplete tuple.
    fn handle_find_information(&self, buf: &mut [u8], start: u16, end: u16) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(buf);

        if start > end || start == 0 {
            return Self::error_response(w, att::ATT_FIND_INFORMATION_REQ, start, AttErrorCode::INVALID_HANDLE);
        }

        let (mut header, mut body) = w.split(2)?;

        header.write(att::ATT_FIND_INFORMATION_RSP)?;
        let mut t = 0;

        self.att_table.iterate_from(start, |it| {
            for (handle, att) in it {
                if handle <= end {
                    // FIND_INFORMATION_RSP "Format" byte (Core spec, Vol 3 Part F, 3.4.3.2):
                    // 0x01 = handle(s) + 16-bit UUID(s), 0x02 = handle(s) + 128-bit UUID(s).
                    // `get_att_type()` returns the real format code (32-bit reports 0x02 and is
                    // widened to 128-bit by `att_bytes()`), so the header byte, this same-type
                    // batching check, and the body written below all agree.
                    let format = att.uuid.get_att_type();
                    if t == 0 {
                        t = format;
                    } else if t != format {
                        break;
                    }
                    let (uuid, len) = att.uuid.att_bytes();
                    let uuid = &uuid[..len];
                    if body.available() < 2 + uuid.len() {
                        break;
                    }
                    body.write(handle)?;
                    body.append(uuid)?;
                }
            }
            Ok::<(), codec::Error>(())
        })?;
        header.write(t)?;

        if body.len() > 2 {
            Ok(header.len() + body.len())
        } else {
            Ok(Self::error_response(
                w,
                att::ATT_FIND_INFORMATION_REQ,
                start,
                AttErrorCode::ATTRIBUTE_NOT_FOUND,
            )?)
        }
    }

    /// Resets response cursor `w` and writes failure of request `opcode`, associated `handle` and raw `code`. Returns
    /// five bytes, or a space error after possible prefix writes; bytes beyond the response are not cleared.
    fn error_response(
        mut w: WriteCursor<'_>,
        opcode: u8,
        handle: u16,
        code: AttErrorCode,
    ) -> Result<usize, codec::Error> {
        w.reset();
        w.write(att::ATT_ERROR_RSP)?;
        w.write(opcode)?;
        w.write(handle)?;
        w.write(code)?;
        Ok(w.len())
    }

    #[cfg(feature = "att-queued-writes")]
    /// Authorizes and queues `value` at byte `offset` for `handle` on `connection`, echoing it into `buf`. Returns
    /// response bytes or codec space error; queue/permission failures become ATT errors. Queue mutation precedes echo
    /// encoding and remains if the output is too short.
    fn handle_prepare_write(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
        offset: u16,
        value: &[u8],
    ) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(buf);

        let err = sealed::DynamicAttributeServer::can_write(self, connection, handle)
            .and_then(|()| connection.prepare_write(handle, offset, value));
        match err {
            Ok(()) => {
                w.write(att::ATT_PREPARE_WRITE_RSP)?;
                w.write(handle)?;
                w.write(offset)?;
                w.append(value)?;
                Ok(w.len())
            }
            Err(e) => Ok(Self::error_response(w, att::ATT_PREPARE_WRITE_REQ, handle, e)?),
        }
    }

    #[cfg(not(feature = "att-queued-writes"))]
    /// Writes REQUEST_NOT_SUPPORTED into `buf` when queued writes are disabled. `handle`, `_connection`, `_offset`
    /// and `_value` are ignored; the error handle is zero. Returns five response bytes or a codec space error.
    fn handle_prepare_write(
        &self,
        _connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
        _offset: u16,
        _value: &[u8],
    ) -> Result<usize, codec::Error> {
        let w = WriteCursor::new(buf);
        Self::error_response(w, att::ATT_PREPARE_WRITE_REQ, 0, AttErrorCode::REQUEST_NOT_SUPPORTED)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Executes `connection`'s queued write when `flags` is 1 or clears it when 0, writing response into `buf`. Returns
    /// response bytes or codec space error. Invalid flags preserve the queue and produce INVALID_PDU; commit clears
    /// it even after write failure. Applied writes/queue clearing survive response-encoding failure.
    fn handle_execute_write(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        flags: u8,
    ) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(buf);
        if flags > 1 {
            return Self::error_response(w, att::ATT_EXECUTE_WRITE_REQ, 0, AttErrorCode::INVALID_PDU);
        }
        if flags == 0x01 {
            let err = connection.with_prepare_write(|pw| {
                if pw.handle == 0 {
                    return Ok(());
                }
                let data = &pw.buf[..pw.len as usize];
                self.att_table
                    .with_attribute_mut(pw.handle, |att| {
                        self.write_attribute_data(connection, pw.handle, pw.offset as usize, att, data)
                    })
                    .unwrap_or(Err(AttErrorCode::ATTRIBUTE_NOT_FOUND))
            });
            connection.clear_prepare_write();
            match err {
                Ok(()) => {
                    w.write(att::ATT_EXECUTE_WRITE_RSP)?;
                    Ok(w.len())
                }
                Err(e) => Ok(Self::error_response(w, att::ATT_EXECUTE_WRITE_REQ, 0, e)?),
            }
        } else {
            connection.clear_prepare_write();
            w.write(att::ATT_EXECUTE_WRITE_RSP)?;
            Ok(w.len())
        }
    }

    #[cfg(not(feature = "att-queued-writes"))]
    /// Writes REQUEST_NOT_SUPPORTED into `buf` when queued writes are disabled; `_connection` and `_flags` are
    /// ignored. Returns five response bytes or codec space error, without queue mutation.
    fn handle_execute_write(
        &self,
        _connection: &Connection<'_, P>,
        buf: &mut [u8],
        _flags: u8,
    ) -> Result<usize, codec::Error> {
        let w = WriteCursor::new(buf);
        Self::error_response(w, att::ATT_EXECUTE_WRITE_REQ, 0, AttErrorCode::REQUEST_NOT_SUPPORTED)
    }

    /// Reads `handle` for `connection` from byte `offset` into response `buf`. Returns encoded bytes or codec space
    /// error; permission/missing/offset failures become ATT errors. Values truncate to capacity and offset exactly at
    /// end returns an empty suffix.
    fn handle_read_blob(
        &self,
        connection: &Connection<'_, P>,
        buf: &mut [u8],
        handle: u16,
        offset: u16,
    ) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(buf);
        w.write(att::ATT_READ_BLOB_RSP)?;

        let err = self
            .att_table
            .with_attribute(handle, |att| {
                let n = self.read_attribute_data(connection, handle, offset as usize, att, w.write_buf())?;
                w.commit(n)?;
                Ok(n)
            })
            .unwrap_or(Err(AttErrorCode::ATTRIBUTE_NOT_FOUND));

        match err {
            Ok(_) => Ok(w.len()),
            Err(e) => Ok(Self::error_response(w, att::ATT_READ_BLOB_REQ, handle, e)?),
        }
    }

    /// Rejects the unsupported multiple-variable read into `buf`: raw `handles` shorter than two u16 entries or with
    /// an odd byte count produce INVALID_PDU, otherwise REQUEST_NOT_SUPPORTED. Returns five response bytes or codec
    /// space error; no attributes are read.
    fn handle_read_multiple(&self, buf: &mut [u8], handles: &[u8]) -> Result<usize, codec::Error> {
        let w = WriteCursor::new(buf);
        // This optional variable-length procedure is not implemented. Validate
        // direct callers too; malformed input must never index an absent handle.
        let code = if handles.len() < 4 || !handles.len().is_multiple_of(2) {
            AttErrorCode::INVALID_PDU
        } else {
            AttErrorCode::REQUEST_NOT_SUPPORTED
        };
        Self::error_response(w, att::ATT_READ_MULTIPLE_VARIABLE_REQ, 0, code)
    }

    /// Processes decoded `packet` for `connection`, writing an ATT response into `rx`. Returns `Some` of response
    /// bytes, `None` for commands/confirmations/externally handled MTU exchange, or codec space errors. Peer failures
    /// are encoded responses; successful writes/queue changes can precede output failure. Caller supplies negotiated
    /// response capacity.
    pub fn process(
        &self,
        connection: &Connection<'_, P>,
        packet: &AttClient,
        rx: &mut [u8],
    ) -> Result<Option<usize>, codec::Error> {
        let len = match packet {
            AttClient::Request(AttReq::ReadByType {
                start,
                end,
                attribute_type,
            }) => self.handle_read_by_type_req(connection, rx, *start, *end, attribute_type)?,

            AttClient::Request(AttReq::ReadByGroupType { start, end, group_type }) => {
                self.handle_read_by_group_type_req(connection, rx, *start, *end, group_type)?
            }
            AttClient::Request(AttReq::FindInformation {
                start_handle,
                end_handle,
            }) => self.handle_find_information(rx, *start_handle, *end_handle)?,

            AttClient::Request(AttReq::Read { handle }) => self.handle_read_req(connection, rx, *handle)?,

            AttClient::Command(AttCmd::Write { handle, data }) => {
                self.handle_write_cmd(connection, rx, *handle, data)?;
                0
            }

            AttClient::Request(AttReq::Write { handle, data }) => {
                self.handle_write_req(connection, rx, *handle, data)?
            }

            AttClient::Request(AttReq::ExchangeMtu { mtu }) => 0, // Done outside,

            AttClient::Request(AttReq::FindByTypeValue {
                start_handle,
                end_handle,
                att_type,
                att_value,
            }) => self.handle_find_type_value(rx, *start_handle, *end_handle, *att_type, att_value)?,

            AttClient::Request(AttReq::PrepareWrite { handle, offset, value }) => {
                self.handle_prepare_write(connection, rx, *handle, *offset, value)?
            }

            AttClient::Request(AttReq::ExecuteWrite { flags }) => self.handle_execute_write(connection, rx, *flags)?,

            AttClient::Request(AttReq::ReadBlob { handle, offset }) => {
                self.handle_read_blob(connection, rx, *handle, *offset)?
            }

            AttClient::Request(AttReq::ReadMultiple { handles }) => self.handle_read_multiple(rx, handles)?,

            AttClient::Confirmation(_) => 0,
        };
        if len > 0 {
            Ok(Some(len))
        } else {
            Ok(None)
        }
    }

    /// Returns the shared attribute table; local table operations bypass connection permission checks.
    pub fn table(&self) -> &AttributeTable<'values, M, ATT_MAX> {
        &self.att_table
    }

    #[cfg(feature = "gatt")]
    /// Stores typed `data` locally at `value_handle`, then sends via `stack` to subscribed peers when the immediately
    /// following attribute is a CCCD. Returns storage/send errors, or success for storage alone when no CCCD follows.
    /// The local write survives any subsequent error/cancellation; peer sends follow notify's sequential
    /// partial-delivery and indication rules.
    pub async fn write_and_notify<C: crate::Controller, T: AsGatt + ?Sized>(
        &self,
        stack: &crate::Stack<'_, C, P>,
        value_handle: u16,
        data: &T,
    ) -> Result<(), Error> {
        self.att_table.write(value_handle, 0, data.as_gatt())?;

        // Check if the next handle is a CCCD
        let Some(cccd_handle) = value_handle.checked_add(1) else {
            return Ok(());
        };
        if self.att_table.uuid(cccd_handle)
            != Some(bt_hci::uuid::descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION.into())
        {
            return Ok(());
        }

        self.notify(stack, value_handle, data).await
    }

    #[cfg(feature = "gatt")]
    /// Sends typed `data` for `value_handle` to subscribed connections in `stack`, without storing it. Returns
    /// InvalidValue unless the following handle is a CCCD, or the first permission/allocation/enqueue/confirmation
    /// error. Indications take precedence over notifications when both bits are set; peers are processed
    /// sequentially. Earlier sends survive later error/cancellation. After indication slot acquisition,
    /// cancellation/failure while awaiting delivery/confirmation disconnects that peer; a 30-second send/confirmation
    /// timeout excludes slot acquisition.
    pub async fn notify<C: crate::Controller, T: AsGatt + ?Sized>(
        &self,
        stack: &crate::Stack<'_, C, P>,
        value_handle: u16,
        data: &T,
    ) -> Result<(), Error> {
        // Check if the next handle is a CCCD
        let cccd_handle = value_handle.checked_add(1).ok_or(Error::InvalidValue)?;
        if self.att_table.uuid(cccd_handle)
            != Some(bt_hci::uuid::descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION.into())
        {
            return Err(Error::InvalidValue);
        }

        let data = data.as_gatt();
        for conn in stack.connections() {
            if self.should_indicate(&conn, cccd_handle) {
                let uns = att::AttUns::Indicate {
                    handle: value_handle,
                    data,
                };
                crate::attribute::send_unsolicited(self, &conn, cccd_handle, uns).await?;
            } else if self.should_notify(&conn, cccd_handle) {
                let uns = att::AttUns::Notify {
                    handle: value_handle,
                    data,
                };
                crate::attribute::send_unsolicited(self, &conn, cccd_handle, uns).await?;
            }
        }
        Ok(())
    }

    /// Returns an owned snapshot of `connection`'s per-client values, or `None` if its identity has no stored slot.
    /// The snapshot may include retained disconnected values.
    pub fn get_client_att_table(&self, connection: &Connection<'_, P>) -> Option<ClientAttTable> {
        self.client_att_tables.get_client_att_table(&connection.peer_identity())
    }

    /// Restores matching values from validated `table` for `connection`, clipping to existing capacities and zeroing
    /// missing/tail bytes. Missing peer slots are ignored; this local operation bypasses permissions and retains the
    /// current layout.
    pub fn set_client_att_table(&self, connection: &Connection<'_, P>, table: &ClientAttTableView<'_>) {
        self.client_att_tables
            .set_client_att_table(&connection.peer_identity(), table);
    }
}

#[cfg(test)]
/// Server wire responses, permission/subscription handling and bounded discovery regressions.
mod tests {
    use core::task::Poll;

    use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};
    use bt_hci::uuid::declarations::{PRIMARY_SERVICE, SECONDARY_SERVICE};
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;

    use super::*;
    use crate::connection_manager::tests::{setup, ADDR_1};
    use crate::prelude::*;
    use crate::Address;

    /// A wire-decoded UUID128 request finds a characteristic originally declared with UUID32.
    #[test]
    fn uuid32_read_by_type_matches_expanded_wire_uuid() {
        let mut table = AttributeTable::<'_, NoopRawMutex, 8>::new();
        let ch = table
            .add_service(Service::new(0x180fu16))
            .add_characteristic_small(0x12345678u32, [CharacteristicProp::Read], 0x5au8)
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        let request = att::Att::Client(AttClient::Request(AttReq::ReadByType {
            start: ch.handle,
            end: ch.handle,
            attribute_type: ch.uuid,
        }));
        let mut wire = [0; 21];
        request.encode(&mut wire).unwrap();
        let att::Att::Client(decoded) = att::Att::decode(&wire).unwrap() else {
            panic!()
        };
        let mut response = [0; 23];
        let len = server.process(&conn, &decoded, &mut response).unwrap().unwrap();
        assert_eq!(
            &response[..len],
            &[att::ATT_READ_BY_TYPE_RSP, 3, ch.handle as u8, 0, 0x5a]
        );
    }

    /// Read By Group Type rejects characteristic/custom grouping even when a matching attribute exists.
    #[test]
    fn read_by_group_type_rejects_unsupported_grouping() {
        let mut table = AttributeTable::<'_, NoopRawMutex, 8>::new();
        let ch = table
            .add_service(Service::new(0x180fu16))
            .add_characteristic_small(0x12345678u32, [CharacteristicProp::Read], 7u8)
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        for group_type in [Uuid::new_short(0x2803), ch.uuid] {
            let mut response = [0; 23];
            let len = server
                .process(
                    &conn,
                    &AttClient::Request(AttReq::ReadByGroupType {
                        start: 1,
                        end: u16::MAX,
                        group_type,
                    }),
                    &mut response,
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                &response[..len],
                &[
                    att::ATT_ERROR_RSP,
                    att::ATT_READ_BY_GROUP_TYPE_REQ,
                    1,
                    0,
                    AttErrorCode::UNSUPPORTED_GROUP_TYPE.to_u8()
                ]
            );
        }
    }

    /// Primary and secondary grouping accept the Bluetooth-base128 equivalents of their assigned UUIDs.
    #[test]
    fn read_by_group_type_accepts_expanded_service_grouping() {
        let mut table = AttributeTable::<'_, NoopRawMutex, 8>::new();
        table.add_service(Service::new(0x180fu16)).build();
        table.add_secondary_service(Service::new(0x1812u16)).build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        for (kind, expected_handle) in [(0x2800, 1u16), (0x2801, 2)] {
            let group_type = Uuid::from(bt_hci::uuid::BluetoothUuid128::base().set_initial_group(kind));
            let mut response = [0; 23];
            let len = server
                .process(
                    &conn,
                    &AttClient::Request(AttReq::ReadByGroupType {
                        start: 1,
                        end: u16::MAX,
                        group_type,
                    }),
                    &mut response,
                )
                .unwrap()
                .unwrap();
            assert_eq!(response[0], att::ATT_READ_BY_GROUP_TYPE_RSP);
            assert_eq!(len, 8);
            assert_eq!(&response[2..4], &expected_handle.to_le_bytes());
        }
    }

    #[test]
    /// Long read-by-type values obey both the response buffer size and one-byte tuple limit across four MTUs.
    fn read_by_type_limits_long_values_and_preserves_response_framing() {
        let value = [0x5a; 300];
        let mut table = AttributeTable::<'_, NoopRawMutex, 8>::new();
        let characteristic = table
            .add_service(Service {
                uuid: PRIMARY_SERVICE.into(),
            })
            .add_characteristic_ro::<[u8; 300], _>(Uuid::new_short(0xabcd), &value)
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        for mtu in [23, 256, 257, 512] {
            let mut buf = [0u8; 512];
            let len = server
                .process(
                    &conn,
                    &AttClient::Request(AttReq::ReadByType {
                        start: characteristic.handle,
                        end: characteristic.handle,
                        attribute_type: characteristic.uuid,
                    }),
                    &mut buf[..mtu],
                )
                .unwrap()
                .unwrap();
            let expected = (mtu - 4).min(253);
            assert_eq!(len, 4 + expected);
            assert_eq!(buf[0], att::ATT_READ_BY_TYPE_RSP);
            assert_eq!(usize::from(buf[1]), expected + 2);
            assert_eq!(&buf[2..4], &characteristic.handle.to_le_bytes());
            assert_eq!(&buf[4..len], &value[..expected]);
        }
    }

    #[test]
    /// Notification and indication CCCD bits are independent, including combined subscriptions and unsubscribed
    /// peers.
    fn notification_and_indication_subscriptions_are_independent() {
        let mut value = [1u8];
        let mut table = AttributeTable::<'_, NoopRawMutex, 8>::new();
        let characteristic = table
            .add_service(Service {
                uuid: PRIMARY_SERVICE.into(),
            })
            .add_characteristic(
                Uuid::new_short(0xabcd),
                [CharacteristicProp::Notify, CharacteristicProp::Indicate],
                1u8,
                &mut value,
            )
            .build();
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(table);
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        let gatt = crate::gatt::GattConnection::try_new(conn, &server).unwrap();
        for bits in 0..=3u8 {
            let mut buf = [0u8; 23];
            server
                .process(
                    gatt.raw(),
                    &AttClient::Request(AttReq::Write {
                        handle: characteristic.cccd_handle.unwrap(),
                        data: &[bits, 0],
                    }),
                    &mut buf,
                )
                .unwrap();
            assert_eq!(characteristic.should_notify(&gatt), bits & 1 != 0);
            assert_eq!(characteristic.should_indicate(&gatt), bits & 2 != 0);
        }
    }

    #[test]
    /// Malformed multiple-read handle lists and invalid discovery ranges return the matching request opcode and ATT
    /// error.
    fn malformed_read_multiple_and_invalid_ranges_return_matching_errors() {
        let server = AttributeServer::<_, DefaultPacketPool, 8, 1>::new(AttributeTable::<'_, NoopRawMutex, 8>::new());
        let mgr = setup();
        mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!()
        };
        let mut buf = [0u8; 64];
        for handles in [&[][..], &[1][..], &[1, 0][..], &[1, 0, 2][..]] {
            let len = server
                .process(&conn, &AttClient::Request(AttReq::ReadMultiple { handles }), &mut buf)
                .unwrap()
                .unwrap();
            assert_eq!(
                &buf[..len],
                &[
                    1,
                    att::ATT_READ_MULTIPLE_VARIABLE_REQ,
                    0,
                    0,
                    AttErrorCode::INVALID_PDU.to_u8()
                ]
            );
        }
        for (start, end) in [(0, 1), (2, 1)] {
            for (request, opcode) in [
                (
                    AttReq::FindInformation {
                        start_handle: start,
                        end_handle: end,
                    },
                    att::ATT_FIND_INFORMATION_REQ,
                ),
                (
                    AttReq::FindByTypeValue {
                        start_handle: start,
                        end_handle: end,
                        att_type: 0x2800,
                        att_value: &[],
                    },
                    att::ATT_FIND_BY_TYPE_VALUE_REQ,
                ),
                (
                    AttReq::ReadByGroupType {
                        start,
                        end,
                        group_type: PRIMARY_SERVICE.into(),
                    },
                    att::ATT_READ_BY_GROUP_TYPE_REQ,
                ),
            ] {
                let len = server
                    .process(&conn, &AttClient::Request(request), &mut buf)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    &buf[..len],
                    &[1, opcode, start as u8, 0, AttErrorCode::INVALID_HANDLE.to_u8()]
                );
            }
        }
    }

    #[test]
    /// Service group discovery reports the actual end before the next service and 65535 for the final group.
    fn test_attribute_server_last_handle_of_group() {
        // This test comes from a situation where a service had exactly 16 handles, this resulted in the
        // last_handle_in_group field of the ReadByGroupType response was 16 aligned (96 to be exact), in this situation
        // the next request will start at 96 + 1, which was one handle beyond the start of the next service.
        //
        // Snippet from the original failure mode:
        // WARN  trouble_host::attribute_server] Looking for group: Uuid16([0, 28]) between 75 and 65535
        // DEBUG trouble_host::attribute_server] [read_by_group] found! Uuid16([0, 28]) 80
        // DEBUG trouble_host::attribute_server] last_handle_in_group: 96
        // DEBUG trouble_host::attribute_server] read_attribute_data: Ok(16)
        // TRACE trouble_host::host] [host] granted send packets = 1, len = 30
        // TRACE trouble_host::host] [host] sent acl packet len = 26
        // TRACE trouble_host::host] [host] inbound l2cap header channel = 4, fragment len = 7, total = 7
        // INFO  main_ble::ble_bas_peripheral] [gatt-attclient]: ReadByGroupType { start: 97, end: 65535, group_type: Uuid16([0, 40]) }
        // INFO  main_ble::ble_bas_peripheral] [gatt] other event
        // WARN  trouble_host::attribute_server] Looking for group: Uuid16([0, 28]) between 97 and 65535
        // WARN  trouble_host::attribute_server] [read_by_group] Dit not find attribute Uuid16([0, 28]) between 97  65535

        // The request:
        // INFO  main_ble::ble_bas_peripheral] [gatt-attclient]: ReadByGroupType { start: 97, end: 65535, group_type: Uuid16([0, 40]) }
        // In trace, the "group_type: Uuid16([0, 40]) }" is decimal, so this becomes group type 0x2800, which is the
        // primary service group.
        let primary_service_group_type = Uuid::new_short(0x2800);

        let _ = env_logger::try_init();
        /// Attribute capacity for the two-service group-end fixture.
        const MAX_ATTRIBUTES: usize = 1024;
        /// Single client slot used by the group-discovery fixture.
        const CONNECTIONS_MAX: usize = 3;
        /// One L2CAP channel slot for the fixture facade type.
        const L2CAP_CHANNELS_MAX: usize = 5;
        /// Single-connection/channel facade sizing used only to instantiate the test server.
        type FacadeDummyType = [u8; 0];

        // Instead of only checking the failure mode, we fuzz the length of the interior service to cross over several
        // multiples of 16.
        for interior_handle_count in 0..=64u8 {
            debug!("Testing with interior handle count of {}", interior_handle_count);

            // Create a new table.
            let mut table: AttributeTable<'_, NoopRawMutex, { MAX_ATTRIBUTES }> = AttributeTable::new();

            // Add a first service, contents don't really matter, but the issue doesn't manifest without this.
            {
                let svc = table.add_service(Service {
                    uuid: Uuid::new_long([10; 16]),
                });
            }

            // Add an interior service that has a varying length.
            {
                let mut svc = table.add_service(Service {
                    uuid: Uuid::new_long([0; 16]),
                });

                for c in 0..interior_handle_count {
                    let _service_instance = svc
                        .add_characteristic_ro::<[u8; 2], _>(Uuid::new_long([c; 16]), &[0, 0])
                        .build();
                }
            }
            // Now add the service at the end, contents don't really matter.
            {
                table.add_service(Service {
                    uuid: Uuid::new_long([8; 16]),
                });
            }

            // Print the table for debugging.
            table.iterate(|it| {
                for (handle, att) in it {
                    let uuid = &att.uuid;
                    if *uuid == PRIMARY_SERVICE.into() || *uuid == SECONDARY_SERVICE.into() {
                        trace!("service 0x{:0>4x?}, 0x{:0>2x?}", handle, uuid,);
                    } else {
                        trace!("  0x{:0>4x?}, 0x{:0>2x?}", handle, uuid,);
                    }
                }
            });

            // Create a server.
            let server = AttributeServer::<_, DefaultPacketPool, MAX_ATTRIBUTES, CONNECTIONS_MAX>::new(table);

            // Create the connection manager.
            let mgr = setup();

            // Try to connect.
            assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());
            unwrap!(mgr.connect(
                ConnHandle::new(0),
                Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
                LeConnRole::Peripheral,
                ConnParams::new(),
            ));

            if let Poll::Ready(conn_handle) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) {
                // We now have a connection, we can send the mocked requests to our attribute server.
                let mut buffer = [0u8; 64];

                let mut start = 1;
                let end = u16::MAX;
                // There are always three services that we should be able to discover.
                for _ in 0..3 {
                    let length = server
                        .handle_read_by_group_type_req(
                            &conn_handle,
                            &mut buffer,
                            start,
                            end,
                            &primary_service_group_type,
                        )
                        .unwrap();
                    let response = &buffer[0..length];
                    trace!("  0x{:0>2x?}", response);
                    // It should be a successful response, because the service should be found, this will assert if
                    // we failed to retrieve the third service.
                    assert_eq!(response[0], att::ATT_READ_BY_GROUP_TYPE_RSP);
                    // The last handle of this group is at byte 4 & 5, so retrieve that and update the start for the
                    // next cycle. We only check the first response here, and ignore any others that may be in the
                    // response.
                    let last_handle = u16::from_le_bytes([response[4], response[5]]);
                    start = last_handle.saturating_add(1);
                }
            } else {
                panic!("expected connection to be accepted");
            };
        }
    }

    #[test]
    /// Find Information uses format 2 for 128-bit UUIDs and expands UUID32 into the same format and byte width.
    fn find_information_reports_128bit_format_and_widens_32bit() {
        // Regression: FIND_INFORMATION_RSP must report Format 0x02 (Core spec Vol 3 Part F,
        // 3.4.3.2) and a full 16-byte UUID body for both 128-bit and 32-bit characteristic
        // UUIDs. Before the fix the format byte was the uuid-kind tag (0x03 for 128-bit, which
        // is reserved), and a 32-bit UUID emitted a 4-byte body under a 0x02 header — both
        // malformed. A 32-bit UUID has no ATT format of its own, so it is widened against the
        // Bluetooth base UUID.
        let _ = env_logger::try_init();
        /// Attribute slots needed for the mixed-width discovery fixture.
        const MAX_ATTRIBUTES: usize = 64;
        /// Single retained-client slot for mixed-width discovery.
        const CONNECTIONS_MAX: usize = 3;

        let uuid128 = [
            0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, 0x34, 0x12, 0x78, 0x56, 0xF0, 0xDE, 0xBC, 0x9A,
        ];
        let uuid32 = Uuid::Uuid32([0x11, 0x22, 0x33, 0x44]);

        let mut table: AttributeTable<'_, NoopRawMutex, { MAX_ATTRIBUTES }> = AttributeTable::new();
        let (h128, h32) = {
            let mut svc = table.add_service(Service {
                uuid: Uuid::new_long([0; 16]),
            });
            let c128 = svc
                .add_characteristic_ro::<[u8; 2], _>(Uuid::new_long(uuid128), &[0, 0])
                .build();
            let c32 = svc.add_characteristic_ro::<[u8; 2], _>(uuid32, &[0, 0]).build();
            (c128.handle, c32.handle)
        };

        let server = AttributeServer::<_, DefaultPacketPool, MAX_ATTRIBUTES, CONNECTIONS_MAX>::new(table);
        let mut buf = [0u8; 64];

        // 128-bit characteristic value: Format 0x02, the 16-byte UUID echoed back verbatim.
        let len = server.handle_find_information(&mut buf, h128, h128).unwrap();
        let rsp = &buf[..len];
        assert_eq!(rsp[0], att::ATT_FIND_INFORMATION_RSP);
        assert_eq!(
            rsp[1], 0x02,
            "128-bit UUID must report Format 0x02, not the 0x03 kind tag"
        );
        assert_eq!(len, 2 + 2 + 16, "one 18-byte entry: 2-byte handle + 16-byte UUID");
        assert_eq!(u16::from_le_bytes([rsp[2], rsp[3]]), h128);
        assert_eq!(&rsp[4..20], &uuid128);

        // 32-bit characteristic value: Format 0x02, widened to the 128-bit base UUID.
        let len = server.handle_find_information(&mut buf, h32, h32).unwrap();
        let rsp = &buf[..len];
        assert_eq!(rsp[0], att::ATT_FIND_INFORMATION_RSP);
        assert_eq!(
            rsp[1], 0x02,
            "32-bit UUID has no ATT format of its own; must report 0x02"
        );
        assert_eq!(len, 2 + 2 + 16, "32-bit UUID must be widened to a 16-byte body");
        assert_eq!(u16::from_le_bytes([rsp[2], rsp[3]]), h32);
        let expected = bt_hci::uuid::BluetoothUuid128::base()
            .set_initial_group(0x4433_2211)
            .to_le_bytes();
        assert_eq!(&rsp[4..20], &expected);
    }
}
