//! Bounded serialized per-client attribute values and identity retention.
//! Handles use 15 bits in this storage format; this is narrower than ATT's wire handle range.

use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;

use super::{AttributeData, AttributeTable, Client};
use crate::att::AttErrorCode;
use crate::config::CLIENT_ATT_TABLE_SIZE;
use crate::{Error, Identity};

/// Bytes occupied by the little-endian count and value-region length.
const HEADER_SIZE: usize = core::mem::size_of::<Header>();
/// Bytes per sorted handle/relative-offset index entry.
const ENTRY_SIZE: usize = core::mem::size_of::<Entry>();
/// High handle bit reserved by this serialized format for a two-byte logical-length prefix.
const VARIABLE_LEN_FLAG: u16 = 0x8000;

/// A compact, fixed-size map of client-specific attribute values (e.g. CCCDs).
///
/// Entries are stored in a flat byte buffer with a sorted index for binary-search lookups.
/// [`CLIENT_ATT_TABLE_SIZE`] determines the total storage available for both the index and values.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Debug)]
#[repr(align(2))]
pub struct ClientAttTable {
    /// Fixed backing allocation; serialization exposes only the declared header, index and value region.
    buf: [u8; CLIENT_ATT_TABLE_SIZE],
}

impl ClientAttTable {
    /// Returns a zeroed builder. Compilation rejects a configured buffer smaller than the header or larger than 65535 bytes.
    pub const fn builder() -> ClientAttTableBuilder {
        const { core::assert!(CLIENT_ATT_TABLE_SIZE >= HEADER_SIZE && CLIENT_ATT_TABLE_SIZE <= u16::MAX as usize) };

        ClientAttTableBuilder {
            buf: [0; CLIENT_ATT_TABLE_SIZE],
            values_len: 0,
        }
    }

    /// Returns a borrowed validated view of the owning table, including current logical value lengths.
    pub const fn view(&self) -> ClientAttTableView<'_> {
        ClientAttTableView { buf: &self.buf }
    }

    /// Returns the encoded entry count and value-region length.
    const fn header(&self) -> Header {
        self.view().header()
    }

    // If `i` is a variable length attribute, set its length to `len`. For fixed length attributes, do nothing.
    /// Sets the logical byte length of entry `i` to `len` when variable; fixed entries keep their capacity as length. Panics for an invalid index or length beyond capacity.
    fn set_variable_len(&mut self, i: usize, len: u16) {
        assert!(len as usize <= self.view().value_capacity(i));
        if self.view().index()[i].is_variable_len() {
            let start = self.view().raw_value_start(i);
            self.buf[start..][..2].copy_from_slice(&len.to_le_bytes());
        }
    }

    /// Returns the current value for handle `key`, or `None` for absent keys or keys outside this format's 1..=32767 range.
    pub fn get(&self, key: u16) -> Option<&[u8]> {
        self.view().get(key)
    }

    /// Writes `data` at byte `offset` in handle `key`. Returns `Ok(())`, or ATTRIBUTE_NOT_FOUND, INVALID_OFFSET (past current length), or INVALID_ATTRIBUTE_VALUE_LENGTH (past capacity), without mutation on rejection. Variable values end at offset plus input length; fixed values retain their full length.
    pub fn write(&mut self, key: u16, offset: usize, data: &[u8]) -> Result<(), AttErrorCode> {
        if key >= VARIABLE_LEN_FLAG {
            return Err(AttErrorCode::ATTRIBUTE_NOT_FOUND);
        }

        let i = self.view().find(key).ok_or(AttErrorCode::ATTRIBUTE_NOT_FOUND)?;
        if offset > self.view().value_len(i) {
            Err(AttErrorCode::INVALID_OFFSET)
        } else if offset + data.len() > self.view().value_capacity(i) {
            Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH)
        } else {
            let start = self.view().value_start(i) + offset;
            let end = start + data.len();
            self.buf[start..end].copy_from_slice(data);

            self.set_variable_len(i, (offset + data.len()) as u16);

            Ok(())
        }
    }

    /// Copies matching values from `src`, truncating to destination capacity and zeroing unused capacity or missing keys. Variable lengths become copied lengths; fixed lengths and the destination index remain unchanged. Extra source keys are ignored.
    pub fn set_values(&mut self, src: &ClientAttTableView<'_>) {
        for i in 0..self.header().att_count() {
            let view = self.view();
            let key = view.index()[i].key();
            match src.get(key) {
                Some(src) => {
                    let start = view.value_start(i);
                    let capacity = view.value_capacity(i);
                    let dest = &mut self.buf[start..][..capacity];

                    let copy_len = dest.len().min(src.len());
                    dest[..copy_len].copy_from_slice(&src[..copy_len]);
                    dest[copy_len..].fill(0);

                    self.set_variable_len(i, copy_len as u16);
                }
                None => {
                    let start = view.raw_value_start(i);
                    let end = view.value_end(i);
                    self.buf[start..end].fill(0);
                }
            }
        }
    }

    /// Zeros the value region, retaining handles and capacities. Variable values become empty; fixed values keep their length with zero contents.
    pub fn clear(&mut self) {
        let header = self.header();
        self.buf[header.values_base()..header.values_end()].fill(0);
    }

    /// Returns header, index and declared value-region bytes, including unused variable capacity; excludes unused backing allocation.
    pub fn raw(&self) -> &[u8] {
        self.view().raw()
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Little-endian entry count followed by value-region byte length; byte alignment permits borrowed decoding.
struct Header(
    /// Two little-endian u16 fields: entry count and value-region length.
    [u8; 4],
);

impl Header {
    /// Returns the number of four-byte index entries.
    const fn att_count(&self) -> usize {
        u16::from_le_bytes([self.0[0], self.0[1]]) as usize
    }

    /// Returns the absolute byte offset immediately after the header and index.
    const fn values_base(&self) -> usize {
        HEADER_SIZE + self.att_count() * ENTRY_SIZE
    }

    /// Returns the encoded byte length of the entire value region, including length prefixes and spare capacity.
    const fn values_len(&self) -> usize {
        u16::from_le_bytes([self.0[2], self.0[3]]) as usize
    }

    /// Returns the exclusive absolute byte offset of the declared value region.
    const fn values_end(&self) -> usize {
        self.values_base() + self.values_len()
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Four-byte serialized handle/variable flag followed by a relative value-region offset.
struct Entry(
    /// Little-endian handle/flag and value-region-relative offset.
    [u8; 4],
);

impl Entry {
    /// Returns the 15-bit handle with the variable-length flag removed.
    const fn key(&self) -> u16 {
        let key = u16::from_le_bytes([self.0[0], self.0[1]]);
        key & !VARIABLE_LEN_FLAG
    }

    /// Returns the byte offset relative to the start of the value region, including any length prefix.
    const fn offset(&self) -> usize {
        u16::from_le_bytes([self.0[2], self.0[3]]) as usize
    }

    /// Returns whether the value begins with a two-byte logical-length prefix.
    const fn is_variable_len(&self) -> bool {
        let key = u16::from_le_bytes([self.0[0], self.0[1]]);
        (key & VARIABLE_LEN_FLAG) != 0
    }

    /// Encodes handle `key`, relative byte `offset`, and `variable_len` flag. The builder must supply a key with the high bit clear.
    const fn set(&mut self, key: u16, offset: u16, variable_len: bool) {
        let flag = if variable_len { VARIABLE_LEN_FLAG } else { 0 };
        let key = (key | flag).to_le_bytes();
        let offset = offset.to_le_bytes();
        self.0 = [key[0], key[1], offset[0], offset[1]];
    }
}

/// A read-only view over a serialized [`ClientAttTable`].
///
/// The view borrows the raw table format directly: a little-endian entry count, a little-endian value byte length, a
/// sorted index of 4-byte entries, and the value bytes. Unlike [`ClientAttTable`], the borrowed bytes may come from
/// arbitrary storage and do not need the owning table's alignment. Use [`try_from_raw()`](Self::try_from_raw) to
/// validate the buffer before reading values.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ClientAttTableView<'a> {
    /// Validated borrowed serialization; may include trailing storage excluded by raw().
    buf: &'a [u8],
}

impl<'a> ClientAttTableView<'a> {
    /// Validates serialized `data` and returns a borrowed view, or InvalidValue for short header/index/value bounds, unsorted or duplicate/zero keys, decreasing/out-of-range offsets, or a variable length beyond capacity. Trailing storage and unused leading value bytes are accepted; no alignment is required.
    pub fn try_from_raw(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < HEADER_SIZE {
            return Err(Error::InvalidValue);
        }

        let map = Self { buf: data };

        if map.header().values_end() > data.len() {
            return Err(Error::InvalidValue);
        }

        // Validate the index
        let mut last_key = 0;
        let mut last_offset = 0;
        for e in map.index().iter() {
            let key = e.key();
            let offset = e.offset();
            if key <= core::mem::replace(&mut last_key, key)
                || offset < core::mem::replace(&mut last_offset, offset)
                || offset > map.header().values_len()
            {
                return Err(Error::InvalidValue);
            }
        }

        // Validate variable length attributes
        for i in 0..map.header().att_count() {
            if map.value_end(i) < map.value_start(i) || map.value_len(i) > map.value_capacity(i) {
                return Err(Error::InvalidValue);
            }
        }

        Ok(map)
    }

    /// Returns the first four bytes as a header; requires a validated or builder-owned buffer.
    const fn header(&self) -> Header {
        Header([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
    }

    /// Returns the declared index as byte-aligned entries. Header bounds must already be validated; no typed alignment beyond u8 is required.
    fn index(&self) -> &[Entry] {
        let header = self.header();
        let chunks = self.buf[HEADER_SIZE..header.values_base()].as_chunks::<ENTRY_SIZE>().0;
        // SAFETY: ClientAttTableEntry is repr(transparent) over [u8; ENTRY_SIZE], so it has the same size and
        // alignment as [u8; ENTRY_SIZE]. Every ENTRY_SIZE-byte chunk is therefore aligned and valid for reads as a
        // ClientAttTableEntry.
        unsafe { core::slice::from_raw_parts(chunks.as_ptr().cast::<Entry>(), chunks.len()) }
    }

    /// Returns the absolute byte start of entry `i`, including a variable-length prefix. Panics when `i` is outside the validated index.
    fn raw_value_start(&self, i: usize) -> usize {
        let header = self.header();
        let entry = self.index()[i];
        entry.offset() + header.values_base()
    }

    /// Returns the absolute payload start of entry `i`, skipping its optional two-byte prefix. Panics for an invalid index.
    fn value_start(&self, i: usize) -> usize {
        let header = self.header();
        let entry = self.index()[i];
        let start = entry.offset() + header.values_base();
        if entry.is_variable_len() {
            start + 2
        } else {
            start
        }
    }

    /// Returns the exclusive capacity end for entry `i`, using the next offset or the value-region end. Internal callers provide a valid index.
    fn value_end(&self, i: usize) -> usize {
        let header = self.header();
        self.index()
            .get(i + 1)
            .map(Entry::offset)
            .unwrap_or(header.values_len())
            + header.values_base()
    }

    /// Returns payload capacity in bytes for entry `i`, saturating malformed reversed bounds to zero during validation; panics for an invalid index.
    fn value_capacity(&self, i: usize) -> usize {
        self.value_end(i).saturating_sub(self.value_start(i))
    }

    /// Returns the logical byte length of entry `i`: its stored prefix for variable values, otherwise its capacity. Requires validated prefix bounds and a valid index.
    fn value_len(&self, i: usize) -> usize {
        let index = self.index();
        if index[i].is_variable_len() {
            let start = self.raw_value_start(i);
            u16::from_le_bytes([self.buf[start], self.buf[start + 1]]) as usize
        } else {
            self.value_capacity(i)
        }
    }

    /// Returns the index of exact handle `key` by binary search, or `None` when absent.
    fn find(&self, key: u16) -> Option<usize> {
        self.index().binary_search_by_key(&key, Entry::key).ok()
    }

    /// Returns the logical bytes for handle `key`, or `None` for absent keys or the reserved high bit. The borrow retains the original backing lifetime.
    pub fn get(&self, key: u16) -> Option<&'a [u8]> {
        if key >= VARIABLE_LEN_FLAG {
            None
        } else {
            let i = self.find(key)?;
            Some(&self.buf[self.value_start(i)..][..self.value_len(i)])
        }
    }

    /// Returns only the declared serialization through the value-region end; ignores trailing bytes supplied at construction.
    pub fn raw(&self) -> &'a [u8] {
        let header = self.header();
        &self.buf[..header.values_end()]
    }
}

/// A builder for [`ClientAttTable`].
///
/// Entries must be pushed in ascending key order. Use [`build()`](Self::build) to finalize.
#[derive(Debug, Clone)]
pub struct ClientAttTableBuilder {
    /// Zero-initialized header/index/value allocation; overflow is accumulated for build-time diagnostics.
    buf: [u8; CLIENT_ATT_TABLE_SIZE],
    /// Required total value-region bytes, including two-byte prefixes for variable entries.
    values_len: u16,
}

impl ClientAttTableBuilder {
    /// Reserves handle `key` with `value_len` bytes of payload capacity; `variable_len` adds a two-byte length prefix initially zero. Fixed values start zeroed at full length. Panics for zero/high-bit keys, capacity above 512, value-size overflow, or nonascending keys while storage fits. Allocation overflow is deferred to build; panics may leave builder counters changed.
    pub fn push(&mut self, key: u16, mut value_len: u16, variable_len: bool) {
        assert!(value_len <= 512, "Bluetooth attributes must be at most 512 bytes");
        assert!(key > 0, "Bluetooth handles must be greater than 0");
        assert!(key <= 0x7fff, "Handle values above 0x7fff are reserved");

        let old_att_count = self.att_count();
        self.set_att_count(old_att_count + 1);
        let offset = self.values_len;

        if variable_len {
            value_len += 2;
        }

        self.values_len = self
            .values_len
            .checked_add(value_len)
            .expect("ClientAttTable buffer overflow");
        self.set_values_len(self.values_len);

        // If we overflow, just keep tracking the total needed size so we can report it in build()
        if self.end() <= CLIENT_ATT_TABLE_SIZE {
            let index = self.index_mut();
            if old_att_count > 0 {
                let last_key = index[old_att_count - 1].key();
                assert!(key > last_key, "keys must be inserted in ascending order");
            }
            index[old_att_count].set(key, offset, variable_len);
        }
    }

    /// Returns the completed zero-initialized table. Panics if the required serialization exceeds configured storage; logs a warning when storage is larger than required.
    pub fn build(self) -> ClientAttTable {
        let end = self.end();
        if end > CLIENT_ATT_TABLE_SIZE {
            panic!(
                "ClientAttTable buffer ({} bytes) overflow. Need {} bytes for exact size",
                CLIENT_ATT_TABLE_SIZE, end
            );
        } else if end < CLIENT_ATT_TABLE_SIZE {
            warn!(
                "ClientAttTable buffer ({} bytes) oversized. Only need {} bytes for exact size",
                CLIENT_ATT_TABLE_SIZE, end
            );
        }

        ClientAttTable { buf: self.buf }
    }

    /// Returns the current encoded entry count, including entries whose required storage overflowed.
    const fn att_count(&self) -> usize {
        u16::from_le_bytes([self.buf[0], self.buf[1]]) as usize
    }

    /// Stores `len` as the little-endian entry count; internal construction must keep it representable as u16.
    fn set_att_count(&mut self, len: usize) {
        self.buf[..2].copy_from_slice(&(len as u16).to_le_bytes())
    }

    /// Stores `len`, the required value-region byte count including length prefixes.
    fn set_values_len(&mut self, len: u16) {
        self.buf[2..4].copy_from_slice(&len.to_le_bytes())
    }

    /// Returns unique mutable byte-aligned index entries. Panics if the declared count exceeds the backing buffer; called only while required storage fits.
    const fn index_mut(&mut self) -> &mut [Entry] {
        let end = self.att_count() * ENTRY_SIZE;
        let (_, slice) = self.buf.split_at_mut(HEADER_SIZE);
        let (slice, _) = slice.split_at_mut(end);
        let chunks = slice.as_chunks_mut::<ENTRY_SIZE>().0;
        // SAFETY: ClientAttTableEntry is repr(transparent) over [u8; ENTRY_SIZE], so it has the same size and
        // alignment as [u8; ENTRY_SIZE]. The mutable chunks come from the table's uniquely borrowed buffer, so the
        // returned entries are uniquely borrowed for the same lifetime.
        unsafe { core::slice::from_raw_parts_mut(chunks.as_mut_ptr() as *mut _, chunks.len()) }
    }

    /// Returns required absolute start of the value region after the current index.
    const fn values_base(&self) -> usize {
        HEADER_SIZE + self.att_count() * ENTRY_SIZE
    }

    /// Returns total required serialization bytes, which may exceed the allocation until build reports overflow.
    const fn end(&self) -> usize {
        self.values_base() + self.values_len as usize
    }
}

/// Bounded per-identity values, retaining bonded disconnected peers until a slot is reused.
pub(crate) struct ClientAttTables<M: RawMutex, const CONN_MAX: usize> {
    /// Mutex-protected client identities and independent value buffers; callbacks execute while the RefCell borrow is held.
    state: Mutex<M, RefCell<[(Client, ClientAttTable); CONN_MAX]>>,
}

impl<M: RawMutex, const CONN_MAX: usize> ClientAttTables<M, CONN_MAX> {
    /// Returns one zeroed client-value layout per connection slot, derived from client-specific entries in `att_table`. Panics if their handles/capacities or total serialization violate the builder limits.
    pub(crate) fn new<const ATT_MAX: usize>(att_table: &AttributeTable<'_, M, ATT_MAX>) -> Self {
        let mut builder = ClientAttTable::builder();
        att_table.iterate(|at| {
            for (handle, att) in at {
                if let AttributeData::ClientSpecific { variable_len, capacity } = att.data {
                    builder.push(handle, capacity, variable_len);
                }
            }
        });
        let base = builder.build();
        let values: [(Client, ClientAttTable); CONN_MAX] = core::array::from_fn(|_| (Client::default(), base.clone()));
        Self {
            state: Mutex::new(RefCell::new(values)),
        }
    }

    /// Connects `peer_identity`, preferring an existing matching identity before an empty slot, then replacing the first disconnected peer with zero values. Returns ConnectionLimitReached when every slot belongs to a connected different peer; successful reconnect preserves retained values.
    pub(crate) fn connect(&self, peer_identity: &Identity) -> Result<(), Error> {
        self.state.lock(|n| {
            trace!("[server] searching for peer {:?}", peer_identity.addr);
            let mut n = n.borrow_mut();
            let empty_slot = Identity::default();
            // Resolve a retained identity before considering an earlier vacant slot.
            for (client, _) in n.iter_mut() {
                if client.identity.match_identity(peer_identity) {
                    client.is_connected = true;
                    return Ok(());
                }
            }
            for (client, _) in n.iter_mut() {
                if client.identity == empty_slot {
                    //  trace!("[server] empty slot: connecting");
                    client.is_connected = true;
                    client.set_identity(*peer_identity);
                    return Ok(());
                }
            }
            trace!("[server] all slots full...");
            // if we got here all slots are full; replace the first disconnected client
            for (client, table) in n.iter_mut() {
                if !client.is_connected {
                    trace!("[server] booting disconnected peer {:?}", client.identity.addr);
                    client.is_connected = true;
                    client.set_identity(*peer_identity);
                    // erase the previous client's config
                    table.clear();
                    return Ok(());
                }
            }
            // Should be unreachable if the max connections (CONN_MAX) matches that defined
            // in HostResources...
            warn!("[server] unable to obtain client attributes slot");
            Err(Error::ConnectionLimitReached)
        })
    }

    /// Marks matching `peer_identity` disconnected. If `bonded` is false, clears its identity and values; otherwise retains values for reconnect. Missing peers are ignored.
    pub(crate) fn disconnect(&self, peer_identity: &Identity, bonded: bool) {
        self.state.lock(|n| {
            let mut n = n.borrow_mut();
            for (client, table) in n.iter_mut() {
                if client.identity.match_identity(peer_identity) {
                    if !bonded {
                        *client = Client::default();
                        table.clear();
                    } else {
                        client.is_connected = false;
                    }
                    break;
                }
            }
        })
    }

    /// Calls `f` with the logical bytes for `att_handle` owned by `peer_identity`, Returns `Some` of its result or `None` for missing peer/handle. The table borrow remains held: `f` must not reenter a mutating table operation.
    pub(crate) fn with_value<R>(
        &self,
        peer_identity: &Identity,
        att_handle: u16,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        self.state.lock(|n| {
            let n = n.borrow();
            for (client, table) in n.iter() {
                if client.identity.match_identity(peer_identity) {
                    return table.get(att_handle).map(f);
                }
            }
            None
        })
    }

    /// Copies the value for `peer_identity` and `att_handle` from byte `offset` into `data`. Returns copied byte count (zero at end), INVALID_OFFSET past end, or ATTRIBUTE_NOT_FOUND for missing peer/handle; short output truncates successfully.
    pub(crate) fn read(
        &self,
        peer_identity: &Identity,
        att_handle: u16,
        offset: usize,
        data: &mut [u8],
    ) -> Result<usize, AttErrorCode> {
        self.state.lock(|n| {
            let n = n.borrow();
            for (client, table) in n.iter() {
                if client.identity.match_identity(peer_identity) {
                    let value = table.get(att_handle).ok_or(AttErrorCode::ATTRIBUTE_NOT_FOUND)?;
                    if offset > value.len() {
                        return Err(AttErrorCode::INVALID_OFFSET);
                    }
                    let value = &value[offset..];
                    let len = value.len().min(data.len());
                    data[..len].copy_from_slice(value);
                    return Ok(len);
                }
            }
            Err(AttErrorCode::ATTRIBUTE_NOT_FOUND)
        })
    }

    /// Writes `data` at byte `offset` for `peer_identity` and `att_handle`. Returns the table write result, including missing handle/peer, invalid offset or capacity errors; variable values truncate at the written end.
    pub(crate) fn write(
        &self,
        peer_identity: &Identity,
        att_handle: u16,
        offset: usize,
        data: &[u8],
    ) -> Result<(), AttErrorCode> {
        self.state.lock(|n| {
            let mut n = n.borrow_mut();
            for (client, table) in n.iter_mut() {
                if client.identity.match_identity(peer_identity) {
                    return table.write(att_handle, offset, data);
                }
            }
            Err(AttErrorCode::ATTRIBUTE_NOT_FOUND)
        })
    }

    /// Returns an owned snapshot of values for `peer_identity`, including a retained disconnected peer, or `None` when absent.
    pub(crate) fn get_client_att_table(&self, peer_identity: &Identity) -> Option<ClientAttTable> {
        self.state.lock(|n| {
            let n = n.borrow();
            for (client, table) in n.iter() {
                if client.identity.match_identity(peer_identity) {
                    return Some(table.clone());
                }
            }
            None
        })
    }

    /// Copies matching values from validated `table` into `peer_identity`'s existing layout, truncating/zeroing as set_values specifies. Missing peers are ignored; connection state and index are unchanged.
    pub(crate) fn set_client_att_table(&self, peer_identity: &Identity, table: &ClientAttTableView<'_>) {
        self.state.lock(|n| {
            let mut n = n.borrow_mut();
            for (client, t) in n.iter_mut() {
                if client.identity.match_identity(peer_identity) {
                    trace!("Setting client attribute table for {:?}", peer_identity.addr);
                    t.set_values(table);
                    break;
                }
            }
        })
    }

    /// Replaces a matching stored identity with `identity`, retaining connection state and values. Returns NotFound when the supplied address/IRK matches no stored peer.
    pub(crate) fn update_identity(&self, identity: Identity) -> Result<(), Error> {
        self.state.lock(|n| {
            let mut n = n.borrow_mut();
            for (client, _) in n.iter_mut() {
                if identity.match_identity(&client.identity) {
                    client.set_identity(identity);
                    return Ok(());
                }
            }
            Err(Error::NotFound)
        })
    }
}

#[cfg(test)]
/// Serialization boundaries, value mutation and retained-client identity regressions.
mod tests {
    use super::{ClientAttTable, ClientAttTableView};
    use crate::att::AttErrorCode;

    /// Reconnection prefers retained bonded CCCDs over an earlier vacancy, without duplicate identities.
    #[test]
    fn bonded_reconnect_preserves_values_before_empty_slot() {
        use super::{AttributeTable, ClientAttTables};
        use crate::prelude::*;
        use crate::{Address, Error, Identity};
        use bt_hci::param::{AddrKind, BdAddr};
        use embassy_sync::blocking_mutex::raw::NoopRawMutex;

        let mut attributes = AttributeTable::<'_, NoopRawMutex, 8>::new();
        let characteristic = attributes
            .add_service(Service::new(0x180fu16))
            .add_characteristic_small(0x2a19u16, [CharacteristicProp::Read, CharacteristicProp::Notify], 0u8)
            .build();
        let handle = characteristic.cccd_handle.unwrap();
        let tables = ClientAttTables::<NoopRawMutex, 2>::new(&attributes);
        let a = Identity::from(Address::new(AddrKind::PUBLIC, BdAddr::new([1; 6])));
        let b = Identity::from(Address::new(AddrKind::PUBLIC, BdAddr::new([2; 6])));
        let c = Identity::from(Address::new(AddrKind::PUBLIC, BdAddr::new([3; 6])));
        tables.connect(&a).unwrap();
        tables.connect(&b).unwrap();
        tables.write(&b, handle, 0, &[1, 0]).unwrap();
        assert_eq!(tables.connect(&c), Err(Error::ConnectionLimitReached));
        tables.disconnect(&a, false);
        tables.disconnect(&b, true);
        for _ in 0..2 {
            tables.connect(&b).unwrap();
            assert_eq!(
                tables.get_client_att_table(&b).unwrap().get(handle),
                Some([1, 0].as_slice())
            );
            tables.disconnect(&b, true);
        }
        tables.connect(&b).unwrap();
        tables.connect(&c).unwrap();
        assert_eq!(tables.connect(&a), Err(Error::ConnectionLimitReached));
        tables.disconnect(&b, false);
        tables.connect(&b).unwrap();
        assert_eq!(
            tables.get_client_att_table(&b).unwrap().get(handle),
            Some([0, 0].as_slice())
        );
        assert_eq!(
            tables.get_client_att_table(&c).unwrap().get(handle),
            Some([0, 0].as_slice())
        );
    }

    /// Checks that serialized `data` is rejected before a borrowed table can expose malformed bounds.
    fn assert_invalid(data: &[u8]) {
        assert!(ClientAttTableView::try_from_raw(data).is_err());
    }

    #[test]
    /// The declared value-region length bounds serialization while borrowed trailing storage remains inaccessible.
    fn raw_view_uses_declared_values_len_and_ignores_trailing_storage() {
        #[rustfmt::skip]
        let data = [
            2, 0, 5, 0,
            1, 0, 0, 0,
            2, 0x80, 2, 0,
            0xaa, 0xbb, 1, 0, 0xcc,
            0xdd, 0xee,
        ];

        let view = ClientAttTableView::try_from_raw(&data).unwrap();

        assert_eq!(view.raw(), &data[..17]);
        assert_eq!(view.get(1), Some([0xaa, 0xbb].as_slice()));
        assert_eq!(view.get(2), Some([0xcc].as_slice()));
    }

    #[test]
    /// Rejects truncated regions, duplicate masked keys and decreasing/out-of-range offsets.
    fn raw_view_rejects_malformed_table_boundaries_and_index() {
        // values_len promises two bytes, but only one is present.
        assert_invalid(&[1, 0, 2, 0, 1, 0, 0, 0, 0]);

        // att_count places the index beyond the provided buffer.
        assert_invalid(&[2, 0, 0, 0, 1, 0, 0, 0]);

        // Keys must be strictly ascending after masking off the variable-length flag.
        #[rustfmt::skip]
        let duplicate_key_after_masking = [
            2, 0, 2, 0,
            1, 0, 0, 0,
            1, 0x80, 1, 0,
            0, 0,
        ];
        assert_invalid(&duplicate_key_after_masking);

        // Offsets must be monotonically increasing and remain within values_len.
        #[rustfmt::skip]
        let decreasing_offset = [
            2, 0, 4, 0,
            1, 0, 2, 0,
            2, 0, 1, 0,
            0, 0, 0, 0,
        ];
        assert_invalid(&decreasing_offset);

        #[rustfmt::skip]
        let offset_past_values_len = [
            1, 0, 1, 0,
            1, 0, 2, 0,
            0,
        ];
        assert_invalid(&offset_past_values_len);
    }

    #[test]
    /// A variable logical length cannot consume the following fixed entry's bytes.
    fn raw_view_rejects_variable_length_exceeding_encoded_capacity() {
        #[rustfmt::skip]
        let data = [
            2, 0, 8, 0,
            1, 0x80, 0, 0,
            2, 0, 6, 0,
            5, 0, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];

        assert_invalid(&data);
    }

    #[test]
    /// Checks append, overwrite truncation and exact-capacity writes, with rejected gaps and overflow preserving data.
    fn variable_length_writes_append_overwrite_truncate_and_preserve_capacity() {
        let mut builder = ClientAttTable::builder();
        builder.push(1, 4, true);
        let mut table = builder.build();

        assert_eq!(table.get(1), Some([].as_slice()));
        assert_eq!(table.write(1, 1, &[0xaa]), Err(AttErrorCode::INVALID_OFFSET));

        table.write(1, 0, &[1, 2]).unwrap();
        table.write(1, 2, &[3, 4]).unwrap();
        assert_eq!(table.get(1), Some([1, 2, 3, 4].as_slice()));
        assert_eq!(
            table.write(1, 4, &[5]),
            Err(AttErrorCode::INVALID_ATTRIBUTE_VALUE_LENGTH)
        );

        table.write(1, 1, &[9]).unwrap();
        assert_eq!(table.get(1), Some([1, 9].as_slice()));

        table.write(1, 2, &[7, 8]).unwrap();
        assert_eq!(table.get(1), Some([1, 9, 7, 8].as_slice()));
    }

    #[test]
    /// Snapshot restore truncates matching values, zero-fills spare/missing destination bytes and ignores unknown source keys.
    fn set_values_copies_matching_keys_truncates_to_capacity_and_zeros_missing_keys() {
        let mut src_builder = ClientAttTable::builder();
        src_builder.push(1, 2, false);
        src_builder.push(2, 5, true);
        src_builder.push(4, 1, false);
        let mut src = src_builder.build();
        src.write(1, 0, &[0x11, 0x22]).unwrap();
        src.write(2, 0, &[0x33, 0x44, 0x55, 0x66, 0x77]).unwrap();
        src.write(4, 0, &[0x88]).unwrap();

        let mut dst_builder = ClientAttTable::builder();
        dst_builder.push(1, 4, false);
        dst_builder.push(2, 3, true);
        dst_builder.push(3, 2, false);
        let mut dst = dst_builder.build();
        dst.write(1, 0, &[0xaa, 0xaa, 0xaa, 0xaa]).unwrap();
        dst.write(2, 0, &[0xbb, 0xbb]).unwrap();
        dst.write(3, 0, &[0xcc, 0xcc]).unwrap();

        dst.set_values(&src.view());

        assert_eq!(dst.get(1), Some([0x11, 0x22, 0, 0].as_slice()));
        assert_eq!(dst.get(2), Some([0x33, 0x44, 0x55].as_slice()));
        assert_eq!(dst.get(3), Some([0, 0].as_slice()));
        assert_eq!(dst.get(4), None);
    }

    #[test]
    /// Clearing preserves serialized index/capacities while zeroing fixed values and emptying variable values.
    fn clear_zeroes_value_region_without_changing_table_shape() {
        let mut builder = ClientAttTable::builder();
        builder.push(1, 2, false);
        builder.push(2, 3, true);
        let mut table = builder.build();
        table.write(1, 0, &[0xaa, 0xbb]).unwrap();
        table.write(2, 0, &[0xcc, 0xdd]).unwrap();
        let raw_len = table.raw().len();
        let mut header_and_index = [0; 12];
        header_and_index.copy_from_slice(&table.raw()[..12]);

        table.clear();

        assert_eq!(table.raw().len(), raw_len);
        assert_eq!(&table.raw()[..12], header_and_index.as_slice());
        assert_eq!(table.get(1), Some([0, 0].as_slice()));
        assert_eq!(table.get(2), Some([].as_slice()));
        assert_eq!(&table.raw()[12..], &[0, 0, 0, 0, 0, 0, 0]);
    }
}

/// Actual trace capture in an isolated test process, so global logger ownership cannot race other tests.
#[cfg(all(test, feature = "security", feature = "log"))]
mod diagnostic_tests {
    extern crate std;
    use super::*;
    use crate::prelude::*;
    use crate::{Address, IdentityResolvingKey};
    use bt_hci::param::{AddrKind, BdAddr};
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;

    /// Trace-only sink for this module; stored lines contain synthetic fixture data only.
    struct Capture;
    /// Captured automatic client-table traces in the isolated child process.
    static LINES: std::sync::Mutex<std::vec::Vec<std::string::String>> = std::sync::Mutex::new(std::vec::Vec::new());
    /// Process-lifetime log sink required by the logging facade.
    static LOGGER: Capture = Capture;

    impl log::Log for Capture {
        /// Returns whether `metadata` originated in this client-table module.
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target() == module_path!().trim_end_matches("::diagnostic_tests")
        }
        /// Formats matching `record` arguments as the production logger would; appends them to the test buffer.
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                LINES.lock().unwrap().push(std::format!("{}", record.args()));
            }
        }
        /// No buffered I/O exists beyond the synchronous in-memory line vector.
        fn flush(&self) {}
    }

    /// Automatic connect, eviction and restore traces are independent of IRKs and client-value contents.
    #[test]
    fn client_diagnostics_omit_identity_keys_and_values() {
        /// Environment marker preventing recursive child-test spawning.
        const CHILD: &str = "TROUBLE_ATT_TRACE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "attribute_server::client_att_table::diagnostic_tests::client_diagnostics_omit_identity_keys_and_values", "--nocapture", "--test-threads=1"])
                .env(CHILD, "1").status().unwrap();
            assert!(status.success());
            return;
        }
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        let mut snapshots = std::vec::Vec::new();
        for seed in [0x11u8, 0x22] {
            let mut attributes = AttributeTable::<'_, NoopRawMutex, 8>::new();
            let ch = attributes
                .add_service(Service::new(0x180fu16))
                .add_characteristic_small(0x2a19u16, [CharacteristicProp::Read, CharacteristicProp::Notify], 0u8)
                .build();
            let tables = ClientAttTables::<NoopRawMutex, 1>::new(&attributes);
            let mut a = Identity::from(Address::new(AddrKind::PUBLIC, BdAddr::new([1; 6])));
            a.irk = IdentityResolvingKey::new(u128::from_le_bytes([seed; 16]));
            let mut b = Identity::from(Address::new(AddrKind::PUBLIC, BdAddr::new([2; 6])));
            b.irk = IdentityResolvingKey::new(u128::from_le_bytes([seed + 1; 16]));
            LINES.lock().unwrap().clear();
            tables.connect(&a).unwrap();
            let mut values = tables.get_client_att_table(&a).unwrap();
            values.write(ch.cccd_handle.unwrap(), 0, &[seed, 0]).unwrap();
            tables.set_client_att_table(&a, &values.view());
            tables.disconnect(&a, true);
            tables.connect(&b).unwrap();
            snapshots.push(LINES.lock().unwrap().clone());
        }
        assert!(snapshots[0].iter().any(|line| line.contains("searching")));
        assert!(snapshots[0].iter().any(|line| line.contains("booting")));
        assert!(snapshots[0].iter().any(|line| line.contains("Setting")));
        assert_eq!(snapshots[0], snapshots[1]);
    }
}
