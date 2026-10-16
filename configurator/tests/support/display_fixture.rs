//! Deterministic maximum-size valid EASA fixture; generated outside timed work.
use std::sync::Arc;

use dashboard_config::startup_asset::*;
use dashboard_config_tools::project as p;

pub fn artwork(
    label: &str,
    original_tag: u8,
) -> Arc<p::Artwork> {
    let mut payloads = vec![
        vec![0x12; FRAME_BYTES],
        vec![0x34; FRAME_BYTES],
        vec![0x56; FRAME_BYTES],
    ];
    let mut compressed = Vec::new();
    // 15,844 four-byte run packets; each packet stays within its scanline.
    for row in 0..HEIGHT {
        let packets = 66 + usize::from(row < 4);
        for packet in 0..packets {
            let count = if packet + 1 == packets { WIDTH - packets + 1 } else { 1 };
            compressed.extend_from_slice(&(0x8000 | (count as u16 - 1)).to_le_bytes());
            compressed.extend_from_slice(&(packet as u16).to_be_bytes());
        }
    }
    payloads.push(compressed);
    let mut bytes = vec![0; HEADER_BYTES + ENTRY_BYTES * 4];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..10].copy_from_slice(&dashboard_config::startup_asset::VERSION.to_le_bytes());
    bytes[10..12].copy_from_slice(&(HEADER_BYTES as u16).to_le_bytes());
    bytes[12..16].copy_from_slice(&(MAX_ASSET_BYTES as u32).to_le_bytes());
    bytes[16..18].copy_from_slice(&(WIDTH as u16).to_le_bytes());
    bytes[18..20].copy_from_slice(&(HEIGHT as u16).to_le_bytes());
    bytes[20..22].copy_from_slice(&4u16.to_le_bytes());
    bytes[24..28].copy_from_slice(&200u32.to_le_bytes());
    for (index, payload) in payloads.into_iter().enumerate() {
        let at = HEADER_BYTES + index * ENTRY_BYTES;
        let offset = bytes.len() as u32;
        bytes[at..at + 4].copy_from_slice(&((index as u32 + 1) * 50).to_le_bytes());
        bytes[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[at + 8..at + 12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes[at + 12..at + 16].copy_from_slice(&crc32(&payload).to_le_bytes());
        bytes[at + 16] = u8::from(index == 3);
        bytes.extend_from_slice(&payload);
    }
    assert_eq!(bytes.len(), MAX_ASSET_BYTES);
    let crc = asset_crc32(&bytes).unwrap();
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
    Asset::parse(&bytes).unwrap();
    let source: Arc<[u8]> = Arc::from([b"GIF89a".as_slice(), &[original_tag]].concat());
    let recipe = p::Recipe {
        geometry: dashboard_config::Geometry::Landscape,
        version: 2,
        converter: p::CONVERTER.into(),
        label: label.into(),
        source_kind: p::SourceKind::Gif,
        source_sha256: p::digest(&source),
        source_length: source.len() as u32,
        original_included: true,
        asset_sha256: p::digest(&bytes),
        fit: p::Fit::Contain,
        matte_rgb: [0; 3],
        sample_fps: 20,
        trim_start_ms: 0,
        trim_end_ms: 200,
        sheet: None,
        gif_background: p::GifBackground::Logical,
    };
    let art = p::Artwork::new(bytes.into(), recipe, Some(source.clone())).unwrap();
    Arc::new(
        art.externalized(Arc::new(
            p::OriginalReference::new(p::digest(&source), source.len()).unwrap(),
        ))
        .unwrap(),
    )
}
