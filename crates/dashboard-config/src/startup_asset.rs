//! Borrowed, bounded EASA v1 assets. No image decoder or allocated frame index.
pub const MAX_ASSET_BYTES: usize = 524_288;
pub const MAX_FRAMES: usize = 300;
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 2;
pub const MIN_FRAME_MS: u32 = 34;
pub const MAGIC: &[u8; 8] = b"EASSET\0\0";
pub const HEADER_BYTES: usize = 32;
pub const ENTRY_BYTES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetError {
    Header,
    Limit,
    Table,
    Crc,
    Encoding,
    Pixels,
}
#[derive(Clone, Copy, Debug)]
pub struct Asset<'a> {
    bytes: &'a [u8],
}
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    payload: &'a [u8],
    encoding: u8,
}
fn u16_at(
    b: &[u8],
    at: usize,
) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(
    b: &[u8],
    at: usize,
) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
pub fn crc32(bytes: &[u8]) -> u32 { !crc_update(!0, bytes) }
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
pub fn asset_crc32(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= HEADER_BYTES).then(|| !crc_update(crc_update(!0, &bytes[..28]), &bytes[32..]))
}
impl<'a> Asset<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, AssetError> {
        let mut validator = Validator::new(bytes)?;
        while !validator.step()? {}
        validator.finish()
    }

    fn header(bytes: &'a [u8]) -> Result<Self, AssetError> {
        if bytes.len() < HEADER_BYTES || bytes.len() > MAX_ASSET_BYTES {
            return Err(AssetError::Limit);
        }
        if &bytes[..8] != MAGIC
            || u16_at(bytes, 8) != 1
            || u16_at(bytes, 10) != 32
            || u32_at(bytes, 12) as usize != bytes.len()
            || u16_at(bytes, 16) != 320
            || u16_at(bytes, 18) != 240
            || u16_at(bytes, 22) != 0
        {
            return Err(AssetError::Header);
        }
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
        Ok(Self { bytes })
    }

    pub const fn bytes(self) -> &'a [u8] { self.bytes }

    pub fn frame_count(self) -> usize { usize::from(u16_at(self.bytes, 20)) }

    pub fn cycle_ms(self) -> u32 { u32_at(self.bytes, 24) }

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
        })
    }

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
    pub const fn payload(self) -> &'a [u8] { self.payload }

    pub const fn encoding(self) -> u8 { self.encoding }

    pub fn rows(self) -> Rows<'a> {
        Rows {
            frame: self,
            offset: 0,
            row: 0,
        }
    }
}
/// One row per call. Raw and RLE write directly to the caller's existing row.
pub struct Rows<'a> {
    frame: Frame<'a>,
    offset: usize,
    row: u16,
}
impl Rows<'_> {
    pub fn row(&self) -> usize { usize::from(self.row) }

    pub fn next(
        &mut self,
        output: &mut [u8],
    ) -> Result<bool, AssetError> {
        if self.row as usize == HEIGHT {
            return Ok(false);
        }
        if output.len() != WIDTH * 2 {
            return Err(AssetError::Pixels);
        }
        let start = self.offset;
        if self.frame.encoding == 0 {
            let bytes = self
                .frame
                .payload
                .get(start..start + WIDTH * 2)
                .ok_or(AssetError::Pixels)?;
            output.copy_from_slice(bytes);
            self.offset += WIDTH * 2;
        } else {
            let mut x = 0;
            while x < WIDTH {
                let header = self
                    .frame
                    .payload
                    .get(self.offset..self.offset + 2)
                    .ok_or(AssetError::Pixels)?;
                let packet = u16_at(header, 0);
                self.offset += 2;
                let count = usize::from(packet & 0x7FFF) + 1;
                if count > WIDTH - x {
                    return Err(AssetError::Pixels);
                }
                if packet & 0x8000 != 0 {
                    let pixel = self
                        .frame
                        .payload
                        .get(self.offset..self.offset + 2)
                        .ok_or(AssetError::Pixels)?;
                    for slot in output[x * 2..(x + count) * 2].chunks_exact_mut(2) {
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
        self.row += 1;
        if self.row as usize == HEIGHT && self.offset != self.frame.payload.len() {
            return Err(AssetError::Pixels);
        }
        Ok(true)
    }
}

/// Incremental full admission: <=2048 CRC bytes or <=4 RLE rows per step.
/// The caller retains its normal event loop and can enforce its own deadline.
pub struct Validator<'a> {
    asset: Asset<'a>,
    global_offset: usize,
    global_crc: u32,
    frame: u16,
    offset: usize,
    row: u16,
    frame_crc: u32,
    done: bool,
}
impl<'a> Validator<'a> {
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
        })
    }

    pub fn step(&mut self) -> Result<bool, AssetError> {
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
                if usize::from(self.row) == HEIGHT {
                    break;
                }
                let start = self.offset;
                let mut x = 0;
                while x < WIDTH {
                    let header = frame
                        .payload
                        .get(self.offset..self.offset + 2)
                        .ok_or(AssetError::Pixels)?;
                    let packet = u16_at(header, 0);
                    let count = usize::from(packet & 0x7FFF) + 1;
                    if count > WIDTH - x {
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
            if usize::from(self.row) < HEIGHT {
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

    pub fn finish(&self) -> Result<Asset<'a>, AssetError> {
        if self.done {
            Ok(self.asset)
        } else {
            Err(AssetError::Pixels)
        }
    }
}
