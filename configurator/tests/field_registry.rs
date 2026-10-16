use std::collections::BTreeSet;

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::field_registry::*;

#[test]
fn led_search_context_and_inspection_location_use_human_labels_and_real_indices() {
    assert_eq!(TargetOwner::Led(false).display_label(), "Normal operation");
    assert_eq!(TargetOwner::Led(true).display_label(), "Warning");
    let editor = Editor::new(default_config()).unwrap();
    let target = Registry::target(&editor, FieldKey::LedPattern, TargetOwner::Led(true)).unwrap();
    assert_eq!(Registry::search_context(&editor, target).unwrap(), "Warning");
    let widget = Registry::target(&editor, FieldKey::WarningEnabled, TargetOwner::Widget(0)).unwrap();
    let resolved = Registry::resolve(&editor, widget).unwrap();
    let detail = resolved.inspection.display_location();
    assert!(detail.contains("widget index 0"), "{detail}");
    assert!(detail.contains("JSON path /widgets/0"), "{detail}");
    assert!(!detail.contains("Some(") && !detail.contains("None"), "{detail}");
}

#[test]
fn query_overflow_preserves_previous_unicode_query_and_stale_result_rejects() {
    let mut query = SearchQuery::default();
    query.set("lambda").unwrap();
    assert!(query.set(&"é".repeat(65)).is_err());
    assert_eq!(query.text(), "lambda");
    query.set(&"é".repeat(64)).unwrap();
    assert_eq!(query.text().len(), 128);
    let mut editor = Editor::new(default_config()).unwrap();
    let target = Registry::search(&editor, "smoothing").unwrap().results[0];
    let stamp = editor.stamp();
    let handle = editor.open_json().unwrap();
    let text = editor.json_draft().unwrap().text().to_string();
    editor.set_json_text(handle, text).unwrap();
    assert_eq!(editor.stamp(), stamp);
    assert!(Registry::resolve(&editor, target).is_err());
}

#[test]
fn error_navigation_preserves_exact_duplicate_and_unknown_json_locations() {
    let mut config = default_config();
    config.widgets[1].id = config.widgets[0].id;
    let mut editor = Editor::new(default_config()).unwrap();
    let handle = editor.open_json().unwrap();
    let token = editor
        .set_json_text(handle, serde_json::to_string(&config).unwrap())
        .unwrap();
    editor.finish_json_parse(token).unwrap();
    let links = Registry::error_targets(&editor);
    let duplicate = links.iter().find(|link| link.path.contains("/widgets/1/id")).unwrap();
    assert!(duplicate.field.is_none());
    let resolved = editor.inspect(duplicate.inspection.unwrap()).unwrap();
    assert_eq!(resolved.widget_index, Some(1));
    assert!(!resolved.id_lookup_unambiguous);
    let before = (editor.stamp(), editor.selection(), editor.history_len());
    assert_eq!(duplicate.path, resolved.json_path.unwrap());
    assert_eq!((editor.stamp(), editor.selection(), editor.history_len()), before);
}

#[test]
fn registry_has_each_editable_descriptor_once_and_unique_numeric_dom_ids() {
    let keys = Registry::descriptors().iter().map(|d| d.key).collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), Registry::descriptors().len());
    let mut expected = BTreeSet::new();
    for field in [
        WidgetField::Label,
        WidgetField::Decimals,
        WidgetField::GraphMinimum,
        WidgetField::GraphMaximum,
        WidgetField::GaugeMinimum,
        WidgetField::GaugeMaximum,
        WidgetField::WarningMessage,
        WidgetField::WarningTrigger,
        WidgetField::WarningClear,
        WidgetField::WarningLoad,
        WidgetField::WarningOn,
        WidgetField::WarningOff,
        WidgetField::SmoothingMilliseconds,
        WidgetField::PeakHoldMilliseconds,
        WidgetField::Reference,
        WidgetField::BandMinimum(0),
        WidgetField::BandMaximum(0),
        WidgetField::BandLabel(0),
        WidgetField::BandColor(0),
        WidgetField::WarningTriggerHigh,
        WidgetField::WarningClearHigh,
        WidgetField::WarningSnoozeSeconds,
    ] {
        expected.insert(Registry::raw_key(RawField::Widget(1, field)));
    }
    for role in ColorRole::ALL {
        expected.insert(Registry::raw_key(RawField::Widget(1, WidgetField::Color(role))));
    }
    for field in [
        FuelField::Ratio,
        FuelField::Percent,
        FuelField::GasolineRatio,
        FuelField::EthanolRatio,
        FuelField::GasolineDensity,
        FuelField::EthanolDensity,
        FuelField::ReferenceCelsius,
    ] {
        expected.insert(Registry::raw_key(RawField::Fuel(field)));
    }
    for field in [
        SourceField::Request,
        SourceField::Response,
        SourceField::PollMilliseconds,
        SourceField::FreshnessMilliseconds,
    ] {
        expected.insert(Registry::raw_key(RawField::Source(Channel::Boost, field)));
    }
    for field in [
        LedField::Color,
        LedField::Brightness,
        LedField::Period,
        LedField::OnPercent,
    ] {
        expected.insert(Registry::raw_key(RawField::Led { warning: false, field }));
    }
    for field in [
        RawField::ViewName(0),
        RawField::ControlHoldMilliseconds,
        RawField::HistorySeconds(Channel::Oil),
        RawField::WarningPageMilliseconds,
        RawField::WarningColor { foreground: true },
        RawField::WarningColor { foreground: false },
    ] {
        expected.insert(Registry::raw_key(field));
    }
    for key in [
        FieldKey::Orientation,
        FieldKey::StatusOrientation,
        FieldKey::SplashOrientation,
        FieldKey::ViewOrientation,
        FieldKey::BootView,
        FieldKey::Span,
        FieldKey::Column,
        FieldKey::Row,
        FieldKey::GaugeStyle,
        FieldKey::GaugeExpand,
        FieldKey::Readout,
        FieldKey::Secondary,
        FieldKey::Graph,
        FieldKey::Average,
        FieldKey::Extrema,
        FieldKey::Trend,
        FieldKey::GraphMode,
        FieldKey::PeakMode,
        FieldKey::ReferenceMode,
        FieldKey::WarningEnabled,
        FieldKey::WarningApplicability,
        FieldKey::WarningMode,
        FieldKey::WarningPriority,
        FieldKey::WarningLatch,
        FieldKey::SourceDefinition,
        FieldKey::SourceRoute,
        FieldKey::SourcePoll,
        FieldKey::FuelMode,
        FieldKey::FuelBasis,
        FieldKey::FuelOrigin,
        FieldKey::FuelDensityMode,
        FieldKey::WarningScope,
        FieldKey::WarningPresentation,
        FieldKey::ControlTap,
        FieldKey::ControlHold,
        FieldKey::EncoderRotation,
        FieldKey::EncoderClick,
        FieldKey::LedPattern,
        FieldKey::ReducedMotion,
        FieldKey::LedSync,
        FieldKey::LedFeedback,
        FieldKey::LedDebug,
        FieldKey::LedLogs,
        FieldKey::LedBacklight,
    ] {
        expected.insert(key);
    }
    assert_eq!(keys, expected, "complete raw and typed binding inventory");
    // Independent families: typed settings, raw fields, D2-D7 and all palette roles.
    for key in [
        FieldKey::Label,
        FieldKey::Span,
        FieldKey::Column,
        FieldKey::Row,
        FieldKey::Readout,
        FieldKey::Secondary,
        FieldKey::Smoothing,
        FieldKey::Graph,
        FieldKey::Average,
        FieldKey::Extrema,
        FieldKey::Trend,
        FieldKey::GaugeStyle,
        FieldKey::GaugeExpand,
        FieldKey::PeakMode,
        FieldKey::ReferenceMode,
        FieldKey::WarningEnabled,
        FieldKey::WarningLatch,
        FieldKey::WarningPriority,
        FieldKey::LedPattern,
        FieldKey::ReducedMotion,
        FieldKey::HistorySeconds,
        FieldKey::SourceDefinition,
        FieldKey::FuelMode,
        FieldKey::ControlTap,
        FieldKey::ControlHold,
    ] {
        assert!(keys.contains(&key), "missing {key:?}");
    }
    let mut config = default_config();
    let template = config.widgets[0].clone();
    config.widgets.clear();
    for i in 0..64 {
        let mut w = template.clone();
        w.id = i + 1;
        w.advanced
            .set_bands(&[
                Band::new(0., 0.5, w.colors.gauge, "Low").unwrap(),
                Band::new(0.5, 1., w.colors.gauge, "Mid").unwrap(),
                Band::new(1., 1.5, w.colors.gauge, "High").unwrap(),
            ])
            .unwrap();
        config.widgets.push(w).unwrap();
    }
    config.widgets[63].id = config.widgets[0].id;
    let mut editor = Editor::new(default_config()).unwrap();
    // A rejected import is not silently accepted; inspect a JSON candidate instead.
    let handle = editor.open_json().unwrap();
    let token = editor
        .set_json_text(handle, serde_json::to_string(&config).unwrap())
        .unwrap();
    editor.finish_json_parse(token).unwrap();
    let targets = Registry::targets(&editor).unwrap();
    let ids = targets.iter().map(FieldTarget::dom_id).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), targets.len());
    assert!(
        ids.iter()
            .all(|id| id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    );
}

#[test]
fn forged_scope_and_absent_band_never_resolve_and_raw_target_stays_authentic() {
    let mut editor = Editor::new(default_config()).unwrap();
    assert!(
        Registry::target(
            &editor,
            FieldKey::LedSync,
            TargetOwner::Button(ControlPage::Dashboard, ButtonId::A)
        )
        .is_err()
    );
    assert!(
        Registry::target(
            &editor,
            FieldKey::ControlHold,
            TargetOwner::Button(ControlPage::Logs, ButtonId::Y)
        )
        .is_err()
    );
    assert!(Registry::target(&editor, FieldKey::BandLabel, TargetOwner::Band(0, 0)).is_err());
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let target = Registry::retained_raw_target(&editor, 0, TargetOwner::Root).unwrap();
    let resolved = Registry::resolve(&editor, target).unwrap();
    assert_eq!(resolved.inspection.raw_index, Some(0));
    assert_eq!(resolved.inspection.field, None);
    assert_eq!(resolved.inspection.json_path, None);
    assert!(Registry::retained_raw_target(&editor, 0, TargetOwner::Widget(0)).is_err());
}

#[test]
fn variant_inactive_fields_explain_actual_absent_input_controls() {
    let editor = Editor::new(default_config()).unwrap();
    for (key, owner) in [
        (FieldKey::FuelRatio, TargetOwner::Root),
        (FieldKey::FuelPercent, TargetOwner::Root),
        (FieldKey::SourceRequest, TargetOwner::Channel(Channel::Oil)),
        (FieldKey::GraphMinimum, TargetOwner::Widget(0)),
    ] {
        let target = Registry::target(&editor, key, owner).unwrap();
        let field = Registry::resolve(&editor, target).unwrap();
        assert!(!field.capability.is_active(), "{key:?} variant is absent");
        assert!(field.capability.reason().is_some());
    }
}

#[test]
fn semantic_locations_cannot_be_relabelled_as_an_unrelated_registry_field() {
    let mut editor = Editor::new(default_config()).unwrap();
    let root = editor.object_inspection_target(InspectionOwner::Root).unwrap();
    editor
        .set_located_raw(root, RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let mut malformed = default_config();
    malformed.widgets[0].label = "bad\n".try_into().unwrap();
    let issue = dashboard_configurator::diagnostic::collect_candidate(&malformed).issues[0];
    assert!(Registry::semantic_target(&editor, issue.location, FieldKey::GaugeStyle, TargetOwner::Widget(0)).is_err());
}

#[test]
fn semantic_led_identity_cannot_focus_the_other_pattern() {
    let editor = Editor::new(default_config()).unwrap();
    let mut malformed = default_config();
    malformed.led.warning.brightness_percent = 101;
    let issue = dashboard_configurator::diagnostic::collect_candidate(&malformed)
        .issues
        .into_iter()
        .find(|issue| issue.error.field == Field::LedBrightness(true))
        .unwrap();
    assert!(
        Registry::semantic_target(
            &editor,
            issue.location,
            FieldKey::LedBrightness,
            TargetOwner::Led(false)
        )
        .is_err()
    );
    assert!(
        Registry::semantic_target(&editor, issue.location, FieldKey::LedBrightness, TargetOwner::Led(true)).is_ok()
    );
}

#[test]
fn bounded_search_covers_aliases_unicode_duplicate_labels_and_exact_omission() {
    let mut editor = Editor::new(default_config()).unwrap();
    for query in ["boost", "AFR", "lambda", "PSI", "smoothing", "stale", "warning"] {
        assert!(!Registry::search(&editor, query).unwrap().results.is_empty(), "{query}");
    }
    assert!(Registry::search(&editor, &"é".repeat(64)).is_ok());
    assert_eq!(
        Registry::search(&editor, &"é".repeat(65)).unwrap_err(),
        RegistryError::QueryTooLong
    );
    let all = Registry::targets(&editor).unwrap();
    let result = Registry::search(&editor, "").unwrap();
    assert_eq!(result.results.len(), 40);
    assert_eq!(result.omitted, all.len() - 40);
    assert_eq!(result, Registry::search(&editor, "").unwrap());
    let mut candidate = editor.preview().config().clone();
    candidate.widgets[1].id = candidate.widgets[0].id;
    candidate.widgets[0].label = "Łambda".try_into().unwrap();
    candidate.widgets[1].label = "Łambda".try_into().unwrap();
    let handle = editor.open_json().unwrap();
    let token = editor
        .set_json_text(handle, serde_json::to_string(&candidate).unwrap())
        .unwrap();
    editor.finish_json_parse(token).unwrap();
    let found = Registry::search(&editor, "łambda").unwrap();
    assert!(found.results.iter().any(|t| t.owner() == TargetOwner::Widget(0)));
    assert!(found.results.iter().any(|t| t.owner() == TargetOwner::Widget(1)));
    assert_eq!(found, Registry::search(&editor, "łambda").unwrap());
}

#[test]
fn capabilities_and_resolution_follow_real_style_span_and_candidate_lineage() {
    let mut editor = Editor::new(default_config()).unwrap();
    let target = Registry::targets(&editor)
        .unwrap()
        .into_iter()
        .find(|t| t.key() == FieldKey::Readout && t.owner() == TargetOwner::Widget(0))
        .unwrap();
    assert_eq!(
        Registry::resolve(&editor, target).unwrap().workspace,
        dashboard_configurator::workspace::Workspace::Views
    );
    let mut w = editor.preview().config().widgets[0].clone();
    for style in GaugeKind::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            w.gauge.kind = style;
            w.placement.span = span;
            assert_eq!(
                Registry::widget_capability(FieldKey::Graph, &w).is_active(),
                w.capabilities().history
            );
            assert_eq!(
                Registry::widget_capability(FieldKey::PeakMode, &w).is_active(),
                w.capabilities().markers
            );
            assert_eq!(
                Registry::widget_capability(FieldKey::Average, &w).is_active(),
                w.capabilities().statistics
            );
        }
    }
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    assert_eq!(
        Registry::resolve(&editor, target).unwrap_err(),
        RegistryError::Editor(EditorError::StaleToken)
    );
}

#[test]
fn persistent_field_ids_follow_current_view_and_widget_positions_without_rebinding_objects() {
    let mut editor = Editor::new(default_config()).unwrap();
    let original = editor.preview().config().views[0].id;
    editor
        .apply(Edit::DuplicateView {
            id: original,
            name: "Copy".into(),
        })
        .unwrap();
    let copy = editor.selection().view;
    let view_binding = FieldId::Raw(RawField::ViewName(copy));
    assert_eq!(
        view_binding.resolve(&editor).unwrap(),
        Registry::dom_id(FieldKey::ViewName, TargetOwner::View(1))
    );
    let old_target = Registry::target(&editor, FieldKey::ViewName, TargetOwner::View(1)).unwrap();
    editor.apply(Edit::ReorderView { id: copy, index: 0 }).unwrap();
    assert_eq!(editor.selection().view, copy);
    assert_eq!(
        view_binding.resolve(&editor).unwrap(),
        Registry::resolve(
            &editor,
            Registry::target(&editor, FieldKey::ViewName, TargetOwner::View(0)).unwrap()
        )
        .unwrap()
        .dom_id
    );
    assert_eq!(
        Registry::resolve(&editor, old_target).unwrap_err(),
        RegistryError::Editor(EditorError::StaleToken)
    );
    editor.apply(Edit::ReorderView { id: copy, index: 1 }).unwrap();
    editor.apply(Edit::DeleteView(original)).unwrap();
    assert_eq!(
        view_binding.resolve(&editor).unwrap(),
        Registry::dom_id(FieldKey::ViewName, TargetOwner::View(0))
    );

    let mut editor = Editor::new(default_config()).unwrap();
    let id = editor.preview().config().widgets[1].id;
    let preceding = editor.preview().config().widgets[0].id;
    let channel = editor.preview().config().widgets[1].channel;
    let view = editor.preview().config().widgets[1].view;
    editor.select(Selection { view, widget: Some(id) }).unwrap();
    let bindings = [
        FieldId::Raw(RawField::Widget(id, WidgetField::Label)),
        FieldId::Widget {
            id,
            key: FieldKey::Readout,
        },
        FieldId::Widget {
            id,
            key: FieldKey::WarningEnabled,
        },
        FieldId::Raw(RawField::Widget(id, WidgetField::SmoothingMilliseconds)),
    ];
    let before = bindings.map(|binding| binding.resolve(&editor).unwrap());
    let preset = bindings[3].preset(&editor).unwrap();
    editor.apply(Edit::ReorderWidget { id, index: 0 }).unwrap();
    for (binding, old) in bindings.into_iter().zip(before) {
        let current = binding.resolve(&editor).unwrap();
        assert_ne!(current, old);
        assert!(current.starts_with("studio-field-w-0-"));
    }
    assert_ne!(bindings[3].preset(&editor).unwrap(), preset);
    editor.apply(Edit::ReorderWidget { id, index: 1 }).unwrap();
    editor.apply(Edit::DeleteWidget(preceding)).unwrap();
    for binding in bindings {
        assert!(binding.resolve(&editor).unwrap().starts_with("studio-field-w-0-"));
    }
    assert_eq!(editor.selection(), Selection { view, widget: Some(id) });
    assert_eq!(editor.preview().config().widgets[0].channel, channel);
}

#[test]
fn inspecting_current_and_other_view_rules_leaves_all_canvas_objects_available() {
    let mut editor = Editor::new(default_config()).unwrap();
    let original = editor.selection().view;
    editor
        .apply(Edit::DuplicateView {
            id: original,
            name: "Other".into(),
        })
        .unwrap();
    editor
        .select(Selection {
            view: original,
            widget: None,
        })
        .unwrap();
    let current = editor
        .editing_config()
        .widgets
        .iter()
        .filter(|w| w.view == original)
        .cloned()
        .collect::<Vec<_>>();
    let other_index = editor
        .editing_config()
        .widgets
        .iter()
        .position(|w| w.view != original)
        .unwrap();
    for index in [1, other_index] {
        let target = Registry::target(&editor, FieldKey::WarningEnabled, TargetOwner::Widget(index)).unwrap();
        let stamp = editor.stamp();
        let selection = editor.selection();
        assert!(Registry::resolve(&editor, target).is_ok());
        for widget in &current {
            assert_eq!(Registry::layout_widget(&editor, widget.id), Some(widget.clone()));
        }
        assert_eq!(editor.stamp(), stamp);
        assert_eq!(editor.selection(), selection);
    }
    // Normal hit/context actions still use the real current layout and reducer.
    editor
        .select(Selection {
            view: original,
            widget: Some(current[0].id),
        })
        .unwrap();
    let menu = dashboard_configurator::widget_actions::WidgetMenu::open(&mut editor, current[1].id, None).unwrap();
    assert_eq!(menu.target(), current[1].id);
    assert_eq!(editor.selection().widget, Some(current[1].id));
    assert!(matches!(
        editor.apply(Edit::ResizeWidget {
            id: current[0].id,
            span: Span::TwoByOne,
        }),
        Err(EditorError::Validation(ValidationError {
            code: ErrorCode::Overlap,
            ..
        }))
    ));
    assert_ne!(
        Registry::layout_widget(&editor, current[0].id).unwrap().placement,
        current[0].placement
    );
    assert_eq!(editor.preview().config().widgets[0].placement, current[0].placement);
    assert_eq!(
        Registry::layout_widget(&editor, current[0].id),
        editor
            .editing_config()
            .widgets
            .iter()
            .find(|w| w.id == current[0].id)
            .cloned()
    );
}

#[test]
fn numeric_gauge_scale_fields_explain_absent_controls_and_retained_repair_still_works() {
    let mut editor = Editor::new(default_config()).unwrap();
    let id = editor.preview().config().widgets[0].id;
    let view = editor.preview().config().widgets[0].view;
    let minimum = editor.preview().config().widgets[0].gauge.min.to_string();
    editor.select(Selection { view, widget: Some(id) }).unwrap();
    for key in [FieldKey::GaugeMinimum, FieldKey::GaugeMaximum, FieldKey::GaugeExpand] {
        let capability = Registry::widget_capability(key, &editor.preview().config().widgets[0]);
        assert!(!capability.is_active(), "{key:?} has no normal Numeric control");
        assert!(capability.reason().unwrap().contains("Numeric"));
    }
    let field = RawField::Widget(id, WidgetField::GaugeMinimum);
    editor.set_raw(editor.field_token(), field, "bad".into()).unwrap();
    assert!(editor.widget_fields_need_repair(
        id,
        &[WidgetField::GaugeMinimum, WidgetField::GaugeMaximum],
        Field::GaugeRange
    ));
    let retained = editor.raw_inspection_target(0).unwrap();
    editor.set_inspected_raw(retained, minimum).unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.selection(), Selection { view, widget: Some(id) });
    let mut widget = editor.preview().config().widgets[0].clone();
    for kind in GaugeKind::ALL.into_iter().filter(|kind| !kind.is_numeric()) {
        widget.gauge.kind = kind;
        for key in [FieldKey::GaugeMinimum, FieldKey::GaugeMaximum, FieldKey::GaugeExpand] {
            assert!(Registry::widget_capability(key, &widget).is_active());
        }
    }
}
