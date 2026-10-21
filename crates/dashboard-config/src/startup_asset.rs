//! Borrowed, bounded EASA assets. v1 is landscape; v2 also supports portrait.
use crate::Geometry;

/// Current EASA container version; version one remains accepted for landscape only.
pub const VERSION: u16 = 2;
/// Complete serialized asset ceiling, including header/table/payloads, in bytes.
pub const MAX_ASSET_BYTES: usize = 524_288;
/// Frame-table entry ceiling; cumulative timing can impose a smaller practical count.
pub const MAX_FRAMES: usize = 300;
/// Landscape pixel width; portrait swaps this with HEIGHT.
pub const WIDTH: usize = 320;
/// Landscape pixel height; portrait swaps this with WIDTH.
pub const HEIGHT: usize = 240;
/// Exact RGB565 decoded bytes for either supported geometry, without an extra framebuffer allocation.
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 2;
/// Minimum positive interval between successive cumulative frame ends, in milliseconds.
pub const MIN_FRAME_MS: u32 = 34;
/// Eight-byte container signature, including two zero bytes.
pub const MAGIC: &[u8; 8] = b"EASSET\0\0";
/// Fixed header length; its final four bytes hold the global CRC.
pub const HEADER_BYTES: usize = 32;
/// Fixed frame-table record size: end time, offset, length, CRC, encoding and three reserved bytes.
pub const ENTRY_BYTES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Admission/decode rejection class; no error permits publishing partially decoded pixels.
pub enum AssetError {
    /// Unsupported signature/version/dimensions, inconsistent total size or nonzero header reservation.
    Header,
    /// Asset/frame count/cycle duration bound or frame-table capacity violation.
    Limit,
    /// Invalid cumulative timing, payload layout, encoding tag or entry reservation.
    Table,
    /// Global or encoded-frame checksum mismatch; CRC is corruption detection, not authentication.
    Crc,
    /// Encoding failure reserved for format producers; current parser rejects unknown tags as Table.
    Encoding,
    /// Invalid decoded row shape, packet boundary, payload consumption or unfinished admission.
    Pixels,
}
#[derive(Clone, Copy, Debug)]
/// Borrowed asset admitted by full structural, checksum and row validation.
/// Private `header` creates an internal provisional value; callers receive one only after full admission.
pub struct Asset<'a> {
    /// Immutable complete serialized container, bounded by MAX_ASSET_BYTES.
    bytes: &'a [u8],
    /// Shape proven by the header; pixel byte count is identical for both orientations.
    geometry: Geometry,
}
/// Host-owned immutable bytes admitted once by the same bounded firmware parser.
/// Clones share pixels; borrowed views cannot outlive this owner. No serialized
/// metadata or caller-provided geometry can bypass admission.
#[cfg(feature = "json")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedAsset {
    /// Shared immutable admitted bytes; cloning does not duplicate frame payloads.
    bytes: alloc::sync::Arc<[u8]>,
    /// Shape derived by the parser, never trusted from caller metadata.
    geometry: Geometry,
}
#[cfg(feature = "json")]
impl OwnedAsset {
    /// Fully admit `bytes` and return shared ownership; any parser error rejects the whole asset.
    pub fn new(bytes: alloc::sync::Arc<[u8]>) -> Result<Self, AssetError> {
        let geometry = Asset::parse(&bytes)?.geometry();
        Ok(Self { bytes, geometry })
    }

    /// Return an admitted borrowed view without reparsing; its lifetime is tied to this owner.
    pub fn as_asset(&self) -> Asset<'_> {
        Asset {
            bytes: &self.bytes,
            geometry: self.geometry,
        }
    }

    /// Return immutable serialized bytes, including the header and checksums.
    pub fn bytes(&self) -> &[u8] { &self.bytes }

    /// Return another reference-counted owner of the same immutable bytes, without a pixel copy.
    pub fn bytes_arc(&self) -> alloc::sync::Arc<[u8]> { self.bytes.clone() }
}
#[derive(Clone, Copy, Debug)]
/// One admitted encoded frame; row decoding writes into caller-owned storage.
pub struct Frame<'a> {
    /// Exactly this frame's encoded bytes, excluding its table record.
    payload: &'a [u8],
    /// Zero for raw RGB565 bytes, one for row-bounded literal/repeat packets.
    encoding: u8,
    /// Expected CRC of encoded payload bytes, not decoded pixels.
    crc: u32,
    /// Decoded row width/height inherited from the admitted asset.
    geometry: Geometry,
}
/// Read little-endian u16 at `at` in `b`; returns the value and panics if two bytes are unavailable.
/// Callers establish the bounded record before invoking this internal helper.
fn u16_at(
    b: &[u8],
    at: usize,
) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
/// Read little-endian u32 at `at` in `b`; returns the value and panics if four bytes are unavailable.
fn u32_at(
    b: &[u8],
    at: usize,
) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
/// Return reflected CRC-32/ISO-HDLC of `bytes` (polynomial 0xEDB88320, initial/final XOR all ones).
/// Empty input yields zero; this is not an authenticity check.
pub fn crc32(bytes: &[u8]) -> u32 { !crc_update(!0, bytes) }
/// Extend uncomplemented state `crc` with `bytes`; returns state for chaining before final inversion.
fn crc_update(
    mut crc: u32,
    bytes: &[u8],
) -> u32 {
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}
/// Excludes the global CRC field at 28..32, not any frame CRC fields.
/// Returns a checksum for `bytes` of at least HEADER_BYTES, otherwise None; does not admit the format.
pub fn asset_crc32(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= HEADER_BYTES).then(|| !crc_update(crc_update(!0, &bytes[..28]), &bytes[32..]))
}
impl<'a> Asset<'a> {
    /// Synchronously validate all `bytes` and return a borrowed asset; no pixel buffer is allocated.
    /// Work is bounded by format limits but occupies the caller until complete; use Validator to yield.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, AssetError> {
        let mut validator = Validator::new(bytes)?;
        while !validator.step()? {}
        validator.finish()
    }

    /// Check `bytes` header and contiguous table layout, returning an internal provisional asset.
    /// Does not check CRC/pixels; the result must not escape before Validator completes.
    fn header(bytes: &'a [u8]) -> Result<Self, AssetError> {
        if bytes.len() < HEADER_BYTES || bytes.len() > MAX_ASSET_BYTES {
            return Err(AssetError::Limit);
        }
        if &bytes[..8] != MAGIC
            || !matches!(u16_at(bytes, 8), 1 | VERSION)
            || u16_at(bytes, 10) != 32
            || u32_at(bytes, 12) as usize != bytes.len()
            || u16_at(bytes, 22) != 0
        {
            return Err(AssetError::Header);
        }
        let geometry = match (u16_at(bytes, 16), u16_at(bytes, 18)) {
            (320, 240) => Geometry::Landscape,
            (240, 320) if u16_at(bytes, 8) == VERSION => Geometry::Portrait,
            _ => return Err(AssetError::Header),
        };
        let count = usize::from(u16_at(bytes, 20));
        let cycle = u32_at(bytes, 24);
        if count == 0
            || count > MAX_FRAMES
            || !(MIN_FRAME_MS..=10000).contains(&cycle)
            || HEADER_BYTES + count * ENTRY_BYTES > bytes.len()
        {
            return Err(AssetError::Limit);
        }
        let mut offset = HEADER_BYTES + count * ENTRY_BYTES;
        let mut end = 0;
        for i in 0..count {
            let e = &bytes[HEADER_BYTES + i * ENTRY_BYTES..HEADER_BYTES + (i + 1) * ENTRY_BYTES];
            let next = u32_at(e, 0);
            let len = u32_at(e, 8) as usize;
            if next.checked_sub(end).is_none_or(|d| d < MIN_FRAME_MS)
                || next > cycle
                || u32_at(e, 4) as usize != offset
                || len == 0
                || e[16] > 1
                || e[17..].iter().any(|b| *b != 0)
            {
                return Err(AssetError::Table);
            }
            offset = offset
                .checked_add(len)
                .filter(|o| *o <= bytes.len())
                .ok_or(AssetError::Table)?;
            end = next;
        }
        if offset != bytes.len() || end != cycle {
            return Err(AssetError::Table);
        }
        Ok(Self { bytes, geometry })
    }

    /// Return the admitted immutable serialized container with its original lifetime.
    pub const fn bytes(self) -> &'a [u8] { self.bytes }

    /// Return validated decoded shape; inversion is presentation metadata outside this container.
    pub const fn geometry(self) -> Geometry { self.geometry }

    /// Return the nonzero admitted frame count, at most MAX_FRAMES.
    pub fn frame_count(self) -> usize { usize::from(u16_at(self.bytes, 20)) }

    /// Return the final cumulative frame end, 34 through 10000 milliseconds.
    pub fn cycle_ms(self) -> u32 { u32_at(self.bytes, 24) }

    /// Return encoded frame `index` in table order; out-of-range indices return None without slicing.
    pub fn frame(
        self,
        index: usize,
    ) -> Option<Frame<'a>> {
        if index >= self.frame_count() {
            return None;
        }
        let e = &self.bytes[HEADER_BYTES + index * ENTRY_BYTES..];
        let offset = u32_at(e, 4) as usize;
        let len = u32_at(e, 8) as usize;
        Some(Frame {
            payload: &self.bytes[offset..offset + len],
            encoding: e[16],
            crc: u32_at(e, 12),
            geometry: self.geometry,
        })
    }

    /// Return the frame at `elapsed` milliseconds: `looping` wraps, otherwise the final frame holds.
    /// `reduced` takes precedence and selects the cycle midpoint, independent of elapsed time.
    /// Cumulative end times are exclusive; admitted positive timing keeps modulo/search defined.
    pub fn frame_at(
        self,
        elapsed: u64,
        looping: bool,
        reduced: bool,
    ) -> Frame<'a> {
        let cycle = u64::from(self.cycle_ms());
        let t = if reduced {
            cycle / 2
        } else if looping {
            elapsed % cycle
        } else {
            elapsed.min(cycle - 1)
        };
        let mut lo = 0;
        let mut hi = self.frame_count();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if u64::from(u32_at(self.bytes, HEADER_BYTES + mid * ENTRY_BYTES)) <= t {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        self.frame(lo).expect("validated cumulative frame ends")
    }
}
impl<'a> Frame<'a> {
    /// Return encoded payload bytes; raw pixels are RGB565 in display byte order.
    pub const fn payload(self) -> &'a [u8] { self.payload }

    /// Return zero for raw bytes or one for row-local RLE/literal packets.
    pub const fn encoding(self) -> u8 { self.encoding }

    /// Return the admitted shape used for row stride and count.
    pub const fn geometry(self) -> Geometry { self.geometry }

    /// Return a fresh row cursor at row zero; no scratch framebuffer is allocated.
    pub fn rows(self) -> Rows<'a> {
        Rows {
            frame: self,
            offset: 0,
            row: 0,
            crc: !0,
        }
    }
}
/// One row per call. Raw and RLE write directly to the caller's existing row.
pub struct Rows<'a> {
    /// Admitted immutable frame being decoded.
    frame: Frame<'a>,
    /// Next unread encoded byte offset.
    offset: usize,
    /// Number of rows already decoded; equals height when exhausted.
    row: u16,
    /// Uncomplemented CRC of encoded bytes consumed so far.
    crc: u32,
}
impl Rows<'_> {
    /// Return rows already consumed, from zero through the frame height.
    pub fn row(&self) -> usize { usize::from(self.row) }

    /// Decode one row into exact-width `output` (two bytes per pixel), returning true if produced.
    /// Returns false after the last row without touching output, even if its size differs.
    /// Wrong output size fails before cursor/output mutation; later decode errors may leave partial
    /// effects and are not latched by this cursor. Valid admitted immutable input excludes those errors.
    pub fn next(
        &mut self,
        output: &mut [u8],
    ) -> Result<bool, AssetError> {
        let width = usize::from(self.frame.geometry.width());
        let height = self.frame.geometry.height();
        if self.row == height {
            return Ok(false);
        }
        if output.len() != width * 2 {
            return Err(AssetError::Pixels);
        }
        let start = self.offset;
        if self.frame.encoding == 0 {
            let bytes = self
                .frame
                .payload
                .get(start..start + width * 2)
                .ok_or(AssetError::Pixels)?;
            output.copy_from_slice(bytes);
            self.offset += width * 2;
        } else {
            let mut x = 0;
            while x < width {
                let header = self
                    .frame
                    .payload
                    .get(self.offset..self.offset + 2)
                    .ok_or(AssetError::Pixels)?;
                let packet = u16_at(header, 0);
                self.offset += 2;
                let count = usize::from(packet & 0x7FFF) + 1;
                if count > width - x {
                    return Err(AssetError::Pixels);
                }
                if packet & 0x8000 != 0 {
                    let pixel = self
                        .frame
                        .payload
                        .get(self.offset..self.offset + 2)
                        .ok_or(AssetError::Pixels)?;
                    for slot in output[x * 2..(x + count) * 2].as_chunks_mut::<2>().0 {
                        slot.copy_from_slice(pixel);
                    }
                    self.offset += 2;
                } else {
                    let pixels = self
                        .frame
                        .payload
                        .get(self.offset..self.offset + count * 2)
                        .ok_or(AssetError::Pixels)?;
                    output[x * 2..(x + count) * 2].copy_from_slice(pixels);
                    self.offset += count * 2;
                }
                x += count;
            }
        }
        self.crc = crc_update(self.crc, &self.frame.payload[start..self.offset]);
        self.row += 1;
        if self.row == height && self.offset != self.frame.payload.len() {
            return Err(AssetError::Pixels);
        }
        if self.row == height && !self.crc != self.frame.crc {
            return Err(AssetError::Crc);
        }
        Ok(true)
    }
}

/// Incremental full admission: <=2048 CRC bytes or <=4 RLE rows per step.
/// The caller retains its normal event loop and can enforce its own deadline.
pub struct Validator<'a> {
    /// Provisional structurally checked container, never externally published before completion.
    asset: Asset<'a>,
    /// Next byte for whole-container CRC, skipping the header's CRC field.
    global_offset: usize,
    /// Uncomplemented whole-container CRC state.
    global_crc: u32,
    /// Frame currently being checked, or frame count after completion.
    frame: u16,
    /// Encoded offset within the current frame.
    offset: usize,
    /// RLE rows checked within the current frame; raw frames use only offset.
    row: u16,
    /// Uncomplemented checksum for the current encoded frame.
    frame_crc: u32,
    /// Every frame fully checked and accepted.
    done: bool,
    /// First step failure, retained permanently so retries cannot bypass rejection.
    failure: Option<AssetError>,
}
impl<'a> Validator<'a> {
    /// Check `bytes` header/table synchronously and return an incomplete incremental validator.
    /// No data escapes admission yet; dropping the validator has no external side effects.
    pub fn new(bytes: &'a [u8]) -> Result<Self, AssetError> {
        Ok(Self {
            asset: Asset::header(bytes)?,
            global_offset: 32,
            global_crc: crc_update(!0, &bytes[..28]),
            frame: 0,
            offset: 0,
            row: 0,
            frame_crc: !0,
            done: false,
            failure: None,
        })
    }

    /// Advance bounded validation; returns false while pending and true after full admission.
    /// First failure is latched across future calls and finish; completed calls remain true.
    pub fn step(&mut self) -> Result<bool, AssetError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = self.step_inner();
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }

    /// Advance at most 2048 CRC bytes or four RLE rows and return completion/error.
    /// Partial state on error is quarantined by the public step wrapper's permanent failure latch.
    fn step_inner(&mut self) -> Result<bool, AssetError> {
        if self.done {
            return Ok(true);
        }
        if self.global_offset < self.asset.bytes.len() {
            let end = (self.global_offset + 2048).min(self.asset.bytes.len());
            self.global_crc = crc_update(self.global_crc, &self.asset.bytes[self.global_offset..end]);
            self.global_offset = end;
            if end == self.asset.bytes.len() && !self.global_crc != u32_at(self.asset.bytes, 28) {
                return Err(AssetError::Crc);
            }
            return Ok(false);
        }
        let frame = self.asset.frame(usize::from(self.frame)).ok_or(AssetError::Table)?;
        let width = usize::from(self.asset.geometry.width());
        let height = self.asset.geometry.height();
        if frame.encoding == 0 {
            if frame.payload.len() != FRAME_BYTES {
                return Err(AssetError::Pixels);
            }
            let end = (self.offset + 2048).min(frame.payload.len());
            self.frame_crc = crc_update(self.frame_crc, &frame.payload[self.offset..end]);
            self.offset = end;
            if end < frame.payload.len() {
                return Ok(false);
            }
        } else {
            for _ in 0..4 {
                if self.row == height {
                    break;
                }
                let start = self.offset;
                let mut x = 0;
                while x < width {
                    let header = frame
                        .payload
                        .get(self.offset..self.offset + 2)
                        .ok_or(AssetError::Pixels)?;
                    let packet = u16_at(header, 0);
                    let count = usize::from(packet & 0x7FFF) + 1;
                    if count > width - x {
                        return Err(AssetError::Pixels);
                    }
                    self.offset = self
                        .offset
                        .checked_add(2 + if packet & 0x8000 != 0 { 2 } else { count * 2 })
                        .filter(|end| *end <= frame.payload.len())
                        .ok_or(AssetError::Pixels)?;
                    x += count;
                }
                self.frame_crc = crc_update(self.frame_crc, &frame.payload[start..self.offset]);
                self.row += 1;
            }
            if self.row < height {
                return Ok(false);
            }
            if self.offset != frame.payload.len() {
                return Err(AssetError::Pixels);
            }
        }
        let expected = u32_at(
            self.asset.bytes,
            HEADER_BYTES + usize::from(self.frame) * ENTRY_BYTES + 12,
        );
        if !self.frame_crc != expected {
            return Err(AssetError::Crc);
        }
        self.frame += 1;
        self.offset = 0;
        self.row = 0;
        self.frame_crc = !0;
        self.done = usize::from(self.frame) == self.asset.frame_count();
        Ok(self.done)
    }

    /// Return the admitted borrowed asset only after all steps succeed; does not perform more work.
    /// An unfinished validator returns Pixels; a failed validator returns its original failure.
    pub fn finish(&self) -> Result<Asset<'a>, AssetError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.done {
            Ok(self.asset)
        } else {
            Err(AssetError::Pixels)
        }
    }
}
