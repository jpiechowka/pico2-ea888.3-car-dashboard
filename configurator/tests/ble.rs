//! BLE editor admission and synthetic adapter-parser regressions; no radio or ECU traffic.

use dashboard_configurator::ble::*;
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::field_registry::*;

#[test]
/// Only six colon-separated hexadecimal octets parse, with uppercase display and no whitespace/sign substitutions.
fn address_is_strict_display_order() {
    let address = [0xC1, 2, 3, 4, 5, 6];
    assert_eq!(parse_address("c1:02:03:04:05:06"), Some(address));
    assert_eq!(format_address(address), "C1:02:03:04:05:06");
    for bad in [
        "",
        "C1:02:03:04:05",
        "C1-02-03-04-05-06",
        " C1:02:03:04:05:06",
        "ZZ:02:03:04:05:06",
        "é:02:03:04:05:06",
        "+1:02:03:04:05:06",
    ] {
        assert_eq!(parse_address(bad), None);
    }
}
#[test]
/// Enabling without a valid peer retains an invalid draft; address repair admits one undoable configuration.
fn rejected_enable_is_repairable_and_one_undo_transaction() {
    let mut editor = Editor::default();
    let original = editor.export_binary().unwrap();
    let mut ble = editor.preview().config().ble;
    ble.enabled = true;
    assert!(editor.apply(Edit::SetBle(ble)).is_err());
    assert!(editor.has_invalid_draft());
    assert!(editor.export_binary().is_err());
    assert!(!editor.preview().config().ble.enabled);
    assert!(editor.editing_config().ble.enabled);
    let targets = Registry::error_targets(&editor);
    assert!(
        targets
            .iter()
            .any(|t| t.path == "/ble/peer/address" && t.field.is_some())
    );
    editor
        .set_raw(
            editor.field_token(),
            RawField::Ble(BleField::Address),
            "C1:02:03:04:05:06".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert!(editor.preview().config().ble.enabled);
    let accepted = editor.export_binary().unwrap();
    assert_ne!(original, accepted);
    editor.undo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), original);
    editor.redo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), accepted);
}
#[test]
/// Repairing a peer or disabling BLE cannot silently discard a separate invalid timing draft.
fn malformed_peer_and_timing_survive_other_repairs_and_disable() {
    let mut e = Editor::default();
    let initial = e.accepted_config_key();
    e.set_raw(e.field_token(), RawField::Ble(BleField::Address), "bad".into())
        .unwrap();
    e.set_raw(e.field_token(), RawField::Ble(BleField::PollGap), "999".into())
        .unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Ble(BleField::Address),
        "C1:02:03:04:05:06".into(),
    )
    .unwrap();
    assert!(e.has_invalid_draft());
    assert_eq!(e.accepted_config_key(), initial);
    assert!(e.apply(Edit::SetBle(BleSettings::DEFAULT)).is_err());
    assert!(e.has_invalid_draft());
    e.set_raw(e.field_token(), RawField::Ble(BleField::PollGap), "10".into())
        .unwrap();
    assert!(!e.has_invalid_draft());
    assert_eq!(e.preview().config().ble.poll_gap_ms, 10);
}
#[test]
/// BLE fields are registry-searchable and complete projects preserve profile, peer address/type and settings.
fn ble_fields_are_searchable_and_project_round_trip_preserves_selection() {
    let mut e = Editor::default();
    let ble = BleSettings {
        enabled: true,
        profile: BleProfile::ObdLinkCx,
        peer: BlePeer {
            address: [0xC1, 2, 3, 4, 5, 6],
            kind: BleAddressKind::RandomStatic,
        },
        ..BleSettings::DEFAULT
    };
    e.apply(Edit::SetBle(ble)).unwrap();
    assert!(Registry::search(&e, "BLE").unwrap().results.len() >= 4);
    let project = e.accepted_project().unwrap();
    let bytes = project
        .encode(dashboard_config_tools::project::ExportMode::Complete)
        .unwrap();
    let decoded = dashboard_config_tools::project::Project::decode(&bytes).unwrap();
    assert_eq!(decode(decoded.dashboard_bytes()).unwrap().config().ble, ble);
}

/// Returns a synthetic enabled simulator for `profile` with a fixed random-static peer; creates no radio connection.
fn simulator(profile: BleProfile) -> dashboard_configurator::ble_simulator::BleSimulator {
    dashboard_configurator::ble_simulator::BleSimulator::new(BleSettings {
        enabled: true,
        profile,
        peer: BlePeer {
            address: [0xC1, 2, 3, 4, 5, 6],
            kind: BleAddressKind::RandomStatic,
        },
        ..BleSettings::DEFAULT
    })
}
#[test]
/// MC+/CX reach adapter-ready while TestPeer is synthetic-ready; fragmented replies share the parser and latency
/// accounting.
fn simulator_uses_profile_gate_and_fragmented_shared_parser() {
    use dashboard_core::ble::link::LinkState;
    for (profile, state) in [
        (BleProfile::VlinkerMcPlus, LinkState::AdapterReady),
        (BleProfile::ObdLinkCx, LinkState::AdapterReady),
        (BleProfile::TestPeer, LinkState::SyntheticReady),
    ] {
        let mut sim = simulator(profile);
        sim.connect().unwrap();
        assert_eq!(sim.snapshot().state, state);
        assert_eq!(sim.snapshot().completed_transactions, 13);
        sim.begin_reply().unwrap();
        assert!(sim.busy());
        sim.complete_fragmented().unwrap();
        assert!(!sim.busy());
        assert_eq!(sim.snapshot().completed_transactions, 14);
        assert_eq!(sim.snapshot().last_latency_ms, Some(3));
    }
}
#[test]
/// Notification loss/timeout disconnect and retire streams; old epochs cannot finish a newly active transaction.
fn simulator_faults_retire_stream_and_old_epochs_cannot_complete_new_work() {
    use dashboard_core::ble::session::Error;
    let mut sim = simulator(BleProfile::TestPeer);
    sim.connect().unwrap();
    sim.begin_reply().unwrap();
    assert_eq!(sim.lose_notification(), Err(Error::NotificationLoss));
    assert!(!sim.connected());
    assert!(!sim.busy());
    assert_eq!(sim.snapshot().last_latency_ms, None);
    sim.connect().unwrap();
    assert_eq!(sim.snapshot().epoch, 2);
    sim.begin_reply().unwrap();
    assert_eq!(sim.stale_reply(), Err(Error::Epoch));
    assert!(sim.busy());
    sim.complete_fragmented().unwrap();
    sim.begin_reply().unwrap();
    assert_eq!(sim.expire_reply(), Err(Error::Timeout));
    assert!(!sim.connected());
    sim.connect().unwrap();
    sim.disconnect();
    assert!(!sim.connected());
}
#[test]
/// Disabled default settings deny synthetic connection without incrementing completed transactions.
fn simulator_disabled_default_never_starts_a_session() {
    let mut sim = dashboard_configurator::ble_simulator::BleSimulator::new(BleSettings::DEFAULT);
    assert!(sim.connect().is_err());
    assert_eq!(sim.snapshot().completed_transactions, 0);
    assert!(!sim.connected());
}
