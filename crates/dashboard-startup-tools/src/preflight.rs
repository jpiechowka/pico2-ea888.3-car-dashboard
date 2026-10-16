use crate::{ConvertError as E, *};
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
pub struct GifInfo {
    pub width: u32,
    pub height: u32,
    pub durations: Vec<u32>,
    pub total: u32,
    pub zero: u16,
}
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
