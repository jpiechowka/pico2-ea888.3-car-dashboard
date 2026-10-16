use crate::encode::Output;
use crate::{ConvertError as E, *};
pub fn convert(
    source: &[u8],
    o: &Options,
) -> Result<Converted, E> {
    if o.sheet.is_some() {
        return Err(E::Options);
    }
    let info = preflight::gif(source)?;
    let mut settings = gif::DecodeOptions::new();
    settings.set_color_output(gif::ColorOutput::Indexed);
    settings.set_memory_limit(gif::MemoryLimit::Bytes((MAX_PIXELS as u64).try_into().unwrap()));
    settings.check_frame_consistency(true);
    settings.check_lzw_end_code(true);
    let mut reader = settings
        .read_info(std::io::Cursor::new(source))
        .map_err(|_| E::Decode)?;
    let global = reader.global_palette().map(<[u8]>::to_vec);
    let background = match o.gif_background {
        GifBackground::Transparent => [0; 4],
        GifBackground::Logical => {
            if let Some(palette) = &global {
                let index = reader.bg_color().ok_or(E::Palette)? * 3;
                let rgb = palette.get(index..index + 3).ok_or(E::Palette)?;
                [rgb[0], rgb[1], rgb[2], 255]
            } else {
                [0; 4]
            }
        }
    };
    let w = info.width as usize;
    let h = info.height as usize;
    let mut canvas = vec![0; w * h * 4];
    for pixel in canvas.as_chunks_mut::<4>().0.iter_mut() {
        pixel.copy_from_slice(&background);
    }
    let mut restore = Vec::new();
    let mut output = Output::new(o, info.total)?;
    let trim_end = output.end;
    let mut time = 0;
    for duration in &info.durations {
        let frame = reader.read_next_frame().map_err(|_| E::Decode)?.ok_or(E::Decode)?;
        let palette = frame.palette.as_deref().or(global.as_deref()).ok_or(E::Palette)?;
        let fw = usize::from(frame.width);
        let fh = usize::from(frame.height);
        let left = usize::from(frame.left);
        let top = usize::from(frame.top);
        if frame.buffer.len() != fw * fh || palette.len() % 3 != 0 || palette.len() > 768 {
            return Err(E::Palette);
        }
        if frame
            .transparent
            .is_some_and(|index| usize::from(index) >= palette.len() / 3)
        {
            return Err(E::Palette);
        }
        if frame.dispose == gif::DisposalMethod::Previous {
            restore.resize(fw * fh * 4, 0);
            for row in 0..fh {
                let at = ((top + row) * w + left) * 4;
                restore[row * fw * 4..(row + 1) * fw * 4].copy_from_slice(&canvas[at..at + fw * 4]);
            }
        }
        for y in 0..fh {
            for x in 0..fw {
                let index = frame.buffer[y * fw + x];
                if usize::from(index) >= palette.len() / 3 {
                    return Err(E::Palette);
                }
                if Some(index) == frame.transparent {
                    continue;
                }
                let rgb = &palette[usize::from(index) * 3..usize::from(index) * 3 + 3];
                let at = ((top + y) * w + left + x) * 4;
                canvas[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        time += duration;
        output.emit(&canvas, w, h, o, time)?;
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in 0..fh {
                    for x in 0..fw {
                        let at = ((top + y) * w + left + x) * 4;
                        canvas[at..at + 4].copy_from_slice(&background);
                    }
                }
            }
            gif::DisposalMethod::Previous => {
                for row in 0..fh {
                    let at = ((top + row) * w + left) * 4;
                    canvas[at..at + fw * 4].copy_from_slice(&restore[row * fw * 4..(row + 1) * fw * 4]);
                }
            }
            gif::DisposalMethod::Any | gif::DisposalMethod::Keep => {}
        }
    }
    if reader.read_next_frame().map_err(|_| E::Decode)?.is_some() {
        return Err(E::Frames);
    }
    let asset = output.finish()?;
    Ok(Converted {
        asset,
        source_width: info.width,
        source_height: info.height,
        source_frames: info.durations.len() as u16,
        source_duration_ms: info.total,
        zero_delay_frames: info.zero,
        timing_changed: info.durations.iter().any(|d| *d != 1000 / u32::from(o.sample_fps)),
        trim_end_ms: trim_end,
    })
}
