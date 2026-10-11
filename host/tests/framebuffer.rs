use dashboard_host_validation::renderer::{FrameBuffer, HEIGHT, St7789Renderer, WIDTH};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

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

#[test]
fn unrepresentable_geometry_cannot_overflow_or_consume_unbounded_colors() {
    for rect in [
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
