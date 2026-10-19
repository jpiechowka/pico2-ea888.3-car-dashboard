//! Bounded opaque capture journal. Hardware adapters own flash exclusion.
#![allow(async_fn_in_trait)]

pub const PAGE_BYTES: usize = 256;
pub const SECTOR_BYTES: usize = 4096;
pub const COPY_BYTES: usize = 12288;
pub const SLOT_BYTES: usize = 24576;
pub const FLASH_OFFSET: u32 = 0x1E6000;
pub const PAYLOAD_MAX: usize = 8192;
pub const SLOT_COUNT: u8 = 4;

/// All offsets are physical flash offsets, confined to the capture reservation.
/// Implementations must serialize journal access for the entire public operation.
/// Each mutation performs exactly one sector erase or page program; hardware owns
/// its bounded maintenance ticket, XIP exclusion and cancellation safety.
pub trait Storage {
    type Error;
    async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Self::Error>;
    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), Self::Error>;
    async fn program_page(
        &mut self,
        offset: u32,
        bytes: &[u8; PAGE_BYTES],
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredCapture {
    pub slot: u8,
    pub copy: u8,
    pub generation: u64,
    pub length: u32,
    pub payload_crc: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyState {
    Empty,
    Corrupt,
    UnsupportedSchema(u16),
    Valid(StoredCapture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotInfo {
    pub copies: [CopyState; 2],
    pub selected: Option<StoredCapture>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    Storage(E),
    InvalidSlot,
    InvalidLength,
    BufferTooSmall,
    Empty,
    Corrupt,
    UnsupportedSchema(u16),
    AmbiguousGeneration,
    GenerationExhausted,
    VerifyFailed,
}

const HEADER_MAGIC: &[u8; 8] = b"BLECAPJ1";
const COMMIT_MAGIC: &[u8; 8] = b"BLECOMT1";
const VERSION: u16 = 1;
const COMMIT_OFFSET: usize = COPY_BYTES - PAGE_BYTES;

fn crc_update(
    mut crc: u32,
    bytes: &[u8],
) -> u32 {
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}

/// CRC-32/ISO-HDLC: reflected polynomial, all-ones initial value and final XOR.
pub fn crc32(bytes: &[u8]) -> u32 { !crc_update(u32::MAX, bytes) }

fn page_crc(bytes: &[u8; PAGE_BYTES]) -> u32 { !crc_update(crc_update(u32::MAX, &bytes[..32]), &bytes[36..]) }

fn u16_at(
    bytes: &[u8],
    at: usize,
) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u32_at(
    bytes: &[u8],
    at: usize,
) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
fn u64_at(
    bytes: &[u8],
    at: usize,
) -> u64 {
    u64::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
        bytes[at + 4],
        bytes[at + 5],
        bytes[at + 6],
        bytes[at + 7],
    ])
}

fn base(
    slot: u8,
    copy: u8,
) -> u32 {
    FLASH_OFFSET + u32::from(slot) * SLOT_BYTES as u32 + u32::from(copy) * COPY_BYTES as u32
}

fn header(
    page: &mut [u8; PAGE_BYTES],
    capture: StoredCapture,
) -> u32 {
    page.fill(255);
    page[..8].copy_from_slice(HEADER_MAGIC);
    page[8..10].copy_from_slice(&VERSION.to_le_bytes());
    page[10..12].copy_from_slice(&(PAGE_BYTES as u16).to_le_bytes());
    page[12] = capture.slot;
    page[13] = capture.copy;
    page[16..24].copy_from_slice(&capture.generation.to_le_bytes());
    page[24..28].copy_from_slice(&capture.length.to_le_bytes());
    page[28..32].copy_from_slice(&capture.payload_crc.to_le_bytes());
    let crc = page_crc(page);
    page[32..36].copy_from_slice(&crc.to_le_bytes());
    crc
}

fn parse_header(
    page: &[u8; PAGE_BYTES],
    slot: u8,
    copy: u8,
) -> CopyState {
    if &page[..8] != HEADER_MAGIC {
        return CopyState::Corrupt;
    }
    let version = u16_at(page, 8);
    if version != VERSION {
        return CopyState::UnsupportedSchema(version);
    }
    let generation = u64_at(page, 16);
    let length = u32_at(page, 24);
    if u16_at(page, 10) != PAGE_BYTES as u16
        || page[12] != slot
        || page[13] != copy
        || page[14..16] != [255; 2]
        || page[36..].iter().any(|&b| b != 255)
        || generation == 0
        || length == 0
        || length > PAYLOAD_MAX as u32
        || page_crc(page) != u32_at(page, 32)
    {
        return CopyState::Corrupt;
    }
    CopyState::Valid(StoredCapture {
        slot,
        copy,
        generation,
        length,
        payload_crc: u32_at(page, 28),
    })
}

fn commit(
    page: &mut [u8; PAGE_BYTES],
    capture: StoredCapture,
    header_crc: u32,
) {
    page.fill(255);
    page[..8].copy_from_slice(COMMIT_MAGIC);
    page[8..10].copy_from_slice(&VERSION.to_le_bytes());
    page[10] = capture.slot;
    page[11] = capture.copy;
    page[12..20].copy_from_slice(&capture.generation.to_le_bytes());
    page[20..24].copy_from_slice(&capture.length.to_le_bytes());
    page[24..28].copy_from_slice(&capture.payload_crc.to_le_bytes());
    page[28..32].copy_from_slice(&header_crc.to_le_bytes());
    let crc = page_crc(page);
    page[32..36].copy_from_slice(&crc.to_le_bytes());
}

fn valid_commit(
    page: &[u8; PAGE_BYTES],
    capture: StoredCapture,
    header_crc: u32,
) -> bool {
    &page[..8] == COMMIT_MAGIC
        && u16_at(page, 8) == VERSION
        && page[10] == capture.slot
        && page[11] == capture.copy
        && u64_at(page, 12) == capture.generation
        && u32_at(page, 20) == capture.length
        && u32_at(page, 24) == capture.payload_crc
        && u32_at(page, 28) == header_crc
        && page[36..].iter().all(|&b| b == 255)
        && page_crc(page) == u32_at(page, 32)
}

async fn erased<S: Storage>(
    storage: &mut S,
    address: u32,
    page: &mut [u8; PAGE_BYTES],
) -> Result<bool, Error<S::Error>> {
    for offset in (0..COPY_BYTES).step_by(PAGE_BYTES) {
        storage
            .read(address + offset as u32, page)
            .await
            .map_err(Error::Storage)?;
        if page.iter().any(|&b| b != 255) {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn payload_valid<S: Storage>(
    storage: &mut S,
    capture: StoredCapture,
    page: &mut [u8; PAGE_BYTES],
) -> Result<bool, Error<S::Error>> {
    let mut crc = u32::MAX;
    let length = capture.length as usize;
    for offset in (0..length).step_by(PAGE_BYTES) {
        let count = (length - offset).min(PAGE_BYTES);
        storage
            .read(
                base(capture.slot, capture.copy) + PAGE_BYTES as u32 + offset as u32,
                &mut page[..count],
            )
            .await
            .map_err(Error::Storage)?;
        crc = crc_update(crc, &page[..count]);
    }
    Ok(!crc == capture.payload_crc)
}

async fn inspect_copy<S: Storage>(
    storage: &mut S,
    slot: u8,
    copy: u8,
) -> Result<CopyState, Error<S::Error>> {
    let mut page = [0; PAGE_BYTES];
    let address = base(slot, copy);
    storage.read(address, &mut page).await.map_err(Error::Storage)?;
    if page.iter().all(|&b| b == 255) {
        return Ok(if erased(storage, address, &mut page).await? {
            CopyState::Empty
        } else {
            CopyState::Corrupt
        });
    }
    let state = parse_header(&page, slot, copy);
    let CopyState::Valid(capture) = state else {
        return Ok(state);
    };
    let header_crc = u32_at(&page, 32);
    storage
        .read(address + COMMIT_OFFSET as u32, &mut page)
        .await
        .map_err(Error::Storage)?;
    if !valid_commit(&page, capture, header_crc) || !payload_valid(storage, capture, &mut page).await? {
        return Ok(CopyState::Corrupt);
    }
    Ok(state)
}

/// Inspect both copies including payload integrity. Unknown formats stay visible.
pub async fn inspect_slot<S: Storage>(
    storage: &mut S,
    slot: u8,
) -> Result<SlotInfo, Error<S::Error>> {
    if slot >= SLOT_COUNT {
        return Err(Error::InvalidSlot);
    }
    let mut info = SlotInfo {
        copies: [CopyState::Empty; 2],
        selected: None,
    };
    for copy in 0..2 {
        let state = inspect_copy(storage, slot, copy).await?;
        if let CopyState::Valid(candidate) = state {
            if let Some(previous) = info.selected {
                if candidate.generation == previous.generation {
                    return Err(Error::AmbiguousGeneration);
                }
                if candidate.generation > previous.generation {
                    info.selected = Some(candidate);
                }
            } else {
                info.selected = Some(candidate);
            }
        }
        info.copies[usize::from(copy)] = state;
    }
    Ok(info)
}

fn unavailable<E>(info: SlotInfo) -> Error<E> {
    for state in info.copies {
        if let CopyState::UnsupportedSchema(version) = state {
            return Error::UnsupportedSchema(version);
        }
    }
    if info.copies == [CopyState::Empty; 2] {
        Error::Empty
    } else {
        Error::Corrupt
    }
}

/// On any error the destination is untrusted and may contain a partial payload.
/// The storage adapter must exclusively own this journal for the entire future.
pub async fn load_slot<S: Storage>(
    storage: &mut S,
    slot: u8,
    destination: &mut [u8],
) -> Result<StoredCapture, Error<S::Error>> {
    let info = inspect_slot(storage, slot).await?;
    let capture = info.selected.ok_or_else(|| unavailable(info))?;
    read_payload(storage, capture, destination).await
}

async fn checked_copy<S: Storage>(
    storage: &mut S,
    capture: StoredCapture,
) -> Result<(), Error<S::Error>> {
    if capture.slot >= SLOT_COUNT || capture.copy >= 2 {
        return Err(Error::InvalidSlot);
    }
    if inspect_copy(storage, capture.slot, capture.copy).await? != CopyState::Valid(capture) {
        return Err(Error::VerifyFailed);
    }
    Ok(())
}

/// Read at most one page from a fully verified copy. Supplied metadata is checked
/// against flash, not trusted as an address. Retain exclusive storage ownership.
pub async fn read_prefix<S: Storage>(
    storage: &mut S,
    capture: StoredCapture,
    prefix: &mut [u8],
) -> Result<(), Error<S::Error>> {
    if prefix.len() > PAGE_BYTES || prefix.len() > capture.length as usize {
        return Err(Error::InvalidLength);
    }
    checked_copy(storage, capture).await?;
    storage
        .read(base(capture.slot, capture.copy) + PAGE_BYTES as u32, prefix)
        .await
        .map_err(Error::Storage)
}

/// Load a particular verified generation, including an older supported payload
/// when the newer copy has an unsupported application format. Never mutates flash.
pub async fn load_copy<S: Storage>(
    storage: &mut S,
    capture: StoredCapture,
    destination: &mut [u8],
) -> Result<StoredCapture, Error<S::Error>> {
    checked_copy(storage, capture).await?;
    read_payload(storage, capture, destination).await
}

async fn read_payload<S: Storage>(
    storage: &mut S,
    capture: StoredCapture,
    destination: &mut [u8],
) -> Result<StoredCapture, Error<S::Error>> {
    let length = capture.length as usize;
    if destination.len() < length {
        return Err(Error::BufferTooSmall);
    }
    for (index, chunk) in destination[..length].chunks_mut(PAGE_BYTES).enumerate() {
        storage
            .read(
                base(capture.slot, capture.copy) + PAGE_BYTES as u32 + (index * PAGE_BYTES) as u32,
                chunk,
            )
            .await
            .map_err(Error::Storage)?;
    }
    if crc32(&destination[..length]) != capture.payload_crc {
        return Err(Error::VerifyFailed);
    }
    Ok(capture)
}

/// Replace the inactive copy only. A failed/cancelled future must never be resumed
/// as an implicit retry. Reinspect after an unknown commit outcome.
pub async fn save_slot<S: Storage>(
    storage: &mut S,
    slot: u8,
    payload: &[u8],
) -> Result<StoredCapture, Error<S::Error>> {
    if slot >= SLOT_COUNT {
        return Err(Error::InvalidSlot);
    }
    if payload.is_empty() || payload.len() > PAYLOAD_MAX {
        return Err(Error::InvalidLength);
    }
    let info = inspect_slot(storage, slot).await?;
    for state in info.copies {
        if let CopyState::UnsupportedSchema(version) = state {
            return Err(Error::UnsupportedSchema(version));
        }
    }
    let (copy, generation) = match info.selected {
        Some(previous) => {
            (
                1 - previous.copy,
                previous.generation.checked_add(1).ok_or(Error::GenerationExhausted)?,
            )
        }
        None if info.copies == [CopyState::Empty; 2] => (0, 1),
        None => return Err(Error::Corrupt),
    };
    let capture = StoredCapture {
        slot,
        copy,
        generation,
        length: payload.len() as u32,
        payload_crc: crc32(payload),
    };
    let address = base(slot, copy);
    for offset in (0..COPY_BYTES).step_by(SECTOR_BYTES) {
        storage
            .erase_sector(address + offset as u32)
            .await
            .map_err(Error::Storage)?;
    }
    let mut page = [255; PAGE_BYTES];
    if !erased(storage, address, &mut page).await? {
        return Err(Error::VerifyFailed);
    }
    let header_crc = header(&mut page, capture);
    storage.program_page(address, &page).await.map_err(Error::Storage)?;
    for (index, chunk) in payload.chunks(PAGE_BYTES).enumerate() {
        page.fill(255);
        page[..chunk.len()].copy_from_slice(chunk);
        storage
            .program_page(address + PAGE_BYTES as u32 + (index * PAGE_BYTES) as u32, &page)
            .await
            .map_err(Error::Storage)?;
    }
    storage.read(address, &mut page).await.map_err(Error::Storage)?;
    if parse_header(&page, slot, copy) != CopyState::Valid(capture)
        || u32_at(&page, 32) != header_crc
        || !payload_valid(storage, capture, &mut page).await?
    {
        return Err(Error::VerifyFailed);
    }
    commit(&mut page, capture, header_crc);
    storage
        .program_page(address + COMMIT_OFFSET as u32, &page)
        .await
        .map_err(Error::Storage)?;
    if inspect_copy(storage, slot, copy).await? != CopyState::Valid(capture) {
        return Err(Error::VerifyFailed);
    }
    Ok(capture)
}

/// Destructive recovery, exclusively after explicit local confirmation. This
/// erases both copies, including unknown schemas; it is never called by save.
/// Interruption may destroy this slot's only valid copy. Other slots are untouched.
pub async fn reset_slot<S: Storage>(
    storage: &mut S,
    slot: u8,
) -> Result<(), Error<S::Error>> {
    if slot >= SLOT_COUNT {
        return Err(Error::InvalidSlot);
    }
    let address = base(slot, 0);
    for offset in (0..SLOT_BYTES).step_by(SECTOR_BYTES) {
        storage
            .erase_sector(address + offset as u32)
            .await
            .map_err(Error::Storage)?;
    }
    let mut page = [0; PAGE_BYTES];
    for copy in 0..2 {
        if !erased(storage, base(slot, copy), &mut page).await? {
            return Err(Error::VerifyFailed);
        }
    }
    Ok(())
}
