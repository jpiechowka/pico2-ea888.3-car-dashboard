//! Grouped ignition editing, repair and synthetic acquisition regressions.
use dashboard_config_tools::project::ExportMode;
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::field_registry::{FieldKey, Registry, TargetOwner};
use dashboard_configurator::preview::{Preview, PreviewError, PreviewOptions, SampleSet};
use dashboard_configurator::simulator::{ControlSimulator, Scenario};
use dashboard_core::ignition::Cylinder;
use dashboard_core::samples::{DataMode, Quality};

/// Returns an editor with an empty first view, keeping normal source and control defaults.
fn empty_editor() -> Editor {
    let mut editor = Editor::default();
    let ids: Vec<_> = editor.preview().config().widgets.iter().map(|w| w.id).collect();
    for id in ids {
        editor.apply(Edit::DeleteWidget(id)).unwrap();
    }
    editor
}

/// Adds a default group to `editor`'s first view and returns its fresh widget ID.
/// Panics if the fixture cannot admit the same reducer edit used by the library button.
fn add_group(editor: &mut Editor) -> u16 {
    editor
        .apply(Edit::AddWidget {
            view: 1,
            channel: WidgetChannel::IgnitionRetard,
            placement: Placement {
                column: 0,
                row: 0,
                span: Span::OneByOne,
            },
        })
        .unwrap();
    editor.selection().widget.unwrap()
}

#[test]
/// Group defaults, all modes and precision survive real edit history and persisted import.
fn grouped_modes_roundtrip_and_undo_preserve_all_cylinders() {
    let mut editor = empty_editor();
    let id = add_group(&mut editor);
    let widget = &editor.preview().config().widgets[0];
    assert_eq!(widget.channel, WidgetChannel::IgnitionRetard);
    assert!(widget.warning.enabled);
    assert_eq!(widget.warning.message, "Ignition retarded");
    assert_eq!(widget.warning.trigger, 0.0);
    assert_eq!(widget.warning.clear, 0.0);
    assert!(!widget.display.graph);
    assert_eq!(widget.advanced.readout, Readout::IgnitionStatusAndNumber);
    for mode in [
        Readout::IgnitionStatus,
        Readout::IgnitionStatusAndNumber,
        Readout::IgnitionNumeric,
    ] {
        let before = editor.preview().config().widgets[0].clone();
        let mut widget = before.clone();
        widget.advanced.readout = mode;
        widget.display.decimals = 3;
        editor.apply(Edit::UpdateWidget(widget.clone())).unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.preview().config().widgets[0], before);
        editor.redo().unwrap();
        assert_eq!(editor.preview().config().widgets[0], widget);
        let bytes = editor.export_json().unwrap();
        let import = editor.begin_import().unwrap();
        editor
            .complete_import(import, ImportFormat::Json, bytes.as_bytes())
            .unwrap();
        assert_eq!(editor.preview().config().widgets[0], widget);
    }
    assert_eq!(editor.preview().config().widgets[0].id, id);
    assert_eq!(editor.preview().config().sources.len(), 8);
}

#[test]
/// Rejected scalar graph/smoothing options remain editable and block exports until repaired.
fn grouped_scalar_options_remain_repairable_without_losing_mode() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let mut widget = editor.preview().config().widgets[0].clone();
    widget.advanced.readout = Readout::IgnitionStatusAndNumber;
    editor.apply(Edit::UpdateWidget(widget.clone())).unwrap();
    widget.display.graph = true;
    widget.advanced.smoothing_ms = 250;
    assert!(editor.apply(Edit::UpdateWidget(widget)).is_err());
    assert!(editor.export_binary().is_err());
    assert!(editor.editing_config().widgets[0].display.graph);
    assert_eq!(editor.editing_config().widgets[0].advanced.smoothing_ms, 250);
    assert_eq!(Registry::validation_key(Field::Graph), Some(FieldKey::Graph));
    assert_eq!(Registry::validation_key(Field::Smoothing), Some(FieldKey::Smoothing));
    let mut repair = editor.editing_config().widgets[0].clone();
    repair.display.graph = false;
    repair.advanced.smoothing_ms = 0;
    editor.apply(Edit::UpdateWidget(repair)).unwrap();
    assert!(editor.export_binary().is_ok());
    assert_eq!(
        editor.preview().config().widgets[0].advanced.readout,
        Readout::IgnitionStatusAndNumber
    );
}

#[test]
/// Shared warning threshold/message and global versus cylinder lines survive edits, undo and JSON import.
fn ignition_warning_settings_and_presentation_roundtrip_without_scalar_leakage() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let before = editor.preview().config().widgets[0].clone();
    let mut widget = before.clone();
    widget.advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
    widget.warning.trigger = -2.0;
    widget.warning.clear = -1.0;
    widget.warning.message = "Check ignition".try_into().unwrap();
    editor.apply(Edit::UpdateWidget(widget.clone())).unwrap();
    assert_eq!(
        Registry::validation_key(Field::IgnitionWarningPresentation),
        Some(FieldKey::IgnitionWarningPresentation)
    );
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets[0], before);
    editor.redo().unwrap();
    let text = editor.export_json().unwrap();
    let import = editor.begin_import().unwrap();
    editor
        .complete_import(import, ImportFormat::Json, text.as_bytes())
        .unwrap();
    assert_eq!(editor.preview().config().widgets[0], widget);
    let bytes = editor.export_project(ExportMode::Complete).unwrap();
    let import = editor.begin_import().unwrap();
    editor.complete_project_import(import, &bytes).unwrap();
    assert_eq!(editor.preview().config().widgets[0], widget);
    assert_eq!(editor.preview().config().sources.len(), 8);
}

#[test]
/// Pointer insertion uses grouped defaults and one undo removes the exact newly allocated widget.
fn group_library_pointer_placement_is_undoable_and_uses_shared_defaults() {
    let mut editor = empty_editor();
    let pointer = editor
        .begin_pointer(17, PointerTarget::Library(WidgetChannel::IgnitionRetard))
        .unwrap();
    editor
        .commit_pointer(
            pointer,
            17,
            Placement {
                column: 1,
                row: 1,
                span: Span::OneByOne,
            },
        )
        .unwrap();
    let widget = &editor.preview().config().widgets[0];
    assert_eq!(widget.channel, WidgetChannel::IgnitionRetard);
    assert_eq!(widget.placement.column, 1);
    assert_eq!(widget.warning.message, "Ignition retarded");
    editor.undo().unwrap();
    assert!(editor.preview().config().widgets.is_empty());
    editor.redo().unwrap();
    assert_eq!(
        editor.preview().config().widgets[0].channel,
        WidgetChannel::IgnitionRetard
    );
}

#[test]
/// Group layout uses normal resize, duplication and deletion history for every mounting.
fn grouped_layout_history_preserves_widget_identity() {
    for orientation in [
        Orientation::Landscape,
        Orientation::LandscapeInverted,
        Orientation::Portrait,
        Orientation::PortraitInverted,
    ] {
        let mut editor = empty_editor();
        editor.apply(Edit::SetOrientation(orientation)).unwrap();
        let id = add_group(&mut editor);
        for span in [Span::TwoByOne, Span::TwoByTwo, Span::OneByOne] {
            editor.apply(Edit::ResizeWidget { id, span }).unwrap();
            assert_eq!(editor.preview().config().widgets[0].placement.span, span);
        }
        editor
            .apply(Edit::DuplicateWidget {
                id,
                view: 1,
                placement: Placement {
                    column: 1,
                    row: 0,
                    span: Span::OneByOne,
                },
            })
            .unwrap();
        let duplicate = editor.selection().widget.unwrap();
        assert_ne!(duplicate, id);
        editor.apply(Edit::DeleteWidget(duplicate)).unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.preview().config().widgets.len(), 2);
        assert!(
            editor
                .preview()
                .config()
                .widgets
                .iter()
                .all(|w| w.channel == WidgetChannel::IgnitionRetard)
        );
    }
}

#[test]
/// Live samples populate every cylinder, and timer-only advancement lets them expire independently of scalar UI.
fn live_demo_populates_group_and_silence_expires_it() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let mut simulator = ControlSimulator::new(editor.preview().clone());
    let now = simulator.now_ms();
    simulator.advance_to(now, Some(dashboard_core::demo::values(now)));
    let snapshot = simulator.snapshot();
    let values = Cylinder::ALL.map(|c| snapshot.ignition.value_at(c, DataMode::Demo, now).unwrap());
    assert!(values.iter().any(|v| *v < 0.0));
    assert!(values.windows(2).any(|v| v[0] != v[1]));
    simulator.advance_to(now + 501, None);
    for cylinder in Cylinder::ALL {
        assert_eq!(
            simulator.snapshot().ignition.quality_at(cylinder, now + 501),
            Quality::Stale
        );
        assert_eq!(
            simulator
                .snapshot()
                .ignition
                .value_at(cylinder, DataMode::Demo, now + 501),
            None
        );
    }
}

#[test]
/// Appearance reconstruction admits the final cylinder sample after the replay window, avoiding stale placeholders.
fn appearance_final_sample_keeps_every_cylinder_fresh() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let mut preview = Preview::new();
    preview
        .render_appearance(editor.preview(), 1, PreviewOptions::default())
        .unwrap();
    let snapshot = preview.snapshot().unwrap();
    for cylinder in Cylinder::ALL {
        assert_eq!(
            snapshot
                .ignition
                .quality_at(cylinder, preview.rendered_at_ms().unwrap()),
            Quality::Valid
        );
        assert!(
            snapshot
                .ignition
                .value_at(cylinder, DataMode::Demo, preview.rendered_at_ms().unwrap())
                .is_some()
        );
    }
}

/// The browser's portrait extreme case paints complete numbers and must not inherit scalar lane warnings.
#[test]
fn portrait_ignition_extremes_fit_actual_pixels_and_have_no_scalar_advisory() {
    use embedded_graphics::mono_font::MonoTextStyle;
    use embedded_graphics::mono_font::iso_8859_1::FONT_6X10;
    use embedded_graphics::prelude::*;
    use embedded_graphics::text::{Baseline, Text};
    let mut raw = default_config();
    raw.orientation = Orientation::Portrait;
    raw.widgets.clear();
    let mut widget = default_ignition_widget(
        9,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        },
    );
    widget.display.decimals = 3;
    widget.warning.enabled = false;
    raw.widgets.push(widget).unwrap();
    let config = validate(raw).unwrap();
    let mut preview = Preview::new();
    preview
        .render_appearance(
            &config,
            1,
            PreviewOptions {
                ignition_values: SampleSet::WidthStress.ignition_values(),
                ..PreviewOptions::default()
            },
        )
        .unwrap();
    // Check actual glyph pixels independently of the fit-advisory implementation.
    for number in ["-327.680°", "327.670°"] {
        let mut glyphs = embedded_graphics::mock_display::MockDisplay::new();
        Text::with_baseline(
            number,
            Point::zero(),
            MonoTextStyle::new(&FONT_6X10, embedded_graphics::pixelcolor::BinaryColor::On),
            Baseline::Top,
        )
        .draw(&mut glyphs)
        .unwrap();
        let points: Vec<_> = glyphs
            .affected_area()
            .points()
            .filter(|point| glyphs.get_pixel(*point) == Some(embedded_graphics::pixelcolor::BinaryColor::On))
            .collect();
        assert!(!points.is_empty());
        let pixels = preview.rgba();
        assert!(
            (26..90usize).any(|y| {
                (0..120usize).any(|x| {
                    points.iter().all(|point| {
                        let offset = ((y + point.y as usize) * 240 + x + point.x as usize) * 4;
                        pixels[offset..offset + 3] == [255, 255, 255]
                    })
                })
            }),
            "complete number {number} must appear in actual raster"
        );
    }
    assert!(
        preview.fit_issues().is_empty(),
        "group must not use scalar lanes: {:?}",
        preview.fit_issues()
    );
}

/// Every admitted group geometry/mode/precision keeps its own rendering policy, including reserved warning badges.
#[test]
fn ignition_fit_advisory_matrix_preserves_scalar_slot_indices() {
    let mut preview = Preview::new();
    for orientation in Orientation::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for readout in [
                Readout::IgnitionNumeric,
                Readout::IgnitionStatus,
                Readout::IgnitionStatusAndNumber,
            ] {
                for decimals in 0..=3 {
                    for warnings in [false, true] {
                        let mut raw = default_config();
                        raw.orientation = orientation;
                        raw.widgets.clear();
                        let mut widget = default_ignition_widget(
                            9,
                            1,
                            Placement {
                                column: 0,
                                row: 0,
                                span,
                            },
                        );
                        widget.advanced.readout = readout;
                        widget.display.decimals = decimals;
                        widget.warning.enabled = warnings;
                        raw.widgets.push(widget).unwrap();
                        let config = validate(raw).unwrap();
                        preview
                            .render_appearance(
                                &config,
                                1,
                                PreviewOptions {
                                    warnings,
                                    ignition_values: SampleSet::WidthStress.ignition_values(),
                                    ..PreviewOptions::default()
                                },
                            )
                            .unwrap();
                        assert!(
                            preview.fit_issues().is_empty(),
                            "{orientation:?}/{span:?}/{readout:?}/{decimals}/{warnings}"
                        );
                        let snapshot = preview.snapshot().unwrap();
                        assert_eq!(
                            snapshot.ignition.value_at(
                                Cylinder::One,
                                DataMode::Demo,
                                preview.rendered_at_ms().unwrap()
                            ),
                            Some(-327.68)
                        );
                    }
                }
            }
        }
    }
    let mut raw = default_config();
    raw.orientation = Orientation::Portrait;
    raw.widgets.clear();
    raw.widgets
        .push(default_widget(
            Channel::Lambda,
            8,
            1,
            Placement {
                column: 1,
                row: 0,
                span: Span::OneByOne,
            },
        ))
        .unwrap();
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].display.average = true;
    raw.widgets[0].display.extrema = true;
    let options = PreviewOptions {
        values: SampleSet::WidthStress.values(),
        ..PreviewOptions::default()
    };
    preview
        .render_appearance(&validate(raw.clone()).unwrap(), 1, options)
        .unwrap();
    let scalar_issues = preview.fit_issues().to_vec();
    assert_eq!(scalar_issues, [8], "fixture must exercise a real scalar fit advisory");
    raw.widgets
        .insert(
            0,
            default_ignition_widget(
                9,
                1,
                Placement {
                    column: 0,
                    row: 0,
                    span: Span::OneByOne,
                },
            ),
        )
        .unwrap();
    preview.render_appearance(&validate(raw).unwrap(), 1, options).unwrap();
    assert_eq!(preview.fit_issues(), scalar_issues);
}

#[test]
/// Mode search/navigation owns the group widget and rejects callbacks after a subsequent edit.
fn group_mode_registry_targets_and_stale_edits_keep_revision_authority() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let target = Registry::target(&editor, FieldKey::Readout, TargetOwner::Widget(0)).unwrap();
    assert!(Registry::resolve(&editor, target).unwrap().capability.is_active());
    for key in [FieldKey::Graph, FieldKey::Smoothing, FieldKey::GaugeStyle] {
        let target = Registry::target(&editor, key, TargetOwner::Widget(0)).unwrap();
        assert!(!Registry::resolve(&editor, target).unwrap().capability.is_active());
    }
    let token = editor.field_token();
    let mut widget = editor.preview().config().widgets[0].clone();
    widget.advanced.readout = Readout::IgnitionStatus;
    editor.apply(Edit::UpdateWidget(widget.clone())).unwrap();
    widget.advanced.readout = Readout::IgnitionStatusAndNumber;
    assert_eq!(
        editor.apply_if_current(token, Edit::UpdateWidget(widget)),
        Err(EditorError::StaleToken)
    );
    assert!(Registry::resolve(&editor, target).is_err());
    assert_eq!(
        editor.preview().config().widgets[0].advanced.readout,
        Readout::IgnitionStatus
    );
}

#[test]
/// Typed scenario paths seed independent cylinders too; faulting cylinder two never erases its siblings.
fn nonnormal_demo_scenarios_preserve_independent_cylinder_quality() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let mut simulator = ControlSimulator::new(editor.preview().clone());
    simulator.set_scenario(Scenario::MinMax).unwrap();
    let now = simulator.now_ms();
    simulator.scenario_step(now).unwrap();
    for cylinder in Cylinder::ALL {
        assert_eq!(
            simulator.snapshot().ignition.value_at(cylinder, DataMode::Demo, now),
            Some(-327.68)
        );
    }
    simulator.set_scenario(Scenario::SourceFault).unwrap();
    let now = simulator.now_ms();
    simulator.scenario_step(now).unwrap();
    simulator.scenario_step(now + 1).unwrap();
    assert_eq!(
        simulator.snapshot().ignition.quality_at(Cylinder::Two, now + 1),
        Quality::TransportError
    );
    for cylinder in [Cylinder::One, Cylinder::Three, Cylinder::Four] {
        assert_eq!(
            simulator.snapshot().ignition.quality_at(cylinder, now + 1),
            Quality::Valid
        );
    }
    simulator.set_scenario(Scenario::Missing).unwrap();
    let now = simulator.now_ms();
    simulator.scenario_step(now).unwrap();
    for cylinder in Cylinder::ALL {
        assert_eq!(
            simulator.snapshot().ignition.value_at(cylinder, DataMode::Demo, now),
            None
        );
    }
}

#[test]
/// An invalid group still produces a bounded diagnostic scene while accepted bytes remain unchanged.
fn invalid_group_projection_retains_raw_settings_and_inspectable_pixels() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let accepted = editor.export_binary().unwrap();
    let mut widget = editor.preview().config().widgets[0].clone();
    widget.display.graph = true;
    widget.advanced.readout = Readout::Psi;
    assert!(editor.apply(Edit::UpdateWidget(widget)).is_err());
    let draft = editor.diagnostic().unwrap();
    let mut preview = Preview::new();
    preview
        .render_diagnostic(editor.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    assert!(editor.editing_config().widgets[0].display.graph);
    assert_eq!(editor.editing_config().widgets[0].advanced.readout, Readout::Psi);
    editor.discard_raw().unwrap();
    assert_eq!(editor.export_binary().unwrap(), accepted);
}

#[test]
/// Appearance stress choices cover group endpoints/zero, while invalid overrides preserve the previous pixels.
fn group_appearance_overrides_are_bounded_and_atomic() {
    let mut editor = empty_editor();
    add_group(&mut editor);
    let mut preview = Preview::new();
    for (set, expected) in [
        (SampleSet::Minimum, -327.68),
        (SampleSet::Maximum, 327.67),
        (SampleSet::Zero, 0.0),
    ] {
        preview
            .render_appearance(
                editor.preview(),
                1,
                PreviewOptions {
                    ignition_values: set.ignition_values(),
                    ..Default::default()
                },
            )
            .unwrap();
        let snapshot = preview.snapshot().unwrap();
        for cylinder in Cylinder::ALL {
            assert_eq!(
                snapshot
                    .ignition
                    .value_at(cylinder, DataMode::Demo, preview.rendered_at_ms().unwrap()),
                Some(expected)
            );
        }
    }
    let before = preview.rgba().to_vec();
    let snapshot = *preview.snapshot().unwrap();
    for value in [f32::NAN, f32::INFINITY, -327.69, 327.68] {
        assert_eq!(
            preview
                .render_appearance(
                    editor.preview(),
                    1,
                    PreviewOptions {
                        ignition_values: Some([0.0, value, 0.0, 0.0]),
                        ..Default::default()
                    }
                )
                .err(),
            Some(PreviewError::InvalidIgnitionSample(Cylinder::Two))
        );
        assert_eq!(preview.rgba(), before);
        assert_eq!(preview.snapshot(), Some(&snapshot));
    }
}
