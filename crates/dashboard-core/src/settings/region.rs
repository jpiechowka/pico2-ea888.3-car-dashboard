//! Physical CS0 offsets, checked before reaching any HAL flash operation.
use super::{PAGE_SIZE, SECTOR_SIZE, STORAGE_SIZE};
pub const BASE: u32 = 0x1FE000;

#[derive(Debug)]
pub struct Range {
    start: u32,
    end: u32,
}
impl Range {
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

    pub fn program(offset: u32) -> Option<Self> {
        offset.is_multiple_of(PAGE_SIZE as u32).then_some(())?;
        Self::read(offset, PAGE_SIZE)
    }

    pub fn erase(offset: u32) -> Option<Self> {
        offset.is_multiple_of(SECTOR_SIZE as u32).then_some(())?;
        Self::read(offset, SECTOR_SIZE)
    }

    pub const fn start(&self) -> u32 { self.start }

    pub const fn end(&self) -> u32 { self.end }
}
