//! Physical CS0 offsets, checked before reaching any HAL flash operation.
use super::{PAGE_SIZE, SECTOR_SIZE, STORAGE_SIZE};
/// Physical CS0 flash byte offset of the two-sector settings reservation, excluding XIP base.
pub const BASE: u32 = 0x1FE000;

/// Checked nonempty physical half-open range wholly inside the settings reservation.
#[derive(Debug)]
pub struct Range {
    /// Inclusive physical byte offset, at least [`BASE`].
    start: u32,
    /// Exclusive physical byte offset, at most BASE plus STORAGE_SIZE.
    end: u32,
}
impl Range {
    /// Returns the physical range for reservation-relative `offset` and byte `length`.
    /// Zero length, usize-to-u32 loss, addition overflow or end beyond the reservation
    /// returns `None`; read offsets need no page/sector alignment.
    pub fn read(
        offset: u32,
        length: usize,
    ) -> Option<Self> {
        let length = u32::try_from(length).ok()?;
        let end = offset.checked_add(length)?;
        if length == 0 || end > STORAGE_SIZE as u32 {
            return None;
        }
        Some(Self {
            start: BASE.checked_add(offset)?,
            end: BASE.checked_add(end)?,
        })
    }

    /// Returns one page's physical range for reservation-relative `offset`, or `None`
    /// unless page-aligned and wholly inside the reservation. Performs no programming.
    pub fn program(offset: u32) -> Option<Self> {
        offset.is_multiple_of(PAGE_SIZE as u32).then_some(())?;
        Self::read(offset, PAGE_SIZE)
    }

    /// Returns one sector's physical range for reservation-relative `offset`, or `None`
    /// unless sector-aligned and wholly inside the reservation. Performs no erasure.
    pub fn erase(offset: u32) -> Option<Self> {
        offset.is_multiple_of(SECTOR_SIZE as u32).then_some(())?;
        Self::read(offset, SECTOR_SIZE)
    }

    /// Returns `self`'s inclusive physical CS0 byte offset, not an XIP virtual address.
    pub const fn start(&self) -> u32 { self.start }

    /// Returns `self`'s exclusive physical CS0 byte offset.
    pub const fn end(&self) -> u32 { self.end }
}
