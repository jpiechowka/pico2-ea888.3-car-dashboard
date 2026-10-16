use dashboard_config::startup_asset::*;

use crate::{ConvertError as Error, Fit, Options};
pub(crate) struct Output {
    frames: Vec<(u32, u8, Vec<u8>)>,
    previous: Vec<u8>,
    bytes: usize,
    pub next: u32,
    pub end: u32,
    start: u32,
    step: u32,
}
impl Output {
    pub fn new(
        options: &Options,
        total: u32,
    ) -> Result<Self, Error> {
        let end = options.trim_end_ms.min(total);
        if end <= options.trim_start_ms {
            return Err(Error::Timeline);
        }
        Ok(Self {
            frames: Vec::new(),
            previous: Vec::new(),
            bytes: HEADER_BYTES,
            next: options.trim_start_ms,
            end,
            start: options.trim_start_ms,
            step: 1000 / u32::from(options.sample_fps),
        })
    }

    pub fn emit(
        &mut self,
        rgba: &[u8],
        w: usize,
        h: usize,
        options: &Options,
        source_end: u32,
    ) -> Result<(), Error> {
        while self.next < source_end && self.next < self.end {
            let mut end = (self.next + self.step).min(self.end);
            if self.end - end < MIN_FRAME_MS {
                end = self.end;
            }
            let relative_end = (end - self.start).max(MIN_FRAME_MS);
            let raw = resize(rgba, w, h, options);
            if raw == self.previous && !self.frames.is_empty() {
                self.frames.last_mut().unwrap().0 = relative_end;
            } else {
                let encoded = encode_frame(&raw);
                let encoding = if encoded.len() < raw.len() { 1 } else { 0 };
                let payload = if encoding == 1 { encoded } else { raw.clone() };
                self.bytes = self
                    .bytes
                    .checked_add(ENTRY_BYTES + payload.len())
                    .filter(|n| *n <= MAX_ASSET_BYTES)
                    .ok_or(Error::AssetLimit)?;
                if self.frames.len() >= MAX_FRAMES {
                    return Err(Error::Frames);
                }
                self.frames.push((relative_end, encoding, payload));
                self.previous = raw;
            }
            self.next = end;
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, Error> {
        if self.frames.is_empty() {
            return Err(Error::Timeline);
        }
        let table_end = HEADER_BYTES + self.frames.len() * ENTRY_BYTES;
        let mut out = Vec::with_capacity(self.bytes);
        out.resize(table_end, 0);
        out[..8].copy_from_slice(MAGIC);
        put16(&mut out, 8, 1);
        put16(&mut out, 10, 32);
        put32(&mut out, 12, self.bytes as u32);
        put16(&mut out, 16, 320);
        put16(&mut out, 18, 240);
        put16(&mut out, 20, self.frames.len() as u16);
        put32(&mut out, 24, self.frames.last().unwrap().0);
        for (index, (end, encoding, payload)) in self.frames.into_iter().enumerate() {
            let at = HEADER_BYTES + index * ENTRY_BYTES;
            let offset = out.len() as u32;
            put32(&mut out, at, end);
            put32(&mut out, at + 4, offset);
            put32(&mut out, at + 8, payload.len() as u32);
            put32(&mut out, at + 12, crc32(&payload));
            out[at + 16] = encoding;
            out.extend(payload);
        }
        let crc = asset_crc32(&out).ok_or(Error::Asset)?;
        put32(&mut out, 28, crc);
        Asset::parse(&out).map_err(|_| Error::Asset)?;
        Ok(out)
    }
}
fn put16(
    b: &mut [u8],
    at: usize,
    v: u16,
) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(
    b: &mut [u8],
    at: usize,
    v: u32,
) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn encode_frame(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_BYTES);
    for row in raw.as_chunks::<{ WIDTH * 2 }>().0 {
        let mut x = 0;
        while x < WIDTH {
            let mut run = 1;
            while x + run < WIDTH && row[x * 2..x * 2 + 2] == row[(x + run) * 2..(x + run) * 2 + 2] {
                run += 1;
            }
            if run >= 2 {
                out.extend_from_slice(&(0x8000 | (run as u16 - 1)).to_le_bytes());
                out.extend_from_slice(&row[x * 2..x * 2 + 2]);
                x += run;
            } else {
                let start = x;
                x += 1;
                while x < WIDTH {
                    if x + 1 < WIDTH && row[x * 2..x * 2 + 2] == row[(x + 1) * 2..(x + 1) * 2 + 2] {
                        break;
                    }
                    x += 1;
                }
                out.extend_from_slice(&(x as u16 - start as u16 - 1).to_le_bytes());
                out.extend_from_slice(&row[start * 2..x * 2]);
            }
            if out.len() >= FRAME_BYTES {
                return raw.to_vec();
            }
        }
    }
    out
}
fn resize(
    rgba: &[u8],
    w: usize,
    h: usize,
    o: &Options,
) -> Vec<u8> {
    let mut out = vec![0; FRAME_BYTES];
    // Integer nearest-neighbor, centered contain/cover and deterministic alpha matte.
    let (scaled_w, scaled_h) = match o.fit {
        Fit::Stretch => (320, 240),
        Fit::Contain => {
            if w * 240 > h * 320 {
                (320, (h * 320 / w).max(1))
            } else {
                ((w * 240 / h).max(1), 240)
            }
        }
        Fit::Cover => {
            if w * 240 > h * 320 {
                ((w * 240 / h).max(320), 240)
            } else {
                (320, (h * 320 / w).max(240))
            }
        }
    };
    let ox = (320_i32 - scaled_w as i32) / 2;
    let oy = (240_i32 - scaled_h as i32) / 2;
    for y in 0..240 {
        for x in 0..320 {
            let sx = x as i32 - ox;
            let sy = y as i32 - oy;
            let mut rgb = o.matte_rgb;
            if sx >= 0 && sy >= 0 && (sx as usize) < scaled_w && (sy as usize) < scaled_h {
                let at = (((sy as usize * h / scaled_h).min(h - 1)) * w + (sx as usize * w / scaled_w).min(w - 1)) * 4;
                let alpha = u32::from(rgba[at + 3]);
                for c in 0..3 {
                    rgb[c] = ((u32::from(rgba[at + c]) * alpha + u32::from(rgb[c]) * (255 - alpha) + 127) / 255) as u8;
                }
            }
            let pixel = ((u16::from(rgb[0]) * 31 + 127) / 255) << 11
                | ((u16::from(rgb[1]) * 63 + 127) / 255) << 5
                | ((u16::from(rgb[2]) * 31 + 127) / 255);
            out[(y * 320 + x) * 2..(y * 320 + x) * 2 + 2].copy_from_slice(&pixel.to_be_bytes());
        }
    }
    out
}
