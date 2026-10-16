use dashboard_config::startup::{StartupProfile, StartupTheme};
use dashboard_config::{default_config, validate};
use dashboard_config_tools::project::*;
#[test]
fn complete_project_round_trip_and_header_bounds() {
    let config = validate(default_config()).unwrap();
    let mut profile = StartupProfile::default();
    profile.theme = StartupTheme::Turbo;
    profile.duration_ms = 7134;
    let project = Project::new(&config, profile, None).unwrap();
    let bytes = project.encode(ExportMode::Complete).unwrap();
    assert!(bytes.starts_with(b"EAVPROJ\0"));
    let restored = Project::decode(&bytes).unwrap();
    assert_eq!(restored.profile(), profile);
    assert_eq!(restored.dashboard_bytes(), project.dashboard_bytes());
    assert_eq!(restored.encode(ExportMode::Complete).unwrap(), bytes);
    for len in 0..272 {
        assert!(Project::decode(&bytes[..len]).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(Project::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[8] = 2;
    assert!(Project::decode(&bad).is_err());
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(Project::decode(&bad).is_err());
}

use std::sync::Arc;
fn artwork() -> Arc<Artwork> {
    use dashboard_config::startup_asset::{MAGIC, asset_crc32, crc32};
    let mut asset = vec![0; 52];
    asset[..8].copy_from_slice(MAGIC);
    asset[8..10].copy_from_slice(&1u16.to_le_bytes());
    asset[10..12].copy_from_slice(&32u16.to_le_bytes());
    asset[16..18].copy_from_slice(&320u16.to_le_bytes());
    asset[18..20].copy_from_slice(&240u16.to_le_bytes());
    asset[20..22].copy_from_slice(&1u16.to_le_bytes());
    asset[24..28].copy_from_slice(&100u32.to_le_bytes());
    asset[32..36].copy_from_slice(&100u32.to_le_bytes());
    asset[36..40].copy_from_slice(&52u32.to_le_bytes());
    let mut payload = Vec::new();
    for _ in 0..240 {
        payload.extend_from_slice(&0x813Fu16.to_le_bytes());
        payload.extend_from_slice(&[0xF8, 0]);
    }
    asset[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    asset[44..48].copy_from_slice(&crc32(&payload).to_le_bytes());
    asset[48] = 1;
    asset.extend(payload);
    let len = asset.len() as u32;
    asset[12..16].copy_from_slice(&len.to_le_bytes());
    let crc = asset_crc32(&asset).unwrap();
    asset[28..32].copy_from_slice(&crc.to_le_bytes());
    let source: Arc<[u8]> = Arc::from(&b"\x89PNG\r\n\x1a\nsynthetic-original"[..]);
    let recipe = Recipe {
        version: 1,
        converter: CONVERTER.into(),
        label: "Example".into(),
        source_kind: SourceKind::Png,
        source_sha256: digest(&source),
        source_length: source.len() as u32,
        original_included: true,
        asset_sha256: digest(&asset),
        fit: Fit::Contain,
        matte_rgb: [0; 3],
        sample_fps: 20,
        trim_start_ms: 0,
        trim_end_ms: 100,
        sheet: None,
        gif_background: GifBackground::Logical,
    };
    Arc::new(Artwork::new(asset.into(), recipe, Some(source)).unwrap())
}
#[test]
fn smaller_projection_preserves_exact_pixels_and_does_not_mutate_original() {
    let art = artwork();
    let config = validate(default_config()).unwrap();
    let profile = StartupProfile {
        theme: StartupTheme::Custom,
        ..StartupProfile::default()
    };
    let project = Project::new(&config, profile, Some(art.clone())).unwrap();
    let full = project.encode(ExportMode::Complete).unwrap();
    let reduced = project.encode(ExportMode::ConvertedOnly).unwrap();
    assert!(full.len() > reduced.len());
    assert!(art.original().is_some());
    let restored = Project::decode(&full).unwrap();
    assert_eq!(restored.artwork().unwrap().original(), art.original());
    let restored = Project::decode(&reduced).unwrap();
    let smaller = restored.artwork().unwrap();
    assert!(smaller.original().is_none());
    assert!(!smaller.recipe().original_included);
    assert_eq!(smaller.converted(), art.converted());
    assert_eq!(smaller.recipe().source_sha256, art.recipe().source_sha256);
    assert_eq!(restored.encode(ExportMode::Complete).unwrap(), reduced);
    assert_eq!(project.encode(ExportMode::Complete).unwrap(), full);
    assert!(Project::new(&config, profile, None).is_err());
    let mut recipe = art.recipe().clone();
    recipe.asset_sha256 = [0; 32];
    assert!(Artwork::new(Arc::from(art.converted()), recipe, art.original().cloned()).is_err());
    let mut recipe = art.recipe().clone();
    recipe.sample_fps = 21;
    assert!(Artwork::new(Arc::from(art.converted()), recipe, art.original().cloned()).is_err());
}
