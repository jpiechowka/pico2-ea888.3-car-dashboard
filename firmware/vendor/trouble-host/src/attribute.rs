//! Attribute storage, typed GATT access, permissions and value-update transport.
//! Declaration layouts follow [Core Vol 3 Part G, section 3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html).
use core::cell::RefCell;
use core::fmt;
use core::marker::PhantomData;

use bt_hci::uuid::declarations::{CHARACTERISTIC, INCLUDE, PRIMARY_SERVICE, SECONDARY_SERVICE};
use bt_hci::uuid::descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::with_timeout;
use heapless::Vec;

use crate::att::{AttErrorCode, AttUns};
use crate::attribute_server::AttributeServer;
use crate::cursor::{ReadCursor, WriteCursor};
use crate::gatt::ATT_TRANSACTION_TIMEOUT;
use crate::prelude::{AsGatt, FixedGattValue, FromGatt, GattConnection, SecurityLevel};
use crate::types::gatt_traits::FromGattError;
pub use crate::types::uuid::Uuid;
use crate::{gatt, Error, PacketPool, MAX_INVALID_DATA_LEN};

/// The maximum size in bytes of an attribute using inline small data storage.
pub const MAX_SMALL_DATA_SIZE: usize = 20;

/// Characteristic declaration property bits; advertising a bit does not itself authorize access or implement a procedure.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CharacteristicProp {
    /// Advertises broadcast support (bit 0).
    Broadcast = 0x01,
    /// Advertises read procedures (bit 1); read permissions are checked separately.
    Read = 0x02,
    /// Advertises write-command support (bit 2), without an ATT response.
    WriteWithoutResponse = 0x04,
    /// Advertises write-request support (bit 3), with an ATT response.
    Write = 0x08,
    /// Advertises unacknowledged notifications (bit 4); sending still requires subscription/access admission.
    Notify = 0x10,
    /// Advertises confirmed indications (bit 5); only one may await confirmation per connection.
    Indicate = 0x20,
    /// Advertises authenticated signed writes (bit 6); setting this flag alone does not implement that procedure.
    AuthenticatedWrite = 0x40,
    /// Signals additional properties in a characteristic extended-properties descriptor (bit 7).
    Extended = 0x80,
}

/// Attribute permissions
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PermissionLevel {
    #[default]
    /// Operation is allowed with no encryption or authentication
    Allowed,
    /// Encryption is required
    EncryptionRequired,
    /// Encryption and authentication are required
    AuthenticationRequired,
    /// Operation is not allowed
    NotAllowed,
}

/// Attribute permissions
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AttPermissions {
    /// Security required for read operations
    pub read: PermissionLevel,
    /// Security required for write operations
    pub write: PermissionLevel,
    /// Minimum encrypted-link key length in bytes (0 disables the minimum); values above 16 deny normal LE keys.
    #[cfg(feature = "legacy-pairing")]
    pub min_key_len: u8,
}

impl AttPermissions {
    /// Returns unencrypted read permission, no write permission and no minimum key length.
    pub(crate) fn read_only() -> Self {
        Self {
            read: PermissionLevel::Allowed,
            write: PermissionLevel::NotAllowed,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        }
    }

    /// Checks current `level` and, with legacy pairing, byte `encryption_key_len`. Returns read denial,
    /// insufficient authentication/key size, or Ok. An unencrypted Allowed read ignores the key-size minimum.
    pub(crate) fn can_read(
        &self,
        level: SecurityLevel,
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<(), AttErrorCode> {
        match self.read {
            PermissionLevel::NotAllowed => Err(AttErrorCode::READ_NOT_PERMITTED),
            PermissionLevel::EncryptionRequired | PermissionLevel::AuthenticationRequired
                if level < SecurityLevel::Encrypted =>
            {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            PermissionLevel::AuthenticationRequired if level < SecurityLevel::EncryptedAuthenticated => {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            _ => {
                #[cfg(feature = "legacy-pairing")]
                if self.min_key_len > 0 && level.encrypted() && encryption_key_len < self.min_key_len {
                    return Err(AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE);
                }
                Ok(())
            }
        }
    }

    /// Checks current `level` and, with legacy pairing, byte `encryption_key_len`. Returns write denial,
    /// insufficient authentication/key size, or Ok. An unencrypted Allowed write ignores the key-size minimum.
    pub(crate) fn can_write(
        &self,
        level: SecurityLevel,
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<(), AttErrorCode> {
        match self.write {
            PermissionLevel::NotAllowed => Err(AttErrorCode::WRITE_NOT_PERMITTED),
            PermissionLevel::EncryptionRequired | PermissionLevel::AuthenticationRequired
                if level < SecurityLevel::Encrypted =>
            {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            PermissionLevel::AuthenticationRequired if level < SecurityLevel::EncryptedAuthenticated => {
                Err(AttErrorCode::INSUFFICIENT_AUTHENTICATION)
            }
            _ => {
                #[cfg(feature = "legacy-pairing")]
                if self.min_key_len > 0 && level.encrypted() && encryption_key_len < self.min_key_len {
                    return Err(AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE);
                }
                Ok(())
            }
        }
    }
}

/// A GATT characteristic declaration value.
pub(crate) struct CharacteristicDeclaration {
    /// Advertised characteristic operations; separate permissions govern actual access.
    pub props: CharacteristicProps,
    /// One-based handle of the immediately following value attribute.
    pub value_handle: u16,
    /// Type UUID of the characteristic value, not the declaration UUID.
    pub uuid: Uuid,
}

impl CharacteristicDeclaration {
    /// Returns a declaration from `props`, one-based `value_handle` and characteristic `uuid`; no admission checks.
    pub(crate) fn new<U: Into<Uuid>, P: Into<CharacteristicProps>>(props: P, value_handle: u16, uuid: U) -> Self {
        Self {
            props: props.into(),
            value_handle,
            uuid: uuid.into(),
        }
    }
}

impl TryFrom<&[u8]> for CharacteristicDeclaration {
    /// Truncated cursor fields or unsupported UUID byte length.
    type Error = Error;

    /// Returns a declaration decoded from little-endian `data`; requires property/handle bytes and
    /// a 2/4/16-byte UUID tail. Does not validate handle or property bits.
    fn try_from(data: &'_ [u8]) -> Result<Self, Self::Error> {
        let mut r = ReadCursor::new(data);
        Ok(CharacteristicDeclaration {
            props: CharacteristicProps(r.read()?),
            value_handle: r.read()?,
            uuid: Uuid::try_from(r.remaining())?,
        })
    }
}

/// Attribute metadata.
pub struct Attribute<'a> {
    /// Attribute type UUID; service declaration values contain a different, service-specific UUID.
    pub(crate) uuid: Uuid,
    /// Read/write security policy applied by the server, not by raw storage access.
    pub(crate) permissions: AttPermissions,
    /// Shared storage or metadata identifying per-client storage.
    pub(crate) data: AttributeData<'a>,
}

impl<'a> Attribute<'a> {
    /// Empty slot sentinel retained for fixed attribute-table initialization.
    const EMPTY: Option<Attribute<'a>> = None;

    /// Copies bytes at `offset` into `data`; Returns byte count or storage/offset error. Does not check permissions.
    pub(crate) fn read(&self, offset: usize, data: &mut [u8]) -> Result<usize, AttErrorCode> {
        self.data.read(offset, data)
    }

    /// Writes `data` at `offset`; Returns storage/offset/length errors. Does not check link permissions.
    pub(crate) fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), AttErrorCode> {
        self.data.write(offset, data)
    }

    /// Returns the stored permission policy without checking any connection.
    pub(crate) fn permissions(&self) -> AttPermissions {
        self.permissions
    }

    /// Returns whether reads are permitted at some security level, not whether a particular peer qualifies.
    pub(crate) fn readable(&self) -> bool {
        self.permissions.read != PermissionLevel::NotAllowed
    }

    /// Returns whether writes are permitted at some security level, not whether a particular peer qualifies.
    pub(crate) fn writable(&self) -> bool {
        self.permissions.write != PermissionLevel::NotAllowed
    }
}

#[derive(Debug, PartialEq, Eq)]
/// Attribute storage with byte-length invariants maintained by builders; per-client values live elsewhere.
pub(crate) enum AttributeData<'d> {
    /// Borrowed immutable bytes shared by all clients.
    ReadOnlyData {
        /// Complete initialized value; its slice length is the logical length and capacity.
        value: &'d [u8],
    },
    /// Borrowed writable backing storage shared by all clients.
    Data {
        /// True makes each successful write set the logical end to offset plus input length.
        variable_len: bool,
        /// Initialized byte length, bounded by value capacity and u16::MAX.
        len: u16,
        /// Backing bytes; only the logical prefix is readable.
        value: &'d mut [u8],
    },
    /// Owned inline bytes for small shared values.
    SmallData {
        /// True allows writes to shorten or extend the logical prefix within capacity.
        variable_len: bool,
        /// Usable byte capacity, at most MAX_SMALL_DATA_SIZE.
        capacity: u8,
        /// Initialized byte length, at most capacity.
        len: u8,
        /// Inline backing bytes; tail beyond logical length is not exposed.
        value: [u8; MAX_SMALL_DATA_SIZE],
    },
    /// Per-connection storage metadata, resolved by the attribute server.
    ClientSpecific {
        /// Whether the per-client value may have a logical length below capacity.
        variable_len: bool,
        /// Maximum per-client bytes; public builders cap this at 512.
        capacity: u16,
    },
}

impl From<Uuid> for AttributeData<'_> {
    /// Returns fixed inline ATT UUID storage for `uuid`; UUID32 expands to the Bluetooth 128-bit base form.
    fn from(uuid: Uuid) -> Self {
        let (bytes, len) = uuid.att_bytes();
        let mut value = [0u8; MAX_SMALL_DATA_SIZE];
        value[..len].copy_from_slice(&bytes[..len]);
        let len = len as u8;
        AttributeData::SmallData {
            variable_len: false,
            capacity: len,
            len,
            value,
        }
    }
}

impl From<bt_hci::uuid::BluetoothUuid16> for AttributeData<'_> {
    /// Returns fixed two-byte inline storage for Bluetooth `uuid` in little-endian order.
    fn from(uuid: bt_hci::uuid::BluetoothUuid16) -> Self {
        let raw = uuid.to_le_bytes();
        let len = raw.len() as u8;
        let mut value = [0u8; MAX_SMALL_DATA_SIZE];
        value[..raw.len()].copy_from_slice(&raw);
        AttributeData::SmallData {
            variable_len: false,
            capacity: len,
            len,
            value,
        }
    }
}

impl From<bt_hci::uuid::BluetoothUuid128> for AttributeData<'_> {
    /// Returns fixed sixteen-byte inline storage for Bluetooth `uuid` in little-endian order.
    fn from(uuid: bt_hci::uuid::BluetoothUuid128) -> Self {
        let raw = uuid.to_le_bytes();
        let len = raw.len() as u8;
        let mut value = [0u8; MAX_SMALL_DATA_SIZE];
        value[..raw.len()].copy_from_slice(&raw);
        AttributeData::SmallData {
            variable_len: false,
            capacity: len,
            len,
            value,
        }
    }
}

impl From<CharacteristicDeclaration> for AttributeData<'_> {
    /// Returns inline property/handle/UUID bytes for `decl`; handle is little-endian and UUID32 expands.
    /// The declaration fits the 20-byte inline capacity for every supported UUID representation.
    fn from(decl: CharacteristicDeclaration) -> Self {
        let (uuid_bytes, uuid_len) = decl.uuid.att_bytes();
        let uuid_raw = &uuid_bytes[..uuid_len];
        let total_len = 1 + 2 + uuid_raw.len(); // props + handle + uuid
        debug_assert!(total_len <= MAX_SMALL_DATA_SIZE);
        let mut value = [0u8; MAX_SMALL_DATA_SIZE];
        value[0] = decl.props.0;
        value[1..3].copy_from_slice(&decl.value_handle.to_le_bytes());
        value[3..3 + uuid_raw.len()].copy_from_slice(uuid_raw);
        let len = total_len as u8;
        AttributeData::SmallData {
            variable_len: false,
            capacity: len,
            len,
            value,
        }
    }
}

impl AttributeData<'_> {
    /// Returns the logical shared byte prefix, or None for per-client metadata.
    /// Panics if internally constructed length exceeds backing storage.
    pub(crate) fn value(&self) -> Option<&[u8]> {
        match self {
            AttributeData::ReadOnlyData { value } => Some(value),
            AttributeData::Data { len, value, .. } => Some(&value[..*len as usize]),
            AttributeData::SmallData { len, value, .. } => Some(&value[..*len as usize]),
            AttributeData::ClientSpecific { .. } => None,
        }
    }

    /// Returns whether writes may change the logical byte length; immutable bytes always return false.
    pub(crate) fn is_variable_len(&self) -> bool {
        match self {
            AttributeData::Data { variable_len, .. }
            | AttributeData::SmallData { variable_len, .. }
            | AttributeData::ClientSpecific { variable_len, .. } => *variable_len,
            _ => false,
        }
    }

    /// Returns byte capacity, including unused writable tail; immutable capacity equals its length.
    pub(crate) fn capacity(&self) -> usize {
        match self {
            AttributeData::ReadOnlyData { value, .. } => value.len(),
            AttributeData::Data { value, .. } => value.len(),
            AttributeData::SmallData { capacity, .. } => usize::from(*capacity),
            AttributeData::ClientSpecific { capacity, .. } => usize::from(*capacity),
        }
    }

    /// Copies the logical value at byte `offset` into `data`, bounded by the output slice. Returns copied
    /// bytes, INVALID_OFFSET beyond the end, or UNLIKELY_ERROR for per-client metadata; exact end returns zero.
    pub(crate) fn read(&self, mut offset: usize, mut data: &mut [u8]) -> Result<usize, AttErrorCode> {
        /// Copies the suffix of `src` selected by `offset` into remaining `dest`; Returns bytes copied.
        /// Consumes skipped offset and advances the destination reference; leaves unwritten bytes unchanged.
        fn append(src: &[u8], offset: &mut usize, dest: &mut &mut [u8]) -> usize {
            if *offset >= src.len() {
                *offset -= src.len();
                0
            } else {
                let d = core::mem::take(dest);
                let n = d.len().min(src.len() - *offset);
                d[..n].copy_from_slice(&src[*offset..][..n]);
                *dest = &mut d[n..];
                *offset = 0;
                n
            }
        }

        let written = match self {
            Self::ReadOnlyData { value } => append(value, &mut offset, &mut data),
            Self::Data { len, value, .. } => {
                let value = &value[..*len as usize];
                append(value, &mut offset, &mut data)
            }
            Self::SmallData { len, value, .. } => {
                let value = &value[..*len as usize];
                append(value, &mut offset, &mut data)
            }
            Self::ClientSpecific { .. } => return Err(AttErrorCode::UNLIKELY_ERROR),
        };

        if offset > 0 {
            Err(AttErrorCode::INVALID_OFFSET)
        } else {
            Ok(written)
        }
    }

    /// Writes `data` at byte `offset` within initialized length/capacity. Returns offset/length/read-only
    /// errors before mutation. Variable values end at offset plus input length; fixed values retain length.
    /// Caller-maintained lengths/capacities must fit backing storage; this method does not check permissions.
    pub(crate) fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), AttErrorCode> {
        match self {
            Self::Data {
                value,
                variable_len,
                len,
            } => {
                if offset > usize::from(*len) {
                    Err(AttErrorCode::INVALID_OFFSET)
                } else if offset + data.len() > value.len() {
                    Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH)
                } else {
                    value[offset..offset + data.len()].copy_from_slice(data);
                    if *variable_len {
                        *len = (offset + data.len()) as u16;
                    }
                    Ok(())
                }
            }
            Self::SmallData {
                variable_len,
                capacity,
                len,
                value,
            } => {
                if offset > usize::from(*len) {
                    Err(AttErrorCode::INVALID_OFFSET)
                } else if offset + data.len() > usize::from(*capacity) {
                    Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH)
                } else {
                    value[offset..offset + data.len()].copy_from_slice(data);
                    if *variable_len {
                        *len = (offset + data.len()) as u8;
                    }
                    Ok(())
                }
            }
            _ => Err(AttErrorCode::WRITE_NOT_PERMITTED),
        }
    }
}

impl fmt::Debug for Attribute<'_> {
    /// Writes UUID and readability/writability flags to `f`; Returns its write result without exposing value bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attribute")
            .field("uuid", &self.uuid)
            .field("readable", &self.readable())
            .field("writable", &self.writable())
            .finish()
    }
}

#[cfg(feature = "defmt")]
impl<'a> defmt::Format for Attribute<'a> {
    /// Writes the metadata-only Debug representation to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", defmt::Debug2Format(self))
    }
}

impl<'a> Attribute<'a> {
    /// Returns an attribute containing converted `uuid`/`data` and supplied `permissions`; no bounds validation.
    pub(crate) fn new<U: Into<Uuid>, T: Into<AttributeData<'a>>>(
        uuid: U,
        permissions: AttPermissions,
        data: T,
    ) -> Attribute<'a> {
        Attribute {
            uuid: uuid.into(),
            permissions,
            data: data.into(),
        }
    }
}

/// Fixed-capacity, one-based GATT attribute table. Builders panic on exhaustion; raw local access
/// bypasses peer permissions. Mutation/iteration closures run under the configured mutex and RefCell borrow.
pub struct AttributeTable<'d, M: RawMutex, const MAX: usize> {
    /// Mutex-protected table; callbacks must not re-enter conflicting table access.
    inner: Mutex<M, RefCell<InnerTable<'d, MAX>>>,
}

/// Contiguous attribute storage; vector index plus one is the ATT handle.
pub(crate) struct InnerTable<'d, const MAX: usize> {
    /// At most MAX entries; keep MAX below u16::MAX so the next handle remains representable.
    attributes: Vec<Attribute<'d>, MAX>,
}

impl<'d, const MAX: usize> InnerTable<'d, MAX> {
    /// Appends owned `attribute`; Returns its one-based handle. Panics on full storage or debug handle overflow.
    fn push(&mut self, attribute: Attribute<'d>) -> u16 {
        let handle = self.next_handle();
        self.attributes.push(attribute).unwrap();
        handle
    }

    /// Returns a borrowed iterator starting at zero-based `pos`; positions at/past the end yield no values.
    fn iterate_from(&self, pos: usize) -> AttributeIterator<'_, 'd> {
        AttributeIterator {
            attributes: &self.attributes,
            pos,
        }
    }

    /// Returns the end of the service starting at one-based `service_handle`, or u16::MAX for the final group.
    fn service_group_end(&self, service_handle: u16) -> u16 {
        self.iterate_from(usize::from(service_handle)).service_group_end()
    }

    /// Returns the next one-based handle; table capacity must keep length below u16::MAX.
    fn next_handle(&self) -> u16 {
        self.attributes.len() as u16 + 1
    }
}

impl<M: RawMutex, const MAX: usize> Default for AttributeTable<'_, M, MAX> {
    /// Returns an empty fixed-capacity table with the configured raw mutex.
    fn default() -> Self {
        Self::new()
    }
}

impl<'d, M: RawMutex, const MAX: usize> AttributeTable<'d, M, MAX> {
    /// Returns an empty GATT table; no heap allocation or link state is created.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(InnerTable { attributes: Vec::new() })),
        }
    }

    /// Runs `f` under the mutex with a shared table borrow; Returns its result.
    /// A conflicting reentrant borrow panics; keep the synchronous callback bounded.
    pub(crate) fn with_inner<F: FnOnce(&InnerTable<'d, MAX>) -> R, R>(&self, f: F) -> R {
        self.inner.lock(|inner| {
            let table = inner.borrow();
            f(&table)
        })
    }

    /// Runs `f` under the mutex with an exclusive table borrow; Returns its result.
    /// Any reentrant table borrow panics; callback mutations persist even if its result represents failure.
    pub(crate) fn with_inner_mut<F: FnOnce(&mut InnerTable<'d, MAX>) -> R, R>(&self, f: F) -> R {
        self.inner.lock(|inner| {
            let mut table = inner.borrow_mut();
            f(&mut table)
        })
    }

    /// Runs `f` with an iterator from the first attribute under a shared lock; Returns its result.
    pub(crate) fn iterate<F: FnOnce(AttributeIterator<'_, 'd>) -> R, R>(&self, f: F) -> R {
        self.with_inner(|table| f(table.iterate_from(0)))
    }

    /// Runs `f` on the one-based `handle` under a shared lock; Returns Some(result), or None for zero/absent handle.
    pub(crate) fn with_attribute<F: FnOnce(&Attribute<'d>) -> R, R>(&self, handle: u16, f: F) -> Option<R> {
        if handle == 0 {
            return None;
        }

        self.with_inner(|table| {
            let i = usize::from(handle) - 1;
            table.attributes.get(i).map(f)
        })
    }

    /// Runs `f` on the one-based `handle` under an exclusive lock; Returns Some(result), or None for zero/absent handle.
    pub(crate) fn with_attribute_mut<F: FnOnce(&mut Attribute<'d>) -> R, R>(&self, handle: u16, f: F) -> Option<R> {
        if handle == 0 {
            return None;
        }

        self.with_inner_mut(|table| {
            let i = usize::from(handle) - 1;
            table.attributes.get_mut(i).map(f)
        })
    }

    /// Runs `f` with a locked iterator beginning at one-based `start` (zero also starts at the first item); Returns its result.
    pub(crate) fn iterate_from<F: FnOnce(AttributeIterator<'_, 'd>) -> R, R>(&self, start: u16, f: F) -> R {
        let i = usize::from(start).saturating_sub(1);
        self.with_inner(|table| f(table.iterate_from(i)))
    }

    /// Appends `attribute` under an exclusive lock; Returns its handle and panics if capacity is exhausted.
    fn push(&mut self, attribute: Attribute<'d>) -> u16 {
        self.with_inner_mut(|table| table.push(attribute))
    }

    /// Appends primary `service` declaration and Returns its builder; panics when the table is full.
    pub fn add_service(&mut self, service: Service) -> ServiceBuilder<'_, 'd, M, MAX> {
        let handle = self.push(Attribute::new(
            PRIMARY_SERVICE,
            AttPermissions::read_only(),
            service.uuid,
        ));
        ServiceBuilder { handle, table: self }
    }

    /// Appends secondary `service` declaration and Returns its builder; panics when the table is full.
    pub fn add_secondary_service(&mut self, service: Service) -> ServiceBuilder<'_, 'd, M, MAX> {
        let handle = self.push(Attribute::new(
            SECONDARY_SERVICE,
            AttPermissions::read_only(),
            service.uuid,
        ));
        ServiceBuilder { handle, table: self }
    }

    /// Returns the policy for one-based `attribute`, or None for zero/absent handle.
    pub fn permissions(&self, attribute: u16) -> Option<AttPermissions> {
        self.with_attribute(attribute, |att| att.permissions())
    }

    /// Returns the type UUID for one-based `attribute`, or None for zero/absent handle.
    pub fn uuid(&self, attribute: u16) -> Option<Uuid> {
        self.with_attribute(attribute, |att| att.uuid)
    }

    /// Replaces immutable storage for one-based `attribute` with borrowed `new_value`; Returns NotFound
    /// for an absent handle or NotSupported for other storage. Does not enforce the former length/type.
    pub(crate) fn set_ro(&self, attribute: u16, new_value: &'d [u8]) -> Result<(), Error> {
        self.with_attribute_mut(attribute, |att| match &mut att.data {
            AttributeData::ReadOnlyData { value, .. } => {
                *value = new_value;
                Ok(())
            }
            _ => Err(Error::NotSupported),
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Reads one-based `attribute` at byte `offset` into `data`; Returns copied bytes or NotFound/storage
    /// errors. Output truncation is successful; exact end returns zero. Per-client values are unsupported.
    /// This local operation does not enforce remote read permissions.
    pub fn read(&self, attribute: u16, offset: usize, data: &mut [u8]) -> Result<usize, Error> {
        self.with_attribute(attribute, |att| att.read(offset, data).map_err(Into::into))
            .unwrap_or(Err(Error::NotFound))
    }

    /// Writes `data` to one-based `attribute` at byte `offset`; Returns NotFound/storage errors.
    /// Variable length becomes offset plus input length; fixed values retain length. Bypasses peer permissions.
    pub fn write(&self, attribute: u16, offset: usize, data: &[u8]) -> Result<(), Error> {
        self.with_attribute_mut(attribute, |att| att.write(offset, data).map_err(Into::into))
            .unwrap_or(Err(Error::NotFound))
    }

    /// Replaces one-based `attribute` with `input`; Returns NotFound, storage errors, or UnexpectedDataLength
    /// if fixed-length storage differs. Variable values accept up to capacity and update their logical length.
    pub(crate) fn set_raw(&self, attribute: u16, input: &[u8]) -> Result<(), Error> {
        self.with_attribute_mut(attribute, |att| {
            if !att.data.is_variable_len() && att.data.capacity() != input.len() {
                Err(Error::UnexpectedDataLength {
                    expected: att.data.capacity(),
                    actual: input.len(),
                })
            } else {
                att.write(0, input).map_err(Into::into)
            }
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Returns the number of stored attributes, including declarations and descriptors.
    pub fn len(&self) -> usize {
        self.with_inner(|table| table.attributes.len())
    }

    /// Returns true when no attributes have been added.
    pub fn is_empty(&self) -> bool {
        self.with_inner(|table| table.attributes.is_empty())
    }

    /// Serializes `input` into the value selected by `attribute_handle`; Returns storage/handle/length errors.
    /// Fixed storage requires exact size; variable storage accepts up to capacity. Bypasses remote permissions.
    pub fn set<T: AttributeHandle>(&self, attribute_handle: &T, input: &T::Value) -> Result<(), Error> {
        let gatt_value = input.as_gatt();
        self.set_raw(attribute_handle.handle(), gatt_value)
    }

    /// Returns the typed value selected by `attribute_handle`. Returns NotFound, NotSupported for per-client
    /// metadata, or CannotConstructGattValue with a bounded byte prefix when decoding fails.
    pub fn get<T: AttributeHandle<Value = V>, V: FromGatt>(&self, attribute_handle: &T) -> Result<T::Value, Error> {
        self.with_attribute(attribute_handle.handle(), |att| {
            let value_slice = att.data.value().ok_or(Error::NotSupported)?;
            T::Value::from_gatt(value_slice).map_err(|_| {
                let mut invalid_data = [0u8; MAX_INVALID_DATA_LEN];
                let len_to_copy = value_slice.len().min(MAX_INVALID_DATA_LEN);
                invalid_data[..len_to_copy].copy_from_slice(&value_slice[..len_to_copy]);

                Error::CannotConstructGattValue(invalid_data)
            })
        })
        .unwrap_or(Err(Error::NotFound))
    }

    /// Returns typed characteristic metadata inferred immediately before value `handle`, or NotFound/decode
    /// error. Type T is not validated against stored bytes; malformed internal declarations can panic.
    pub fn find_characteristic_by_value_handle<T: AsGatt>(&self, handle: u16) -> Result<Characteristic<T>, Error> {
        if handle == 0 {
            return Err(Error::NotFound);
        }

        self.iterate_from(handle - 1, |mut it| {
            if let Some((_, att)) = it.next() {
                if att.uuid == CHARACTERISTIC.into() {
                    let decl = CharacteristicDeclaration::try_from(att.data.value().unwrap())?;
                    let props = decl.props;
                    let uuid = decl.uuid;
                    if it.next().is_some() {
                        let end_handle = it.characteristic_group_end();
                        let cccd_handle = it.next().and_then(|(handle, att)| {
                            (att.uuid == CLIENT_CHARACTERISTIC_CONFIGURATION.into()).then_some(handle)
                        });

                        return Ok(Characteristic {
                            handle,
                            cccd_handle,
                            end_handle,
                            props,
                            uuid,
                            phantom: PhantomData,
                        });
                    }
                }
            }
            Err(Error::NotFound)
        })
    }

    #[cfg(feature = "security")]
    /// Returns the numeric AES-CMAC GATT database hash over mandated declaration/descriptor fields.
    /// Runs synchronously under a shared table lock; value changes outside hashed fields do not alter the hash.
    /// See Bluetooth Core Vol 3, Part G, section 7.3.1.
    pub fn hash(&self) -> u128 {
        use bt_hci::uuid::*;

        use crate::security_manager::crypto::AesCmac;

        /// Primary-service declaration: include handle, type and complete value in the hash.
        const PRIMARY_SERVICE: Uuid = Uuid::Uuid16(declarations::PRIMARY_SERVICE.to_le_bytes());
        /// Secondary-service declaration: include handle, type and complete value in the hash.
        const SECONDARY_SERVICE: Uuid = Uuid::Uuid16(declarations::SECONDARY_SERVICE.to_le_bytes());
        /// Included-service declaration: include handle, type and complete value in the hash.
        const INCLUDED_SERVICE: Uuid = Uuid::Uuid16(declarations::INCLUDE.to_le_bytes());
        /// Characteristic declaration: include handle, type and complete value in the hash.
        const CHARACTERISTIC: Uuid = Uuid::Uuid16(declarations::CHARACTERISTIC.to_le_bytes());
        /// Extended-properties descriptor: include handle, type and complete value in the hash.
        const CHARACTERISTIC_EXTENDED_PROPERTIES: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_EXTENDED_PROPERTIES.to_le_bytes());

        /// User-description descriptor: hash handle/type, excluding mutable value.
        const CHARACTERISTIC_USER_DESCRIPTION: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_USER_DESCRIPTION.to_le_bytes());
        /// Client-configuration descriptor: hash handle/type, excluding per-client value.
        const CLIENT_CHARACTERISTIC_CONFIGURATION: Uuid =
            Uuid::Uuid16(descriptors::CLIENT_CHARACTERISTIC_CONFIGURATION.to_le_bytes());
        /// Server-configuration descriptor: hash handle/type only.
        const SERVER_CHARACTERISTIC_CONFIGURATION: Uuid =
            Uuid::Uuid16(descriptors::SERVER_CHARACTERISTIC_CONFIGURATION.to_le_bytes());
        /// Presentation-format descriptor: hash handle/type only.
        const CHARACTERISTIC_PRESENTATION_FORMAT: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_PRESENTATION_FORMAT.to_le_bytes());
        /// Aggregate-format descriptor: hash handle/type only.
        const CHARACTERISTIC_AGGREGATE_FORMAT: Uuid =
            Uuid::Uuid16(descriptors::CHARACTERISTIC_AGGREGATE_FORMAT.to_le_bytes());

        let mut mac = AesCmac::db_hash();

        self.iterate(|it| {
            for (handle, att) in it {
                match att.uuid {
                    PRIMARY_SERVICE
                    | SECONDARY_SERVICE
                    | INCLUDED_SERVICE
                    | CHARACTERISTIC
                    | CHARACTERISTIC_EXTENDED_PROPERTIES => {
                        mac.update(handle.to_le_bytes()).update(att.uuid.as_raw());
                        let value = att.data.value().unwrap_or(&[]);
                        mac.update(value);
                    }
                    CHARACTERISTIC_USER_DESCRIPTION
                    | CLIENT_CHARACTERISTIC_CONFIGURATION
                    | SERVER_CHARACTERISTIC_CONFIGURATION
                    | CHARACTERISTIC_PRESENTATION_FORMAT
                    | CHARACTERISTIC_AGGREGATE_FORMAT => {
                        mac.update(handle.to_le_bytes()).update(att.uuid.as_raw());
                    }
                    _ => {}
                }
            }
        });

        mac.finalize()
    }
}

/// A type which holds a handle to an attribute in the attribute table
pub trait AttributeHandle {
    /// The data type which the attribute contains
    type Value: AsGatt + ?Sized;

    /// Returns the attribute's handle
    fn handle(&self) -> u16;
}

impl<T: AsGatt + ?Sized> AttributeHandle for Characteristic<T> {
    /// Serialized characteristic value type; the handle itself does not validate stored bytes.
    type Value = T;

    /// Returns the one-based characteristic value handle.
    fn handle(&self) -> u16 {
        self.handle
    }
}

/// Invalid handle value
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InvalidHandle;

impl core::fmt::Display for InvalidHandle {
    /// Writes the InvalidHandle marker to `f`; Returns its write result.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for InvalidHandle {}

impl From<InvalidHandle> for Error {
    /// Maps `value` to the host InvalidValue error; Returns no handle detail.
    fn from(value: InvalidHandle) -> Self {
        Error::InvalidValue
    }
}

/// Builder for constructing GATT service definitions.
pub struct ServiceBuilder<'r, 'd, M: RawMutex, const MAX: usize> {
    /// One-based service declaration handle retained through builder operations.
    handle: u16,
    /// Exclusive table borrow; appended declarations/values persist if a later operation panics.
    table: &'r mut AttributeTable<'d, M, MAX>,
}

impl<'d, M: RawMutex, const MAX: usize> ServiceBuilder<'_, 'd, M, MAX> {
    /// Appends declaration, `uuid` value with `permissions`/`data`, and optional CCCD selected by `props`.
    /// Returns a characteristic builder; capacity/handle overflow can panic after earlier insertions.
    fn add_characteristic_internal<T: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        props: CharacteristicProps,
        permissions: AttPermissions,
        data: AttributeData<'d>,
    ) -> CharacteristicBuilder<'_, 'd, T, M, MAX> {
        let chrc_uuid = uuid;
        // First the characteristic declaration
        let (handle, cccd_handle) = self.table.with_inner_mut(|table| {
            let value_handle = table.next_handle() + 1;
            let declaration = CharacteristicDeclaration::new(props, value_handle, uuid);
            table.push(Attribute::new(CHARACTERISTIC, AttPermissions::read_only(), declaration));

            // Then the value declaration
            let h = table.push(Attribute::new(uuid, permissions, data));
            debug_assert!(h == value_handle);

            // Add optional CCCD handle
            let cccd_handle = if props.has_cccd() {
                let handle = table.push(Attribute::new(
                    CLIENT_CHARACTERISTIC_CONFIGURATION,
                    AttPermissions {
                        read: PermissionLevel::Allowed,
                        write: PermissionLevel::Allowed,
                        #[cfg(feature = "legacy-pairing")]
                        min_key_len: 0,
                    },
                    AttributeData::ClientSpecific {
                        variable_len: false,
                        capacity: 2,
                    },
                ));

                Some(handle)
            } else {
                None
            };

            (value_handle, cccd_handle)
        });

        CharacteristicBuilder {
            handle: Characteristic {
                handle,
                cccd_handle,
                // Temporarily set to value handle; updated in build() after descriptors are added
                end_handle: cccd_handle.unwrap_or(handle),
                props,
                uuid: chrc_uuid,
                phantom: PhantomData,
            },
            table: self.table,
        }
    }

    /// Appends characteristic `uuid`/`props`, copying serialized `value` into borrowed `store`.
    /// Returns its builder; panics if initial bytes exceed storage or attribute capacity. Caller supplies
    /// storage fitting T and u16 byte lengths. Permissions initially follow properties, without authentication.
    pub fn add_characteristic<T: AsGatt, U: Into<Uuid>, P: Into<CharacteristicProps>>(
        &mut self,
        uuid: U,
        props: P,
        value: T,
        store: &'d mut [u8],
    ) -> CharacteristicBuilder<'_, 'd, T, M, MAX> {
        let props: CharacteristicProps = props.into();
        let permissions = props.default_permissions();
        let bytes = value.as_gatt();
        store[..bytes.len()].copy_from_slice(bytes);
        let variable_len = T::MAX_SIZE != T::MIN_SIZE;
        let len = bytes.len() as u16;
        self.add_characteristic_internal(
            uuid.into(),
            props,
            permissions,
            AttributeData::Data {
                value: store,
                variable_len,
                len,
            },
        )
    }

    /// Appends characteristic `uuid`/`props` with inline serialized `value`; Returns its builder.
    /// Panics if initial/minimum size exceeds 20 bytes or table capacity; usable capacity is min(T::MAX_SIZE,20).
    pub fn add_characteristic_small<T: AsGatt, U: Into<Uuid>, P: Into<CharacteristicProps>>(
        &mut self,
        uuid: U,
        props: P,
        value: T,
    ) -> CharacteristicBuilder<'_, 'd, T, M, MAX> {
        assert!(T::MIN_SIZE <= MAX_SMALL_DATA_SIZE);

        let props: CharacteristicProps = props.into();
        let permissions = props.default_permissions();
        let bytes = value.as_gatt();
        assert!(bytes.len() <= MAX_SMALL_DATA_SIZE);
        let mut value = [0; MAX_SMALL_DATA_SIZE];
        value[..bytes.len()].copy_from_slice(bytes);
        let variable_len = T::MAX_SIZE != T::MIN_SIZE;
        let capacity = T::MAX_SIZE.min(MAX_SMALL_DATA_SIZE) as u8;
        let len = bytes.len() as u8;
        self.add_characteristic_internal(
            uuid.into(),
            props,
            permissions,
            AttributeData::SmallData {
                variable_len,
                capacity,
                len,
                value,
            },
        )
    }

    /// Appends readable characteristic `uuid` borrowing immutable `value`; Returns its builder.
    /// Panics on table exhaustion; the value must outlive the table.
    pub fn add_characteristic_ro<T: AsGatt + ?Sized, U: Into<Uuid>>(
        &mut self,
        uuid: U,
        value: &'d T,
    ) -> CharacteristicBuilder<'_, 'd, T, M, MAX> {
        let props: CharacteristicProps = [CharacteristicProp::Read].into();
        let permissions = props.default_permissions();
        self.add_characteristic_internal(
            uuid.into(),
            props,
            permissions,
            AttributeData::ReadOnlyData { value: value.as_gatt() },
        )
    }

    /// Appends per-client characteristic `uuid`/`props`; Returns its builder.
    /// Panics if T::MAX_SIZE exceeds 512 or the table fills; actual client bytes are allocated separately.
    pub fn add_characteristic_client<T: AsGatt, U: Into<Uuid>, P: Into<CharacteristicProps>>(
        &mut self,
        uuid: U,
        props: P,
    ) -> CharacteristicBuilder<'_, 'd, T, M, MAX> {
        assert!(T::MAX_SIZE <= 512);
        let props: CharacteristicProps = props.into();
        let permissions = props.default_permissions();
        let variable_len = T::MAX_SIZE != T::MIN_SIZE;
        self.add_characteristic_internal(
            uuid.into(),
            props,
            permissions,
            AttributeData::ClientSpecific {
                variable_len,
                capacity: T::MAX_SIZE as u16,
            },
        )
    }

    /// Includes service at one-based `handle`; Returns the new declaration handle or InvalidHandle when
    /// absent/not a service. Only 16-bit service UUIDs are embedded; table exhaustion still panics.
    pub fn add_included_service(&mut self, handle: u16) -> Result<u16, InvalidHandle> {
        self.table.with_inner_mut(|table| {
            if handle > 0 && table.attributes.len() >= usize::from(handle) {
                let i = usize::from(handle - 1);
                let att = &table.attributes[i];
                let service_uuid = att.uuid;
                if service_uuid == Uuid::from(PRIMARY_SERVICE) || service_uuid == Uuid::from(SECONDARY_SERVICE) {
                    let last_handle_in_group = table.service_group_end(handle);

                    // Included service values only include 16-bit UUIDs per the Bluetooth spec
                    let uuid = att.data.value().and_then(|val| (val.len() == 2).then_some(val));

                    // Encode: handle_LE(2) + last_handle_in_group_LE(2) + optional_uuid(0 or 2)
                    let mut value = [0u8; MAX_SMALL_DATA_SIZE];
                    let mut w = WriteCursor::new(&mut value);
                    w.write(handle).unwrap();
                    w.write(last_handle_in_group).unwrap();
                    if let Some(uuid) = uuid {
                        w.append(uuid).unwrap();
                    }
                    let len = w.len() as u8;

                    Ok(table.push(Attribute::new(
                        INCLUDE,
                        AttPermissions::read_only(),
                        AttributeData::SmallData {
                            variable_len: false,
                            capacity: len,
                            len,
                            value,
                        },
                    )))
                } else {
                    Err(InvalidHandle)
                }
            } else {
                Err(InvalidHandle)
            }
        })
    }

    /// Returns the service declaration handle; already appended table entries remain installed.
    pub fn build(self) -> u16 {
        self.handle
    }
}

/// A characteristic in the attribute table.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Characteristic<T: AsGatt + ?Sized> {
    /// Handle value assigned to the Client Characteristic Configuration Descriptor (if any)
    pub cccd_handle: Option<u16>,
    /// Handle value assigned to this characteristic when it is added to the Gatt Attribute Table
    pub handle: u16,
    /// Last attribute handle belonging to this characteristic (value handle + descriptors)
    pub end_handle: u16,
    /// Properties of this characteristic
    pub props: CharacteristicProps,
    /// UUID of this characteristic
    pub uuid: Uuid,
    /// Associates serialization type T without owning a value.
    pub(crate) phantom: PhantomData<T>,
}

impl<T: AsGatt + ?Sized> Characteristic<T> {
    /// Returns whether `connection` has enabled this characteristic notification CCCD; absent CCCD is false.
    pub fn should_notify<P: PacketPool>(&self, connection: &GattConnection<'_, '_, P>) -> bool {
        let Some(cccd_handle) = self.cccd_handle else {
            return false;
        };
        connection.server.should_notify(connection.raw(), cccd_handle)
    }

    /// Serializes `value` and notifies `connection`; `store` updates server storage before subscription checks.
    /// Returns Ok for no subscriber or successful queue admission, not delivery; missing CCCD, storage,
    /// security, assembly and enqueue errors propagate. Cancellation can leave the stored value changed.
    pub async fn notify<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &T,
        store: bool,
    ) -> Result<(), Error> {
        self.notify_raw(connection, value.as_gatt(), store).await
    }

    /// Notifies `connection` with raw `value`; `store` updates storage before subscription/permission checks.
    /// Returns Ok for no subscriber or successful queue admission, not delivery. Missing CCCD, storage,
    /// security, packet/MTU and enqueue errors propagate; cancellation does not roll back stored bytes.
    pub async fn notify_raw<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &[u8],
        store: bool,
    ) -> Result<(), Error> {
        let server = connection.server;
        if store {
            server.set(connection.raw(), self.handle, value)?;
        }

        let cccd_handle = self.cccd_handle.ok_or(Error::NotFound)?;
        let conn = connection.raw();
        if !server.should_notify(conn, cccd_handle) {
            // No reason to fail?
            return Ok(());
        }

        let uns = AttUns::Notify {
            handle: self.handle,
            data: value,
        };
        send_unsolicited(server, conn, cccd_handle, uns).await
    }

    /// Returns whether `connection` has enabled this characteristic indication CCCD; absent CCCD is false.
    pub fn should_indicate<P: PacketPool>(&self, connection: &GattConnection<'_, '_, P>) -> bool {
        let Some(cccd_handle) = self.cccd_handle else {
            return false;
        };
        connection.server.should_indicate(connection.raw(), cccd_handle)
    }

    /// Serializes `value` and indicates to `connection`; `store` updates storage before subscription checks.
    /// Returns Ok for no subscriber or received confirmation; storage/security/assembly/enqueue errors propagate.
    /// See [`Self::indicate_raw`] for serialization, timeout and cancellation semantics.
    pub async fn indicate<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &T,
        store: bool,
    ) -> Result<(), Error> {
        self.indicate_raw(connection, value.as_gatt(), store).await
    }

    /// Indicates raw `value` to `connection`; `store` updates storage before subscription/permission checks.
    /// Returns Ok for no subscriber or received confirmation; missing CCCD, storage, security, assembly
    /// and enqueue errors propagate. Concurrent indications serialize without a queue-wait deadline; after
    /// assembly, enqueue plus confirmation have a 30-second deadline. Timeout returns Timeout and closes
    /// the bearer; cancellation after that point also closes it to prevent ambiguous late confirmations.
    pub async fn indicate_raw<P: PacketPool>(
        &self,
        connection: &GattConnection<'_, '_, P>,
        value: &[u8],
        store: bool,
    ) -> Result<(), Error> {
        let server = connection.server;
        if store {
            server.set(connection.raw(), self.handle, value)?;
        }

        let cccd_handle = self.cccd_handle.ok_or(Error::NotFound)?;
        let conn = connection.raw();
        if !server.should_indicate(conn, cccd_handle) {
            // No reason to fail?
            return Ok(());
        }

        let uns = AttUns::Indicate {
            handle: self.handle,
            data: value,
        };
        send_unsolicited(server, conn, cccd_handle, uns).await
    }

    /// Stores serialized `value` in shared `server` storage; Returns handle/storage/length errors.
    /// Does not send traffic, enforce peer permissions or select per-client storage.
    pub fn set<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        value: &T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_raw(self.handle, value)?;
        Ok(())
    }

    /// Rebinds shared immutable storage in `server` to borrowed `value`; Returns missing-handle or
    /// unsupported-storage errors. Does not enforce the previous length or send traffic.
    pub fn set_ro<'a, M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'a, M, P, AT, CN>,
        value: &'a T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_ro(self.handle, value)?;
        Ok(())
    }

    /// Returns the typed shared value from `server`, or handle/storage/decode errors; no peer permissions checked.
    pub fn get<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
    ) -> Result<T, Error>
    where
        T: FromGatt,
    {
        server.table().get(self)
    }

    /// Runs `f` over shared bytes in `server` under its table lock; Returns Some(result) or None for
    /// missing/per-client storage. Do not re-enter conflicting table access from the callback.
    pub fn with_raw_value<M: RawMutex, P: PacketPool, R, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        server
            .table()
            .with_attribute(self.handle, |att| att.data.value().map(f))
            .flatten()
    }

    /// Returns the attribute handle for the characteristic's client characteristic configuration descriptor (if available)
    pub fn cccd_handle(&self) -> Option<CharacteristicPropertiesHandle> {
        self.cccd_handle.map(CharacteristicPropertiesHandle)
    }

    /// Returns identical handles/UUID/properties with raw byte serialization type; no stored bytes are converted.
    pub fn to_raw(self) -> Characteristic<[u8]> {
        Characteristic {
            cccd_handle: self.cccd_handle,
            handle: self.handle,
            end_handle: self.end_handle,
            props: self.props,
            uuid: self.uuid,
            phantom: PhantomData,
        }
    }
}

/// Authorizes CCCD `cccd_handle` through `server` for `conn`, then sends notification/indication `uns`.
/// Returns permission, encryption-retry, assembly or enqueue errors; notification success means queued.
/// An indication owns a serialized slot and Returns success only after confirmation. Its 30-second
/// timeout includes enqueue time; failure/cancellation after assembly disconnects before releasing the
/// slot. Cancellation while acquiring the slot has not begun this transaction; prior storage changes persist.
pub(crate) async fn send_unsolicited<P: PacketPool>(
    server: &dyn crate::attribute_server::DynamicAttributeServer<P>,
    conn: &crate::connection::Connection<'_, P>,
    cccd_handle: u16,
    uns: AttUns<'_>,
) -> Result<(), Error> {
    // CCCD write permission governs receipt, including restored subscriptions.
    if let Err(err) = server.can_write(conn, cccd_handle) {
        #[cfg(feature = "security")]
        {
            if conn.is_bonded_peer() && !conn.security_level().is_ok_and(|l| l.encrypted()) {
                conn.try_enable_encryption().await?;
                server.can_write(conn, cccd_handle)?;
            } else {
                return Err(err.into());
            }
        }
        #[cfg(not(feature = "security"))]
        return Err(err.into());
    }
    let indication = matches!(uns, AttUns::Indicate { .. });
    if !indication {
        let pdu = gatt::assemble(conn, crate::att::AttServer::Unsolicited(uns))?;
        conn.send(pdu).await?;
        return Ok(());
    }
    conn.acquire_indication_slot().await?;
    // Assembly failure has not started a transaction. Release the slot without
    // disconnecting; after sending begins, a late confirmation is ambiguous.
    let completed = core::cell::Cell::new(true);
    let _drop = crate::host::OnDrop::new(|| {
        if !completed.get() {
            // A late confirmation has no transaction ID. Cancellation must
            // close this bearer before another indication can own the slot.
            conn.disconnect();
        }
        conn.release_indication_slot();
    });
    let pdu = gatt::assemble(conn, crate::att::AttServer::Unsolicited(uns))?;
    completed.set(false);
    let outcome = with_timeout(ATT_TRANSACTION_TIMEOUT, async {
        conn.send(pdu).await?;
        conn.wait_indication_confirmation().await
    })
    .await;
    match outcome {
        Ok(Ok(())) => {
            completed.set(true);
            Ok(())
        }
        Ok(Err(err)) => Err(err),
        Err(_) => Err(Error::Timeout),
    }
}

/// Attribute handle for a characteristic's properties
pub struct CharacteristicPropertiesHandle(
    /// One-based CCCD handle; constructors do not check the descriptor type.
    u16,
);

impl AttributeHandle for CharacteristicPropertiesHandle {
    /// Single-octet characteristic property bitmask exposed by this typed handle.
    type Value = CharacteristicProps;

    /// Returns the stored one-based descriptor handle.
    fn handle(&self) -> u16 {
        self.0
    }
}

/// Builder for characteristics.
pub struct CharacteristicBuilder<'r, 'd, T: AsGatt + ?Sized, M: RawMutex, const MAX: usize> {
    /// Value/CCCD metadata; build refreshes the last descriptor handle.
    handle: Characteristic<T>,
    /// Exclusive table borrow for appending descriptors and changing permissions.
    table: &'r mut AttributeTable<'d, M, MAX>,
}

impl<'r, 'd, T: AsGatt + ?Sized, M: RawMutex, const MAX: usize> CharacteristicBuilder<'r, 'd, T, M, MAX> {
    /// Appends descriptor `uuid` with `permissions` and owned/borrowed `data`; Returns its typed handle.
    /// Panics on exhausted table capacity; DT is a serialization marker, not a runtime check.
    fn add_descriptor_internal<DT: AsGatt + ?Sized>(
        &mut self,
        uuid: Uuid,
        permissions: AttPermissions,
        data: AttributeData<'d>,
    ) -> Descriptor<DT> {
        let handle = self.table.push(Attribute::new(uuid, permissions, data));

        Descriptor {
            handle,
            uuid,
            phantom: PhantomData,
        }
    }

    /// Appends descriptor `uuid`/`permissions`, copying serialized `value` into borrowed `store`; Returns
    /// its handle. Panics on short storage/table exhaustion; caller keeps storage lengths within u16.
    pub fn add_descriptor<DT: AsGatt, U: Into<Uuid>>(
        &mut self,
        uuid: U,
        permissions: AttPermissions,
        value: DT,
        store: &'d mut [u8],
    ) -> Descriptor<DT> {
        let bytes = value.as_gatt();
        store[..bytes.len()].copy_from_slice(bytes);
        let variable_len = DT::MAX_SIZE != DT::MIN_SIZE;
        let len = bytes.len() as u16;
        self.add_descriptor_internal(
            uuid.into(),
            permissions,
            AttributeData::Data {
                value: store,
                variable_len,
                len,
            },
        )
    }

    /// Appends descriptor `uuid`/`permissions` with inline `value`; Returns its handle.
    /// Panics if initial/minimum bytes exceed 20 or table fills; capacity is min(DT::MAX_SIZE,20).
    pub fn add_descriptor_small<DT: AsGatt, U: Into<Uuid>>(
        &mut self,
        uuid: U,
        permissions: AttPermissions,
        value: DT,
    ) -> Descriptor<DT> {
        assert!(DT::MIN_SIZE <= MAX_SMALL_DATA_SIZE);

        let bytes = value.as_gatt();
        assert!(bytes.len() <= MAX_SMALL_DATA_SIZE);
        let mut value = [0; MAX_SMALL_DATA_SIZE];
        value[..bytes.len()].copy_from_slice(bytes);
        let variable_len = DT::MAX_SIZE != DT::MIN_SIZE;
        let capacity = DT::MAX_SIZE.min(MAX_SMALL_DATA_SIZE) as u8;
        let len = bytes.len() as u8;
        self.add_descriptor_internal(
            uuid.into(),
            permissions,
            AttributeData::SmallData {
                variable_len,
                capacity,
                len,
                value,
            },
        )
    }

    /// Appends descriptor `uuid` borrowing immutable `data`, with `read_permission` and writes disabled.
    /// Returns its handle; panics on table exhaustion.
    pub fn add_descriptor_ro<DT: AsGatt + ?Sized, U: Into<Uuid>>(
        &mut self,
        uuid: U,
        read_permission: PermissionLevel,
        data: &'d DT,
    ) -> Descriptor<DT> {
        let permissions = AttPermissions {
            write: PermissionLevel::NotAllowed,
            read: read_permission,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        };
        self.add_descriptor_internal(
            uuid.into(),
            permissions,
            AttributeData::ReadOnlyData { value: data.as_gatt() },
        )
    }

    /// Appends per-client descriptor `uuid`/`permissions`; Returns its handle.
    /// Panics if DT::MAX_SIZE exceeds 512 or table capacity; bytes are stored separately per client.
    pub fn add_descriptor_client<DT: AsGatt, U: Into<Uuid>>(
        &mut self,
        uuid: U,
        permissions: AttPermissions,
    ) -> Descriptor<DT> {
        assert!(DT::MAX_SIZE <= 512);
        let variable_len = DT::MAX_SIZE != DT::MIN_SIZE;
        self.add_descriptor_internal(
            uuid.into(),
            permissions,
            AttributeData::ClientSpecific {
                variable_len,
                capacity: DT::MAX_SIZE as u16,
            },
        )
    }

    /// Sets value read policy to `read`; Returns this builder. Does not change CCCD permissions.
    pub fn read_permission(self, read: PermissionLevel) -> Self {
        self.table
            .with_attribute_mut(self.handle.handle, |att| att.permissions.read = read);
        self
    }

    /// Sets value write policy to `write`; Returns this builder. Does not change CCCD permissions.
    pub fn write_permission(self, write: PermissionLevel) -> Self {
        self.table
            .with_attribute_mut(self.handle.handle, |att| att.permissions.write = write);
        self
    }

    /// Sets CCCD write policy to `write`; Returns this builder. Panics when notify/indicate did not create a CCCD.
    pub fn cccd_permission(self, write: PermissionLevel) -> Self {
        let Some(handle) = self.handle.cccd_handle else {
            panic!("Can't set CCCD permission on characteristics without notify or indicate properties.");
        };

        self.table
            .with_attribute_mut(handle, |att| att.permissions.write = write);
        self
    }

    /// Set the minimum encryption key length required for this characteristic
    #[cfg(feature = "legacy-pairing")]
    /// Sets minimum encrypted-link byte key length to `len`; Returns this builder without range validation.
    /// Zero disables this minimum; it does not independently require an encrypted link.
    pub fn min_key_len(self, len: u8) -> Self {
        self.table
            .with_attribute_mut(self.handle.handle, |att| att.permissions.min_key_len = len);
        self
    }

    /// Returns the same in-progress builder with raw byte serialization type; stored bytes remain unchanged.
    pub fn to_raw(self) -> CharacteristicBuilder<'r, 'd, [u8], M, MAX> {
        CharacteristicBuilder {
            handle: self.handle.to_raw(),
            table: self.table,
        }
    }
    /// Returns characteristic metadata after setting end_handle to the current final table entry.
    pub fn build(mut self) -> Characteristic<T> {
        self.handle.end_handle = self.table.with_inner(|t| t.next_handle() - 1);
        self.handle
    }
}

/// Characteristic descriptor handle.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug)]
pub struct Descriptor<T: AsGatt + ?Sized> {
    /// One-based descriptor handle in its source table.
    pub(crate) handle: u16,
    /// Descriptor type UUID, independent of serialized value type T.
    pub(crate) uuid: Uuid,
    /// Serialization type marker; no value is owned.
    pub(crate) phantom: PhantomData<T>,
}

impl<T: AsGatt + ?Sized> AttributeHandle for Descriptor<T> {
    /// Descriptor serialization type; runtime bytes are validated only on typed reads.
    type Value = T;

    /// Returns the one-based descriptor handle.
    fn handle(&self) -> u16 {
        self.handle
    }
}

impl<T: AsGatt + ?Sized> Descriptor<T> {
    /// Returns this descriptor's one-based attribute handle.
    pub fn handle(&self) -> u16 {
        self.handle
    }

    /// Returns the borrowed descriptor type UUID.
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    /// Stores serialized `value` in shared `server` storage; Returns handle/length/storage errors.
    /// Does not send traffic or enforce remote write permissions.
    pub fn set<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
        value: &T,
    ) -> Result<(), Error> {
        let value = value.as_gatt();
        server.table().set_raw(self.handle, value)?;
        Ok(())
    }

    /// Returns the typed shared descriptor value from `server`, or handle/storage/decode errors.
    pub fn get<M: RawMutex, P: PacketPool, const AT: usize, const CN: usize>(
        &self,
        server: &AttributeServer<'_, M, P, AT, CN>,
    ) -> Result<T, Error>
    where
        T: FromGatt,
    {
        server.table().get(self)
    }
}

/// Iterator over attributes.
pub struct AttributeIterator<'a, 'd> {
    /// Borrowed contiguous table; handles are slice indices plus one.
    attributes: &'a [Attribute<'d>],
    /// Zero-based index of the next entry; reaching/passing length exhausts iteration.
    pos: usize,
}

impl<'a, 'd> Iterator for AttributeIterator<'a, 'd> {
    /// One-based handle paired with a borrowed attribute.
    type Item = (u16, &'a Attribute<'d>);

    /// Returns the next handle/attribute and advances the index, or None at/past the end.
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.attributes.len() {
            let att = &self.attributes[self.pos];
            self.pos += 1;
            let handle = self.pos as u16;
            Some((handle, att))
        } else {
            None
        }
    }
}

impl<'a, 'd> AttributeIterator<'a, 'd> {
    /// Returns the handle before the next primary/secondary declaration after the current position,
    /// or u16::MAX for the final service. Does not advance the iterator.
    pub fn service_group_end(&self) -> u16 {
        // We return the 0-based index of the next service definition. When interpretted as a 1-based handle,
        // this represents the handle of the last attribute before the next service definition.
        self.attributes
            .iter()
            .enumerate()
            .skip(self.pos)
            .find(|(_, attr)| attr.uuid == PRIMARY_SERVICE.into() || attr.uuid == SECONDARY_SERVICE.into())
            .map(|(i, _)| i as u16)
            .unwrap_or(u16::MAX)
    }

    /// Returns the handle before the next characteristic/service declaration, or the last actual table
    /// handle when none follows. Does not advance the iterator.
    pub fn characteristic_group_end(&self) -> u16 {
        // We return the 0-based index of the next characteristic or service definition. When interpretted as a 1-based handle,
        // this represents the handle of the last attribute before the next characteristic or service definition.
        self.attributes
            .iter()
            .enumerate()
            .skip(self.pos)
            .find(|(_, attr)| {
                attr.uuid == PRIMARY_SERVICE.into()
                    || attr.uuid == SECONDARY_SERVICE.into()
                    || attr.uuid == CHARACTERISTIC.into()
            })
            .map(|(i, _)| i as u16)
            .unwrap_or(self.attributes.len() as u16)
    }
}

/// A GATT service.
pub struct Service {
    /// UUID of the service.
    pub uuid: Uuid,
}

impl Service {
    /// Returns a service with converted `uuid`; no table entry is created until added.
    pub fn new<U: Into<Uuid>>(uuid: U) -> Self {
        Self { uuid: uuid.into() }
    }
}

/// Properties of a characteristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CharacteristicProps(
    /// Raw characteristic operation bitmask.
    u8,
);

impl<'a> From<&'a [CharacteristicProp]> for CharacteristicProps {
    /// Returns the union of property flags in borrowed `props`; duplicates have no additional effect.
    fn from(props: &'a [CharacteristicProp]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= *prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl<'a, const N: usize> From<&'a [CharacteristicProp; N]> for CharacteristicProps {
    /// Returns the union of flags in borrowed array `props`; an empty array yields zero.
    fn from(props: &'a [CharacteristicProp; N]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= *prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl<const N: usize> From<[CharacteristicProp; N]> for CharacteristicProps {
    /// Returns the union of flags in owned array `props`; an empty array yields zero.
    fn from(props: [CharacteristicProp; N]) -> Self {
        let mut val: u8 = 0;
        for prop in props {
            val |= prop as u8;
        }
        CharacteristicProps(val)
    }
}

impl From<u8> for CharacteristicProps {
    /// Returns raw property bits from `value` without validating supported combinations.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl CharacteristicProps {
    /// Returns whether any flag in `props` is present; an empty slice returns false.
    pub fn any(&self, props: &[CharacteristicProp]) -> bool {
        for p in props {
            if (*p as u8) & self.0 != 0 {
                return true;
            }
        }
        false
    }

    /// Returns unencrypted read/write permissions implied by operation bits; no authentication or key
    /// minimum is inferred even from AuthenticatedWrite. Other operations do not grant local read/write.
    pub(crate) fn default_permissions(&self) -> AttPermissions {
        let read = if (self.0 & CharacteristicProp::Read as u8) != 0 {
            PermissionLevel::Allowed
        } else {
            PermissionLevel::NotAllowed
        };

        let write = if (self.0
            & (CharacteristicProp::Write as u8
                | CharacteristicProp::WriteWithoutResponse as u8
                | CharacteristicProp::AuthenticatedWrite as u8))
            != 0
        {
            PermissionLevel::Allowed
        } else {
            PermissionLevel::NotAllowed
        };

        AttPermissions {
            read,
            write,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        }
    }

    /// Returns whether Notify or Indicate requires a client configuration descriptor.
    pub fn has_cccd(&self) -> bool {
        (self.0 & (CharacteristicProp::Indicate as u8 | CharacteristicProp::Notify as u8)) != 0
    }

    /// Returns the unchanged eight property bits.
    pub fn to_raw(self) -> u8 {
        self.0
    }
}

impl FixedGattValue for CharacteristicProps {
    /// One byte on the GATT wire.
    const SIZE: usize = 1;
}

impl FromGatt for CharacteristicProps {
    /// Returns the raw property byte in `data`, or InvalidLength unless exactly one byte; all bit patterns accepted.
    fn from_gatt(data: &[u8]) -> Result<Self, FromGattError> {
        if data.len() != Self::SIZE {
            return Err(FromGattError::InvalidLength);
        }

        Ok(CharacteristicProps(data[0]))
    }
}

impl AsGatt for CharacteristicProps {
    /// Minimum encoded length: one property octet.
    const MIN_SIZE: usize = Self::SIZE;
    /// Maximum encoded length: one property octet.
    const MAX_SIZE: usize = Self::SIZE;

    /// Returns a borrowed one-byte representation with all property bits preserved.
    fn as_gatt(&self) -> &[u8] {
        AsGatt::as_gatt(&self.0)
    }
}

/// A value of an attribute.
pub struct AttributeValue<'d, M: RawMutex> {
    /// Raw-mutex-protected borrowed byte storage; this wrapper currently has no access methods.
    value: Mutex<M, &'d mut [u8]>,
}

impl<M: RawMutex> AttributeValue<'_, M> {}

/// CCCD flag values.
#[derive(Clone, Copy)]
pub enum CCCDFlag {
    /// Notifications enabled.
    Notify = 0x1,
    /// Indications enabled.
    Indicate = 0x2,
}

/// CCCD flag.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct CCCD(
    /// Raw little-endian wire value after decoding; bits 0/1 enable notify/indicate.
    pub(crate) u16,
);

impl<const T: usize> From<[CCCDFlag; T]> for CCCD {
    /// Returns the union of subscription flags in `props`; duplicates are idempotent.
    fn from(props: [CCCDFlag; T]) -> Self {
        let mut val: u16 = 0;
        for prop in props {
            val |= prop as u16;
        }
        CCCD(val)
    }
}

impl From<u16> for CCCD {
    /// Returns raw CCCD bits from `value`, including reserved bits; validation belongs to the server.
    fn from(value: u16) -> Self {
        CCCD(value)
    }
}

impl CCCD {
    /// Returns all sixteen stored CCCD bits, including reserved bits.
    pub fn raw(&self) -> u16 {
        self.0
    }

    /// Clears notification, indication and reserved bits.
    pub fn disable(&mut self) {
        self.0 = 0;
    }

    /// Returns whether any requested flag in `props` is enabled; empty input returns false.
    pub fn any(&self, props: &[CCCDFlag]) -> bool {
        for p in props {
            if (*p as u16) & self.0 != 0 {
                return true;
            }
        }
        false
    }

    /// Sets the notification bit to `is_enabled`, preserving indication and all other bits.
    pub fn set_notify(&mut self, is_enabled: bool) {
        let mask: u16 = CCCDFlag::Notify as u16;
        self.0 = if is_enabled { self.0 | mask } else { self.0 & !mask };
    }

    /// Returns whether the notification subscription bit is set.
    pub fn should_notify(&self) -> bool {
        (self.0 & (CCCDFlag::Notify as u16)) != 0
    }

    /// Sets the indication bit to `is_enabled`, preserving notification and all other bits.
    pub fn set_indicate(&mut self, is_enabled: bool) {
        let mask: u16 = CCCDFlag::Indicate as u16;
        self.0 = if is_enabled { self.0 | mask } else { self.0 & !mask };
    }

    /// Returns whether the indication subscription bit is set.
    pub fn should_indicate(&self) -> bool {
        (self.0 & (CCCDFlag::Indicate as u16)) != 0
    }
}

#[cfg(test)]
/// Database-hash known answers and variable-length replacement boundary regressions.
mod tests {
    extern crate std;

    /// A service built from UUID32 stores the required 128-bit UUID on its ATT-readable declaration.
    #[test]
    fn uuid32_service_declaration_uses_bluetooth_base_uuid() {
        use super::*;
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        let mut table: AttributeTable<'_, NoopRawMutex, 4> = AttributeTable::new();
        let handle = table.add_service(Service::new(0x12345678u32)).build();
        let mut bytes = [0; 20];
        let len = table.read(handle, 0, &mut bytes).unwrap();
        assert_eq!(
            &bytes[..len],
            &[0xfb, 0x34, 0x9b, 0x5f, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0x78, 0x56, 0x34, 0x12]
        );
    }

    /// A characteristic declaration preserves properties/handle and expands its UUID32 to sixteen bytes.
    #[test]
    fn uuid32_characteristic_declaration_uses_bluetooth_base_uuid() {
        use super::*;
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        let mut table: AttributeTable<'_, NoopRawMutex, 4> = AttributeTable::new();
        let ch = table
            .add_service(Service::new(0x180fu16))
            .add_characteristic_small(0x12345678u32, [CharacteristicProp::Read], 7u8)
            .build();
        let mut bytes = [0; 20];
        let len = table.read(ch.handle - 1, 0, &mut bytes).unwrap();
        assert_eq!(len, 19);
        assert_eq!(&bytes[..3], &[CharacteristicProp::Read as u8, ch.handle as u8, 0]);
        assert_eq!(
            &bytes[3..len],
            &[0xfb, 0x34, 0x9b, 0x5f, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0x78, 0x56, 0x34, 0x12]
        );
        let parsed = CharacteristicDeclaration::try_from(&bytes[..len]).unwrap();
        assert!(matches!(parsed.uuid, Uuid::Uuid128(_)));
    }

    #[cfg(feature = "security")]
    #[test]
    /// Known AES-CMAC transcripts cover GAP/GATT/custom declarations and selective descriptor hashing.
    fn database_hash() {
        use bt_hci::uuid::characteristic::{
            APPEARANCE, CLIENT_SUPPORTED_FEATURES, DATABASE_HASH, DEVICE_NAME, SERVICE_CHANGED,
        };
        use bt_hci::uuid::declarations::{CHARACTERISTIC, PRIMARY_SERVICE};
        use bt_hci::uuid::descriptors::{
            CHARACTERISTIC_PRESENTATION_FORMAT, CHARACTERISTIC_USER_DESCRIPTION, CLIENT_CHARACTERISTIC_CONFIGURATION,
        };
        use bt_hci::uuid::service::{GAP, GATT};
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;

        use super::*;

        // The raw message data that should be hashed for this attribute table is:
        //
        // 0100 0028 0018
        // 0200 0328 020300002a
        // 0400 0328 020500012a
        //
        // 0600 0028 0118
        // 0700 0328 200800052a
        // 0900 0229
        // 0a00 0328 0a0b00292b
        // 0c00 0328 020d002a2b
        //
        // 0e00 0028 f0debc9a785634127856341278563412
        // 0f00 0328 121000f1debc9a785634127856341278563412
        // 1100 0229
        // 1200 0129
        // 1300 0429
        //
        // The message hash can be calculated on the command line with:
        // > xxd -plain -revert message.txt message.bin
        // > openssl mac -cipher AES-128-CBC -macopt hexkey:00000000000000000000000000000000 -in message.bin CMAC

        let mut table: AttributeTable<'static, NoopRawMutex, 20> = AttributeTable::new();

        let ro = AttPermissions::read_only();
        let cccd_perms = AttPermissions {
            read: PermissionLevel::Allowed,
            write: PermissionLevel::Allowed,
            #[cfg(feature = "legacy-pairing")]
            min_key_len: 0,
        };

        // GAP service (handles 0x001 - 0x005)
        table.push(Attribute::new(PRIMARY_SERVICE, ro, GAP));

        let expected = 0xd4cdec10804db3f147b4d7d10baa0120;
        let actual = table.hash();
        assert_eq!(
            actual, expected,
            "\nexpected: {:#032x}\nactual: {:#032x}",
            expected, actual
        );

        // Device name characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new([CharacteristicProp::Read], 0x0003, DEVICE_NAME),
        ));

        table.push(Attribute::new(
            DEVICE_NAME,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        // Appearance characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new([CharacteristicProp::Read], 0x0005, APPEARANCE),
        ));

        table.push(Attribute::new(
            APPEARANCE,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        let expected = 0x6c329e3f1d52c03f174980f6b4704875;
        let actual = table.hash();
        assert_eq!(
            actual, expected,
            "\nexpected: {:#032x}\n  actual: {:#032x}",
            expected, actual
        );

        // GATT service (handles 0x006 - 0x000d)
        table.push(Attribute::new(PRIMARY_SERVICE, ro, GATT));

        // Service changed characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new([CharacteristicProp::Indicate], 0x0008, SERVICE_CHANGED),
        ));

        table.push(Attribute::new(
            SERVICE_CHANGED,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        table.push(Attribute::new(
            CLIENT_CHARACTERISTIC_CONFIGURATION,
            cccd_perms,
            AttributeData::ClientSpecific {
                variable_len: false,
                capacity: 2,
            },
        ));

        // Client supported features characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new(
                [CharacteristicProp::Read, CharacteristicProp::Write],
                0x000b,
                CLIENT_SUPPORTED_FEATURES,
            ),
        ));

        table.push(Attribute::new(
            CLIENT_SUPPORTED_FEATURES,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        // Database hash characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new([CharacteristicProp::Read], 0x000d, DATABASE_HASH),
        ));

        table.push(Attribute::new(
            DATABASE_HASH,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        let expected = 0x16ce756326c5062bf74022f845c2b21f;
        let actual = table.hash();
        assert_eq!(
            actual, expected,
            "\nexpected: {:#032x}\n  actual: {:#032x}",
            expected, actual
        );

        /// Custom 128-bit service UUID used by the final frozen hash transcript.
        const CUSTOM_SERVICE: u128 = 0x12345678_12345678_12345678_9abcdef0;
        /// Custom characteristic UUID adjacent to the frozen service UUID.
        const CUSTOM_CHARACTERISTIC: u128 = 0x12345678_12345678_12345678_9abcdef1;

        // Custom service (handles 0x00e - 0x0013)
        table.push(Attribute::new(PRIMARY_SERVICE, ro, Uuid::from(CUSTOM_SERVICE)));

        // Custom characteristic
        table.push(Attribute::new(
            CHARACTERISTIC,
            ro,
            CharacteristicDeclaration::new(
                [CharacteristicProp::Notify, CharacteristicProp::Read],
                0x0010,
                CUSTOM_CHARACTERISTIC,
            ),
        ));

        table.push(Attribute::new(
            CUSTOM_CHARACTERISTIC,
            ro,
            AttributeData::ReadOnlyData { value: b"" },
        ));

        table.push(Attribute::new(
            CLIENT_CHARACTERISTIC_CONFIGURATION,
            cccd_perms,
            AttributeData::ClientSpecific {
                variable_len: false,
                capacity: 2,
            },
        ));

        table.push(Attribute::new(
            CHARACTERISTIC_USER_DESCRIPTION,
            ro,
            AttributeData::ReadOnlyData {
                value: b"Custom Characteristic",
            },
        ));

        table.push(Attribute::new(
            CHARACTERISTIC_PRESENTATION_FORMAT,
            ro,
            AttributeData::ReadOnlyData {
                value: &[4, 0, 0, 0x27, 1, 0, 0],
            },
        ));

        let expected = 0xc7352cced28d6608d4b057d247d8be76;
        let actual = table.hash();
        assert_eq!(
            actual, expected,
            "\nexpected: {:#032x}\n  actual: {:#032x}",
            expected, actual
        );
    }

    #[test]
    /// Replacing a borrowed variable value with capacity-sized bytes updates its logical length.
    fn set_updates_variable_length_when_value_fills_backing_storage() {
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        use heapless::Vec;

        use super::*;

        let mut storage = [0u8; 4];
        let mut table: AttributeTable<'_, NoopRawMutex, 4> = AttributeTable::new();
        let initial = Vec::<u8, 4>::from_slice(b"ab").unwrap();
        let characteristic = table
            .add_service(Service {
                uuid: Uuid::new_long([0x10; 16]),
            })
            .add_characteristic(
                Uuid::new_long([0x11; 16]),
                [CharacteristicProp::Read, CharacteristicProp::Write],
                initial,
                &mut storage,
            )
            .build();

        let replacement = Vec::<u8, 4>::from_slice(b"wxyz").unwrap();
        table.set(&characteristic, &replacement).unwrap();

        let stored: Vec<u8, 4> = table.get(&characteristic).unwrap();
        assert_eq!(stored.as_slice(), b"wxyz");
    }

    #[test]
    /// Replacing an inline string at its full capacity exposes every new byte through the typed getter.
    fn set_updates_small_variable_length_when_value_reaches_capacity() {
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;
        use heapless::String;

        use super::*;

        let mut table: AttributeTable<'_, NoopRawMutex, 4> = AttributeTable::new();
        let initial = String::<8>::try_from("hi").unwrap();
        let characteristic = table
            .add_service(Service {
                uuid: Uuid::new_long([0x12; 16]),
            })
            .add_characteristic_small(
                Uuid::new_long([0x13; 16]),
                [CharacteristicProp::Read, CharacteristicProp::Write],
                initial,
            )
            .build();

        let replacement = String::<8>::try_from("12345678").unwrap();
        table.set(&characteristic, &replacement).unwrap();

        let stored: String<8> = table.get(&characteristic).unwrap();
        assert_eq!(stored.as_str(), "12345678");
    }
}
