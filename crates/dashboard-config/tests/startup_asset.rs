use dashboard_config::startup_asset::*;
fn asset(rle: bool) -> Vec<u8> {
    let payload = if rle {
        let mut p = Vec::new();
        for _ in 0..240 {
            p.extend_from_slice(&0x813F_u16.to_le_bytes());
            p.extend_from_slice(&[0xF8, 0]);
        }
        p
    } else {
        vec![0; FRAME_BYTES]
    };
    let mut b = vec![0; 52];
    b[..8].copy_from_slice(MAGIC);
    b[8..10].copy_from_slice(&1_u16.to_le_bytes());
    b[10..12].copy_from_slice(&32_u16.to_le_bytes());
    b[16..18].copy_from_slice(&320_u16.to_le_bytes());
    b[18..20].copy_from_slice(&240_u16.to_le_bytes());
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
fn checksummed_structural_errors_are_rejected_before_pixels() {
    // Repair the envelope checksum so these probe format admission, not CRC rejection.
    let cases: &[(usize, &[u8], AssetError)] = &[
        (8, &2_u16.to_le_bytes(), AssetError::Header),
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
        assert!(output.chunks_exact(2).all(|pixel| pixel == [0xF8, 0]));
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
    for pixel in expected[2..638].chunks_exact_mut(2) {
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
