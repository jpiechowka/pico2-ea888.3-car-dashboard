use dashboard_config::startup_asset::*;
fn asset(rle: bool) -> Vec<u8> { asset_in(rle, 320, 240, 1) }

fn asset_in(
    rle: bool,
    width: u16,
    height: u16,
    version: u16,
) -> Vec<u8> {
    let payload = if rle {
        let mut p = Vec::new();
        for _ in 0..height {
            p.extend_from_slice(&(0x8000 | (width - 1)).to_le_bytes());
            p.extend_from_slice(&[0xF8, 0]);
        }
        p
    } else {
        vec![0; FRAME_BYTES]
    };
    let mut b = vec![0; 52];
    b[..8].copy_from_slice(MAGIC);
    b[8..10].copy_from_slice(&version.to_le_bytes());
    b[10..12].copy_from_slice(&32_u16.to_le_bytes());
    b[16..18].copy_from_slice(&width.to_le_bytes());
    b[18..20].copy_from_slice(&height.to_le_bytes());
    b[20..22].copy_from_slice(&1_u16.to_le_bytes());
    b[24..28].copy_from_slice(&100_u32.to_le_bytes());
    b[32..36].copy_from_slice(&100_u32.to_le_bytes());
    b[36..40].copy_from_slice(&52_u32.to_le_bytes());
    b[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    b[44..48].copy_from_slice(&crc32(&payload).to_le_bytes());
    b[48] = u8::from(rle);
    b.extend(payload);
    let size = b.len() as u32;
    b[12..16].copy_from_slice(&size.to_le_bytes());
    let crc = asset_crc32(&b).unwrap();
    b[28..32].copy_from_slice(&crc.to_le_bytes());
    b
}
#[test]
fn raw_rle_and_incremental_admission_agree() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
    for rle in [false, true] {
        let bytes = asset(rle);
        let a = Asset::parse(&bytes).unwrap();
        assert_eq!(a.frame_count(), 1);
        let mut validator = Validator::new(&bytes).unwrap();
        let mut calls = 0;
        while !validator.step().unwrap() {
            calls += 1;
            assert!(calls < 1000);
        }
        assert!(calls > 1);
        assert_eq!(validator.finish().unwrap().cycle_ms(), 100);
        let mut rows = a.frame_at(u64::MAX, true, false).rows();
        let mut row = [0; 640];
        for _ in 0..240 {
            assert!(rows.next(&mut row).unwrap());
            if rle {
                assert_eq!(&row[..2], &[0xF8, 0]);
            }
        }
        assert!(!rows.next(&mut row).unwrap());
    }
}
#[test]
fn corrupt_and_truncated_assets_never_admit() {
    let bytes = asset(true);
    for n in 0..bytes.len() {
        assert!(Asset::parse(&bytes[..n]).is_err());
    }
    for index in [0, 8, 12, 16, 20, 22, 24, 28, 32, 36, 40, 44, 48, 49, 52, 54] {
        let mut bad = bytes.clone();
        bad[index] ^= 1;
        assert!(Asset::parse(&bad).is_err(), "byte {index}");
    }
    let mut bad = bytes;
    bad[52..54].copy_from_slice(&0x8140_u16.to_le_bytes());
    let crc = crc32(&bad[52..]);
    bad[44..48].copy_from_slice(&crc.to_le_bytes());
    let crc = asset_crc32(&bad).unwrap();
    bad[28..32].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(Asset::parse(&bad).unwrap_err(), AssetError::Pixels);
}

fn refresh_global_crc(bytes: &mut [u8]) {
    let crc = asset_crc32(bytes).unwrap();
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn portrait_asset_decodes_all_320_rows_with_240_pixel_stride() {
    use dashboard_config::Geometry;
    for rle in [false, true] {
        for geometry in Geometry::ALL {
            let width = geometry.width();
            let height = geometry.height();
            let mut bytes = asset_in(rle, width, height, 2);
            if !rle {
                for (index, pixel) in bytes[52..].as_chunks_mut::<2>().0.iter_mut().enumerate() {
                    let row = index / usize::from(width);
                    let column = index % usize::from(width);
                    pixel.copy_from_slice(&((row * 41 + column * 13) as u16).to_be_bytes());
                }
                let crc = crc32(&bytes[52..]);
                bytes[44..48].copy_from_slice(&crc.to_le_bytes());
                refresh_global_crc(&mut bytes);
            }
            let admitted = Asset::parse(&bytes).expect("v2 asset");
            assert_eq!(admitted.geometry(), geometry);
            let frame = admitted.frame(0).unwrap();
            assert_eq!(frame.geometry(), geometry);
            let mut rows = frame.rows();
            let mut output = vec![0; usize::from(width) * 2];
            for row in 0..height {
                assert!(rows.next(&mut output).unwrap());
                for (column, pixel) in output.as_chunks::<2>().0.iter().enumerate() {
                    let expected = if rle {
                        0xF800
                    } else {
                        (usize::from(row) * 41 + column * 13) as u16
                    };
                    assert_eq!(u16::from_be_bytes(*pixel), expected);
                }
            }
            assert!(!rows.next(&mut output).unwrap());
        }
    }
}

#[test]
fn checksum_failure_cannot_be_resumed_into_success() {
    let mut bytes = asset(true);
    bytes[28] ^= 1;
    let mut validator = Validator::new(&bytes).unwrap();
    loop {
        match validator.step() {
            Err(AssetError::Crc) => break,
            Ok(false) => {}
            other => panic!("expected global checksum failure, got {other:?}"),
        }
    }
    for _ in 0..400 {
        assert_eq!(validator.step(), Err(AssetError::Crc));
        assert_eq!(validator.finish().unwrap_err(), AssetError::Crc);
    }
}

#[test]
fn portrait_dimensions_rows_and_failed_admission_remain_bounded() {
    for (width, height, version) in [
        (240_u16, 320_u16, 1_u16),
        (320, 320, 2),
        (240, 240, 2),
        (0, 320, 2),
        (u16::MAX, 240, 2),
    ] {
        let mut bytes = asset(false);
        bytes[8..10].copy_from_slice(&version.to_le_bytes());
        bytes[16..18].copy_from_slice(&width.to_le_bytes());
        bytes[18..20].copy_from_slice(&height.to_le_bytes());
        refresh_global_crc(&mut bytes);
        assert_eq!(Asset::parse(&bytes).unwrap_err(), AssetError::Header);
    }
    for mode in 0..3 {
        let mut bytes = asset_in(true, 240, 320, 2);
        match mode {
            0 => bytes[44] ^= 1,                                           // frame CRC, with a valid global CRC
            1 => bytes[52..54].copy_from_slice(&0x80F0_u16.to_le_bytes()), // 241 pixels in a 240-pixel row
            _ => bytes[52..54].copy_from_slice(&0xFFFF_u16.to_le_bytes()),
        }
        refresh_global_crc(&mut bytes);
        let mut validator = Validator::new(&bytes).unwrap();
        let failure = loop {
            match validator.step() {
                Ok(false) => {}
                Ok(true) => panic!("invalid asset admitted"),
                Err(error) => break error,
            }
        };
        for _ in 0..400 {
            assert_eq!(validator.step(), Err(failure));
            assert_eq!(validator.finish().unwrap_err(), failure);
        }
    }
}

#[test]
fn checksummed_structural_errors_are_rejected_before_pixels() {
    // Repair the envelope checksum so these probe format admission, not CRC rejection.
    let cases: &[(usize, &[u8], AssetError)] = &[
        (8, &3_u16.to_le_bytes(), AssetError::Header),
        (10, &31_u16.to_le_bytes(), AssetError::Header),
        (16, &319_u16.to_le_bytes(), AssetError::Header),
        (18, &241_u16.to_le_bytes(), AssetError::Header),
        (22, &1_u16.to_le_bytes(), AssetError::Header),
        (20, &0_u16.to_le_bytes(), AssetError::Limit),
        (20, &301_u16.to_le_bytes(), AssetError::Limit),
        (24, &33_u32.to_le_bytes(), AssetError::Limit),
        (24, &10001_u32.to_le_bytes(), AssetError::Limit),
        (32, &33_u32.to_le_bytes(), AssetError::Table),
        (32, &99_u32.to_le_bytes(), AssetError::Table),
        (32, &101_u32.to_le_bytes(), AssetError::Table),
        (36, &51_u32.to_le_bytes(), AssetError::Table),
        (36, &53_u32.to_le_bytes(), AssetError::Table),
        (40, &0_u32.to_le_bytes(), AssetError::Table),
        (40, &u32::MAX.to_le_bytes(), AssetError::Table),
        (48, &[2], AssetError::Table),
        (49, &[1], AssetError::Table),
        (50, &[1], AssetError::Table),
        (51, &[1], AssetError::Table),
        (44, &0_u32.to_le_bytes(), AssetError::Crc),
    ];
    for &(offset, replacement, expected) in cases {
        let mut bytes = asset(true);
        bytes[offset..offset + replacement.len()].copy_from_slice(replacement);
        refresh_global_crc(&mut bytes);
        assert_eq!(Asset::parse(&bytes).unwrap_err(), expected, "offset {offset}");
    }
}

#[test]
fn admission_must_finish_and_row_output_must_have_exact_capacity() {
    let bytes = asset(true);
    let mut validator = Validator::new(&bytes).unwrap();
    assert_eq!(validator.finish().unwrap_err(), AssetError::Pixels);
    while !validator.step().unwrap() {
        assert_eq!(validator.finish().unwrap_err(), AssetError::Pixels);
    }
    assert!(validator.step().unwrap());
    let admitted = validator.finish().unwrap();
    assert_eq!(admitted.bytes(), bytes);
    assert!(admitted.frame(1).is_none());
    assert!(admitted.frame(usize::MAX).is_none());
    let mut rows = admitted.frame(0).unwrap().rows();
    for size in [0, WIDTH * 2 - 1, WIDTH * 2 + 1] {
        let mut output = vec![0xA5; size];
        assert_eq!(rows.next(&mut output), Err(AssetError::Pixels));
        assert_eq!(rows.row(), 0);
        assert!(output.iter().all(|byte| *byte == 0xA5));
    }
    let mut output = [0xA5; WIDTH * 2];
    for expected_row in 1..=HEIGHT {
        assert!(rows.next(&mut output).unwrap());
        assert_eq!(rows.row(), expected_row);
        assert!(output.as_chunks::<2>().0.iter().all(|pixel| *pixel == [0xF8, 0]));
    }
    output.fill(0xA5);
    assert!(!rows.next(&mut output).unwrap());
    assert!(output.iter().all(|byte| *byte == 0xA5));
}

#[test]
fn mixed_literal_and_repeat_packets_preserve_every_pixel() {
    let mut bytes = asset(true);
    bytes.truncate(52);
    let mut expected = [0; WIDTH * 2];
    // A one-pixel literal, a 318-pixel repeat, then another one-pixel literal.
    expected[..2].copy_from_slice(&[0x07, 0xE0]);
    for pixel in expected[2..638].as_chunks_mut::<2>().0 {
        pixel.copy_from_slice(&[0x12, 0x34]);
    }
    expected[638..].copy_from_slice(&[0, 0x1F]);
    for _ in 0..HEIGHT {
        bytes.extend_from_slice(&[0, 0, 0x07, 0xE0, 0x3D, 0x81, 0x12, 0x34, 0, 0, 0, 0x1F]);
    }
    let len = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&len.to_le_bytes());
    bytes[40..44].copy_from_slice(&(len - 52).to_le_bytes());
    let crc = crc32(&bytes[52..]);
    bytes[44..48].copy_from_slice(&crc.to_le_bytes());
    refresh_global_crc(&mut bytes);
    let admitted = Asset::parse(&bytes).unwrap();
    let mut rows = admitted.frame(0).unwrap().rows();
    let mut output = [0; WIDTH * 2];
    for _ in 0..HEIGHT {
        assert!(rows.next(&mut output).unwrap());
        assert_eq!(output, expected);
    }
    assert!(!rows.next(&mut output).unwrap());
}
