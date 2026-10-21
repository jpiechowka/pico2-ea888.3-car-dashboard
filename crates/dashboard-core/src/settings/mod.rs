//! Versioned settings journal shared by host fault injection and the flash owner.
//! Two sectors, eight records each, separate data/commit program pages. CRC is
//! corruption detection, not authentication or a guarantee against any brownout.
use crate::render::FpsMode;
/// Debounced single-owner persistence intent, immutable tickets and bounded retries.
pub mod policy;
/// Checked mapping from journal-relative offsets to reserved physical flash ranges.
pub mod region;

/// Physical flash program page size in bytes.
pub const PAGE_SIZE: usize = 256;
/// Physical flash erase sector size in bytes.
pub const SECTOR_SIZE: usize = 4096;
/// Two-sector journal reservation size in bytes.
pub const STORAGE_SIZE: usize = 2 * SECTOR_SIZE;
/// Bytes per record: one data page followed by one separately programmed commit page.
const RECORD_SIZE: usize = 2 * PAGE_SIZE;
/// Total record slots across both sectors, including empty/corrupt slots.
const RECORDS: usize = STORAGE_SIZE / RECORD_SIZE;
/// Record slots per erase sector.
const PER_SECTOR: usize = SECTOR_SIZE / RECORD_SIZE;

/// Validated legacy-compatible preference payload; boot normalization resets brightness only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Legacy persisted brightness percent in 0..=100; current boot always normalizes to 100.
    brightness: u8,
    /// Legacy nonzero restore brightness percent in 1..=100, normalized to 100 at boot.
    restore_brightness: u8,
    /// Physical dashboard boost preference: true PSI, false bar; fixed widget units ignore it.
    boost_psi: bool,
    /// Software-transfer FPS display mode.
    fps: FpsMode,
}
impl Default for Settings {
    /// Returns Full/100% brightness, bar boost preference and FPS display Off.
    fn default() -> Self {
        Self {
            brightness: 100,
            restore_brightness: 100,
            boost_psi: false,
            fps: FpsMode::Off,
        }
    }
}
impl Settings {
    /// Retain the journal format and durable unit/FPS choices, but always boot
    /// fully lit. Runtime brightness changes no longer schedule flash writes.
    /// Returns `self` with both legacy brightness fields set to 100, preserving unit/FPS preferences.
    pub const fn for_boot(self) -> Self {
        Self {
            brightness: 100,
            restore_brightness: 100,
            ..self
        }
    }

    /// Returns validated settings from `brightness` percent 0..=100, nonzero
    /// `restore_brightness` percent 1..=100, `boost_psi` preference and `fps` mode.
    /// Invalid brightness ranges return `None`; no persistence or boot normalization occurs.
    pub fn new(
        brightness: u8,
        restore_brightness: u8,
        boost_psi: bool,
        fps: FpsMode,
    ) -> Option<Self> {
        (brightness <= 100 && (1..=100).contains(&restore_brightness)).then_some(Self {
            brightness,
            restore_brightness,
            boost_psi,
            fps,
        })
    }

    /// Returns `self`'s legacy brightness percentage, 0..=100.
    pub const fn brightness(self) -> u8 { self.brightness }

    /// Returns `self`'s legacy nonzero restore percentage, 1..=100.
    pub const fn restore_brightness(self) -> u8 { self.restore_brightness }

    /// Returns whether `self` selects PSI for the dashboard's FollowBoost preference.
    pub const fn boost_psi(self) -> bool { self.boost_psi }

    /// Returns `self`'s software-transfer FPS display mode.
    pub const fn fps(self) -> FpsMode { self.fps }

    /// Returns `self` as the frozen four-byte v1 payload: brightness, restore, canonical
    /// unit boolean and FPS code 0..3. Does not normalize legacy brightness.
    fn bytes(self) -> [u8; 4] {
        [
            self.brightness,
            self.restore_brightness,
            u8::from(self.boost_psi),
            match self.fps {
                FpsMode::Off => 0,
                FpsMode::Instant => 1,
                FpsMode::Average => 2,
                FpsMode::Combined => 3,
            },
        ]
    }

    /// Returns validated settings from exact four-byte `bytes`, or `None` for wrong
    /// width, noncanonical unit/FPS codes or brightness outside admitted ranges.
    fn decode(bytes: &[u8]) -> Option<Self> {
        let [brightness, restore, unit, fps] = *bytes else {
            return None;
        };
        let fps = match fps {
            0 => FpsMode::Off,
            1 => FpsMode::Instant,
            2 => FpsMode::Average,
            3 => FpsMode::Combined,
            _ => return None,
        };
        if unit > 1 {
            return None;
        }
        Self::new(brightness, restore, unit != 0, fps)
    }
}

/// All offsets are relative to the reserved 8192-byte area. A hardware adapter
/// must enforce this range and exclusive access itself; never expose raw FLASH.
pub trait Storage {
    /// Adapter read/program/erase failure retained unchanged by the journal.
    type Error;
    /// Reads reservation-relative `offset` into all of `out`, returning unit on success.
    /// `self` must enforce bounds/exclusion; errors may leave a partial untrusted output.
    fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), Self::Error>;
    /// Programs complete `page` at page-aligned reservation-relative `offset`, returning
    /// unit after adapter completion. Caller erases/verifies separately; errors may leave
    /// partial programming. `self` owns flash/XIP/core/IRQ exclusion.
    fn program(
        &mut self,
        offset: u32,
        page: &[u8; PAGE_SIZE],
    ) -> Result<(), Self::Error>;
    /// Erases one sector at aligned reservation-relative `offset`, returning unit after
    /// adapter completion. Errors may partially erase; `self` owns exclusive flash access.
    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), Self::Error>;
}

/// Journal admission or transaction failure; write errors do not prove no write occurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    /// Underlying storage operation failed.
    Storage(
        /// Original adapter error, possibly after partial effects.
        E,
    ),
    /// Erase/program/commit readback failed exact equality or envelope admission.
    Readback,
    /// More than one valid record claims the same generation.
    Ambiguous,
    /// Structurally intact unsupported envelope blocks reading/mutation through this version.
    Unsupported,
    /// Latest valid generation cannot be incremented without wrapping.
    Exhausted,
}
/// Fully committed journal record with a nonzero generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// Checked monotonic generation; zero is invalid on storage admission.
    pub generation: u64,
    /// Validated preference payload, retaining legacy brightness bytes.
    pub settings: Settings,
}
/// Complete bounded journal scan, preserving free-slot and corruption evidence for saving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    /// Valid record with greatest generation, or absent if no committed record survives.
    pub latest: Option<Record>,
    /// Nonempty rejected/torn records, at most sixteen; erased slots do not count.
    pub invalid_records: u8,
    /// Physical record index of `latest`, absent when no valid record exists.
    latest_slot: Option<usize>,
    /// One bit per completely erased two-page record slot.
    erased: u16,
}

/// CRC-32/ISO-HDLC: reflected 0x04C11DB7, init/xorout FFFFFFFF.
/// Returns the checksum of `bytes`, zero for empty input; this detects corruption
/// and is not authentication. Implementation uses reflected polynomial 0xEDB88320.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
/// Returns whether every byte in `bytes` is erased 0xFF; empty input is vacuously erased.
fn erased(bytes: &[u8]) -> bool { bytes.iter().all(|byte| *byte == 255) }
/// Returns little-endian u32 from exact four-byte `bytes`; panics for any other length.
fn word(bytes: &[u8]) -> u32 { u32::from_le_bytes(bytes.try_into().unwrap()) }
/// Returns little-endian generation from exact eight-byte `bytes`; panics for other lengths.
fn generation(bytes: &[u8]) -> u64 { u64::from_le_bytes(bytes.try_into().unwrap()) }

/// Returns canonical `(data, commit)` pages for `record`, with 0xFF padding and linked
/// CRC/generation fields. Caller supplies a nonzero generation; this encoder performs no I/O.
fn encode(record: Record) -> ([u8; PAGE_SIZE], [u8; PAGE_SIZE]) {
    let mut data = [255; PAGE_SIZE];
    data[..4].copy_from_slice(b"EA88");
    data[4..6].copy_from_slice(&1u16.to_le_bytes());
    data[6..8].copy_from_slice(&4u16.to_le_bytes());
    data[8..16].copy_from_slice(&record.generation.to_le_bytes());
    data[16..20].copy_from_slice(&record.settings.bytes());
    // Stable envelope: checksum position/coverage never depends on schema or
    // payload length, so future longer schemas can be recognized and protected.
    let checksum = crc32(&data[..252]);
    data[252..].copy_from_slice(&checksum.to_le_bytes());
    let mut commit = [255; PAGE_SIZE];
    commit[..4].copy_from_slice(b"CMT1");
    commit[4..12].copy_from_slice(&record.generation.to_le_bytes());
    commit[12..16].copy_from_slice(&checksum.to_le_bytes());
    let checksum = crc32(&commit[..16]);
    commit[16..20].copy_from_slice(&checksum.to_le_bytes());
    (data, commit)
}

/// A structurally intact unsupported envelope blocks mutation, so downgrade
/// cannot silently replace a newer schema. Ordinary corrupt/torn slots are inert.
/// Returns a committed record from paired `data`/`commit` pages when magic, CRC,
/// generation, reserved bytes and v1 settings agree; corrupt/torn bytes return `Ok(None)`.
/// Returns `Err(())` only for an otherwise structurally intact nonzero-generation
/// envelope whose schema/payload-width marker differs from supported v1/four bytes.
fn decode(
    data: &[u8; PAGE_SIZE],
    commit: &[u8; PAGE_SIZE],
) -> Result<Option<Record>, ()> {
    if &data[..4] != b"EA88"
        || &commit[..4] != b"CMT1"
        || crc32(&data[..252]) != word(&data[252..])
        || crc32(&commit[..16]) != word(&commit[16..20])
        || data[8..16] != commit[4..12]
        || data[252..] != commit[12..16]
        || !erased(&commit[20..])
    {
        return Ok(None);
    }
    let generation = generation(&data[8..16]);
    if generation == 0 {
        return Ok(None);
    }
    if data[4..8] != [1, 0, 4, 0] {
        return Err(());
    }
    if !erased(&data[20..252]) {
        return Ok(None);
    }
    Ok(Settings::decode(&data[16..20]).map(|settings| Record { generation, settings }))
}

/// Returns a complete scan of sixteen record slots through exclusively owned `storage`.
/// Selects greatest valid generation, counts nonempty corrupt records and records erased
/// slots. Any storage error, intact unsupported schema or duplicate valid generation
/// fails the whole scan. Read-only; no defaults are written and no corruption is erased.
pub fn load<S: Storage>(storage: &mut S) -> Result<Scan, Error<S::Error>> {
    let mut scan = Scan {
        latest: None,
        invalid_records: 0,
        latest_slot: None,
        erased: 0,
    };
    let mut generations = [0u64; RECORDS];
    let (mut data, mut commit) = ([0; PAGE_SIZE], [0; PAGE_SIZE]);
    for (slot, seen_generation) in generations.iter_mut().enumerate() {
        let offset = (slot * RECORD_SIZE) as u32;
        storage.read(offset, &mut data).map_err(Error::Storage)?;
        storage
            .read(offset + PAGE_SIZE as u32, &mut commit)
            .map_err(Error::Storage)?;
        if erased(&data) && erased(&commit) {
            scan.erased |= 1 << slot;
            continue;
        }
        if let Some(record) = decode(&data, &commit).map_err(|()| Error::Unsupported)? {
            *seen_generation = record.generation;
            if scan.latest.is_none_or(|latest| record.generation > latest.generation) {
                scan.latest = Some(record);
                scan.latest_slot = Some(slot);
            }
        } else {
            scan.invalid_records += 1;
        }
    }
    for (i, value) in generations.iter().enumerate() {
        if *value != 0 && generations[i + 1..].contains(value) {
            return Err(Error::Ambiguous);
        }
    }
    Ok(scan)
}

/// One immutable transaction, including readback. Does not feed the watchdog,
/// retry a failed operation, or clear an old valid sector before the new commit.
/// Returns the verified record for `settings` using exclusively owned `storage`.
/// Equal latest settings return the existing record without mutation. Otherwise uses
/// an erased record slot, or erases the sector not containing the latest valid record,
/// then writes/verifies data before commit and reverifies their pairing.
/// Admission/exhaustion errors precede writes; storage/readback errors can follow partial
/// writes or an already durable commit. Reinspect before an explicit retry; no automatic retry occurs.
pub fn save<S: Storage>(
    storage: &mut S,
    settings: Settings,
) -> Result<Record, Error<S::Error>> {
    let scan = load(storage)?;
    if let Some(latest) = scan.latest
        && latest.settings == settings
    {
        return Ok(latest);
    }
    let generation = scan
        .latest
        .map_or(0, |r| r.generation)
        .checked_add(1)
        .ok_or(Error::Exhausted)?;
    let active = scan.latest_slot.unwrap_or(0) / PER_SECTOR;
    let free = |sector| (sector * PER_SECTOR..(sector + 1) * PER_SECTOR).find(|slot| scan.erased & (1u16 << slot) != 0);
    let slot = if let Some(slot) = free(active).or_else(|| free(1 - active)) {
        slot
    } else {
        let sector = if scan.latest.is_some() { 1 - active } else { 0 };
        let offset = (sector * SECTOR_SIZE) as u32;
        storage.erase(offset).map_err(Error::Storage)?;
        let mut page = [0; PAGE_SIZE];
        for offset in (offset..offset + SECTOR_SIZE as u32).step_by(PAGE_SIZE) {
            storage.read(offset, &mut page).map_err(Error::Storage)?;
            if !erased(&page) {
                return Err(Error::Readback);
            }
        }
        sector * PER_SECTOR
    };
    let record = Record { generation, settings };
    let (data, commit) = encode(record);
    let offset = (slot * RECORD_SIZE) as u32;
    let mut readback = [0; PAGE_SIZE];
    // Recheck both destination pages even if the scan/erase said they were free.
    for offset in [offset, offset + PAGE_SIZE as u32] {
        storage.read(offset, &mut readback).map_err(Error::Storage)?;
        if !erased(&readback) {
            return Err(Error::Readback);
        }
    }
    storage.program(offset, &data).map_err(Error::Storage)?;
    storage.read(offset, &mut readback).map_err(Error::Storage)?;
    if readback != data {
        return Err(Error::Readback);
    }
    storage
        .program(offset + PAGE_SIZE as u32, &commit)
        .map_err(Error::Storage)?;
    storage
        .read(offset + PAGE_SIZE as u32, &mut readback)
        .map_err(Error::Storage)?;
    if readback != commit {
        return Err(Error::Readback);
    }
    // Read both pages again after commit, since both define durable success.
    let mut committed_data = [0; PAGE_SIZE];
    storage.read(offset, &mut committed_data).map_err(Error::Storage)?;
    if decode(&committed_data, &readback) != Ok(Some(record)) {
        return Err(Error::Readback);
    }
    Ok(record)
}
