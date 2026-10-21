//! Editor export/migration through boot codec and shared synthetic runtime integration.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_core::acquisition::AcquisitionModel;
use dashboard_core::alarms::{RuleIndex, Status};
use dashboard_core::presentation::{PresentationState, Unit};
use dashboard_core::samples::SensorId;
use dashboard_core::warning_state::WarningCommand;

/// Asserts fixture `config` drives canonical/raw warnings, requested PSI presentation,
/// elapsed smoothing, history, latch/snooze acknowledgment and reduced-motion LED behavior;
/// requires the specially configured boost rule and panics on runtime divergence.
fn exercise_runtime(config: &ValidatedConfig) {
    use dashboard_core::status_led::{LedController, PresentedScene};
    let duty = LedController::new().update(
        &config.config().led,
        PresentedScene::EMPTY,
        true,
        1000,
        config.config().reduced_motion,
    );
    assert_eq!(duty.green, if config.config().reduced_motion { 10000 } else { 0 });
    assert_eq!((duty.red, duty.blue), (0, 0));
    let mut owner = AcquisitionModel::demo_configured(config);
    let mut display = PresentationState::new();
    let samples = |boost| [boost, 90., 95., 80., 30., 700., 13.8, 1.];
    owner.ingest_batch(samples(0.), 1000);
    display.prepare(config, &owner.snapshot(), config.config().boot_view, 1000, false);
    owner.ingest_batch(samples(3.), 1100);
    let snapshot = owner.snapshot();
    let readings = display.prepare(config, &snapshot, config.config().boot_view, 1100, false);
    let value = readings.widget(0).unwrap();
    assert_eq!(value.unit, Unit::Psi);
    if config.config().reduced_motion {
        assert_eq!(value.value, Some(3.));
    } else {
        assert!(value.value.unwrap() > 0. && value.value.unwrap() < 3.);
    }
    assert_eq!(snapshot.peak_at(RuleIndex::new(0).unwrap(), 1100), Some(3.));
    assert_eq!(snapshot.sensor(SensorId::Boost).history().duration_seconds(), 30);
    owner.ingest_batch(samples(3.), 1200);
    let active = owner.snapshot();
    let rule = RuleIndex::new(0).unwrap();
    assert_eq!(active.rule_status_at(rule, 1200), Status::Active);
    assert!(active.warning_eligibility(rule, 1200).latched);
    assert_eq!(
        owner
            .apply_warning_command(
                WarningCommand {
                    request_id: 1,
                    run_id: active.warning_run_id().unwrap(),
                    observed_event_serial: active.warning_event_serial(),
                    rules: 1,
                },
                1200
            )
            .accepted,
        1
    );
    let acknowledged = owner.snapshot().warning_eligibility(rule, 1200);
    assert!(acknowledged.acknowledged && acknowledged.snoozed && acknowledged.visible());
}

#[test]
/// Accepted settings survive persistence, JSON and boot binary into shared runtime; corrupt boot bytes atomically fall
/// back to defaults.
fn ordinary_exports_and_persisted_envelope_reach_current_boot_and_advanced_runtime() {
    let mut c = default_config();
    c.history_seconds[0] = 30;
    c.reduced_motion = true;
    c.led.normal.kind = LedPatternKind::Breathe;
    let w = &mut c.widgets[0];
    w.advanced.readout = Readout::Psi;
    w.advanced.smoothing_ms = 5000;
    w.advanced.peak_mode = PeakSelection::Maximum;
    w.advanced.peak_hold_ms = 60000;
    w.warning.enabled = true;
    w.warning.mode = LimitMode::OutsideWindow;
    w.warning.trigger = -0.5;
    w.warning.clear = 0.;
    w.warning.advanced.clear_high = 1.;
    w.warning.advanced.trigger_high = 2.;
    w.warning.advanced.latch = true;
    w.warning.advanced.snooze_s = 5;
    w.warning.on_ms = 100;
    c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let mut editor = Editor::new(c).unwrap();
    let saved = editor.begin_save().unwrap();
    let restored = Editor::from_saved_state(saved.json().as_bytes()).unwrap();
    assert_eq!(restored.preview(), editor.preview());
    assert!(!restored.export_json().unwrap().contains('\n'));
    for config in [
        restored.preview().clone(),
        from_json_bytes(restored.export_json().unwrap().as_bytes()).unwrap(),
    ] {
        exercise_runtime(&config);
    }
    let binary = restored.export_binary().unwrap();
    let mut slot = ConfigSlot::new();
    let boot = slot.load_at_boot(&binary);
    assert_eq!(boot.fault, None);
    exercise_runtime(&boot.config);
    let mut animated = boot.config.config().clone();
    animated.reduced_motion = false;
    exercise_runtime(&validate(animated).unwrap());
    // A corrupt new image atomically replaces every advanced option with defaults.
    let fallback = slot.load_at_boot(b"invalid image");
    assert!(fallback.fault.is_some());
    assert_eq!(fallback.config.config(), &default_config());
    let owner = AcquisitionModel::demo_configured(&fallback.config);
    assert!(core::ptr::eq(owner.configuration().unwrap(), &fallback.config));
}

#[test]
/// Frozen v1-v3 JSON/EAV imports migrate through saved envelopes to current default bytes and admitted boot
/// acquisition.
fn all_supported_legacy_json_and_binary_inputs_reach_current_boot_with_defaults() {
    for (format, bytes) in [
        (
            ImportFormat::Json,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v1-default.json").as_slice(),
        ),
        (
            ImportFormat::Json,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v2-default.json").as_slice(),
        ),
        (
            ImportFormat::Json,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v3-default.json").as_slice(),
        ),
        (
            ImportFormat::Binary,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v1-default.eav").as_slice(),
        ),
        (
            ImportFormat::Binary,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v2-default.eav").as_slice(),
        ),
        (
            ImportFormat::Binary,
            include_bytes!("../../crates/dashboard-config/tests/fixtures/v3-default.eav").as_slice(),
        ),
    ] {
        let mut editor = Editor::default();
        let import = editor.begin_import().unwrap();
        editor.complete_import(import, format, bytes).unwrap();
        let save = editor.begin_save().unwrap();
        let restored = Editor::from_saved_state(save.json().as_bytes()).unwrap();
        let binary = restored.export_binary().unwrap();
        assert_eq!(&binary, include_bytes!("../../configs/default.eav"));
        let mut slot = ConfigSlot::new();
        let boot = slot.load_at_boot(&binary);
        assert_eq!(boot.fault, None);
        assert_eq!(boot.config.config(), &default_config());
        let mut owner = AcquisitionModel::demo_configured(&boot.config);
        assert!(owner.ingest_batch([1., 90., 95., 80., 30., 700., 13.8, 1.], 1000));
        assert_eq!(owner.snapshot().value_at(SensorId::Boost, 1000), Some(1.));
    }
}
