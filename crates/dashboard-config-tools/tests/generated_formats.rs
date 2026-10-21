//! Seeded project mutation and maximum-capacity codec regressions; no hardware or source-image decoding.
#[path = "../../../test-support/generated.rs"]
mod generated;
use dashboard_config::startup::StartupProfile;
use dashboard_config::{default_config, validate};
use dashboard_config_tools::project::{ExportMode, Project};

#[test]
/// Mutate headers and resealed nested inputs; accepted values must be canonical and every prefix must fail.
///
/// `DASHBOARD_FORMAT_CASES` defaults to 512 and clamps to 1..=8192; a nonnumeric value fails the test.
fn seeded_project_descriptor_and_nested_profile_mutations() {
    /// Fixed xorshift seed recorded in failure diagnostics for exact replay.
    const SEED: u32 = 0x17EA_7002;
    let mut rng = SEED;
    let count = std::env::var("DASHBOARD_FORMAT_CASES").map_or(512, |v| v.parse::<usize>().unwrap().clamp(1, 8192));
    let original = Project::new(&validate(default_config()).unwrap(), StartupProfile::DEFAULT, None)
        .unwrap()
        .encode(ExportMode::Complete)
        .unwrap();
    let legacy = include_bytes!("fixtures/v1-landscape.eavp");
    let migrated = Project::decode(legacy).unwrap().encode(ExportMode::Complete).unwrap();
    assert_eq!(
        Project::decode(&migrated)
            .unwrap()
            .encode(ExportMode::Complete)
            .unwrap(),
        migrated
    );
    let mut rejected = 0;
    for case in 0..count {
        let _case = generated::Case::new("format", u64::from(SEED), case);
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let mut bytes = original.clone();
        let at = rng as usize % bytes.len();
        bytes[at] ^= 1 << (rng % 8);
        // Every third input repairs outer integrity, reaching nested admission.
        if case % 3 == 0 {
            let crc = dashboard_config::crc32(&bytes[..28]);
            bytes[28..32].copy_from_slice(&crc.to_le_bytes());
            for descriptor in [32, 80] {
                let start = u32::from_le_bytes(original[descriptor + 4..descriptor + 8].try_into().unwrap()) as usize;
                let len = u32::from_le_bytes(original[descriptor + 8..descriptor + 12].try_into().unwrap()) as usize;
                let part = &bytes[start..start + len];
                let crc = dashboard_config::crc32(part);
                let digest = dashboard_config_tools::project::digest(part);
                bytes[descriptor + 12..descriptor + 16].copy_from_slice(&crc.to_le_bytes());
                bytes[descriptor + 16..descriptor + 48].copy_from_slice(&digest);
            }
        }
        match Project::decode(&bytes) {
            Ok(project) => {
                assert_eq!(
                    project.encode(ExportMode::Complete).unwrap(),
                    bytes,
                    "seed={SEED:x} case={case}"
                );
                assert!(project.profile().valid());
                let dashboard = dashboard_config::decode(project.dashboard_bytes()).unwrap();
                validate(dashboard.config().clone()).unwrap();
            }
            Err(_) => rejected += 1,
        }
    }
    assert!(rejected > count / 2);
    // Contiguous section rules must reject every truncated prefix even when
    // the prefix still contains a complete dashboard section.
    for len in 0..original.len() {
        assert!(Project::decode(&original[..len]).is_err(), "prefix={len}");
    }
}

#[test]
/// All mountings admit eight views/64 widgets within the reduced limit and round-trip byte-for-byte.
fn full_capacity_projects_roundtrip_in_every_orientation() {
    use dashboard_config::*;
    for orientation in Orientation::ALL {
        let mut config = default_config();
        config.orientation = orientation;
        config.views.clear();
        config.widgets.clear();
        for view in 1..=8 {
            config
                .views
                .push(View {
                    id: view,
                    name: "Capacity".try_into().unwrap(),
                    orientation: None,
                })
                .unwrap();
            for cell in 0..8 {
                let columns = orientation.geometry().columns();
                config
                    .widgets
                    .push(default_widget(
                        Channel::ALL[cell as usize],
                        u16::from(view) * 8 + u16::from(cell),
                        view,
                        Placement {
                            column: cell % columns,
                            row: cell / columns,
                            span: Span::OneByOne,
                        },
                    ))
                    .unwrap();
            }
        }
        assert_eq!((config.views.len(), config.widgets.len()), (8, 64));
        let valid = validate(config).unwrap();
        let project = Project::new(&valid, StartupProfile::DEFAULT, None).unwrap();
        let bytes = project.encode(ExportMode::Complete).unwrap();
        assert!(bytes.len() <= dashboard_config_tools::project::MAX_REDUCED_BYTES);
        let restored = Project::decode(&bytes).unwrap();
        assert_eq!(decode(restored.dashboard_bytes()).unwrap(), valid);
        assert_eq!(restored.encode(ExportMode::Complete).unwrap(), bytes);
    }
}
