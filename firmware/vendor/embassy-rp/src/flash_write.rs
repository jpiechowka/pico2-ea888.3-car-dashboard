//! Pure admission for the RP235x no-padding program path; no pointers are dereferenced.
//! The source contract intentionally admits only lower512KiB SRAM, matching this
//! application's buffers; it does not claim to cover all RP235x SRAM or its aliases.

/// Flash program-page size in bytes, shared with the public HAL constant.
pub(crate) const PAGE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Rejected input geometry before flash exclusion; the caller maps to HAL errors.
pub(crate) enum RamWriteError {
    /// Empty input or source extent outside the admitted SRAM range, including overflow.
    OutOfBounds,
    /// Flash offset or nonzero input length is not a whole program-page multiple.
    Unaligned,
}

/// Checks physical flash `offset` alignment, nonzero byte `length`, and the complete
/// source extent at `address` within [0x20000000,0x20080000). Returns unit on success,
/// Empty input returns OutOfBounds first, then invalid page geometry returns Unaligned,
/// then a source-range/overflow failure returns OutOfBounds.
/// Source-byte alignment is unrestricted. The caller must separately validate flash
/// capacity and derive address/length from the actual live slice, never caller metadata.
pub(crate) fn check_ram_write(
    offset: u32,
    length: usize,
    address: usize,
) -> Result<(), RamWriteError> {
    if length == 0 {
        return Err(RamWriteError::OutOfBounds);
    }
    if !(offset as usize).is_multiple_of(PAGE_BYTES) || !length.is_multiple_of(PAGE_BYTES) {
        return Err(RamWriteError::Unaligned);
    }
    if !(0x2000_0000..0x2008_0000).contains(&address) || address.checked_add(length).is_none_or(|end| end > 0x2008_0000)
    {
        return Err(RamWriteError::OutOfBounds);
    }
    Ok(())
}
