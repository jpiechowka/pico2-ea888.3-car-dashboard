use dashboard_config::startup_asset::Asset;
use dashboard_startup_tools::*;
fn png_image(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    bytes
}
fn gif_image() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, 2, 1, &[255, 0, 0, 0, 255, 0]).unwrap();
        for color in [0, 1] {
            let f = gif::Frame {
                width: 2,
                height: 1,
                delay: 10,
                buffer: std::borrow::Cow::Owned(vec![color; 2]),
                ..Default::default()
            };
            encoder.write_frame(&f).unwrap();
        }
    }
    bytes
}
#[test]
fn png_alpha_matte_and_determinism() {
    let source = png_image(1, 1, &[255, 0, 0, 128]);
    let options = Options {
        fit: Fit::Stretch,
        matte_rgb: [0, 0, 255],
        ..Default::default()
    };
    let converted = convert(&source, &options).unwrap();
    assert_eq!(converted.asset, convert(&source, &options).unwrap().asset);
    let asset = Asset::parse(&converted.asset).unwrap();
    assert_eq!(asset.frame_count(), 1);
    let mut row = [0; 640];
    asset.frame(0).unwrap().rows().next(&mut row).unwrap();
    assert_eq!(&row[..2], &[0x80, 0x0F]);
}
#[test]
fn gif_resampling_and_loop_seeks() {
    let converted = convert(
        &gif_image(),
        &Options {
            kind: SourceKind::Gif,
            fit: Fit::Stretch,
            ..Default::default()
        },
    )
    .unwrap();
    let asset = Asset::parse(&converted.asset).unwrap();
    assert_eq!(asset.cycle_ms(), 200);
    assert_eq!(asset.frame_count(), 2);
    assert_eq!(
        asset.frame_at(0, true, false).payload(),
        asset.frame_at(200, true, false).payload()
    );
    assert_ne!(
        asset.frame_at(99, true, false).payload(),
        asset.frame_at(100, true, false).payload()
    );
    assert_eq!(
        asset.frame_at(199, false, false).payload(),
        asset.frame_at(u64::MAX, false, false).payload()
    );
}
#[test]
fn sheet_grid_and_clip() {
    let source = png_image(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
    let options = Options {
        kind: SourceKind::SpriteSheet,
        fit: Fit::Stretch,
        sheet: Some(Sheet {
            columns: 2,
            rows: 1,
            frame_ms: 100,
        }),
        ..Default::default()
    };
    let converted = convert(&source, &options).unwrap();
    assert_eq!(converted.source_frames, 2);
    assert_eq!(Asset::parse(&converted.asset).unwrap().frame_count(), 2);
    let bad = Options {
        sheet: Some(Sheet {
            columns: 3,
            rows: 1,
            frame_ms: 100,
        }),
        ..options
    };
    assert!(convert(&source, &bad).is_err());
}
#[test]
fn truncation_trailing_crc_and_limits() {
    for (source, kind) in [
        (png_image(1, 1, &[0, 0, 0, 255]), SourceKind::Png),
        (gif_image(), SourceKind::Gif),
    ] {
        let options = Options {
            kind,
            ..Default::default()
        };
        for n in 0..source.len() {
            assert!(convert(&source[..n], &options).is_err(), "{kind:?} {n}");
        }
        let mut trailing = source;
        trailing.push(0);
        assert!(convert(&trailing, &options).is_err());
    }
    let source = png_image(1, 1, &[0, 0, 0, 255]);
    assert!(
        convert(
            &source,
            &Options {
                sample_fps: 21,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        convert(
            &source,
            &Options {
                trim_start_ms: 1000,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        convert(&vec![0; MAX_SOURCE_BYTES + 1], &Options::default()).err(),
        Some(ConvertError::SourceLimit)
    );
}

#[test]
fn gif_disposal_previous_background_and_reserved_controls() {
    let mut source = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut source, 2, 1, &[255, 0, 0, 0, 255, 0, 0, 0, 255]).unwrap();
        for (buffer, left, width, dispose) in [
            (vec![0, 0], 0, 2, gif::DisposalMethod::Keep),
            (vec![1], 0, 1, gif::DisposalMethod::Previous),
            (vec![2], 1, 1, gif::DisposalMethod::Background),
            (vec![1], 0, 1, gif::DisposalMethod::Keep),
        ] {
            encoder
                .write_frame(&gif::Frame {
                    width,
                    height: 1,
                    left,
                    delay: 10,
                    dispose,
                    buffer: std::borrow::Cow::Owned(buffer),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    let converted = convert(
        &source,
        &Options {
            kind: SourceKind::Gif,
            fit: Fit::Stretch,
            ..Default::default()
        },
    )
    .unwrap();
    let asset = Asset::parse(&converted.asset).unwrap();
    let mut row = [0; 640];
    asset.frame_at(200, false, false).rows().next(&mut row).unwrap();
    assert_eq!(&row[..2], &[0xF8, 0]);
    assert_eq!(&row[638..], &[0, 0x1F]);
    asset.frame_at(300, false, false).rows().next(&mut row).unwrap();
    assert_eq!(&row[..2], &[0x07, 0xE0]);
    assert_eq!(&row[638..], &[0xF8, 0]);
    let gce = source.windows(3).position(|b| b == [0x21, 0xF9, 4]).unwrap();
    for flags in [0x10, 0x02] {
        let mut malformed = source.clone();
        malformed[gce + 3] = flags;
        assert_eq!(
            convert(
                &malformed,
                &Options {
                    kind: SourceKind::Gif,
                    ..Default::default()
                }
            )
            .err(),
            Some(ConvertError::Unsupported)
        );
    }
}

#[test]
fn incompressible_output_does_not_expand_asset_budget() {
    let mut pixels = vec![0; 640 * 480 * 4];
    let mut seed = 1234567_u32;
    for pixel in pixels.as_chunks_mut::<4>().0.iter_mut() {
        for c in &mut pixel[..3] {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *c = (seed >> 24) as u8;
        }
        pixel[3] = 255;
    }
    let source = png_image(640, 480, &pixels);
    let result = convert(
        &source,
        &Options {
            kind: SourceKind::SpriteSheet,
            fit: Fit::Stretch,
            sheet: Some(Sheet {
                columns: 2,
                rows: 2,
                frame_ms: 100,
            }),
            ..Default::default()
        },
    );
    assert_eq!(result.err(), Some(ConvertError::AssetLimit));
}

fn decoded_pixels(
    asset: &Asset<'_>,
    at: u64,
) -> Vec<u16> {
    let mut rows = asset.frame_at(at, false, false).rows();
    let mut row = [0; 640];
    let mut pixels = Vec::new();
    while rows.next(&mut row).unwrap() {
        pixels.extend(row.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p)));
    }
    assert_eq!(pixels.len(), 320 * 240);
    pixels
}

#[test]
fn fit_modes_center_letterboxing_crop_and_preserve_every_pixel() {
    // Four square source stripes make all three fit policies distinguishable.
    let source = png_image(
        4,
        1,
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
    );
    for fit in [Fit::Contain, Fit::Cover, Fit::Stretch] {
        let converted = convert(
            &source,
            &Options {
                fit,
                matte_rgb: [255, 0, 255],
                ..Default::default()
            },
        )
        .unwrap();
        let asset = Asset::parse(&converted.asset).unwrap();
        let pixels = decoded_pixels(&asset, 0);
        for y in 0..240 {
            for x in 0..320 {
                let expected = match fit {
                    Fit::Contain if !(80..160).contains(&y) => 0xF81F,
                    Fit::Cover => {
                        if x < 160 {
                            0x07E0
                        } else {
                            0x001F
                        }
                    }
                    _ => [0xF800, 0x07E0, 0x001F, 0xFFFF][x / 80],
                };
                assert_eq!(pixels[y * 320 + x], expected, "{fit:?} ({x},{y})");
            }
        }
    }
}

#[test]
fn sheet_trim_samples_row_major_cells_and_extends_short_clips_explicitly() {
    let source = png_image(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
    );
    let options = Options {
        kind: SourceKind::SpriteSheet,
        fit: Fit::Stretch,
        sheet: Some(Sheet {
            columns: 2,
            rows: 2,
            frame_ms: 100,
        }),
        trim_start_ms: 100,
        trim_end_ms: 300,
        ..Default::default()
    };
    let converted = convert(&source, &options).unwrap();
    assert_eq!(
        (
            converted.source_frames,
            converted.source_duration_ms,
            converted.trim_end_ms
        ),
        (4, 400, 300)
    );
    let asset = Asset::parse(&converted.asset).unwrap();
    assert_eq!((asset.frame_count(), asset.cycle_ms()), (2, 200));
    for (at, expected) in [(0, 0x07E0), (99, 0x07E0), (100, 0x001F), (u64::MAX, 0x001F)] {
        assert!(decoded_pixels(&asset, at).iter().all(|pixel| *pixel == expected));
    }
    let converted = convert(
        &source,
        &Options {
            trim_start_ms: 399,
            trim_end_ms: 10000,
            ..options
        },
    )
    .unwrap();
    assert_eq!(converted.trim_end_ms, 400);
    let asset = Asset::parse(&converted.asset).unwrap();
    assert_eq!((asset.frame_count(), asset.cycle_ms()), (1, 34));
    assert!(decoded_pixels(&asset, 0).iter().all(|pixel| *pixel == 0xFFFF));
}
