use std::collections::BTreeSet;

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::field_registry::*;

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
        config.widgets.push(w).unwrap();
    }
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
