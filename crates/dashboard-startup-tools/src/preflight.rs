//! Structural, checksum and cumulative-work admission before allocating decoded frames.
//! Image decompression remains the decoder's job; these parsers do not prove valid pixel data.
use crate::{ConvertError as E, *};
/// Return the admitted PNG dimensions from bounded `b` (at most `MAX_SOURCE_BYTES` at callers).
/// Require one first IHDR, CRC-correct chunks (at most 65,536), no APNG controls and an empty
/// IEND at exact EOF. IDAT ordering, color/compression semantics and pixels are checked later.
/// Malformed framing/CRC returns `Malformed`; APNG returns `Unsupported`; size errors propagate.
pub fn png(b: &[u8]) -> Result<(u32, u32), E> {
    if b.get(..8) != Some(b"\x89PNG\r\n\x1a\n") {
        return Err(E::Malformed);
    }
    let mut at = 8;
    let mut size = None;
    let mut chunks = 0;
    loop {
        let header = b.get(at..at + 8).ok_or(E::Malformed)?;
        let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .filter(|n| *n <= b.len())
            .ok_or(E::Malformed)?;
        let kind = &header[4..];
        let payload = &b[at + 8..end - 4];
        if dashboard_config::startup_asset::crc32(&b[at + 4..end - 4])
            != u32::from_be_bytes(b[end - 4..end].try_into().unwrap())
        {
            return Err(E::Malformed);
        }
        chunks += 1;
        if chunks > 65536 {
            return Err(E::Malformed);
        }
        if at == 8 {
            if kind != b"IHDR" || len != 13 {
                return Err(E::Malformed);
            }
            let w = u32::from_be_bytes(payload[..4].try_into().unwrap());
            let h = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            dimensions(w, h)?;
            size = Some((w, h));
        } else if kind == b"IHDR" {
            return Err(E::Malformed);
        }
        if [b"acTL", b"fcTL", b"fdAT"].contains(&kind.try_into().unwrap()) {
            return Err(E::Unsupported);
        }
        if kind == b"IEND" {
            if len != 0 || end != b.len() {
                return Err(E::Malformed);
            }
            return size.ok_or(E::Malformed);
        }
        at = end;
    }
}
/// Preflight facts for a complete GIF stream; durations correspond one-to-one with image blocks.
pub struct GifInfo {
    /// Validated logical-canvas width in pixels.
    pub width: u32,
    /// Validated logical-canvas height in pixels.
    pub height: u32,
    /// Source image delays in milliseconds; missing/zero delays normalize to 100 ms.
    pub durations: Vec<u32>,
    /// Checked sum of normalized delays, at most 60,000 ms.
    pub total: u32,
    /// Count of explicit zero-delay graphic controls, excluding absent controls.
    pub zero: u16,
}
/// Borrow the next `n` bytes from `b`, advancing `at` only on success.
/// Return `Malformed` on cursor addition overflow or missing bytes; the cursor remains unchanged.
fn take<'a>(
    b: &'a [u8],
    at: &mut usize,
    n: usize,
) -> Result<&'a [u8], E> {
    let end = at.checked_add(n).filter(|e| *e <= b.len()).ok_or(E::Malformed)?;
    let out = &b[*at..end];
    *at = end;
    Ok(out)
}
/// Skip GIF length-prefixed sub-blocks in `b` through their zero terminator, advancing `at`.
/// Return `Malformed` for truncation; already consumed blocks remain consumed on failure.
fn blocks(
    b: &[u8],
    at: &mut usize,
) -> Result<(), E> {
    loop {
        let n = usize::from(take(b, at, 1)?[0]);
        if n == 0 {
            return Ok(());
        }
        take(b, at, n)?;
    }
}
/// Return bounded GIF87a/89a structure/timeline facts for compressed `b`.
/// Require nonempty images, in-canvas rectangles, valid control framing, supported disposal,
/// bounded frames/area/time and exact trailer EOF. Comments and the two recognized loop
/// extensions are admitted; loop counts do not control startup playback. Duplicate/dangling
/// graphic controls fail. LZW bytes and palette indices are validated only by the later decoder.
/// Errors distinguish malformed framing, unsupported controls and resource/timeline bounds.
pub fn gif(b: &[u8]) -> Result<GifInfo, E> {
    if b.len() < 13 || (&b[..6] != b"GIF87a" && &b[..6] != b"GIF89a") {
        return Err(E::Malformed);
    }
    let width = u32::from(u16::from_le_bytes([b[6], b[7]]));
    let height = u32::from(u16::from_le_bytes([b[8], b[9]]));
    dimensions(width, height)?;
    let mut at = 13;
    if b[10] & 128 != 0 {
        take(b, &mut at, 3 * (1_usize << ((b[10] & 7) + 1)))?;
    }
    let mut durations = Vec::new();
    let mut total = 0_u32;
    let mut area = 0_usize;
    let mut delay = 100;
    let mut zero = 0;
    let mut gce = false;
    loop {
        match take(b, &mut at, 1)?[0] {
            0x3B => {
                if at != b.len() || durations.is_empty() || gce {
                    return Err(E::Malformed);
                }
                return Ok(GifInfo {
                    width,
                    height,
                    durations,
                    total,
                    zero,
                });
            }
            0x21 => {
                match take(b, &mut at, 1)?[0] {
                    0xF9 => {
                        if gce {
                            return Err(E::Malformed);
                        }
                        let g = take(b, &mut at, 6)?;
                        if g[0] != 4 || g[5] != 0 || g[1] & 0xE0 != 0 {
                            return Err(E::Malformed);
                        }
                        if (g[1] >> 2) & 7 > 3 || g[1] & 2 != 0 {
                            return Err(E::Unsupported);
                        }
                        delay = u32::from(u16::from_le_bytes([g[2], g[3]])) * 10;
                        if delay == 0 {
                            delay = 100;
                            zero += 1;
                        }
                        gce = true;
                    }
                    0xFE => blocks(b, &mut at)?,
                    0xFF => {
                        let n = usize::from(take(b, &mut at, 1)?[0]);
                        let app = take(b, &mut at, n)?;
                        if app != b"NETSCAPE2.0" && app != b"ANIMEXTS1.0" {
                            return Err(E::Unsupported);
                        }
                        let loopdata = take(b, &mut at, 5)?;
                        if loopdata[0] != 3 || loopdata[1] != 1 || loopdata[4] != 0 {
                            return Err(E::Malformed);
                        }
                    }
                    _ => return Err(E::Unsupported),
                }
            }
            0x2C => {
                let f = take(b, &mut at, 9)?;
                let x = u32::from(u16::from_le_bytes([f[0], f[1]]));
                let y = u32::from(u16::from_le_bytes([f[2], f[3]]));
                let w = u32::from(u16::from_le_bytes([f[4], f[5]]));
                let h = u32::from(u16::from_le_bytes([f[6], f[7]]));
                dimensions(w, h)?;
                if x + w > width || y + h > height || f[8] & 0x18 != 0 {
                    return Err(E::Malformed);
                }
                area = area
                    .checked_add((w * h) as usize)
                    .filter(|n| *n <= MAX_DECODED_PIXELS)
                    .ok_or(E::Frames)?;
                if durations.len() >= MAX_SOURCE_FRAMES {
                    return Err(E::Frames);
                }
                total = total
                    .checked_add(delay)
                    .filter(|t| *t <= MAX_SOURCE_MS)
                    .ok_or(E::Timeline)?;
                durations.push(delay);
                delay = 100;
                gce = false;
                if f[8] & 128 != 0 {
                    take(b, &mut at, 3 * (1_usize << ((f[8] & 7) + 1)))?;
                }
                let code = take(b, &mut at, 1)?[0];
                if !(2..=8).contains(&code) {
                    return Err(E::Malformed);
                }
                blocks(b, &mut at)?;
            }
            _ => return Err(E::Malformed),
        }
    }
}
