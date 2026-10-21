//! Synthetic device/inspection identity and explicit genuine-board confirmation contracts.

use dashboard_configurator::helper_client::{Connection, DeviceSelection};
use dashboard_helper_protocol::*;
/// Returns a decoded synthetic protocol2 session/capability; invalid fixture encoding panics.
fn connection() -> Connection {
    Connection::decode(
        format!(
            r#"{{"protocol_version":2,"session_id":"{}","capability":"{}"}}"#,
            "a".repeat(32),
            "b".repeat(64)
        )
        .as_bytes(),
    )
    .unwrap()
}
/// Returns descriptor-only RP2350 BOOTSEL evidence with unknown capacity and no flash readiness.
fn device() -> DeviceSummary {
    DeviceSummary {
        device_id: DeviceId::new("c".repeat(32)).unwrap(),
        descriptor_sha256: Sha256::new("d".repeat(64)).unwrap(),
        vid: 0x2E8A,
        pid: 0x000F,
        serial: Some(Text::new("ABC123").unwrap()),
        product: None,
        status: DeviceStatus::DescriptorOnly,
        reason: Text::new("Detected").unwrap(),
        capacity: DeviceCapacity {
            state: DeviceCapacityState::Unverified,
            bytes: None,
            provenance: None,
        },
        flash_ready: false,
    }
}
/// Returns complete synthetic picotool facts tied to the fixture session/device; the capacity estimate remains
/// unverified.
fn inspection() -> DeviceInspection {
    let d = device();
    DeviceInspection {
        session_id: connection().session().clone(),
        device_id: d.device_id,
        descriptor_sha256: d.descriptor_sha256,
        complete: true,
        facts: Some(BootselFacts {
            revision: Text::new("A3").unwrap(),
            chip_id: Text::new("0x0123456789abcdef").unwrap(),
            boot_random: Text::new("00000001:00000002:00000003:00000004").unwrap(),
            available_cpus: Text::new("ARM, RISC-V").unwrap(),
            secure_boot: false,
            debug_enabled: true,
            secure_debug_enabled: true,
            unpartitioned_families: Text::new("rp2350-arm-s").unwrap(),
            flash_size_estimate: Some(4194304),
        }),
        information_sha256: Some(Sha256::new("e".repeat(64)).unwrap()),
        reason: Text::new("Inspected").unwrap(),
        capacity: d.capacity,
        flash_ready: false,
    }
}
#[test]
/// Genuine-Pico-2-W acknowledgment requires current complete inspection plus explicit choice and resets on reselection.
fn confirmation_requires_current_complete_inspection_and_explicit_choice() {
    let c = connection();
    let mut state = DeviceSelection::default();
    assert!(state.acknowledge(true).is_err());
    state.select(device()).unwrap();
    assert!(state.acknowledge(true).is_err());
    state.accept(&c, inspection()).unwrap();
    assert!(state.acknowledgement().is_none());
    state.acknowledge(true).unwrap();
    let ack = state.acknowledgement().unwrap();
    assert_eq!(ack.board, BoardContract::GenuinePico2W);
    assert_eq!(ack.session_id, *c.session());
    assert_eq!(ack.information_sha256.as_str(), "e".repeat(64));
    state.acknowledge(false).unwrap();
    assert!(state.acknowledgement().is_none());
    state.acknowledge(true).unwrap();
    state.select(device()).unwrap();
    assert!(state.acknowledgement().is_none());
    assert!(state.inspection().is_none());
}
#[test]
/// Wrong session/device/descriptor, missing facts, forged readiness and ambiguous descriptors cannot authorize
/// flashing.
fn foreign_and_changed_inspection_cannot_authorize_selected_device() {
    for variant in 0..5 {
        let mut state = DeviceSelection::default();
        state.select(device()).unwrap();
        let mut report = inspection();
        match variant {
            0 => report.session_id = SessionId::new("f".repeat(32)).unwrap(),
            1 => report.device_id = DeviceId::new("f".repeat(32)).unwrap(),
            2 => report.descriptor_sha256 = Sha256::new("f".repeat(64)).unwrap(),
            3 => report.facts = None,
            _ => report.flash_ready = true,
        };
        assert!(state.accept(&connection(), report).is_err());
        assert!(state.acknowledge(true).is_err());
    }
    let mut state = DeviceSelection::default();
    let mut d = device();
    d.status = DeviceStatus::Ambiguous;
    assert!(state.select(d).is_err());
    assert!(state.selected().is_none());
}
#[test]
/// Incomplete reinspection removes prior confirmation, and clearing selection also drops inspection authority.
fn failed_reinspection_removes_previous_confirmation() {
    let mut state = DeviceSelection::default();
    state.select(device()).unwrap();
    state.accept(&connection(), inspection()).unwrap();
    state.acknowledge(true).unwrap();
    let mut report = inspection();
    report.complete = false;
    report.facts = None;
    report.information_sha256 = None;
    state.accept(&connection(), report).unwrap();
    assert!(state.acknowledgement().is_none());
    assert!(state.acknowledge(true).is_err());
    state.clear();
    assert!(state.selected().is_none());
    assert!(state.inspection().is_none());
}

#[test]
/// Flash requests bind project/config/startup identities, options, session and confirmed device;
/// changed source, artwork, session or cleared selection denies authorization.
fn flash_request_binds_complete_project_artifact_options_and_confirmed_device() {
    use dashboard_configurator::build_flash::BuildSelection;
    use dashboard_configurator::editor::Editor;
    use dashboard_configurator::helper_client::PreparedBuild;
    let selection = BuildSelection::default();
    let mut editor = Editor::default();
    let prepared = PreparedBuild::capture(&editor, &selection, 0).unwrap();
    let project = dashboard_config_tools::project::Project::decode(prepared.bytes()).unwrap();
    let hash = |bytes: &[u8]| {
        Sha256::new(
            dashboard_config_tools::project::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
        .unwrap()
    };
    let artifact = ArtifactSummary {
        artifact_id: ArtifactId::new("1".repeat(64)).unwrap(),
        session_id: connection().session().clone(),
        editor_revision: 12,
        project_sha256: hash(prepared.bytes()),
        config_sha256: hash(project.dashboard_bytes()),
        startup_profile_sha256: hash(&project.profile().encode().unwrap()),
        startup_asset_sha256: hash(&[]),
        source_sha256: hash(b"source"),
        elf_sha256: hash(b"elf"),
        uf2_sha256: hash(b"uf2"),
        converter_sha256: hash(b"converter"),
        options: selection.normalized().unwrap(),
        uf2_bytes: 512,
        hardware_qualified: false,
        flash_ready: false,
    };
    let mut device_state = DeviceSelection::default();
    device_state.select(device()).unwrap();
    device_state.accept(&connection(), inspection()).unwrap();
    device_state.acknowledge(true).unwrap();
    let request = prepared
        .authorize_flash(&artifact, &connection(), &device_state)
        .unwrap();
    assert_eq!(request.device_id, device().device_id);
    assert_eq!(
        request.board_acknowledgement.information_sha256.as_str(),
        "e".repeat(64)
    );
    assert_eq!(request.project_sha256, artifact.project_sha256);
    assert_eq!(request.editor_revision, 12);
    for variant in 0..3 {
        let mut wrong = artifact.clone();
        match variant {
            0 => wrong.project_sha256 = hash(b"other"),
            1 => wrong.session_id = SessionId::new("f".repeat(32)).unwrap(),
            _ => wrong.startup_asset_sha256 = hash(b"newart"),
        };
        assert!(prepared.authorize_flash(&wrong, &connection(), &device_state).is_err());
    }
    let mut profile = editor.startup_profile();
    profile.duration_ms += 1;
    editor.set_startup_profile(profile).unwrap();
    let changed = PreparedBuild::capture(&editor, &selection, 0).unwrap();
    assert!(
        changed
            .authorize_flash(&artifact, &connection(), &device_state)
            .is_err()
    );
    device_state.clear();
    assert!(
        prepared
            .authorize_flash(&artifact, &connection(), &device_state)
            .is_err()
    );
}
