use dashboard_config::startup::{StartupProfile, StartupTheme};
use dashboard_config::{default_config, validate};
use dashboard_config_tools::project::*;
#[test]
fn frozen_legacy_project_migrates_to_current_dashboard_and_profile() {
    let project = Project::decode(include_bytes!("fixtures/v1-landscape.eavp")).unwrap();
    let dashboard = dashboard_config::decode(project.dashboard_bytes()).unwrap();
    assert_eq!(dashboard.config().orientation, dashboard_config::Orientation::Landscape);
    assert!(dashboard.config().views.iter().all(|view| view.orientation.is_none()));
    assert!(project.profile().status_orientation.is_none());
    assert!(project.profile().splash_orientation.is_none());
    let bytes = project.encode(ExportMode::Complete).unwrap();
    assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 2);
    assert_eq!(
        Project::decode(&bytes).unwrap().encode(ExportMode::Complete).unwrap(),
        bytes
    );
}
#[test]
fn complete_project_round_trip_and_header_bounds() {
    let config = validate(default_config()).unwrap();
    let profile = StartupProfile {
        theme: StartupTheme::Turbo,
        duration_ms: 7134,
        ..StartupProfile::default()
    };
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
    bad[8] = 3;
    assert!(Project::decode(&bad).is_err());
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(Project::decode(&bad).is_err());
}

use std::sync::Arc;
fn artwork() -> Arc<Artwork> { artwork_in(dashboard_config::Geometry::Landscape) }
fn artwork_in(geometry: dashboard_config::Geometry) -> Arc<Artwork> {
    use dashboard_config::startup_asset::{MAGIC, asset_crc32, crc32};
    let mut asset = vec![0; 52];
    asset[..8].copy_from_slice(MAGIC);
    asset[8..10].copy_from_slice(&dashboard_config::startup_asset::VERSION.to_le_bytes());
    asset[10..12].copy_from_slice(&32u16.to_le_bytes());
    asset[16..18].copy_from_slice(&geometry.width().to_le_bytes());
    asset[18..20].copy_from_slice(&geometry.height().to_le_bytes());
    asset[20..22].copy_from_slice(&1u16.to_le_bytes());
    asset[24..28].copy_from_slice(&100u32.to_le_bytes());
    asset[32..36].copy_from_slice(&100u32.to_le_bytes());
    asset[36..40].copy_from_slice(&52u32.to_le_bytes());
    let mut payload = Vec::new();
    for _ in 0..geometry.height() {
        payload.extend_from_slice(&(0x8000 | (geometry.width() - 1)).to_le_bytes());
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
        version: RECIPE_VERSION,
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
        geometry,
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

#[test]
fn splash_geometry_mismatch_cannot_be_admitted_as_a_project() {
    let config = validate(default_config()).unwrap();
    let profile = StartupProfile {
        theme: StartupTheme::Custom,
        splash_orientation: Some(dashboard_config::Orientation::Portrait),
        ..StartupProfile::default()
    };
    assert!(matches!(
        Project::new(&config, profile, Some(artwork())),
        Err(ProjectError::Orientation)
    ));
}

#[test]
fn all_mountings_resolve_inheritance_without_reinterpreting_artwork() {
    use dashboard_config::{Geometry, Orientation};
    for default in Orientation::ALL {
        let mut config = default_config();
        config.orientation = default;
        // Keep the default eight single-cell widgets in the new grid.
        for (index, widget) in config.widgets.iter_mut().enumerate() {
            widget.placement.column = index as u8 % default.geometry().columns();
            widget.placement.row = index as u8 / default.geometry().columns();
        }
        let config = validate(config).unwrap();
        for override_ in [None].into_iter().chain(Orientation::ALL.map(Some)) {
            let geometry = override_.unwrap_or(default).geometry();
            let profile = StartupProfile {
                theme: StartupTheme::Custom,
                splash_orientation: override_,
                status_orientation: Some(Orientation::PortraitInverted),
                ..Default::default()
            };
            let art = artwork_in(geometry);
            let project = Project::new(&config, profile, Some(art.clone())).unwrap();
            let bytes = project.encode(ExportMode::Complete).unwrap();
            let restored = Project::decode(&bytes).unwrap();
            assert_eq!(restored.profile(), profile);
            assert_eq!(restored.artwork().unwrap().converted(), art.converted());
            assert_eq!(restored.encode(ExportMode::Complete).unwrap(), bytes);
            let other = if geometry == Geometry::Portrait {
                Geometry::Landscape
            } else {
                Geometry::Portrait
            };
            assert!(matches!(
                project.with_artwork(artwork_in(other)),
                Err(ProjectError::Orientation)
            ));
        }
    }
}

fn sections(bytes: &[u8]) -> [Vec<u8>; 5] {
    core::array::from_fn(|index| {
        let at = 32 + index * 48;
        let offset = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let length = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        bytes[offset..offset + length].to_vec()
    })
}

/// Independently reseal containers so semantic failures survive valid checksums.
fn container(
    version: u16,
    parts: &[Vec<u8>; 5],
) -> Vec<u8> {
    let mut bytes = vec![0u8; 272];
    bytes[..8].copy_from_slice(b"EAVPROJ\0");
    bytes[8..10].copy_from_slice(&version.to_le_bytes());
    bytes[10..12].copy_from_slice(&5u16.to_le_bytes());
    bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
    for (index, part) in parts.iter().enumerate() {
        let at = 32 + index * 48;
        bytes[at..at + 2].copy_from_slice(&(index as u16 + 1).to_le_bytes());
        if part.is_empty() {
            continue;
        }
        let offset = bytes.len() as u32;
        bytes[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[at + 8..at + 12].copy_from_slice(&(part.len() as u32).to_le_bytes());
        bytes[at + 12..at + 16].copy_from_slice(&dashboard_config::crc32(part).to_le_bytes());
        bytes[at + 16..at + 48].copy_from_slice(&digest(part));
        bytes.extend_from_slice(part);
    }
    let length = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&length.to_le_bytes());
    let crc = dashboard_config::crc32(&bytes[..28]);
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn legacy_artwork_migration_preserves_pixels_original_and_producer() {
    let mut parts = sections(include_bytes!("fixtures/v1-landscape.eavp"));
    let art = artwork();
    let profile = StartupProfile {
        theme: StartupTheme::Custom,
        ..Default::default()
    };
    parts[1] = profile.encode().unwrap().to_vec();
    parts[1][4] = 1;
    parts[2] = art.converted().to_vec();
    parts[2][8..10].copy_from_slice(&1u16.to_le_bytes());
    let crc = dashboard_config::startup_asset::asset_crc32(&parts[2]).unwrap();
    parts[2][28..32].copy_from_slice(&crc.to_le_bytes());
    // Synthetic historical artwork section: exactly the old field order/shape.
    let mut recipe = art.recipe().clone();
    recipe.version = 1;
    recipe.converter = "dashboard-startup-tools/1".into();
    recipe.asset_sha256 = digest(&parts[2]);
    let json = serde_json::to_string(&recipe).unwrap();
    parts[3] = format!("{}}}", json.strip_suffix(",\"geometry\":\"Landscape\"}").unwrap()).into_bytes();
    parts[4] = art.original().unwrap().to_vec();
    let restored = Project::decode(&container(1, &parts)).unwrap();
    let migrated = restored.artwork().unwrap();
    assert_eq!(migrated.recipe().version, RECIPE_VERSION);
    assert_eq!(migrated.recipe().converter, "dashboard-startup-tools/1");
    assert_eq!(migrated.geometry(), dashboard_config::Geometry::Landscape);
    assert_eq!(migrated.converted(), parts[2]);
    assert_eq!(migrated.original().unwrap().as_ref(), parts[4]);
    let current = restored.encode(ExportMode::Complete).unwrap();
    assert_eq!(
        Project::decode(&current).unwrap().encode(ExportMode::Complete).unwrap(),
        current
    );
    // A current profile in a resealed old container is not a historical file.
    parts[1][4] = 2;
    assert_eq!(
        Project::decode(&container(1, &parts)).unwrap_err(),
        ProjectError::Profile
    );
}

#[test]
fn resealed_mixed_versions_mismatch_and_noncanonical_recipes_are_rejected() {
    let project = Project::new(
        &validate(default_config()).unwrap(),
        StartupProfile {
            theme: StartupTheme::Custom,
            ..Default::default()
        },
        Some(artwork()),
    )
    .unwrap();
    let original = sections(&project.encode(ExportMode::Complete).unwrap());
    let mut parts = original.clone();
    parts[0] = sections(include_bytes!("fixtures/v1-landscape.eavp"))[0].clone();
    assert_eq!(
        Project::decode(&container(PROJECT_VERSION, &parts)).unwrap_err(),
        ProjectError::Dashboard
    );
    assert_eq!(
        Project::decode(&container(1, &original)).unwrap_err(),
        ProjectError::Dashboard
    );
    let mut parts = original.clone();
    parts[1][4] = 1;
    assert_eq!(
        Project::decode(&container(PROJECT_VERSION, &parts)).unwrap_err(),
        ProjectError::Profile
    );
    let mut parts = original.clone();
    parts[1][13] = 2; // Valid Portrait override, wrong for the retained landscape pixels.
    assert_eq!(
        Project::decode(&container(PROJECT_VERSION, &parts)).unwrap_err(),
        ProjectError::Orientation
    );
    for changed in [
        String::from_utf8(original[3].clone())
            .unwrap()
            .replace("\"geometry\":\"Landscape\"", "\"geometry\":\"Portrait\""),
        String::from_utf8(original[3].clone())
            .unwrap()
            .replace(",\"geometry\":\"Landscape\"", ""),
        String::from_utf8(original[3].clone()).unwrap().replace(
            "\"geometry\":\"Landscape\"",
            "\"geometry\":\"Landscape\",\"geometry\":\"Landscape\"",
        ),
        format!(" {}", String::from_utf8(original[3].clone()).unwrap()),
    ] {
        let mut parts = original.clone();
        parts[3] = changed.into_bytes();
        assert_eq!(
            Project::decode(&container(PROJECT_VERSION, &parts)).unwrap_err(),
            ProjectError::Recipe
        );
    }
}

mod firmware_build {
    include!("../../../firmware/pico2/build.rs");
    pub fn run() { main(); }
}

#[test]
fn firmware_build_child() {
    if std::env::var_os("EA888_TEST_STARTUP_BUILD").is_some() {
        firmware_build::run();
    }
}

#[test]
fn direct_cargo_build_uses_prepared_default_and_explicit_splash_override() {
    use dashboard_config::{Geometry, Orientation};
    let root = std::env::temp_dir().join(format!("ea888-startup-admission-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let mut case = 0;
    for default in Orientation::ALL {
        let mut config = default_config();
        config.orientation = default;
        for (index, widget) in config.widgets.iter_mut().enumerate() {
            widget.placement.column = index as u8 % default.geometry().columns();
            widget.placement.row = index as u8 / default.geometry().columns();
        }
        let config = dashboard_config_tools::compile(&validate(config).unwrap()).unwrap();
        for override_ in [None].into_iter().chain(Orientation::ALL.map(Some)) {
            let effective = override_.unwrap_or(default).geometry();
            let profile = StartupProfile {
                theme: StartupTheme::Custom,
                splash_orientation: override_,
                ..Default::default()
            };
            for geometry in Geometry::ALL {
                let dir = root.join(case.to_string());
                case += 1;
                std::fs::create_dir(&dir).unwrap();
                std::fs::write(dir.join("config.eav"), &config).unwrap();
                std::fs::write(dir.join("profile.easp"), profile.encode().unwrap()).unwrap();
                let art = artwork_in(geometry);
                std::fs::write(dir.join("asset.easa"), art.converted()).unwrap();
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                for (key, _) in std::env::vars_os() {
                    if key
                        .to_str()
                        .is_some_and(|name| name.starts_with("CARGO_FEATURE_") || name.starts_with("DASHBOARD_"))
                    {
                        command.env_remove(key);
                    }
                }
                let output = command
                    .args(["--exact", "firmware_build_child", "--nocapture"])
                    .env("EA888_TEST_STARTUP_BUILD", "1")
                    .env("CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
                    .env("OUT_DIR", &dir)
                    .env("OPT_LEVEL", "3")
                    .env("DASHBOARD_CONFIG", dir.join("config.eav"))
                    .env("DASHBOARD_STARTUP_PROFILE", dir.join("profile.easp"))
                    .env("DASHBOARD_STARTUP_ASSET", dir.join("asset.easa"))
                    .output()
                    .unwrap();
                let trace = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    output.status.success(),
                    geometry == effective,
                    "{default:?}/{override_:?}/{geometry:?}: {trace}"
                );
                if geometry == effective {
                    assert_eq!(std::fs::read(dir.join("startup_asset.bin")).unwrap(), art.converted());
                    assert_eq!(std::fs::read(dir.join("dashboard.eav")).unwrap(), config);
                } else {
                    assert!(trace.contains("Orientation"), "unexpected failure: {trace}");
                    assert!(!dir.join("startup_asset.bin").exists());
                    assert!(!dir.join("startup_profile.bin").exists());
                    assert!(!dir.join("dashboard.eav").exists());
                }
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_sheet_timing_is_distinct_from_converted_frame_minimum() {
    let art = artwork();
    let mut recipe = art.recipe().clone();
    recipe.source_kind = SourceKind::SpriteSheet;
    recipe.sheet = Some(Sheet {
        columns: 2,
        rows: 1,
        frame_ms: 1,
    });
    assert!(Artwork::new(Arc::from(art.converted()), recipe.clone(), art.original().cloned()).is_ok());
    recipe.sheet.as_mut().unwrap().frame_ms = 0;
    assert!(Artwork::new(Arc::from(art.converted()), recipe.clone(), art.original().cloned()).is_err());
    recipe.sheet.as_mut().unwrap().frame_ms = 60000;
    assert!(Artwork::new(Arc::from(art.converted()), recipe, art.original().cloned()).is_err());
}

#[test]
fn portable_recipe_cannot_exceed_converter_clip_duration() {
    let art = artwork();
    let mut recipe = art.recipe().clone();
    recipe.trim_start_ms = 20000;
    recipe.trim_end_ms = 30000;
    assert!(Artwork::new(Arc::from(art.converted()), recipe.clone(), art.original().cloned()).is_ok());
    recipe.trim_end_ms = 30001;
    assert!(matches!(
        Artwork::new(Arc::from(art.converted()), recipe, art.original().cloned()),
        Err(ProjectError::Recipe)
    ));
}

#[test]
fn immutable_original_handles_preserve_identity_without_retaining_source_bytes() {
    let art = artwork();
    let source = art.original().unwrap().clone();
    let reference = Arc::new(OriginalReference::new(digest(&source), source.len()).unwrap());
    let external = Arc::new(art.externalized(reference.clone()).unwrap());
    assert!(external.original().is_none());
    assert!(external.has_original());
    assert_eq!(external.identity(), art.identity());
    assert!(Arc::ptr_eq(external.original_reference().unwrap(), &reference));
    assert_eq!(reference.key().len(), 64);
    let config = validate(default_config()).unwrap();
    let profile = StartupProfile {
        theme: StartupTheme::Custom,
        ..StartupProfile::default()
    };
    let project = Project::new(&config, profile, Some(external.clone())).unwrap();
    assert!(matches!(
        project.encode(ExportMode::Complete),
        Err(ProjectError::SourceUnavailable)
    ));
    let reduced = project.encode(ExportMode::ConvertedOnly).unwrap();
    let restored = Project::decode(&reduced).unwrap();
    let reattached = restored.artwork().unwrap().externalized(reference).unwrap();
    assert_eq!(reattached.identity(), art.identity());
    assert!(external.materialized(Arc::from(&b"wrong"[..])).is_err());
    let mut same_length_wrong_source = source.to_vec();
    *same_length_wrong_source.last_mut().unwrap() ^= 1;
    assert!(matches!(
        external.materialized(same_length_wrong_source.into()),
        Err(ProjectError::Source)
    ));
    assert_eq!(
        external.identity(),
        art.identity(),
        "failed materialization cannot mutate backing identity"
    );
    let full = project
        .with_artwork(Arc::new(external.materialized(source).unwrap()))
        .unwrap();
    assert_eq!(
        Project::decode(&full.encode(ExportMode::Complete).unwrap())
            .unwrap()
            .artwork()
            .unwrap()
            .identity(),
        art.identity()
    );
    assert!(OriginalReference::new([0; 32], 0).is_err());
    assert!(OriginalReference::new([0; 32], MAX_ORIGINAL_BYTES + 1).is_err());
    assert!(
        art.externalized(Arc::new(OriginalReference::new([0; 32], 1).unwrap()))
            .is_err()
    );
}

#[test]
fn eavp2_v5_dashboard_migrates_while_retaining_artwork() {
    let project = Project::new(
        &validate(default_config()).unwrap(),
        StartupProfile {
            theme: StartupTheme::Custom,
            ..Default::default()
        },
        Some(artwork()),
    )
    .unwrap();
    let mut parts = sections(&project.encode(ExportMode::Complete).unwrap());
    parts[0] = include_bytes!("../../dashboard-config/tests/fixtures/v5-default.eav").to_vec();
    let migrated = Project::decode(&container(2, &parts)).unwrap();
    assert_eq!(
        dashboard_config::decode(migrated.dashboard_bytes())
            .unwrap()
            .config()
            .ble,
        dashboard_config::BleSettings::DEFAULT
    );
    assert_eq!(migrated.artwork().unwrap().converted(), parts[2]);
    assert_eq!(migrated.artwork().unwrap().original().unwrap().as_ref(), parts[4]);
    assert_eq!(migrated.profile(), project.profile());
    let current = migrated.encode(ExportMode::Complete).unwrap();
    assert_eq!(
        Project::decode(&current).unwrap().encode(ExportMode::Complete).unwrap(),
        current
    );
    // Canonical historical decoder must still reject trailing bytes, even resealed.
    parts[0].push(0);
    assert!(Project::decode(&container(2, &parts)).is_err());
}
