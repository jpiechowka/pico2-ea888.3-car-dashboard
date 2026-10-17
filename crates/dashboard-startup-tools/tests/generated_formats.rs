#[path = "../../../test-support/generated.rs"]
mod generated;
use dashboard_config::Geometry;
use dashboard_config::startup_asset::{Asset, Validator};
use dashboard_startup_tools::*;

fn source(kind: SourceKind) -> Vec<u8> {
    let mut bytes = Vec::new();
    if kind == SourceKind::Png {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0, 255])
            .unwrap();
    } else {
        let mut encoder = gif::Encoder::new(&mut bytes, 1, 1, &[255, 0, 0, 0, 255, 0]).unwrap();
        encoder
            .write_frame(&gif::Frame {
                width: 1,
                height: 1,
                delay: 10,
                buffer: std::borrow::Cow::Owned(vec![0]),
                ..Default::default()
            })
            .unwrap();
    }
    bytes
}
fn check_asset(bytes: &[u8]) {
    let direct = Asset::parse(bytes);
    let mut incremental = match Validator::new(bytes) {
        Ok(v) => v,
        Err(error) => {
            assert_eq!(direct.unwrap_err(), error);
            return;
        }
    };
    for _ in 0..4096 {
        match incremental.step() {
            Ok(false) => continue,
            Ok(true) => {
                let asset = incremental.finish().unwrap();
                assert_eq!(asset.bytes(), direct.unwrap().bytes());
                for index in 0..asset.frame_count() {
                    let mut rows = asset.frame(index).unwrap().rows();
                    let width = usize::from(asset.geometry().width()) * 2;
                    let mut guarded = vec![0xA5; width + 2];
                    for _ in 0..asset.geometry().height() {
                        assert!(rows.next(&mut guarded[1..width + 1]).unwrap());
                        assert_eq!((guarded[0], guarded[width + 1]), (0xA5, 0xA5));
                    }
                    assert!(!rows.next(&mut guarded[1..width + 1]).unwrap());
                }
                return;
            }
            Err(error) => {
                assert_eq!(direct.unwrap_err(), error);
                for _ in 0..3 {
                    assert_eq!(incremental.step(), Err(error));
                    assert_eq!(incremental.finish().unwrap_err(), error);
                }
                return;
            }
        }
    }
    panic!("asset validator exceeded bounded step count");
}

#[test]
fn seeded_source_and_resealed_asset_mutations_preserve_decode_contracts() {
    const SEED: u32 = 0x17EA_5A02;
    let mut rng = SEED;
    let count = std::env::var("DASHBOARD_FORMAT_CASES").map_or(128, |v| v.parse::<usize>().unwrap().clamp(1, 2048));
    for kind in [SourceKind::Png, SourceKind::Gif] {
        let original = source(kind);
        for geometry in Geometry::ALL {
            let options = Options {
                kind,
                geometry,
                fit: Fit::Stretch,
                trim_end_ms: 100,
                ..Default::default()
            };
            let converted = convert(&original, &options).unwrap();
            let asset = Asset::parse(&converted.asset).unwrap();
            let mut row = vec![0; usize::from(geometry.width()) * 2];
            asset.frame(0).unwrap().rows().next(&mut row).unwrap();
            assert!(row.as_chunks::<2>().0.iter().all(|pixel| *pixel == [0xF8, 0]));
            for case in 0..count {
                let _case = generated::Case::new("format", u64::from(SEED), case);
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let mut bytes = original.clone();
                let at = rng as usize % bytes.len();
                bytes[at] ^= 1 << (rng % 8);
                if let Ok(result) = convert(&bytes, &options) {
                    assert_eq!(
                        result.asset,
                        convert(&bytes, &options).unwrap().asset,
                        "seed={SEED:x} case={case}"
                    );
                    assert_eq!(Asset::parse(&result.asset).unwrap().geometry(), geometry);
                    check_asset(&result.asset);
                }
                let mut bytes = converted.asset.clone();
                let at = rng as usize % bytes.len();
                bytes[at] ^= 1 << (rng % 8);
                if case % 2 == 0 {
                    // Keep table offsets fixed; independently refresh frame/global CRC
                    // to exercise pixel packet limits past integrity admission.
                    let crc = dashboard_config::startup_asset::crc32(&bytes[52..]);
                    bytes[44..48].copy_from_slice(&crc.to_le_bytes());
                    let crc = dashboard_config::startup_asset::asset_crc32(&bytes).unwrap();
                    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
                }
                check_asset(&bytes);
            }
        }
    }
}
