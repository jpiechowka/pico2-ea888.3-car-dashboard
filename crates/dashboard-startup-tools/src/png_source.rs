//! Static PNG decoding, RGBA normalization and row-major sprite extraction.
use crate::encode::Output;
use crate::{ConvertError as E, *};
/// Decode bounded `source` after PNG preflight using already validated `o`; return native asset facts.
/// Expand palette/low-bit samples and strip 16-bit channels to eight bits; ignore text/ICC data.
/// The decoder has an 8 MiB allocation budget in addition to caller-owned RGBA/output buffers.
/// A static PNG lasts 1000 ms; sprites require exact grid division and bounded count/duration.
/// Errors discard every temporary/output buffer. Caller must enforce source bytes/options first.
pub fn convert(
    source: &[u8],
    o: &Options,
) -> Result<Converted, E> {
    let (width, height) = preflight::png(source)?;
    let mut decoder =
        png::Decoder::new_with_limits(std::io::Cursor::new(source), png::Limits { bytes: 8 * 1024 * 1024 });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| E::Decode)?;
    if reader.info().animation_control.is_some() {
        return Err(E::Unsupported);
    }
    let len = reader
        .output_buffer_size()
        .filter(|n| *n <= MAX_PIXELS * 4)
        .ok_or(E::Dimensions)?;
    let mut decoded = vec![0; len];
    let info = reader.next_frame(&mut decoded).map_err(|_| E::Decode)?;
    reader.finish().map_err(|_| E::Decode)?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err(E::Decode),
    };
    let pixels = dimensions(width, height)?;
    if info.bit_depth != png::BitDepth::Eight || info.buffer_size() != pixels * channels {
        return Err(E::Decode);
    }
    let mut rgba = vec![0; pixels * 4];
    for (src, dst) in decoded[..info.buffer_size()]
        .chunks_exact(channels)
        .zip(rgba.as_chunks_mut::<4>().0.iter_mut())
    {
        match channels {
            1 => dst.copy_from_slice(&[src[0], src[0], src[0], 255]),
            2 => dst.copy_from_slice(&[src[0], src[0], src[0], src[1]]),
            3 => dst.copy_from_slice(&[src[0], src[1], src[2], 255]),
            4 => dst.copy_from_slice(src),
            _ => unreachable!(),
        }
    }
    drop(decoded);
    drop(reader);
    let (cols, rows, frame_ms) = if o.kind == SourceKind::SpriteSheet {
        let sheet = o.sheet.ok_or(E::Options)?;
        if sheet.columns == 0
            || sheet.rows == 0
            || sheet.frame_ms == 0
            || width % u32::from(sheet.columns) != 0
            || height % u32::from(sheet.rows) != 0
        {
            return Err(E::Options);
        }
        (
            usize::from(sheet.columns),
            usize::from(sheet.rows),
            u32::from(sheet.frame_ms),
        )
    } else {
        if o.sheet.is_some() {
            return Err(E::Options);
        }
        (1, 1, 1000)
    };
    let count = cols
        .checked_mul(rows)
        .filter(|n| *n <= MAX_SOURCE_FRAMES)
        .ok_or(E::Frames)?;
    let total = (count as u32)
        .checked_mul(frame_ms)
        .filter(|n| *n <= MAX_SOURCE_MS)
        .ok_or(E::Timeline)?;
    let mut output = Output::new(o, total)?;
    let trim_end = output.end;
    let w = width as usize / cols;
    let h = height as usize / rows;
    let mut cell = vec![0; w * h * 4];
    for index in 0..count {
        let x = index % cols * w;
        let y = index / cols * h;
        for row in 0..h {
            let at = ((y + row) * width as usize + x) * 4;
            cell[row * w * 4..(row + 1) * w * 4].copy_from_slice(&rgba[at..at + w * 4]);
        }
        output.emit(&cell, w, h, o, (index as u32 + 1) * frame_ms)?;
    }
    let asset = output.finish()?;
    Ok(Converted {
        asset,
        source_width: width,
        source_height: height,
        source_frames: count as u16,
        source_duration_ms: total,
        zero_delay_frames: 0,
        timing_changed: count > 1 && frame_ms != 1000 / u32::from(o.sample_fps),
        trim_end_ms: trim_end,
    })
}
