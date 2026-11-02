//! Independent GIF89a LZW fixtures test the locked decoder through public conversion.
use dashboard_startup_tools::{ConvertError, Options, SourceKind, convert};

/// Return a one-pixel GIF with supplied LZW `codes` in one bounded data sub-block.
/// The fixed header declares a two-color palette and minimum code size2; codes
/// are packed least-significant-bit first per GIF89a Appendix F. Panics on an
/// oversized test block instead of silently truncating its encoded length.
fn image(codes: &[u8]) -> Vec<u8> {
    let mut source =
        b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02"
            .to_vec();
    source.push(u8::try_from(codes.len()).unwrap());
    source.extend_from_slice(codes);
    source.extend_from_slice(&[0, 0x3B]);
    source
}

#[test]
/// Structural preflight alone cannot reject these intact containers: conversion
/// must require the LZW end code and exactly the declared number of pixels.
fn missing_end_code_and_wrong_pixel_count_are_rejected() {
    let options = Options {
        kind: SourceKind::Gif,
        ..Options::default()
    };
    // Three-bit codes [clear4, pixel0, end5] yield one black pixel.
    let valid = convert(&image(&[0x44, 0x01]), &options).unwrap();
    let asset = dashboard_config::startup_asset::Asset::parse(&valid.asset).unwrap();
    let mut row = [0xFF; 640];
    assert!(asset.frame(0).unwrap().rows().next(&mut row).unwrap());
    assert!(row.iter().all(|byte| *byte == 0));
    for codes in [
        &[0x04][..],       // clear4,pixel0; missing end5.
        &[0x2C][..],       // clear4,end5; no pixel.
        &[0x44, 0x0A][..], // clear4,pixel0,pixel1,end5; one excess pixel.
    ] {
        assert_eq!(convert(&image(codes), &options).err(), Some(ConvertError::Decode));
    }
}
