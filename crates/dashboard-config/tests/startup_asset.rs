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
