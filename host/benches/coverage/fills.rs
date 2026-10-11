use std::cell::Cell;
use std::hint::black_box;

use criterion::{Criterion, Throughput};
use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

fn cases() -> [(&'static str, Rectangle, usize, usize); 4] {
    [
        ("full", Rectangle::new(Point::zero(), Size::new(320, 240)), 76800, 76800),
        (
            "narrow_1px",
            Rectangle::new(Point::new(3, 5), Size::new(1, 200)),
            200,
            200,
        ),
        (
            "odd_x_width",
            Rectangle::new(Point::new(3, 9), Size::new(127, 81)),
            10287,
            10287,
        ),
        (
            "clipped_four_edges",
            Rectangle::new(Point::new(-3, -2), Size::new(329, 245)),
            76800,
            79612,
        ),
    ]
}

fn color(index: usize) -> Rgb565 {
    Rgb565::new((index % 32) as u8, ((index / 7) % 64) as u8, ((index / 13) % 32) as u8)
}

fn draw(
    frame: &mut FrameBuffer,
    rect: &Rectangle,
    solid: bool,
) {
    let mut display = St7789Renderer::new(frame);
    if solid {
        display.fill_solid(black_box(rect), black_box(Rgb565::GREEN)).unwrap();
    } else {
        let count = rect.size.width as usize * rect.size.height as usize;
        display
            .fill_contiguous(black_box(rect), (0..black_box(count)).map(color))
            .unwrap();
    }
    black_box(frame);
}

pub fn verify() {
    for (label, rect, written, consumed) in cases() {
        for solid in [true, false] {
            let mut frame = Box::new(FrameBuffer::new());
            draw(&mut frame, &rect, solid);
            let mut pixels = 0;
            // Independent coordinate oracle, including every untouched pixel.
            for y in 0..240i32 {
                for x in 0..320i32 {
                    let sx = x - rect.top_left.x;
                    let sy = y - rect.top_left.y;
                    let expected = if sx >= 0 && sy >= 0 && sx < rect.size.width as i32 && sy < rect.size.height as i32
                    {
                        pixels += 1;
                        if solid {
                            Rgb565::GREEN
                        } else {
                            color(sy as usize * rect.size.width as usize + sx as usize)
                        }
                    } else {
                        Rgb565::BLACK
                    };
                    let at = (y as usize * 320 + x as usize) * 2;
                    assert_eq!(
                        &frame[at..at + 2],
                        &expected.into_storage().to_be_bytes(),
                        "{label}, {solid}, ({x},{y})"
                    );
                }
            }
            assert_eq!(pixels, written);
            if !solid {
                let count = Cell::new(0);
                let colors = (0..rect.size.width as usize * rect.size.height as usize)
                    .map(color)
                    .inspect(|_| count.set(count.get() + 1));
                let mut counted = Box::new(FrameBuffer::new());
                St7789Renderer::new(&mut counted)
                    .fill_contiguous(&rect, colors)
                    .unwrap();
                assert_eq!(count.get(), consumed, "{label} source iterator consumption");
                assert!(counted == frame);
            }
        }
    }
}

pub fn benchmarks(c: &mut Criterion) {
    verify();
    for solid in [true, false] {
        let mut group = c.benchmark_group(if solid { "fill_solid" } else { "fill_contiguous" });
        for (label, rect, written, consumed) in cases() {
            eprintln!("WORK fill {label}: written_pixels={written} contiguous_source_elements={consumed}");
            group.throughput(Throughput::Bytes(written as u64 * 2));
            let mut frame = Box::new(FrameBuffer::new());
            group.bench_function(label, |b| {
                b.iter(|| draw(black_box(&mut frame), black_box(&rect), black_box(solid)))
            });
        }
        group.finish();
    }
}
