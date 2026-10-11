//! Versioned settings journal shared by host fault injection and the flash owner.
//! Two sectors, eight records each, separate data/commit program pages. CRC is
//! corruption detection, not authentication or a guarantee against any brownout.
use crate::render::FpsMode;
pub mod policy;
pub mod region;

pub const PAGE_SIZE: usize = 256;
pub const SECTOR_SIZE: usize = 4096;
pub const STORAGE_SIZE: usize = 2 * SECTOR_SIZE;
const RECORD_SIZE: usize = 2 * PAGE_SIZE;
const RECORDS: usize = STORAGE_SIZE / RECORD_SIZE;
const PER_SECTOR: usize = SECTOR_SIZE / RECORD_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    brightness: u8,
    restore_brightness: u8,
    boost_psi: bool,
    fps: FpsMode,
}
impl Default for Settings {
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

    pub const fn brightness(self) -> u8 { self.brightness }

    pub const fn restore_brightness(self) -> u8 { self.restore_brightness }

    pub const fn boost_psi(self) -> bool { self.boost_psi }

    pub const fn fps(self) -> FpsMode { self.fps }

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
    type Error;
    fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), Self::Error>;
    fn program(
        &mut self,
        offset: u32,
        page: &[u8; PAGE_SIZE],
    ) -> Result<(), Self::Error>;
    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    Storage(E),
    Readback,
    Ambiguous,
    Unsupported,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub generation: u64,
    pub settings: Settings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    pub latest: Option<Record>,
    pub invalid_records: u8,
    latest_slot: Option<usize>,
    erased: u16,
}

/// CRC-32/ISO-HDLC: reflected 0x04C11DB7, init/xorout FFFFFFFF.
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
fn erased(bytes: &[u8]) -> bool { bytes.iter().all(|byte| *byte == 255) }
fn word(bytes: &[u8]) -> u32 { u32::from_le_bytes(bytes.try_into().unwrap()) }
fn generation(bytes: &[u8]) -> u64 { u64::from_le_bytes(bytes.try_into().unwrap()) }

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
