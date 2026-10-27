//! Editor transaction, repair, identity, migration and serialization regressions.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

#[test]
/// Accepted configuration identity advances on canonical transitions, including undo/redo back to old bytes, and is
/// unique to its editor owner.
fn accepted_configuration_key_tracks_canonical_transitions_instead_of_current_content() {
    let mut editor = Editor::default();
    let first = editor.accepted_config_key();
    let bytes = editor.export_binary().unwrap();
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    let second = editor.accepted_config_key();
    assert_ne!(first, second);
    editor.apply(Edit::SetReducedMotion(false)).unwrap();
    assert_eq!(editor.export_binary().unwrap(), bytes);
    assert_ne!(editor.accepted_config_key(), first);
    let third = editor.accepted_config_key();
    editor.undo().unwrap();
    assert_ne!(editor.accepted_config_key(), second);
    assert_ne!(editor.accepted_config_key(), third);
    let undone = editor.accepted_config_key();
    editor.redo().unwrap();
    assert_ne!(editor.accepted_config_key(), undone);
    assert_ne!(editor.accepted_config_key(), first);
    assert_ne!(Editor::default().accepted_config_key(), first);
}

#[test]
/// Selection, rejected/discarded drafts and equal-content imports can advance editor revisions without changing
/// accepted configuration identity.
fn accepted_configuration_key_ignores_selection_noop_import_and_repaired_draft_activity() {
    let mut editor = Editor::default();
    let first = editor.accepted_config_key();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    assert_eq!(editor.accepted_config_key(), first);
    let accepted = editor.accepted_revision();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_ne!(editor.revision(), accepted);
    assert_eq!(editor.accepted_revision(), accepted);
    assert_eq!(editor.accepted_config_key(), first);
    editor.discard_raw().unwrap();
    assert_eq!(editor.accepted_revision(), editor.revision());
    assert_eq!(editor.accepted_config_key(), first);
    let json = editor.export_json().unwrap();
    let old = editor.accepted_revision();
    let import = editor.begin_import().unwrap();
    editor
        .complete_import(import, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_ne!(editor.accepted_revision(), old);
    assert_eq!(editor.accepted_config_key(), first);
    // Import changes persisted selection alone and adds a history entry.
    editor.undo().unwrap();
    assert_eq!(editor.selection().widget, Some(1));
    assert_eq!(editor.accepted_config_key(), first);
    editor.redo().unwrap();
    assert_eq!(editor.accepted_config_key(), first);
}

#[test]
/// JSON import preserves negative zero and changes canonical configuration identity despite floating-point numeric
/// equality.
fn signed_zero_import_changes_configuration_key_and_keeps_canonical_bytes() {
    let mut editor = Editor::default();
    let before = editor.accepted_config_key();
    let mut config = editor.preview().config().clone();
    config.widgets[0].advanced.reference = -0.0;
    let text = serde_json::to_string(&config).unwrap();
    let import = editor.begin_import().unwrap();
    editor
        .complete_import(import, ImportFormat::Json, text.as_bytes())
        .unwrap();
    assert!(
        editor.preview().config().widgets[0]
            .advanced
            .reference
            .is_sign_negative()
    );
    assert_ne!(editor.accepted_config_key(), before);
}

#[test]
/// Read-only inspection leaves active pointer/import ownership intact and snapshot admission reports that actual
/// blocker.
fn snapshot_admission_reports_actual_pointer_and_import_blockers() {
    let mut editor = Editor::default();
    let mut invalid = default_config();
    invalid.widgets[0].label.clear();
    let location = dashboard_configurator::diagnostic::collect_candidate(&invalid).issues[0].location;
    let pointer = editor.begin_pointer(1, PointerTarget::Widget(1)).unwrap();
    let target = editor.inspection_target(location).unwrap();
    editor.inspect(target).unwrap();
    assert_eq!(editor.active_pointer(), Some(pointer));
    assert_eq!(editor.action_readiness().block, Some(ActionBlock::PointerActive));
    assert_eq!(editor.export_json(), Err(EditorError::PointerActive));
    assert!(matches!(editor.begin_save(), Err(EditorError::PointerActive)));
    editor.cancel_pointer(pointer).unwrap();
    let import = editor.begin_import().unwrap();
    let target = editor.inspection_target(location).unwrap();
    editor.inspect(target).unwrap();
    assert!(editor.import_is_current(import));
    assert_eq!(editor.action_readiness().block, Some(ActionBlock::ImportPending));
    assert_eq!(editor.export_binary(), Err(EditorError::ImportPending));
    assert!(matches!(editor.begin_save(), Err(EditorError::ImportPending)));
    editor.cancel_import(import).unwrap();
    assert_eq!(editor.action_readiness().block, None);
    assert!(editor.export_binary().is_ok());
}

#[test]
/// Graph repair controls remain available through style changes or graph disabling; full repair commits the accumulated
/// draft once.
fn review_widget_repair_visibility_keeps_graph_fields_across_style_and_graph_disable() {
    let fields = [WidgetField::GraphMinimum, WidgetField::GraphMaximum];
    let mut c = default_config();
    c.widgets.truncate(1);
    c.widgets[0].display.graph = true;
    let (min, max) = Channel::Boost.domain();
    c.widgets[0].display.graph_range = GraphRange::Fixed { min, max };
    let mut e = Editor::new(c).unwrap();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let original = e.preview().clone();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::GraphMinimum),
        "bad".into(),
    )
    .unwrap();
    assert!(e.widget_fields_need_repair(1, &fields, Field::GraphRange));
    let mut w = e.editing_config().widgets[0].clone();
    w.gauge.kind = GaugeKind::Horizontal;
    w.display.graph = false;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert_eq!(e.editing_config().widgets[0].gauge.kind, GaugeKind::Horizontal);
    assert!(!e.editing_config().widgets[0].display.graph);
    assert!(e.widget_fields_need_repair(1, &fields, Field::GraphRange));
    assert!(!e.widget_fields_need_repair(2, &fields, Field::GraphRange));
    assert_eq!(e.preview(), &original);
    assert_eq!(e.history_len(), (0, 0));
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::GraphMinimum),
        "-0.4".into(),
    )
    .unwrap();
    assert!(!e.has_invalid_draft());
    assert!(!e.widget_fields_need_repair(1, &fields, Field::GraphRange));
    assert_eq!(e.history_len(), (1, 0));
    e.undo().unwrap();
    assert_eq!(e.preview(), &original);
}

#[test]
/// Typed invalid gauge, graph and warning values expose the correct repair controls even without retained raw text.
fn review_widget_repair_visibility_reveals_inactive_typed_errors_without_raw_text() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.gauge.min = f32::NAN;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert!(e.raw_fields().is_empty());
    assert!(e.widget_fields_need_repair(
        1,
        &[WidgetField::GaugeMinimum, WidgetField::GaugeMaximum],
        Field::GaugeRange
    ));
    assert!(!e.widget_fields_need_repair(
        1,
        &[WidgetField::GraphMinimum, WidgetField::GraphMaximum],
        Field::GraphRange
    ));
    e.discard_raw().unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.gauge.kind = GaugeKind::Horizontal;
    w.display.graph_range = GraphRange::Fixed { min: 2., max: 1. };
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert!(e.raw_fields().is_empty());
    assert!(e.widget_fields_need_repair(
        1,
        &[WidgetField::GraphMinimum, WidgetField::GraphMaximum],
        Field::GraphRange
    ));
    assert!(!e.widget_fields_need_repair(
        2,
        &[WidgetField::GraphMinimum, WidgetField::GraphMaximum],
        Field::GraphRange
    ));
    e.discard_raw().unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.warning.min_load_percent = 101.;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert!(e.raw_fields().is_empty());
    assert!(e.widget_fields_need_repair(1, &[WidgetField::WarningLoad], Field::WarningLoad));
}

#[test]
/// Disabling a warning cannot hide its invalid raw timing; repairing the value admits the combined edit once.
fn review_disabling_warning_keeps_its_retained_raw_fields_repairable() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.warning.enabled = true;
    e.apply(Edit::UpdateWidget(w)).unwrap();
    let original = e.preview().clone();
    let history = e.history_len();
    let field = RawField::Widget(1, WidgetField::WarningOn);
    e.set_raw(e.field_token(), field, "bad".into()).unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.warning.enabled = false;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert!(!e.editing_config().widgets[0].warning.enabled);
    assert_eq!(e.raw_fields()[0].text, "bad");
    assert!(e.widget_fields_need_repair(1, &[WidgetField::WarningOn], Field::WarningOn));
    assert_eq!(e.preview(), &original);
    assert_eq!(e.history_len(), history);
    e.set_raw(e.field_token(), field, "1000".into()).unwrap();
    assert!(!e.has_invalid_draft());
    assert!(!e.preview().config().widgets[0].warning.enabled);
    assert_eq!(e.history_len().0, history.0 + 1);
}

#[test]
/// Battery window-to-direct warning transitions restore correct hysteresis polarity, clear unused high bounds and
/// remain individually undoable.
fn review_battery_window_restores_both_direct_polarities_atomically() {
    for window in [LimitMode::OutsideWindow, LimitMode::InsideWindow] {
        for direct in [LimitMode::High, LimitMode::Low] {
            let mut c = default_config();
            c.widgets.clear();
            c.widgets
                .push(default_widget(
                    Channel::Battery,
                    1,
                    1,
                    Placement {
                        column: 0,
                        row: 0,
                        span: Span::OneByOne,
                    },
                ))
                .unwrap();
            let mut e = Editor::new(c).unwrap();
            let original = e.preview().clone();
            e.apply(Edit::SetWarningMode { id: 1, mode: window }).unwrap();
            let windowed = e.preview().clone();
            let stale = e.field_token();
            e.apply_if_current(stale, Edit::SetWarningMode { id: 1, mode: direct })
                .unwrap();
            let w = &e.preview().config().widgets[0].warning;
            assert_eq!(
                (w.trigger, w.clear),
                if direct == LimitMode::Low {
                    (11.5, 12.)
                } else {
                    (12., 11.5)
                }
            );
            assert_eq!(w.mode, direct);
            assert_eq!(w.advanced.trigger_high.to_bits(), 0);
            assert_eq!(w.advanced.clear_high.to_bits(), 0);
            assert_eq!(e.history_len(), (2, 0));
            assert_eq!(
                e.apply_if_current(stale, Edit::SetWarningMode { id: 1, mode: window }),
                Err(EditorError::StaleToken)
            );
            e.undo().unwrap();
            assert_eq!(e.preview(), &windowed);
            e.undo().unwrap();
            assert_eq!(e.preview(), &original);
            e.redo().unwrap();
            e.redo().unwrap();
            assert_eq!(e.preview().config().widgets[0].warning.mode, direct);
        }
    }
}

#[test]
/// Fixed-range raw errors prevent switching their parent to Auto until repaired, without advancing revision on rejected
/// parent changes.
fn review_fixed_graph_raw_fields_keep_their_parent_and_repair_path() {
    for (field, text) in [(WidgetField::GraphMinimum, "-0.4"), (WidgetField::GraphMaximum, "4.4")] {
        let mut c = default_config();
        c.widgets.truncate(1);
        let (min, max) = Channel::Boost.domain();
        c.widgets[0].display.graph_range = GraphRange::Fixed { min, max };
        let mut e = Editor::new(c).unwrap();
        e.select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
        let original = e.preview().clone();
        let old = e.field_token();
        let raw = RawField::Widget(1, field);
        e.set_raw(old, raw, "bad".into()).unwrap();
        let invalid = e.field_token();
        let rev = e.revision();
        let history = e.history_len();
        let mut attempted = e.editing_config().widgets[0].clone();
        attempted.display.graph_range = GraphRange::Auto;
        assert_eq!(
            e.apply_if_current(invalid, Edit::UpdateWidget(attempted.clone())),
            Err(EditorError::RepairFieldFirst(raw))
        );
        assert_eq!(
            e.editing_config().widgets[0].display.graph_range,
            GraphRange::Fixed { min, max },
            "transition must not hide the retained field"
        );
        assert_eq!(e.raw_fields()[0].field, raw);
        assert_eq!(e.raw_fields()[0].text, "bad");
        assert_eq!(e.revision(), rev);
        assert_eq!(e.history_len(), history);
        assert_eq!(e.preview(), &original);
        assert_eq!(
            e.apply_if_current(old, Edit::UpdateWidget(attempted.clone())),
            Err(EditorError::StaleToken)
        );
        e.set_raw(e.field_token(), raw, text.into()).unwrap();
        assert_eq!(e.history_len().0, history.0 + 1);
        assert!(!e.has_invalid_draft());
        let repaired = e.preview().clone();
        assert_eq!(
            e.apply_if_current(invalid, Edit::UpdateWidget(attempted)),
            Err(EditorError::StaleToken)
        );
        let mut w = e.editing_config().widgets[0].clone();
        w.display.graph_range = GraphRange::Auto;
        e.apply(Edit::UpdateWidget(w)).unwrap();
        e.undo().unwrap();
        assert_eq!(e.preview(), &repaired);
        e.undo().unwrap();
        assert_eq!(e.preview(), &original);
    }
}

#[test]
/// Fuel variant changes cannot remove retained invalid fields; all pending repairs commit together and retire old
/// callbacks.
fn review_fuel_raw_fields_keep_their_variant_and_commit_complete_repairs_once() {
    let mut common = Blend::illustrative();
    common.density_reference = DensityReference::CommonCelsius(20.);
    let mut unknown = common;
    unknown.density_reference = DensityReference::UnknownAssumed;
    for (fuel, field, replacement, text) in [
        (
            Fuel::Stoichiometric { ratio: 14.7 },
            FuelField::Ratio,
            Fuel::EquivalentGasoline,
            "14.5",
        ),
        (
            Fuel::EthanolBlend(Blend::illustrative()),
            FuelField::Percent,
            Fuel::Stoichiometric { ratio: 14.7 },
            "10",
        ),
        (
            Fuel::EthanolBlend(common),
            FuelField::ReferenceCelsius,
            Fuel::EthanolBlend(unknown),
            "21",
        ),
    ] {
        let mut c = default_config();
        c.fuel = fuel;
        let mut e = Editor::new(c).unwrap();
        let original = e.preview().clone();
        let old = e.field_token();
        let raw = RawField::Fuel(field);
        e.set_raw(old, raw, "bad".into()).unwrap();
        e.set_raw(e.field_token(), RawField::WarningPageMilliseconds, "bad".into())
            .unwrap();
        let invalid = e.field_token();
        let rev = e.revision();
        assert_eq!(
            e.apply_if_current(invalid, Edit::SetFuel(replacement)),
            Err(EditorError::RepairFieldFirst(raw))
        );
        assert_eq!(
            e.editing_config().fuel,
            fuel,
            "transition must not hide the retained fuel field"
        );
        assert_eq!(e.revision(), rev);
        assert_eq!(e.history_len(), (0, 0));
        assert_eq!(e.preview(), &original);
        assert_eq!(e.raw_fields().len(), 2);
        assert_eq!(e.raw_fields()[0].text, "bad");
        assert_eq!(
            e.apply_if_current(old, Edit::SetFuel(replacement)),
            Err(EditorError::StaleToken)
        );
        e.set_raw(e.field_token(), raw, text.into()).unwrap();
        assert_eq!(e.raw_fields()[1].text, "bad");
        assert!(e.has_invalid_draft());
        assert_eq!(e.history_len(), (0, 0));
        e.set_raw(e.field_token(), RawField::WarningPageMilliseconds, "2500".into())
            .unwrap();
        assert!(!e.has_invalid_draft());
        assert_eq!(e.history_len(), (1, 0));
        let repaired = e.preview().clone();
        assert_eq!(
            e.apply_if_current(invalid, Edit::SetFuel(replacement)),
            Err(EditorError::StaleToken)
        );
        e.apply(Edit::SetFuel(replacement)).unwrap();
        e.undo().unwrap();
        assert_eq!(e.preview(), &repaired);
        e.undo().unwrap();
        assert_eq!(e.preview(), &original);
    }
}

#[test]
/// Typing into a nonexistent band field fails before creating raw state or advancing revision.
fn raw_band_targets_must_exist_before_accepting_the_keystroke() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let rev = e.revision();
    assert_eq!(
        e.set_raw(
            e.field_token(),
            RawField::Widget(1, WidgetField::BandMinimum(3)),
            "1".into()
        ),
        Err(EditorError::FieldUnavailable)
    );
    assert_eq!(e.revision(), rev);
    assert!(e.raw_fields().is_empty());
}

#[test]
/// Band deletion remaps remaining raw fields, timing repairs preserve combined edits, and accepted global changes
/// retire async work.
fn advanced_raw_fields_globals_and_band_removal_preserve_atomic_history() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let mut w = e.preview().config().widgets[0].clone();
    w.advanced
        .set_bands(&[
            Band::new(0., 1., w.colors.gauge, "ONE").unwrap(),
            Band::new(1., 2., w.colors.gauge, "TWO").unwrap(),
        ])
        .unwrap();
    e.apply(Edit::UpdateWidget(w)).unwrap();
    let before = e.preview().clone();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::BandMinimum(0)),
        "bad".into(),
    )
    .unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::BandLabel(1)),
        "EDIT".into(),
    )
    .unwrap();
    e.apply(Edit::RemoveBand { id: 1, index: 0 }).unwrap();
    assert_eq!(e.preview().config().widgets[0].advanced.bands()[0].label(), "EDIT");
    assert!(!e.has_invalid_draft());
    e.undo().unwrap();
    assert_eq!(e.preview(), &before);
    e.redo().unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::SmoothingMilliseconds),
        "1".into(),
    )
    .unwrap();
    assert!(e.has_invalid_draft());
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::SmoothingMilliseconds),
        "750".into(),
    )
    .unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::PeakHoldMilliseconds),
        "1000".into(),
    )
    .unwrap();
    let trigger = e.preview().config().widgets[0].warning.trigger;
    let mut w = e.preview().config().widgets[0].clone();
    w.advanced.readout = Readout::Psi;
    e.apply(Edit::UpdateWidget(w)).unwrap();
    assert_eq!(e.preview().config().widgets[0].warning.trigger, trigger);
    e.apply(Edit::SetHistory {
        channel: Channel::Boost,
        seconds: 30,
    })
    .unwrap();
    e.apply(Edit::SetReducedMotion(true)).unwrap();
    let stale = e.begin_save().unwrap();
    let import = e.begin_import().unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Led {
            warning: false,
            field: LedField::Brightness,
        },
        "0".into(),
    )
    .unwrap();
    assert!(!e.save_is_current(stale.token()));
    assert_eq!(
        e.complete_import(import, ImportFormat::Json, b"{}"),
        Err(EditorError::StaleToken)
    );
    e.set_raw(
        e.field_token(),
        RawField::Led {
            warning: true,
            field: LedField::Period,
        },
        "199".into(),
    )
    .unwrap();
    assert!(e.has_invalid_draft());
    let mut led = e.editing_config().led;
    led.warning.kind = LedPatternKind::Off;
    assert!(e.apply(Edit::SetLed(led)).is_err(), "Off cannot hide invalid timing");
    e.set_raw(
        e.field_token(),
        RawField::Led {
            warning: true,
            field: LedField::Period,
        },
        "400".into(),
    )
    .unwrap();
    assert_eq!(e.preview().config().led.warning.kind, LedPatternKind::Off);
}

#[test]
/// A fourth band is rejected atomically; view duplication copies accepted advanced settings under fresh widget IDs.
fn fourth_band_rejection_does_not_mutate_widget_or_editor_and_duplicates_copy_settings() {
    let mut e = Editor::default();
    let mut w = e.preview().config().widgets[0].clone();
    w.advanced.smoothing_ms = 250;
    let bands = [
        Band::new(0., 1., w.colors.gauge, "ONE").unwrap(),
        Band::new(1., 2., w.colors.gauge, "TWO").unwrap(),
        Band::new(2., 3., w.colors.gauge, "THREE").unwrap(),
    ];
    w.advanced.set_bands(&bands).unwrap();
    let unchanged = w.advanced;
    let four = [bands[0], bands[1], bands[2], bands[2]];
    assert_eq!(w.advanced.set_bands(&four).unwrap_err().field, Field::BandCount);
    assert_eq!(w.advanced, unchanged);
    e.apply(Edit::UpdateWidget(w)).unwrap();
    e.apply(Edit::DuplicateView {
        id: 1,
        name: "COPY".into(),
    })
    .unwrap();
    let config = e.preview().config();
    let copy = config
        .widgets
        .iter()
        .find(|w| w.view == e.selection().view && w.channel == WidgetChannel::Boost)
        .unwrap();
    assert_ne!(copy.id, 1);
    assert_eq!(copy.advanced, unchanged);
}

#[test]
/// Window mode presets and return to direct thresholds each form one reversible transaction with unused bounds cleared.
fn advanced_window_mode_presets_switch_and_undo_as_one_operation() {
    let mut e = Editor::default();
    let original = e.preview().clone();
    e.apply(Edit::SetWarningMode {
        id: 1,
        mode: LimitMode::OutsideWindow,
    })
    .unwrap();
    assert_eq!(e.preview().config().widgets[0].warning.mode, LimitMode::OutsideWindow);
    e.apply(Edit::SetWarningMode {
        id: 1,
        mode: LimitMode::InsideWindow,
    })
    .unwrap();
    e.apply(Edit::SetWarningMode {
        id: 1,
        mode: LimitMode::High,
    })
    .unwrap();
    assert_eq!(
        e.preview().config().widgets[0].warning.advanced.trigger_high.to_bits(),
        0
    );
    e.undo().unwrap();
    e.undo().unwrap();
    e.undo().unwrap();
    assert_eq!(e.preview(), &original);
}

#[test]
/// Typed repairs can resolve retained advanced errors while unrelated raw text remains intact until repaired.
fn advanced_rejected_widget_repairs_atomically_without_losing_raw_text() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let before = e.preview().clone();
    let history = e.history_len();
    let mut w = before.config().widgets[0].clone();
    w.advanced.reference_mode = ReferenceMode::Manual;
    w.advanced.reference = 999.;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert_eq!(e.preview(), &before);
    let mut w = e.editing_config().widgets[0].clone();
    w.advanced.reference_mode = ReferenceMode::Off;
    e.apply(Edit::UpdateWidget(w)).unwrap();
    assert!(!e.has_invalid_draft());
    assert_eq!(e.history_len().0, history.0 + 1);
    e.undo().unwrap();
    assert_eq!(e.preview(), &before);
    e.redo().unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::GaugeMinimum),
        "oops".into(),
    )
    .unwrap();
    let mut w = e.editing_config().widgets[0].clone();
    w.advanced.readout = Readout::Kpa;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    assert_eq!(e.raw_fields()[0].text, "oops");
    assert_eq!(e.editing_config().widgets[0].advanced.readout, Readout::Kpa);
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::GaugeMinimum),
        "-0.5".into(),
    )
    .unwrap();
    assert_eq!(e.preview().config().widgets[0].advanced.readout, Readout::Kpa);
}

#[test]
/// A retained latch error accepts numeric repair and commits once controls provide a reachable acknowledgment route.
fn typed_warning_failure_can_be_repaired_with_a_new_numeric_field() {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let mut w = e.preview().config().widgets[0].clone();
    w.warning.enabled = true;
    w.warning.advanced.latch = true;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::WarningOn),
        "100".into(),
    )
    .unwrap();
    let mut controls = e.editing_config().controls;
    controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    e.apply(Edit::SetControls(controls)).unwrap();
    assert!(e.preview().config().widgets[0].warning.advanced.latch);
    assert_eq!(e.preview().config().widgets[0].warning.on_ms, 100);
}

#[test]
/// The frozen v1 saved envelope migrates its schema while retaining custom label and widget selection.
fn saved_v1_migrates_layout_labels_and_selection_without_changing_boot_view() {
    let mut config: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../crates/dashboard-config/tests/fixtures/v1-default.json"
    ))
    .unwrap();
    config["widgets"][0]["label"] = "MY BOOST".into();
    let saved = serde_json::json!({"editor_version":1,"selected_view":1,"selected_widget":1,"config":config});
    let editor = Editor::from_saved_state(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(editor.preview().config().version, VERSION);
    assert_eq!(editor.selection().widget, Some(1));
    assert_eq!(editor.preview().config().widgets[0].label, "MY BOOST");
    assert_eq!(
        editor.preview().config().widgets[0].gauge,
        Gauge::for_channel(Channel::Boost)
    );
}

#[test]
/// Nonfinite gauge limits remain diagnostic-only until a valid raw repair restores export.
fn invalid_gauge_limits_are_visible_repairable_and_never_exported() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    let mut widget = editor.preview().config().widgets[0].clone();
    widget.gauge.kind = GaugeKind::Circular;
    editor.apply(Edit::UpdateWidget(widget)).unwrap();
    let token = editor.field_token();
    editor
        .set_raw(token, RawField::Widget(1, WidgetField::GaugeMinimum), "NaN".into())
        .unwrap();
    assert!(editor.export_binary().is_err());
    assert!(editor.diagnostic().is_some());
    let token = editor.field_token();
    editor
        .set_raw(token, RawField::Widget(1, WidgetField::GaugeMinimum), "-0.5".into())
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[0].gauge.min, -0.5);
}

#[test]
/// High/Low transitions swap trigger and clear thresholds reversibly; an unsupported lambda mode preserves accepted
/// state.
fn warning_direction_changes_preserve_hysteresis_and_undo_atomically() {
    let mut editor = Editor::default();
    for id in [1, 3, 5, 8] {
        let original = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .warning
            .clone();
        let mode = match original.mode {
            LimitMode::High => LimitMode::Low,
            LimitMode::Low => LimitMode::High,
            _ => continue,
        };
        editor.apply(Edit::SetWarningMode { id, mode }).unwrap();
        let changed = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .warning
            .clone();
        assert_eq!(changed.mode, mode);
        assert_eq!((changed.trigger, changed.clear), (original.clear, original.trigger));
        editor.undo().unwrap();
        assert_eq!(
            editor
                .preview()
                .config()
                .widgets
                .iter()
                .find(|w| w.id == id)
                .unwrap()
                .warning,
            original
        );
    }
    let before = editor.preview().clone();
    assert!(
        editor
            .apply(Edit::SetWarningMode {
                id: 1,
                mode: LimitMode::LambdaDeviation
            })
            .is_err()
    );
    assert_eq!(editor.preview(), &before);
}

/// Returns a validated default editor with no widgets; panics if the fixture configuration is rejected.
fn empty() -> Editor {
    let mut config = default_config();
    config.widgets.clear();
    Editor::new(config).unwrap()
}

/// Returns a placement with zero-based cell `column` and `row` and requested `span`, without validating bounds.
fn placement(
    column: u8,
    row: u8,
    span: Span,
) -> Placement {
    Placement { column, row, span }
}

/// Adds an Oil widget to the selected view in `editor` at cell `column`/`row` with `span`; returns its selected ID and
/// panics on failed admission.
fn add(
    editor: &mut Editor,
    column: u8,
    row: u8,
    span: Span,
) -> u16 {
    editor
        .apply(Edit::AddWidget {
            view: editor.selection().view,
            channel: Channel::Oil.into(),
            placement: placement(column, row, span),
        })
        .unwrap();
    editor.selection().widget.unwrap()
}

#[test]
/// Invalid placement retains a repairable draft and retires callbacks while preserving accepted bytes/history; discard
/// restores accepted selection.
fn rejected_move_and_resize_preserve_document_but_retire_callbacks_and_block_export() {
    let mut editor = empty();
    let first = add(&mut editor, 0, 0, Span::OneByOne);
    add(&mut editor, 1, 0, Span::OneByOne);
    let before = editor.preview().clone();
    let selection = editor.selection();
    let history = editor.history_len();
    for edit in [
        Edit::MoveWidget {
            id: first,
            view: 1,
            column: 1,
            row: 0,
        },
        Edit::ResizeWidget {
            id: first,
            span: Span::TwoByOne,
        },
        Edit::MoveWidget {
            id: first,
            view: 1,
            column: u8::MAX,
            row: 0,
        },
    ] {
        let revision = editor.revision();
        let token = editor.field_token();
        assert!(matches!(editor.apply(edit), Err(EditorError::Validation(_))));
        assert_eq!(editor.preview(), &before);
        assert!(
            editor
                .editing_config()
                .widgets
                .iter()
                .any(|w| Some(w.id) == editor.selection().widget)
        );
        assert_ne!(editor.revision(), revision);
        assert_eq!(editor.history_len(), history);
        assert!(editor.has_invalid_draft());
        assert_eq!(editor.export_json(), Err(EditorError::InvalidDraft));
        assert_eq!(editor.export_binary(), Err(EditorError::InvalidDraft));
        assert!(matches!(editor.begin_save(), Err(EditorError::InvalidDraft)));
        assert_eq!(
            editor.apply_if_current(token, Edit::DeleteWidget(first)),
            Err(EditorError::StaleToken)
        );
        editor.discard_raw().unwrap();
        assert_eq!(editor.selection(), selection, "discard restores the accepted selection");
    }
    editor
        .apply(Edit::MoveWidget {
            id: first,
            view: 1,
            column: 2,
            row: 0,
        })
        .unwrap();
    editor
        .apply(Edit::ResizeWidget {
            id: first,
            span: Span::TwoByTwo,
        })
        .unwrap();
    assert_eq!(
        editor.preview().config().widgets[0].placement,
        placement(2, 0, Span::TwoByTwo)
    );
}

#[test]
/// Duplication preserves content under fresh IDs, and deletion repairs boot/selection references without allowing the
/// last view to disappear.
fn duplicate_and_delete_preserve_fields_assign_new_ids_and_repair_boot_and_selection() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::TwoByOne);
    let mut changed = editor.preview().config().widgets[0].clone();
    changed.label = "long preserved label".try_into().unwrap();
    changed.display.decimals = 2;
    changed.warning.message = "Custom warning".try_into().unwrap();
    editor.apply(Edit::UpdateWidget(changed.clone())).unwrap();
    editor
        .apply(Edit::DuplicateWidget {
            id: widget,
            view: 1,
            placement: placement(2, 0, Span::TwoByOne),
        })
        .unwrap();
    let duplicate = editor.selection().widget.unwrap();
    assert_ne!(duplicate, widget);
    assert_eq!(editor.preview().config().widgets[1].warning, changed.warning);
    assert!(
        editor
            .apply(Edit::ResizeWidget {
                id: duplicate,
                span: Span::OneByOne
            })
            .is_err()
    );
    assert_eq!(editor.preview().config().widgets[1].label, changed.label);
    editor.discard_raw().unwrap();
    editor
        .apply(Edit::DuplicateView {
            id: 1,
            name: "Track".into(),
        })
        .unwrap();
    let copied_view = editor.selection().view;
    assert_ne!(copied_view, 1);
    let copied: Vec<_> = editor
        .preview()
        .config()
        .widgets
        .iter()
        .filter(|w| w.view == copied_view)
        .collect();
    assert_eq!(copied.len(), 2);
    assert!(copied.iter().all(|w| w.id != widget && w.id != duplicate));
    assert_ne!(copied[0].id, copied[1].id);
    assert_eq!(copied[0].label, changed.label);
    editor.apply(Edit::SetBootView(copied_view)).unwrap();
    editor.apply(Edit::DeleteView(copied_view)).unwrap();
    assert_eq!(editor.selection(), Selection { view: 1, widget: None });
    assert_eq!(editor.preview().config().boot_view, 1);
    assert_eq!(editor.preview().config().widgets.len(), 2);
    assert_eq!(editor.apply(Edit::DeleteView(1)), Err(EditorError::LastView));
    editor.apply(Edit::DeleteWidget(widget)).unwrap();
    assert_eq!(editor.preview().config().widgets.len(), 1);
}

#[test]
/// Maximum view/widget capacity round-trips; further duplication fails without revision or identity changes.
fn capacity_failure_is_atomic_and_ids_do_not_renumber_existing_objects() {
    let mut editor = Editor::default();
    for n in 2..=8 {
        editor
            .apply(Edit::DuplicateView {
                id: 1,
                name: format!("View {n}"),
            })
            .unwrap();
    }
    assert_eq!(editor.preview().config().widgets.len(), 64);
    round_trip(&mut editor);
    let before = editor.preview().clone();
    let revision = editor.revision();
    assert_eq!(
        editor.apply(Edit::DuplicateView {
            id: 1,
            name: "Overflow".into()
        }),
        Err(EditorError::ViewCapacity)
    );
    assert_eq!(
        editor.apply(Edit::DuplicateWidget {
            id: 1,
            view: 1,
            placement: placement(0, 0, Span::OneByOne)
        }),
        Err(EditorError::WidgetCapacity)
    );
    assert_eq!(editor.preview(), &before);
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.preview().config().widgets[0].id, 1);
}

#[test]
/// View ordering determines wrapped selection after deletion and is restored through history.
fn ordered_views_wrap_selection_on_delete_and_reordering_is_undoable() {
    let mut editor = empty();
    editor.apply(Edit::AddView { name: "Second".into() }).unwrap();
    let second = editor.selection().view;
    editor.apply(Edit::AddView { name: "Third".into() }).unwrap();
    let third = editor.selection().view;
    editor.apply(Edit::ReorderView { id: third, index: 0 }).unwrap();
    assert_eq!(
        editor.preview().config().views.iter().map(|v| v.id).collect::<Vec<_>>(),
        vec![third, 1, second]
    );
    editor.apply(Edit::DeleteView(second)).unwrap();
    assert_eq!(editor.selection().view, third);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().views.len(), 3);
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().views.len(), 2);
}

#[test]
/// History retains the latest 32 edits, and a new accepted branch clears redo availability.
fn undo_is_bounded_and_a_new_accepted_edit_discards_redo() {
    let mut editor = empty();
    for n in 1..=40 {
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: format!("Name {n}"),
            })
            .unwrap();
    }
    for _ in 0..32 {
        editor.undo().unwrap();
    }
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Name 8");
    assert_eq!(editor.undo(), Err(EditorError::NoUndo));
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Name 9");
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New branch".into(),
        })
        .unwrap();
    assert_eq!(editor.redo(), Err(EditorError::NoRedo));
}

#[test]
/// Missing IDs and invalid typed values cannot replace accepted configuration; valid fuel input updates the canonical
/// ratio.
fn selection_and_typed_edits_reject_dangling_ids_and_invalid_values() {
    let mut editor = Editor::default();
    assert_eq!(
        editor.select(Selection {
            view: 200,
            widget: None
        }),
        Err(EditorError::MissingView(200))
    );
    assert_eq!(
        editor.select(Selection {
            view: 1,
            widget: Some(200)
        }),
        Err(EditorError::MissingWidget(200))
    );
    let before = editor.preview().clone();
    assert!(
        editor
            .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: f32::NAN }))
            .is_err()
    );
    assert!(editor.apply(Edit::SetBootView(200)).is_err());
    editor.discard_raw().unwrap();
    assert!(editor.apply(Edit::SetBootView(200)).is_err());
    editor.discard_raw().unwrap();
    assert!(
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: "bad\nname".into()
            })
            .is_err()
    );
    assert_eq!(editor.preview(), &before);
    editor.discard_raw().unwrap();
    editor
        .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 }))
        .unwrap();
    assert_eq!(editor.preview().stoich(), 14.0);
}

#[test]
/// An incomplete numeric field blocks conflicting actions and export while preserving accepted state; valid completion
/// creates one undo entry.
fn partial_numeric_text_is_retained_without_replacing_preview_or_allowing_export() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let preview = editor.preview().clone();
    let history = editor.history_len();
    let old_revision = editor.revision();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "-".into(),
        )
        .unwrap();
    assert_eq!(editor.raw_fields()[0].text, "-");
    assert!(editor.has_invalid_draft());
    assert!(editor.draft_error().is_some());
    assert_eq!(editor.preview(), &preview);
    assert_eq!(editor.history_len(), history);
    assert_ne!(editor.revision(), old_revision);
    assert_eq!(
        editor.select(Selection { view: 1, widget: None }),
        Err(EditorError::InvalidDraft)
    );
    assert_eq!(editor.apply(Edit::DeleteWidget(5)), Err(EditorError::InvalidDraft));
    assert_eq!(editor.undo(), Err(EditorError::InvalidDraft));
    assert_eq!(editor.export_json(), Err(EditorError::InvalidDraft));
    assert_eq!(editor.export_binary(), Err(EditorError::InvalidDraft));
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "140".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[4].warning.trigger, 140.0);
    assert_eq!(editor.history_len().0, history.0 + 1);
    assert!(editor.export_json().is_ok());
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &preview);
}

#[test]
/// Hysteresis fields admit together once consistent, and explicit raw discard retires delayed field events.
fn interdependent_invalid_fields_commit_together_and_explicit_discard_cancels_old_events() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let original = editor.preview().clone();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "100".into(),
        )
        .unwrap();
    assert!(editor.has_invalid_draft()); // Existing clear 125 is above the proposed trigger.
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningClear),
            "90".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft());
    let warning = &editor.preview().config().widgets[4].warning;
    assert_eq!((warning.trigger, warning.clear), (100.0, 90.0));
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &original);
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::Label),
            "Far too long for this narrow widget".into(),
        )
        .unwrap();
    assert_eq!(editor.raw_fields()[0].text, "Far too long for this narrow widget");
    let old_field = editor.field_token();
    editor.discard_raw().unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(
        editor.set_raw(old_field, RawField::Widget(5, WidgetField::Label), "Late".into()),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview(), &original);
}

#[test]
/// Nonfinite fuel text stays invalid; stale selection authority, mismatched widget targets and oversize raw text are
/// rejected.
fn raw_nonfinite_values_and_fields_from_old_selections_cannot_enter_exports() {
    let mut editor = Editor::default();
    editor
        .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.7 }))
        .unwrap();
    for raw in ["NaN", "inf", "-inf", "", "1e999"] {
        editor
            .set_raw(editor.field_token(), RawField::Fuel(FuelField::Ratio), raw.into())
            .unwrap();
        assert_eq!(editor.raw_fields()[0].text, raw);
        assert!(editor.export_json().is_err());
        assert_eq!(editor.preview().stoich(), 14.7);
    }
    editor.discard_raw().unwrap();
    let old_field = editor.field_token();
    editor
        .select(Selection {
            view: 1,
            widget: Some(2),
        })
        .unwrap();
    assert_eq!(
        editor.set_raw(old_field, RawField::ViewName(1), "Old field".into()),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::Label),
            "Wrong".into()
        ),
        Err(EditorError::SelectionMismatch)
    );
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Fuel(FuelField::Ratio),
            "x".repeat(MAX_RAW_TEXT_BYTES + 1)
        ),
        Err(EditorError::RawTooLong)
    );
}

#[test]
/// Pointer completion requires the current session and original pointer ID, consumes successful ownership once and
/// rejects replay.
fn pointer_commit_requires_current_request_original_pointer_and_revision() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let old = editor.begin_pointer(7, PointerTarget::Widget(widget)).unwrap();
    let current = editor.begin_pointer(7, PointerTarget::Widget(widget)).unwrap();
    assert_eq!(
        editor.commit_pointer(old, 7, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.commit_pointer(current, 8, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.active_pointer(), Some(current));
    editor
        .commit_pointer(current, 7, placement(2, 0, Span::TwoByTwo))
        .unwrap();
    assert_eq!(
        editor.preview().config().widgets[0].placement,
        placement(2, 0, Span::TwoByTwo)
    );
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(
        editor.commit_pointer(current, 7, placement(0, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
}

#[test]
/// Rejected drops retain diagnostic candidates; cancellation retires pointer authority and successful library drops add
/// the requested channel.
fn pointer_cancel_and_rejected_drop_do_not_modify_the_document() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    add(&mut editor, 1, 0, Span::OneByOne);
    let before = editor.preview().clone();
    let pointer = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    assert!(
        editor
            .commit_pointer(pointer, 1, placement(1, 0, Span::OneByOne))
            .is_err()
    );
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(editor.preview(), &before);
    assert!(editor.has_invalid_draft());
    editor.discard_raw().unwrap();
    let pointer = editor
        .begin_pointer(2, PointerTarget::Library(Channel::Lambda.into()))
        .unwrap();
    editor.cancel_pointer(pointer).unwrap();
    assert_eq!(
        editor.commit_pointer(pointer, 2, placement(2, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview(), &before);
    let pointer = editor
        .begin_pointer(2, PointerTarget::Library(Channel::Lambda.into()))
        .unwrap();
    editor
        .commit_pointer(pointer, 2, placement(2, 0, Span::OneByOne))
        .unwrap();
    assert_eq!(editor.preview().config().widgets[2].channel, WidgetChannel::Lambda);
}

#[test]
/// Accepted edits, history changes and invalid raw input retire pointer sessions and prevent delayed drops.
fn edits_view_changes_undo_and_raw_input_retire_existing_pointer_transactions() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor.apply(Edit::AddView { name: "Other".into() }).unwrap();
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    editor
        .select(Selection {
            view: 1,
            widget: Some(widget),
        })
        .unwrap();
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor.undo().unwrap();
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    editor
        .select(Selection {
            view: 1,
            widget: Some(widget),
        })
        .unwrap();
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(widget, WidgetField::Decimals),
            "-".into(),
        )
        .unwrap();
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.begin_pointer(1, PointerTarget::Widget(widget)),
        Err(EditorError::InvalidDraft)
    );
}

#[test]
/// Import replacement is one undoable edit; importing equal content still retires previously captured field callbacks.
fn imported_replacement_is_one_undoable_transaction_and_cancels_same_document_field_events() {
    let mut editor = empty();
    let before = editor.preview().clone();
    let json = Editor::default().export_json().unwrap();
    let token = editor.begin_import().unwrap();
    editor
        .complete_import(token, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(editor.preview().config().widgets.len(), 8);
    assert_eq!(editor.selection(), Selection { view: 1, widget: None });
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &before);
    editor.redo().unwrap();
    let field = editor.field_token();
    let token = editor.begin_import().unwrap();
    editor
        .complete_import(token, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(
        editor.set_raw(field, RawField::ViewName(1), "Late".into()),
        Err(EditorError::StaleToken)
    );
}

#[test]
/// Supersession, accepted/raw edits and cancellation retire import handles before their completion can replace state.
fn older_or_edited_import_requests_cannot_replace_current_state() {
    let mut editor = empty();
    let bytes = Editor::default().export_binary().unwrap();
    let old = editor.begin_import().unwrap();
    let current = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    editor.complete_import(current, ImportFormat::Binary, &bytes).unwrap();
    let old = editor.begin_import().unwrap();
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New edit".into(),
        })
        .unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview().config().views[0].name.as_str(), "New edit");
    let old = editor.begin_import().unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.begin_import(), Err(EditorError::InvalidDraft));
    editor.discard_raw().unwrap();
    let token = editor.begin_import().unwrap();
    editor.cancel_import(token).unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
}

#[test]
/// Malformed JSON and corrupt/oversized binary imports preserve accepted configuration, revision and history.
fn malformed_oversize_duplicate_unknown_and_trailing_imports_are_atomic() {
    let mut editor = empty();
    let good = Editor::default().export_json().unwrap();
    let version_field = format!("\"version\":{VERSION}");
    let bad_inputs = [
        good.replacen(&version_field, &format!("{version_field}, {version_field}"), 1),
        good.replacen(&version_field, &format!("{version_field}, \"unknown\":1"), 1),
        format!("{good} null"),
        good.replacen("14.7", "null", 1)
            .replacen("\"trigger\":2.5", "\"trigger\":null", 1),
        " ".repeat(MAX_JSON_BYTES + 1),
    ];
    let before = editor.preview().clone();
    let revision = editor.revision();
    for bytes in bad_inputs {
        let token = editor.begin_import().unwrap();
        assert!(
            editor
                .complete_import(token, ImportFormat::Json, bytes.as_bytes())
                .is_err()
        );
        assert_eq!(editor.preview(), &before);
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.history_len(), (0, 0));
    }
    let mut corrupt = Editor::default().export_binary().unwrap();
    corrupt[12] ^= 1;
    let token = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &corrupt),
        Err(EditorError::Codec(ConfigError::Checksum))
    );
    let token = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &vec![0; MAX_FILE_BYTES + 1]),
        Err(EditorError::Codec(ConfigError::Length))
    );
    assert_eq!(editor.preview(), &before);
}

#[test]
/// Save success requires acknowledgment; storage failures retain the last saved revision while valid exports remain
/// usable.
fn persistence_requires_success_acknowledgment_and_failed_storage_keeps_exports_usable() {
    let mut editor = Editor::default();
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    let request = editor.begin_save().unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saving);
    assert_eq!(editor.last_saved_revision(), None);
    let revision = editor.revision();
    editor.acknowledge_save(request.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    assert_eq!(editor.last_saved_revision(), Some(revision));
    for failure in [
        StorageFailure::Denied,
        StorageFailure::Quota,
        StorageFailure::Unavailable,
    ] {
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: format!("{failure:?}"),
            })
            .unwrap();
        let request = editor.begin_save().unwrap();
        assert_eq!(
            editor.acknowledge_save(request.token(), Err(failure)),
            Err(EditorError::Storage(failure))
        );
        assert_eq!(editor.save_status(), SaveStatus::Failed(failure));
        assert_eq!(editor.last_saved_revision(), Some(revision));
        assert!(editor.export_json().is_ok());
        assert!(editor.export_binary().is_ok());
    }
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(editor.save_status(), SaveStatus::InvalidDraft);
    assert!(matches!(editor.begin_save(), Err(EditorError::InvalidDraft)));
}

#[test]
/// Stale successful writes cannot mark newer state saved; raw draft admission remains invalid after late completion.
fn late_save_acknowledgments_never_mark_newer_edits_or_requests_saved() {
    let mut editor = Editor::default();
    let old_revision = editor.revision();
    let old = editor.begin_save().unwrap();
    let current = editor.begin_save().unwrap();
    assert_eq!(
        editor.acknowledge_save(old.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Saving);
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New edit".into(),
        })
        .unwrap();
    assert_eq!(
        editor.acknowledge_save(current.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.last_saved_revision(), Some(old_revision));
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    let request = editor.begin_save().unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(
        editor.acknowledge_save(request.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::InvalidDraft);
}

#[test]
/// Saving waits for pointer completion/cancel, then persists final selection; rejected placement additionally requires
/// repair or discard.
fn pointer_selection_save_waits_for_noop_cancel_rejection_or_successful_move() {
    for outcome in 0..4 {
        let mut editor = empty();
        let first = add(&mut editor, 0, 0, Span::OneByOne);
        add(&mut editor, 1, 0, Span::OneByOne);
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: "Pending edit".into(),
            })
            .unwrap();
        let previous = editor.begin_save().unwrap();
        let pointer = editor.begin_pointer(7, PointerTarget::Widget(first)).unwrap();
        assert!(!editor.save_is_current(previous.token()));
        // Snapshot actions wait until the real pointer transaction ends.
        assert!(matches!(editor.begin_save(), Err(EditorError::PointerActive)));
        assert_eq!(editor.active_pointer(), Some(pointer));
        match outcome {
            0 => {
                editor
                    .commit_pointer(pointer, 7, placement(0, 0, Span::OneByOne))
                    .unwrap();
            }
            1 => editor.cancel_pointer(pointer).unwrap(),
            2 => {
                assert!(
                    editor
                        .commit_pointer(pointer, 7, placement(1, 0, Span::OneByOne))
                        .is_err()
                );
            }
            _ => {
                editor
                    .commit_pointer(pointer, 7, placement(2, 1, Span::OneByOne))
                    .unwrap();
            }
        }
        assert_eq!(editor.active_pointer(), None);
        assert!(!editor.save_is_current(previous.token()));
        if outcome == 2 {
            assert_eq!(editor.save_status(), SaveStatus::InvalidDraft);
            assert!(matches!(editor.begin_save(), Err(EditorError::InvalidDraft)));
            editor.discard_raw().unwrap();
        }
        let final_save = editor.begin_save().unwrap();
        assert!(editor.save_is_current(final_save.token()));
        editor.acknowledge_save(final_save.token(), Ok(())).unwrap();
        assert_eq!(editor.save_status(), SaveStatus::Saved);
        let restored = Editor::from_saved_state(final_save.json().as_bytes()).unwrap();
        assert_eq!(restored.preview(), editor.preview());
        assert_eq!(
            restored.selection(),
            Selection {
                view: 1,
                widget: Some(first)
            }
        );
        assert_eq!(restored.preview().config().views[0].name, "Pending edit");
    }
}

#[test]
/// Saved envelopes retain independent selection and boot view, reset history, and reject corrupt
/// identity/schema/length/trailing data.
fn saved_envelope_restores_selection_separately_from_boot_view_and_rejects_corruption() {
    let mut editor = empty();
    editor
        .apply(Edit::AddView {
            name: "Selected".into(),
        })
        .unwrap();
    let selected_view = editor.selection().view;
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let request = editor.begin_save().unwrap();
    let restored = Editor::from_saved_state(request.json().as_bytes()).unwrap();
    assert_eq!(restored.preview(), editor.preview());
    assert_eq!(restored.preview().config().boot_view, 1);
    assert_eq!(
        restored.selection(),
        Selection {
            view: selected_view,
            widget: Some(widget)
        }
    );
    assert_eq!(restored.save_status(), SaveStatus::Saved);
    assert_eq!(restored.history_len(), (0, 0));
    let value: serde_json::Value = serde_json::from_str(request.json()).unwrap();
    for (field, bad) in [
        ("editor_version", 2),
        ("selected_view", 200),
        ("selected_widget", 60000),
    ] {
        let mut corrupt = value.clone();
        corrupt[field] = bad.into();
        assert!(Editor::from_saved_state(serde_json::to_string(&corrupt).unwrap().as_bytes()).is_err());
    }
    for text in [
        request
            .json()
            .replacen("\"editor_version\":1", "\"editor_version\":1,\"editor_version\":1", 1),
        request
            .json()
            .replacen("\"editor_version\":1", "\"editor_version\":1,\"extra\":1", 1),
        format!("{} null", request.json()),
        " ".repeat(MAX_SAVED_BYTES + 1),
    ] {
        assert!(Editor::from_saved_state(text.as_bytes()).is_err());
    }
}

#[test]
/// Field, pointer, import and save handles cannot cross editor instances even when local counters align.
fn replacing_editor_instance_cannot_reuse_tokens_from_an_older_instance() {
    let mut old = Editor::default();
    let mut replacement = Editor::default();
    assert_eq!(
        replacement.set_raw(old.field_token(), RawField::ViewName(1), "Wrong owner".into()),
        Err(EditorError::StaleToken)
    );
    let old_pointer = old.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    let current_pointer = replacement.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    assert_eq!(
        replacement.commit_pointer(old_pointer, 7, placement(0, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(replacement.active_pointer(), Some(current_pointer));
    old.cancel_pointer(old_pointer).unwrap();
    let bytes = old.export_binary().unwrap();
    let old_import = old.begin_import().unwrap();
    let current_import = replacement.begin_import().unwrap();
    assert_eq!(
        replacement.complete_import(old_import, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    replacement
        .complete_import(current_import, ImportFormat::Binary, &bytes)
        .unwrap();
    let mut old = Editor::default();
    let mut replacement = Editor::default();
    let old_save = old.begin_save().unwrap();
    let current_save = replacement.begin_save().unwrap();
    assert_eq!(
        replacement.acknowledge_save(old_save.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(replacement.save_status(), SaveStatus::Saving);
    replacement.acknowledge_save(current_save.token(), Ok(())).unwrap();
}

#[test]
/// Typed inspector callbacks require current selection authority before mutating global settings.
fn typed_inspector_callbacks_must_still_belong_to_the_current_selection() {
    let mut editor = Editor::default();
    let old = editor.field_token();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    assert_eq!(
        editor.apply_if_current(old, Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 })),
        Err(EditorError::StaleToken)
    );
    editor
        .apply_if_current(
            editor.field_token(),
            Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 }),
        )
        .unwrap();
    assert_eq!(editor.preview().stoich(), 14.0);
}

/// Checks JSON, binary, firmware decoder and saved-envelope round trips for `editor`; starts a save and panics on any
/// mismatch or rejected fixture.
fn round_trip(editor: &mut Editor) {
    let json = editor.export_json().unwrap();
    let binary = editor.export_binary().unwrap();
    let mut imported = empty();
    let request = imported.begin_import().unwrap();
    imported
        .complete_import(request, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(imported.preview().config(), editor.preview().config());
    assert_eq!(imported.export_binary().unwrap(), binary);
    let request = imported.begin_import().unwrap();
    imported
        .complete_import(request, ImportFormat::Binary, &binary)
        .unwrap();
    assert_eq!(imported.preview(), editor.preview());
    // This is the same bounded decoder called by the firmware boot path.
    assert_eq!(decode(&binary).unwrap(), *editor.preview());
    let saved = editor.begin_save().unwrap();
    let restored = Editor::from_saved_state(saved.json().as_bytes()).unwrap();
    assert_eq!(restored.preview(), editor.preview());
    assert_eq!(restored.selection(), editor.selection());
}

#[test]
/// Display flag combinations, fuel assumptions, presentation modes and supported spans survive
/// editor/decoder/persistence round trips.
fn every_display_option_fuel_variant_warning_mode_and_span_round_trips() {
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Wide".into() }).unwrap();
    add(&mut editor, 0, 0, Span::TwoByTwo);
    add(&mut editor, 2, 0, Span::TwoByOne);
    add(&mut editor, 2, 1, Span::OneByOne);
    for flags in 0_u8..32 {
        let mut widget = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == 2)
            .unwrap()
            .clone();
        widget.display = DisplayOptions {
            decimals: flags % 4,
            graph: flags & 1 != 0,
            average: flags & 2 != 0,
            extrema: flags & 4 != 0,
            trend: flags & 8 != 0,
            afr: flags & 16 != 0,
            graph_range: if flags % 2 == 0 {
                GraphRange::Auto
            } else {
                GraphRange::Fixed { min: 0.7, max: 1.2 }
            },
        };
        widget.warning.enabled = flags % 2 == 0;
        widget.warning.applicability = if flags % 2 == 0 {
            Applicability::DemoOnly
        } else {
            Applicability::VehicleAndDemo
        };
        widget.warning.message = "Configured lambda".try_into().unwrap();
        widget.advanced.readout = if widget.display.afr {
            Readout::LambdaAndAfr
        } else {
            Readout::Lambda
        };
        editor.apply(Edit::UpdateWidget(widget)).unwrap();
        editor
            .apply(Edit::SetWarningPresentation(WarningPresentation {
                scope: if flags & 1 == 0 {
                    WarningScope::AllViews
                } else {
                    WarningScope::CurrentView
                },
                mode: if flags & 2 == 0 {
                    WarningMode::CycleOne
                } else {
                    WarningMode::ListThree
                },
                page_ms: 500 + u16::from(flags) * 250,
            }))
            .unwrap();
        let mut blend = Blend::illustrative();
        blend.percent = 35.5;
        let fuel = match flags % 5 {
            0 => Fuel::EquivalentGasoline,
            1 => Fuel::Stoichiometric { ratio: 14.0 },
            2 => {
                blend.basis = BlendBasis::Mass;
                Fuel::EthanolBlend(blend)
            }
            3 => Fuel::EthanolBlend(blend),
            _ => {
                blend.composition_origin = CompositionOrigin::Measured;
                blend.density_reference = DensityReference::CommonCelsius(20.0);
                Fuel::EthanolBlend(blend)
            }
        };
        editor.apply(Edit::SetFuel(fuel)).unwrap();
        round_trip(&mut editor);
    }
}

#[test]
/// Every catalogue definition and supported route survives serialization with valid polling choices; this does not
/// qualify ECU access.
fn every_catalogue_source_and_route_survives_editor_and_firmware_round_trips() {
    let mut editor = Editor::default();
    for id in DEFINITIONS {
        let entry = definition(id);
        let channel = entry.channel.unwrap_or(Channel::Oil);
        for route in [
            Route::Unknown,
            Route::DeploymentProfile,
            Route::Physical11 {
                request: 0x7E0,
                response: 0x7E8,
            },
        ] {
            let mut source = entry.source(channel);
            source.route = route;
            source.freshness_ms = 1;
            for poll in [Poll::Off, Poll::EveryMs(10000)] {
                if id == DefinitionId::Disabled && poll != Poll::Off {
                    continue;
                }
                source.poll = poll;
                editor.apply(Edit::SetSource { channel, source }).unwrap();
                round_trip(&mut editor);
            }
        }
    }
}

#[test]
/// Representative widget, fuel, source and global raw fields update their exact canonical targets and units.
fn all_raw_inspector_targets_apply_to_the_correct_field() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let mut widget = editor.preview().config().widgets[4].clone();
    widget.display.graph_range = GraphRange::Fixed {
        min: -100.0,
        max: 2000.0,
    };
    editor.apply(Edit::UpdateWidget(widget)).unwrap();
    for (field, text) in [
        (WidgetField::Label, "Engine oil"),
        (WidgetField::Decimals, "2"),
        (WidgetField::GraphMinimum, "-50"),
        (WidgetField::GraphMaximum, "300"),
        (WidgetField::WarningMessage, "Oil warning"),
        (WidgetField::WarningTrigger, "140"),
        (WidgetField::WarningClear, "120"),
        (WidgetField::WarningLoad, "55.5"),
        (WidgetField::WarningOn, "1500"),
        (WidgetField::WarningOff, "900"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Widget(5, field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let widget = &editor.preview().config().widgets[4];
    assert_eq!(widget.label.as_str(), "Engine oil");
    assert_eq!(widget.display.decimals, 2);
    assert_eq!(widget.display.graph_range, GraphRange::Fixed { min: -50.0, max: 300.0 });
    assert_eq!(widget.warning.message.as_str(), "Oil warning");
    assert_eq!(
        (
            widget.warning.trigger,
            widget.warning.clear,
            widget.warning.min_load_percent
        ),
        (140.0, 120.0, 55.5)
    );
    assert_eq!((widget.warning.on_ms, widget.warning.off_ms), (1500, 900));
    let mut blend = Blend::illustrative();
    blend.density_reference = DensityReference::CommonCelsius(20.0);
    editor.apply(Edit::SetFuel(Fuel::EthanolBlend(blend))).unwrap();
    for (field, text) in [
        (FuelField::Percent, "30"),
        (FuelField::GasolineRatio, "14.1"),
        (FuelField::EthanolRatio, "9.1"),
        (FuelField::GasolineDensity, "0.74"),
        (FuelField::EthanolDensity, "0.79"),
        (FuelField::ReferenceCelsius, "21"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Fuel(field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let Fuel::EthanolBlend(blend) = editor.preview().config().fuel else {
        panic!("blend lost");
    };
    assert_eq!(
        (blend.percent, blend.gasoline_stoich, blend.ethanol_stoich),
        (30.0, 14.1, 9.1)
    );
    assert_eq!((blend.gasoline_density, blend.ethanol_density), (0.74, 0.79));
    assert_eq!(blend.density_reference, DensityReference::CommonCelsius(21.0));
    let mut source = editor.preview().config().sources[Channel::Oil.index()];
    source.route = Route::Physical11 {
        request: 0x7E0,
        response: 0x7E8,
    };
    editor
        .apply(Edit::SetSource {
            channel: Channel::Oil,
            source,
        })
        .unwrap();
    for (field, text) in [
        (SourceField::Request, "0x7e1"),
        (SourceField::Response, "0X7e9"),
        (SourceField::PollMilliseconds, "2500"),
        (SourceField::FreshnessMilliseconds, "1500"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Source(Channel::Oil, field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let source = editor.preview().config().sources[Channel::Oil.index()];
    assert_eq!(
        source.route,
        Route::Physical11 {
            request: 0x7E1,
            response: 0x7E9
        }
    );
    assert_eq!(source.poll, Poll::EveryMs(2500));
    assert_eq!(source.freshness_ms, 1500);
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "2500".into())
        .unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "Edited".into())
        .unwrap();
    assert_eq!(editor.preview().config().warnings.page_ms, 2500);
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Edited");
}

#[test]
/// Cross-view movement selects the moved widget, and ordering/movement undo restores prior placement and selection.
fn widget_order_and_cross_view_moves_keep_selection_and_undo_coherent() {
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Other".into() }).unwrap();
    let other = editor.selection().view;
    editor
        .apply(Edit::MoveWidget {
            id: 5,
            view: other,
            column: 0,
            row: 0,
        })
        .unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: other,
            widget: Some(5)
        }
    );
    editor.apply(Edit::ReorderWidget { id: 5, index: 0 }).unwrap();
    assert_eq!(editor.preview().config().widgets[0].id, 5);
    assert_eq!(editor.preview().config().widgets[1].id, 1);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets[4].id, 5);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets[4].view, 1);
    assert_eq!(
        editor.selection(),
        Selection {
            view: other,
            widget: None
        }
    );
}

#[test]
/// Deleting another view repairs a boot reference while retaining a surviving selected widget.
fn deleting_an_unselected_view_preserves_a_surviving_widget_selection() {
    let mut editor = Editor::default();
    editor
        .apply(Edit::AddView {
            name: "Remove me".into(),
        })
        .unwrap();
    let removed = editor.selection().view;
    editor.apply(Edit::SetBootView(removed)).unwrap();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    editor.apply(Edit::DeleteView(removed)).unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: 1,
            widget: Some(5)
        }
    );
    assert_eq!(editor.preview().config().boot_view, 1);
}

#[test]
/// An acknowledged older write makes newer content visibly dirty; successful retry restores saved status and duplicate
/// acknowledgment is harmless.
fn an_actual_older_write_after_a_newer_success_cannot_leave_a_false_saved_indicator() {
    let mut editor = Editor::default();
    let old_revision = editor.revision();
    let old = editor.begin_save().unwrap();
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "Newest".into(),
        })
        .unwrap();
    let current = editor.begin_save().unwrap();
    editor.acknowledge_save(current.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    // The acknowledgment means the old bytes were actually written, not just
    // that a stale timer woke up. Current content must stay visibly unsaved.
    assert_eq!(
        editor.acknowledge_save(old.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    assert_eq!(editor.last_saved_revision(), Some(old_revision));
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Newest");
    let retry = editor.begin_save().unwrap();
    editor.acknowledge_save(retry.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    assert_eq!(
        editor.acknowledge_save(retry.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Saved);
}

#[test]
/// Prepared-save authority expires on newer save requests or raw edits, allowing adapters to reject stale writes before
/// storage access.
fn storage_adapter_can_check_a_prepared_write_before_touching_storage() {
    let mut editor = Editor::default();
    let old = editor.begin_save().unwrap();
    assert!(editor.save_is_current(old.token()));
    let new = editor.begin_save().unwrap();
    assert!(!editor.save_is_current(old.token()));
    assert!(editor.save_is_current(new.token()));
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert!(!editor.save_is_current(new.token()));
}

#[test]
/// Binary import retains the negative-zero fuel payload byte-for-byte instead of treating numeric equality as no
/// change.
fn importing_signed_zero_preserves_its_canonical_payload_instead_of_treating_it_as_a_noop() {
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend::illustrative());
    let mut editor = Editor::new(config.clone()).unwrap();
    let Fuel::EthanolBlend(blend) = &mut config.fuel else {
        unreachable!()
    };
    blend.percent = -0.0;
    let mut buffer = vec![0; MAX_FILE_BYTES];
    let validated = validate(config).unwrap();
    let bytes = encode(&validated, &mut buffer).unwrap();
    let request = editor.begin_import().unwrap();
    editor.complete_import(request, ImportFormat::Binary, bytes).unwrap();
    let Fuel::EthanolBlend(blend) = editor.preview().config().fuel else {
        unreachable!()
    };
    assert_eq!(blend.percent.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(editor.export_binary().unwrap(), bytes);
}

#[test]
/// Boundary decimal rounding and signed zero yield identical f32 bits and binary payloads through raw, JSON and
/// saved-state paths.
fn decimal_raw_json_and_saved_inputs_preserve_the_same_device_value() {
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: 0.0,
        ..Blend::illustrative()
    });
    let mut initial = Editor::new(config.clone()).unwrap();
    let json_template = initial.export_json().unwrap();
    let saved_template = initial.begin_save().unwrap().json().to_owned();
    assert!(json_template.contains("\"percent\":0.0"));
    assert!(saved_template.contains("\"percent\":0.0"));
    for (text, expected_bits) in [
        ("1.0000000596046448", 0x3F80_0001),
        ("1.0000000596046447", 0x3F80_0000),
        ("1.000000059604644775390625", 0x3F80_0000),
        ("1.0000001788139343", 0x3F80_0001),
        ("0.0", 0x0000_0000),
        ("-0.0", 0x8000_0000),
        ("12.75", 0x414C_0000),
    ] {
        let mut typed = Editor::new(config.clone()).unwrap();
        typed
            .set_raw(typed.field_token(), RawField::Fuel(FuelField::Percent), text.into())
            .unwrap();
        assert!(!typed.has_invalid_draft());
        let Fuel::EthanolBlend(blend) = typed.preview().config().fuel else {
            unreachable!()
        };
        assert_eq!(blend.percent.to_bits(), expected_bits, "raw decimal {text}");
        let binary = typed.export_binary().unwrap();

        let json = json_template.replacen("\"percent\":0.0", &format!("\"percent\":{text}"), 1);
        let mut imported = Editor::new(config.clone()).unwrap();
        let request = imported.begin_import().unwrap();
        imported
            .complete_import(request, ImportFormat::Json, json.as_bytes())
            .unwrap();
        assert_eq!(imported.export_binary().unwrap(), binary, "JSON decimal {text}");

        let saved = saved_template.replacen("\"percent\":0.0", &format!("\"percent\":{text}"), 1);
        let restored = Editor::from_saved_state(saved.as_bytes()).unwrap();
        assert_eq!(restored.export_binary().unwrap(), binary, "saved decimal {text}");
        round_trip(&mut typed);
    }
}

#[test]
/// Raw and JSON decimal parsing reject endpoints that round to the same f32, preserving accepted bytes and history.
fn decimal_raw_and_json_graph_bounds_reject_the_same_equal_endpoints() {
    let mut config = default_config();
    let id = config.widgets[0].id;
    config.widgets[0].display.graph_range = GraphRange::Fixed {
        min: 0.0,
        max: f32::from_bits(0x3F80_0001),
    };
    let mut typed = Editor::new(config.clone()).unwrap();
    typed
        .select(Selection {
            view: config.widgets[0].view,
            widget: Some(id),
        })
        .unwrap();
    typed
        .set_raw(
            typed.field_token(),
            RawField::Widget(id, WidgetField::GraphMinimum),
            "1.0000000596046448".into(),
        )
        .unwrap();
    let Some(EditorError::Validation(expected)) = typed.draft_error() else {
        panic!("equal graph endpoints must remain an invalid raw draft")
    };
    assert_eq!(expected.field, Field::GraphRange);
    assert_eq!(expected.code, ErrorCode::Bounds);
    assert_eq!(typed.preview().config(), &config);

    let mut imported = Editor::new(config).unwrap();
    let before = imported.export_binary().unwrap();
    let revision = imported.revision();
    let template = imported.export_json().unwrap();
    assert!(template.contains("\"min\":0.0"));
    let json = template.replacen("\"min\":0.0", "\"min\":1.0000000596046448", 1);
    let request = imported.begin_import().unwrap();
    assert_eq!(
        imported.complete_import(request, ImportFormat::Json, json.as_bytes()),
        Err(EditorError::Codec(ConfigError::Validation(expected)))
    );
    assert_eq!(imported.export_binary().unwrap(), before);
    assert_eq!(imported.revision(), revision);
    assert_eq!(imported.history_len(), (0, 0));
}

#[test]
/// Colour repair preserves rejected text, retires stale callbacks, quantizes to RGB565 and round-trips one reversible
/// accepted change.
fn colour_raw_edits_are_repairable_atomic_quantized_and_retire_callbacks() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    let original = editor.export_binary().unwrap();
    let field = RawField::Widget(1, WidgetField::Color(ColorRole::Value));
    let old = editor.field_token();
    editor.set_raw(old, field, "#000000".into()).unwrap();
    assert!(editor.has_invalid_draft());
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
    assert_eq!(editor.preview().config().widgets[0].colors.value, Color(65535));
    assert_eq!(editor.diagnostic().unwrap().config.widgets[0].colors.value, Color(0));
    assert!(matches!(
        editor.set_raw(old, field, "#FFFFFF".into()),
        Err(EditorError::StaleToken)
    ));
    for bad in ["#FFF", "#GG0000", "#00000000", "#000000\n"] {
        editor.set_raw(editor.field_token(), field, bad.into()).unwrap();
        assert!(editor.has_invalid_draft());
    }
    editor.set_raw(editor.field_token(), field, "#11FF99".into()).unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(
        editor.preview().config().widgets[0].colors.value.hex().as_str(),
        "#10FF9C"
    );
    let exported = editor.export_binary().unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), original);
    editor.redo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), exported);
    let saved = editor.begin_save().unwrap().json().to_owned();
    assert_eq!(
        Editor::from_saved_state(saved.as_bytes())
            .unwrap()
            .export_binary()
            .unwrap(),
        exported
    );
    let global = RawField::WarningColor { foreground: true };
    editor.set_raw(editor.field_token(), global, "#FF0000".into()).unwrap();
    assert!(editor.has_invalid_draft());
    editor
        .set_raw(
            editor.field_token(),
            RawField::WarningColor { foreground: false },
            "#000000".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft(), "repair pair atomically");
}

#[test]
/// A light palette can be repaired across all contrast-dependent roles without discarding the draft, then reused by
/// every gauge kind.
fn light_background_can_be_repaired_across_all_roles_without_discarding_edits() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    let field = |role| RawField::Widget(1, WidgetField::Color(role));
    editor
        .set_raw(editor.field_token(), field(ColorRole::Background), "#FFFFFF".into())
        .unwrap();
    assert!(editor.has_invalid_draft());
    for role in [
        ColorRole::Label,
        ColorRole::Value,
        ColorRole::Secondary,
        ColorRole::Graph,
        ColorRole::Gauge,
        ColorRole::Ticks,
        ColorRole::Average,
        ColorRole::Minimum,
        ColorRole::Maximum,
        ColorRole::Trend,
        ColorRole::Peak,
    ] {
        editor
            .set_raw(editor.field_token(), field(role), "#000000".into())
            .unwrap();
    }
    // Last remaining failure is black indicator against the stored dark track.
    assert!(editor.has_invalid_draft());
    editor
        .set_raw(editor.field_token(), field(ColorRole::Track), "#FFFFFF".into())
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[0].colors.background, Color(65535));
    for colour in ["#080808", "#101010", "#181818", "#202020"] {
        editor
            .set_raw(editor.field_token(), field(ColorRole::Value), colour.into())
            .unwrap();
        assert!(
            !editor.has_invalid_draft(),
            "successive picker revisions must stay usable"
        );
    }
    for kind in GaugeKind::ALL {
        let mut widget = editor.preview().config().widgets[0].clone();
        widget.gauge.kind = kind;
        editor.apply(Edit::UpdateWidget(widget)).unwrap();
    }
}

#[test]
/// Frozen v2 saved state supplies current control defaults while preserving gauge limits, palette and selection.
fn saved_v2_migrates_controls_and_preserves_gauge_colours_and_selection() {
    let mut config: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../crates/dashboard-config/tests/fixtures/v2-default.json"
    ))
    .unwrap();
    config["widgets"][0]["gauge"]["max"] = 4.0.into();
    let saved = serde_json::json!({"editor_version":1,"selected_view":1,"selected_widget":1,"config":config});
    let editor = Editor::from_saved_state(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(editor.preview().config().version, VERSION);
    assert_eq!(editor.preview().config().controls, ControlMap::default());
    assert_eq!(editor.selection().widget, Some(1));
    assert_eq!(editor.preview().config().widgets[0].gauge.max, 4.0);
    assert_eq!(editor.preview().config().widgets[0].colors, WidgetColors::default());
    let exported = editor.export_binary().unwrap();
    assert_eq!(decode(&exported).unwrap().config(), editor.preview().config());
}

#[test]
/// Advanced appearance, warnings, controls and history use ordinary current-schema save/export paths; the frozen v3
/// envelope still migrates.
fn v4_full_schema_editor_save_export_and_restore_use_normal_apis() {
    let mut c = default_config();
    c.history_seconds = [900; 8];
    c.reduced_motion = true;
    c.led.normal.kind = LedPatternKind::Off;
    c.led.on_debug = true;
    c.led.backlight_off = true;
    c.widgets[0].advanced.readout = Readout::Psi;
    c.widgets[0].advanced.secondary = Secondary::Kpa;
    c.widgets[0].advanced.smoothing_ms = 750;
    c.widgets[0].advanced.peak_mode = PeakSelection::Minimum;
    c.widgets[0].advanced.reference_mode = ReferenceMode::Manual;
    c.widgets[0].advanced.reference = 1.25;
    c.widgets[0]
        .advanced
        .set_bands(&[
            Band::new(-1.0, 0.0, Color(65535), "LOW").unwrap(),
            Band::new(0.0, 2.0, Color(65535), "MID").unwrap(),
            Band::new(2.0, 3.0, Color(65535), "HIGH").unwrap(),
        ])
        .unwrap();
    c.widgets[0].warning.mode = LimitMode::OutsideWindow;
    c.widgets[0].warning.trigger = -1.0;
    c.widgets[0].warning.clear = 0.0;
    c.widgets[0].warning.advanced.trigger_high = 3.0;
    c.widgets[0].warning.advanced.clear_high = 2.0;
    c.widgets[3].warning.advanced.latch = true;
    c.widgets[3].warning.advanced.snooze_s = 900;
    c.widgets[3].warning.advanced.priority = Priority::Critical;
    c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let mut editor = Editor::new(c.clone()).unwrap();
    let json = editor.export_json().unwrap();
    assert!(!json.contains('\n'));
    assert_eq!(from_json_bytes(json.as_bytes()).unwrap().config(), &c);
    let binary = editor.export_binary().unwrap();
    assert_eq!(decode(&binary).unwrap().config(), &c);
    let request = editor.begin_save().unwrap();
    assert!(request.json().len() <= MAX_SAVED_BYTES);
    let restored = Editor::from_saved_state(request.json().as_bytes()).unwrap();
    assert_eq!(restored.preview().config(), &c);
    let legacy = serde_json::json!({"editor_version":1,"selected_view":1,"selected_widget":1,"config":serde_json::from_slice::<serde_json::Value>(include_bytes!("../../crates/dashboard-config/tests/fixtures/v3-default.json")).unwrap()});
    assert_eq!(
        Editor::from_saved_state(&serde_json::to_vec(&legacy).unwrap())
            .unwrap()
            .preview()
            .config(),
        &default_config()
    );
}
