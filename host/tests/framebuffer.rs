//! Production framebuffer byte order, bounded clipping and exact source-iterator consumption.
use dashboard_host_validation::renderer::{FrameBuffer, HEIGHT, St7789Renderer, WIDTH};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Fixed storage size/alignment and full clears preserve big-endian panel RGB565 for all primary colours.
#[test]
fn storage_is_fixed_size_aligned_and_clear_writes_big_endian_pixels() {
    assert_eq!(size_of::<FrameBuffer>(), WIDTH * HEIGHT * 2);
    assert!(align_of::<FrameBuffer>() >= 4);
    let mut buffer = Box::new(FrameBuffer::new());
    assert_eq!(buffer.as_ptr().align_offset(4), 0);
    for color in [Rgb565::BLACK, Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE] {
        St7789Renderer::new(&mut buffer).clear_buffer(color);
        let expected = color.into_storage().to_be_bytes();
        assert!(buffer.as_chunks::<2>().0.iter().all(|pixel| *pixel == expected));
    }
}

/// Deterministic random clipped rectangles and short sources match an independent per-coordinate colour oracle.
#[test]
fn clipped_contiguous_pixels_match_independent_coordinate_reference() {
    let mut seed = 0x12345678u32;
    for _ in 0..200 {
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed
        };
        let x = (next() % 400) as i32 - 40;
        let y = (next() % 320) as i32 - 40;
        let w = next() % 60;
        let h = next() % 60;
        let count = next() % (w * h + 1);
        let color = |i: u32| Rgb565::new((i % 32) as u8, (i % 64) as u8, ((i / 32) % 32) as u8);
        let mut buffer = Box::new(FrameBuffer::new());
        St7789Renderer::new(&mut buffer)
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(w, h)),
                (0..count).map(color),
            )
            .unwrap();
        for py in 0..HEIGHT {
            for px in 0..WIDTH {
                let sx = px as i32 - x;
                let sy = py as i32 - y;
                let index = sy as i64 * w as i64 + sx as i64;
                let expected = if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 && index < count as i64 {
                    color(index as u32).into_storage()
                } else {
                    0
                };
                let offset = (py * WIDTH + px) * 2;
                assert_eq!(
                    u16::from_be_bytes([buffer[offset], buffer[offset + 1]]),
                    expected,
                    "rect=({x},{y},{w},{h}), count={count}, pixel=({px},{py})"
                );
            }
        }
    }
}

/// Odd/even widths at the final panel row write only their clipped pixels, including zero-width spans.
#[test]
fn solid_fill_handles_odd_row_edges_without_touching_neighbours() {
    for x in [0, 1, 2, 317, 318, 319] {
        for width in [0, 1, 2, 3, 4, 5] {
            let mut buffer = Box::new(FrameBuffer::new());
            St7789Renderer::new(&mut buffer)
                .fill_solid(&Rectangle::new(Point::new(x, 239), Size::new(width, 2)), Rgb565::WHITE)
                .unwrap();
            for py in 0..HEIGHT {
                for px in 0..WIDTH {
                    let offset = (py * WIDTH + px) * 2;
                    let value = u16::from_be_bytes([buffer[offset], buffer[offset + 1]]);
                    assert_eq!(
                        value,
                        if py == 239 && px >= x as usize && px < x as usize + width as usize {
                            65535
                        } else {
                            0
                        }
                    );
                }
            }
        }
    }
}

/// Solid fill handles row alignment, negative origins and overrun against a sentinel-filled independent reference.
#[test]
fn solid_spans_match_nonzero_pixels_across_rows_and_alignments() {
    for (x, y, width, height) in [
        (0, 0, 320, 1),
        (0, 1, 320, 3),
        (0, 239, 320, 2),
        (-7, 238, 400, 9),
        (-3, -2, 329, 245),
        (1, 6, 1, 3),
        (2, 6, 3, 3),
        (317, 237, 9, 4),
        (319, 0, 1, 1),
        (3, 5, 126, 4),
        (2, 9, 128, 3),
    ] {
        for color in [Rgb565::new(7, 41, 19), Rgb565::RED, Rgb565::BLUE] {
            let mut buffer = Box::<FrameBuffer>::default();
            buffer.fill(0xA5);
            St7789Renderer::new(&mut buffer)
                .fill_solid(&Rectangle::new(Point::new(x, y), Size::new(width, height)), color)
                .unwrap();
            for py in 0..HEIGHT {
                for px in 0..WIDTH {
                    let inside = px as i64 >= i64::from(x)
                        && (px as i64) < i64::from(x) + i64::from(width)
                        && py as i64 >= i64::from(y)
                        && (py as i64) < i64::from(y) + i64::from(height);
                    let expected = if inside {
                        color.into_storage().to_be_bytes()
                    } else {
                        [0xA5; 2]
                    };
                    let at = (py * WIDTH + px) * 2;
                    assert_eq!(
                        buffer[at..at + 2],
                        expected,
                        "rect=({x},{y},{width},{height}), pixel=({px},{py}), color={color:?}"
                    );
                }
            }
        }
    }
}

/// Extreme/unrepresentable geometry leaves bytes unchanged and cannot even poll the supplied colour iterator.
#[test]
fn unrepresentable_geometry_cannot_overflow_or_consume_unbounded_colors() {
    for rect in [
        Rectangle::new(Point::new(i32::MAX, 0), Size::new(1, 1)),
        Rectangle::new(Point::new(0, i32::MAX), Size::new(1, 1)),
        Rectangle::new(Point::new(i32::MAX, i32::MAX), Size::new(u32::MAX, u32::MAX)),
        Rectangle::new(Point::new(i32::MIN, i32::MIN), Size::new(u32::MAX, u32::MAX)),
        Rectangle::new(Point::new(-1, -1), Size::new(u32::MAX, u32::MAX)),
    ] {
        let mut buffer = Box::new(FrameBuffer::new());
        let mut renderer = St7789Renderer::new(&mut buffer);
        renderer.fill_solid(&rect, Rgb565::WHITE).unwrap();
        renderer
            .fill_contiguous(&rect, std::iter::from_fn(|| panic!("invalid geometry consumed colors")))
            .unwrap();
        assert!(buffer.iter().all(|byte| *byte == 0));
    }
}

/// Clipped source consumption stops exactly after the final needed value or first exhaustion, without extra next calls.
#[test]
fn contiguous_fill_preserves_consumption_at_clipping_and_exhaustion_boundaries() {
    // An extra next(), a missing row skip, or a write after exhaustion must fail.
    for (x, y, width, height) in [
        (-2, -1, 5, 4),
        (318, 238, 5, 4),
        (-2, 238, 324, 4),
        (3, 7, 3, 2),
        (320, 0, 3, 2),
        (0, 240, 3, 2),
        (0, 0, 0, 2),
    ] {
        let source_index = |px: usize, py: usize| {
            let sx = px as i64 - i64::from(x);
            let sy = py as i64 - i64::from(y);
            (sx >= 0 && sy >= 0 && sx < i64::from(width) && sy < i64::from(height))
                .then_some((sy * i64::from(width) + sx) as usize)
        };
        let required = (0..HEIGHT)
            .flat_map(|py| (0..WIDTH).filter_map(move |px| source_index(px, py)))
            .max()
            .map_or(0, |last| last + 1);
        let color = |index: usize| Rgb565::new((index % 32) as u8, ((index / 3) % 64) as u8, ((index / 7) % 32) as u8);
        let first = (0..HEIGHT)
            .flat_map(|py| (0..WIDTH).filter_map(move |px| source_index(px, py)))
            .min()
            .unwrap_or(0);
        for available in [
            0,
            first.saturating_sub(1),
            first,
            first + 1,
            first + 2,
            first + 3,
            width as usize - usize::from(width > 0),
            width as usize,
            width as usize + 1,
            required.saturating_sub(1),
            required,
            required + 1,
        ] {
            let expected_calls = required.min(available + 1);
            let mut calls = 0;
            let mut buffer = Box::<FrameBuffer>::default();
            buffer.fill(0xA5);
            let colors = std::iter::from_fn(|| {
                assert!(
                    calls < expected_calls,
                    "excess next: rect=({x},{y},{width},{height}), available={available}"
                );
                let index = calls;
                calls += 1;
                (index < available).then(|| color(index))
            });
            St7789Renderer::new(&mut buffer)
                .fill_contiguous(&Rectangle::new(Point::new(x, y), Size::new(width, height)), colors)
                .unwrap();
            assert_eq!(calls, expected_calls, "source consumption for {available} colors");
            for py in 0..HEIGHT {
                for px in 0..WIDTH {
                    let expected = source_index(px, py)
                        .filter(|index| *index < available)
                        .map_or([0xA5; 2], |index| color(index).into_storage().to_be_bytes());
                    let at = (py * WIDTH + px) * 2;
                    assert_eq!(buffer[at..at + 2], expected, "pixel=({px},{py}), available={available}");
                }
            }
        }
    }
}

/// Both orientations expose packed row widths and exactly bound final-row/final-pixel access in the same storage.
#[test]
fn both_geometries_use_packed_rows_and_bound_last_pixel() {
    use dashboard_config::Geometry;
    for geometry in Geometry::ALL {
        let mut buffer = Box::<FrameBuffer>::default();
        let mut renderer = St7789Renderer::with_geometry(&mut buffer, geometry);
        assert_eq!(renderer.geometry(), geometry);
        assert_eq!(
            renderer.size(),
            Size::new(geometry.width().into(), geometry.height().into())
        );
        assert_eq!(
            renderer.row_bytes_mut(0).unwrap().len(),
            usize::from(geometry.width()) * 2
        );
        renderer
            .row_bytes_mut(usize::from(geometry.height()) - 1)
            .unwrap()
            .fill(0x55);
        assert!(renderer.row_bytes_mut(usize::from(geometry.height())).is_none());
        renderer
            .draw_iter([Pixel(
                Point::new(i32::from(geometry.width()) - 1, i32::from(geometry.height()) - 1),
                Rgb565::RED,
            )])
            .unwrap();
        assert_eq!(&buffer[buffer.len() - 2..], &Rgb565::RED.into_storage().to_be_bytes());
        assert!(
            buffer[..buffer.len() - usize::from(geometry.width()) * 2]
                .iter()
                .all(|byte| *byte == 0)
        );
    }
}

/// Solid and contiguous fills agree with independent clipping at portrait's bottom-right corner.
#[test]
fn portrait_clipped_bottom_rows_match_reference_for_solid_and_contiguous() {
    use dashboard_config::Geometry;
    for contiguous in [false, true] {
        let mut buffer = Box::<FrameBuffer>::default();
        let mut renderer = St7789Renderer::with_geometry(&mut buffer, Geometry::Portrait);
        let area = Rectangle::new(Point::new(237, 318), Size::new(7, 5));
        if contiguous {
            renderer
                .fill_contiguous(&area, core::iter::repeat(Rgb565::GREEN))
                .unwrap();
        } else {
            renderer.fill_solid(&area, Rgb565::GREEN).unwrap();
        }
        for y in 0..320 {
            for x in 0..240 {
                let at = (y * 240 + x) * 2;
                let expected = if y >= 318 && x >= 237 {
                    Rgb565::GREEN
                } else {
                    Rgb565::BLACK
                };
                assert_eq!(&buffer[at..at + 2], &expected.into_storage().to_be_bytes());
            }
        }
    }
}
