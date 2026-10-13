use dashboard_configurator::appearance_presets::*;
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::workspace::{Command, CommandEffect, WorkspaceState, dispatch_with_appearance};

fn selected() -> Editor {
    let mut e = Editor::default();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    e
}
fn changed(e: &Editor) -> AppearanceSnapshot {
    let mut s = e.selected_appearance(e.stamp()).unwrap();
    s.colors = ColorPalette::Monochrome.colors();
    s.decimals = 3;
    s
}
fn state(
    e: &Editor
) -> (
    EditorStamp,
    AcceptedConfigKey,
    Revision,
    Selection,
    (usize, usize),
    String,
) {
    (
        e.stamp(),
        e.accepted_config_key(),
        e.accepted_revision(),
        e.selection(),
        e.history_len(),
        e.export_json().unwrap(),
    )
}

#[test]
fn selected_view_all_change_only_the_explicit_allowlist_and_one_undo() {
    for scope in AppearanceScope::ALL {
        let mut e = selected();
        let mut config = e.preview().config().clone();
        for w in &mut config.widgets {
            w.gauge.expand = true;
            w.display.graph_range = GraphRange::Fixed {
                min: w.channel.domain().0,
                max: w.channel.domain().1,
            };
            w.advanced.smoothing_ms = 250;
            w.advanced.peak_hold_ms = 1234;
            w.advanced.reference = -0.0;
            w.advanced.peak_mode = PeakSelection::Maximum;
            let (low, high) = w.channel.domain();
            w.advanced
                .set_bands(&[Band::new(low, high, Color(0xFFFF), "band").unwrap()])
                .unwrap();
        }
        e = Editor::new(config.clone()).unwrap();
        e.select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
        let before = e.export_binary().unwrap();
        let history = e.history_len();
        let s = changed(&e);
        let request = e.appearance_request(scope, s, e.stamp()).unwrap();
        let preview = e.appearance_preview(request).unwrap();
        assert!(preview.changed);
        assert_eq!(preview.error, None);
        e.apply(Edit::ApplyAppearance(request)).unwrap();
        assert_eq!(e.history_len().0, history.0 + 1);
        let after = e.export_binary().unwrap();
        // Restore only the independently enumerated allowed fields. Every
        // remaining serialized value, including signed zero, must be identical.
        let mut normalized = e.preview().config().clone();
        for (old, new) in config.widgets.iter().zip(&mut normalized.widgets) {
            new.colors = old.colors;
            new.gauge.kind = old.gauge.kind;
            new.display.decimals = old.display.decimals;
            new.display.graph = old.display.graph;
            new.display.average = old.display.average;
            new.display.extrema = old.display.extrema;
            new.display.trend = old.display.trend;
            assert_eq!(new.advanced.reference.to_bits(), old.advanced.reference.to_bits());
        }
        assert_eq!(
            serde_json::to_string(&normalized).unwrap(),
            serde_json::to_string(&config).unwrap()
        );
        for (old, new) in config.widgets.iter().zip(&e.preview().config().widgets) {
            let targeted = match scope {
                AppearanceScope::Selected => old.id == 1,
                AppearanceScope::ThisView => old.view == 1,
                AppearanceScope::AllWidgets => true,
            };
            if targeted {
                assert_eq!(new.colors, s.colors);
            } else {
                assert_eq!(old, new);
            }
        }
        e.undo().unwrap();
        assert_eq!(e.export_binary().unwrap(), before);
        e.redo().unwrap();
        assert_eq!(e.export_binary().unwrap(), after);
    }
}

#[test]
fn capabilities_use_the_resulting_style_and_preserve_unsupported_preferences() {
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        for kind in GaugeKind::ALL {
            let mut c = default_config();
            c.widgets.truncate(1);
            c.widgets[0].placement = Placement {
                column: 0,
                row: 0,
                span,
            };
            c.widgets[0].display.graph = true;
            c.widgets[0].display.average = true;
            c.widgets[0].display.extrema = true;
            c.widgets[0].display.trend = true;
            let mut e = Editor::new(c).unwrap();
            e.select(Selection {
                view: 1,
                widget: Some(1),
            })
            .unwrap();
            let mut s = changed(&e);
            s.gauge_style = kind;
            s.graph = false;
            s.average = false;
            s.extrema = false;
            s.trend = false;
            let request = e.appearance_request(AppearanceScope::Selected, s, e.stamp()).unwrap();
            let preview = e.appearance_preview(request).unwrap();
            let omissions = if kind.is_numeric() {
                0
            } else if span == Span::TwoByTwo {
                2
            } else {
                4
            };
            assert_eq!(preview.omitted_fields, omissions);
            assert_eq!(preview.target_count, 1);
            e.apply(Edit::ApplyAppearance(request)).unwrap();
            let w = &e.preview().config().widgets[0];
            assert_eq!(w.gauge.kind, kind);
            assert_eq!(w.display.graph, !kind.is_numeric());
            assert_eq!(w.display.trend, !kind.is_numeric());
            assert_eq!(w.display.average, !kind.is_numeric() && span != Span::TwoByTwo);
            assert_eq!(w.display.extrema, !kind.is_numeric() && span != Span::TwoByTwo);
        }
    }
}

#[test]
fn all_64_widgets_are_one_transaction_and_noop_is_explicit() {
    let mut c = default_config();
    c.views.clear();
    c.widgets.clear();
    for view in 1..=8 {
        c.views
            .push(View {
                id: view,
                name: "View".try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8 {
            let mut w = default_config().widgets[0].clone();
            w.id = u16::from(view) * 8 + cell;
            w.view = view;
            w.placement = Placement {
                column: (cell % 4) as u8,
                row: (cell / 4) as u8,
                span: Span::OneByOne,
            };
            c.widgets.push(w).unwrap();
        }
    }
    let mut e = Editor::new(c).unwrap();
    e.select(Selection {
        view: 1,
        widget: Some(8),
    })
    .unwrap();
    let before = e.export_binary().unwrap();
    let request = e
        .appearance_request(AppearanceScope::AllWidgets, changed(&e), e.stamp())
        .unwrap();
    assert_eq!(e.appearance_preview(request).unwrap().target_count, 64);
    e.apply(Edit::ApplyAppearance(request)).unwrap();
    let after = state(&e);
    let request = e
        .appearance_request(AppearanceScope::AllWidgets, changed(&e), e.stamp())
        .unwrap();
    assert!(!e.appearance_preview(request).unwrap().changed);
    e.apply(Edit::ApplyAppearance(request)).unwrap();
    assert_eq!(state(&e), after);
    e.undo().unwrap();
    assert_eq!(e.export_binary().unwrap(), before);
}

#[test]
fn color_band_conflict_rejects_without_publishing_a_draft_or_retiring_save() {
    let mut e = selected();
    let mut c = e.preview().config().clone();
    c.widgets[0].colors = WidgetColors::default();
    c.widgets[0]
        .advanced
        .set_bands(&[Band::new(-1., 2., Color(0xFFFF), "band").unwrap()])
        .unwrap();
    e = Editor::new(c).unwrap();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    let before = state(&e);
    let save = e.begin_save().unwrap();
    let mut s = changed(&e);
    // Valid black readable/gauge foregrounds over white, but white excluded band
    // cannot contrast with a white track.
    s.colors.background = Color(0xFFFF);
    for role in ColorRole::ALL {
        if !matches!(
            role,
            ColorRole::Background
                | ColorRole::Track
                | ColorRole::Border
                | ColorRole::WarningBackground
                | ColorRole::WarningForeground
        ) {
            role.set(&mut s.colors, Color(0));
        }
    }
    s.colors.track = Color(0xFFFF);
    assert_eq!(s.colors.invalid_role(), None);
    let request = e.appearance_request(AppearanceScope::AllWidgets, s, e.stamp()).unwrap();
    assert!(e.appearance_preview(request).unwrap().error.is_some());
    assert!(e.apply(Edit::ApplyAppearance(request)).is_err());
    assert_eq!(state(&e), before);
    assert!(!e.has_invalid_draft());
    assert!(e.save_is_current(save.token()));
}

#[test]
fn stale_owner_pending_json_raw_pointer_and_import_cannot_apply_or_copy() {
    let mut e = selected();
    let request = e
        .appearance_request(AppearanceScope::Selected, changed(&e), e.stamp())
        .unwrap();
    let mut replacement = selected();
    let before = state(&replacement);
    assert!(replacement.apply(Edit::ApplyAppearance(request)).is_err());
    assert_eq!(state(&replacement), before);
    let json = e.open_json().unwrap();
    let text = e.export_json().unwrap();
    e.set_json_text(json, text.clone()).unwrap();
    let stamp = e.stamp();
    assert_eq!(e.apply(Edit::ApplyAppearance(request)), Err(EditorError::JsonPending));
    assert_eq!(e.selected_appearance(stamp), Err(EditorError::JsonPending));
    assert_eq!(e.json_draft().unwrap().text(), text);
    e.discard_json().unwrap();
    let request = e
        .appearance_request(AppearanceScope::Selected, changed(&e), e.stamp())
        .unwrap();
    let import = e.begin_import().unwrap();
    assert_eq!(e.apply(Edit::ApplyAppearance(request)), Err(EditorError::ImportPending));
    assert!(e.import_is_current(import));
    e.cancel_import(import).unwrap();
    let pointer = e.begin_pointer(12, PointerTarget::Widget(1)).unwrap();
    assert_eq!(e.apply(Edit::ApplyAppearance(request)), Err(EditorError::PointerActive));
    assert_eq!(e.active_pointer(), Some(pointer));
    e.cancel_pointer(pointer).unwrap();
    e.set_raw(e.field_token(), RawField::Widget(1, WidgetField::Decimals), "-".into())
        .unwrap();
    let stamp = e.stamp();
    assert_eq!(e.selected_appearance(stamp), Err(EditorError::InvalidDraft));
    assert!(e.apply(Edit::ApplyAppearance(request)).is_err());
    assert_eq!(e.raw_fields()[0].text, "-");
}

#[test]
fn palettes_and_snapshot_wire_are_quantized_strict_and_have_no_excluded_fields() {
    assert_eq!(ColorPalette::Default.colors(), WidgetColors::default());
    let e = selected();
    for palette in ColorPalette::ALL {
        let mut s = changed(&e);
        s.colors = palette.colors();
        assert!(s.validate().is_ok());
        let text = serde_json::to_string(&s).unwrap();
        let decoded: AppearanceSnapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(decoded, s);
        for extra in ["id", "placement", "warning", "advanced", "sources", "display"] {
            assert!(
                serde_json::from_str::<AppearanceSnapshot>(&text.replacen('{', &format!("{{\"{extra}\":0,"), 1))
                    .is_err()
            );
        }
        assert!(serde_json::from_str::<AppearanceSnapshot>(&text.replace("Numeric", "Exploit")).is_err());
        assert!(serde_json::from_str::<AppearanceSnapshot>(&text.replace("#FFFFFF", "bad-color")).is_err());
    }
}

#[test]
fn bounded_preset_import_names_capacity_duplicate_and_invalid_restore() {
    let e = selected();
    let s = changed(&e);
    let mut store = PresetStore::restore(Ok(None));
    for name in ["", " ", "é", "bad\nname", &"x".repeat(33)] {
        assert!(store.add(name, s).is_err());
    }
    for n in 0..16 {
        store.add(&format!("Preset {n}"), s).unwrap();
    }
    assert_eq!(store.records().len(), 16);
    assert_eq!(store.add("Preset 0", s), Err(PresetError::DuplicateName));
    assert_eq!(store.add("Seventeenth", s), Err(PresetError::Capacity));
    let text = store.export().unwrap();
    assert!(text.len() <= MAX_PRESET_BYTES);
    let mut restored = PresetStore::restore(Ok(Some(text.clone())));
    assert_eq!(restored.records(), store.records());
    let before = restored.export().unwrap();
    for invalid in [
        "{}".into(),
        "x".repeat(MAX_PRESET_BYTES + 1),
        text.replace("\"version\":1", "\"version\":99"),
    ] {
        assert!(restored.import(&invalid).is_err());
        assert_eq!(restored.export().unwrap(), before);
        assert!(matches!(
            PresetStore::restore(Ok(Some(invalid))).status(),
            PresetStatus::Unavailable(_)
        ));
    }
    let long_name = "<script data='x'>0123456789abc";
    let mut fresh = PresetStore::restore(Ok(None));
    fresh.add(long_name, s).unwrap();
    fresh.import(&fresh.export().unwrap()).unwrap();
}

#[test]
fn storage_conflict_failed_save_and_stale_completion_preserve_memory() {
    let e = selected();
    let s = changed(&e);
    let mut store = PresetStore::restore(Ok(None));
    store.add("Mine", s).unwrap();
    let request = store.begin_save().unwrap();
    assert_eq!(store.check_write(&request, Some("other")), Err(PresetError::Conflict));
    store.observe(Ok(Some("other".into())));
    assert_eq!(store.status(), PresetStatus::Conflict);
    assert_eq!(store.records().len(), 1);
    assert!(!store.save_is_current(&request));
    let request = store.begin_replace(Some("other")).unwrap();
    assert_eq!(store.check_write(&request, Some("other")), Ok(()));
    assert_eq!(store.check_write(&request, Some("newer")), Err(PresetError::Conflict));
    for failure in [
        StorageFailure::Quota,
        StorageFailure::Denied,
        StorageFailure::Unavailable,
    ] {
        let request = store.begin_replace(Some("other")).unwrap();
        store.finish_save(&request, Err(failure)).unwrap();
        assert_eq!(store.status(), PresetStatus::Failed(failure));
        assert_eq!(store.records().len(), 1);
        assert!(store.export().is_ok());
    }
    let request = store.begin_replace(Some("other")).unwrap();
    store.add("New", s).unwrap();
    assert!(!store.save_is_current(&request));
    assert!(store.finish_save(&request, Ok(())).is_err());
    assert_eq!(store.status(), PresetStatus::Conflict);
    let request = store.begin_replace(Some("other")).unwrap();
    store.finish_save(&request, Ok(())).unwrap();
    assert_eq!(store.status(), PresetStatus::Saved);
    store.observe(Ok(None));
    assert_eq!(store.status(), PresetStatus::Conflict);
    let replacement = PresetStore::restore(Ok(None));
    assert!(!replacement.save_is_current(&request));
    let request = store.begin_replace(None).unwrap();
    store.cancel();
    assert!(!store.save_is_current(&request));
    assert_eq!(store.check_write(&request, None), Err(PresetError::Stale));
    assert_eq!(store.finish_save(&request, Ok(())), Err(PresetError::Stale));
}

#[test]
fn palette_copy_and_preview_produce_actual_typed_authority_without_editing() {
    let mut e = selected();
    let before = state(&e);
    let mut workspace = WorkspaceState::default();
    let stamp = e.stamp();
    let mut session = AppearanceSession::default();
    let copy_offer = session.offer(&e, stamp);
    let CommandEffect::CopiedAppearance(s) =
        dispatch_with_appearance(&mut e, &mut workspace, Command::CopyAppearance, stamp, Some(copy_offer)).unwrap()
    else {
        panic!("copy effect");
    };
    assert_eq!(s, e.selected_appearance(stamp).unwrap());
    assert_eq!(state(&e), before);
    let s = changed(&e);
    session.choose(AppearanceSource::Preset(s), None).unwrap();
    let offer = session.offer(&e, stamp);
    let CommandEffect::PreviewAppearance(request) = dispatch_with_appearance(
        &mut e,
        &mut workspace,
        Command::ApplyAppearance(AppearanceScope::AllWidgets),
        stamp,
        Some(offer),
    )
    .unwrap() else {
        panic!("preview effect");
    };
    assert_eq!(state(&e), before);
    assert!(e.appearance_preview(request).unwrap().changed);
    e.apply(Edit::ApplyAppearance(request)).unwrap();
    assert!(
        dispatch_with_appearance(&mut e, &mut workspace, Command::CopyAppearance, stamp, Some(copy_offer)).is_err()
    );
}

#[test]
fn explicit_source_choices_retire_old_offers_and_confirmations() {
    let e = selected();
    let mut session = AppearanceSession::default();
    let original = session.offer(&e, e.stamp());
    assert_eq!(original.snapshot(), e.selected_appearance(e.stamp()).ok());
    session.copy(changed(&e)).unwrap();
    // Copy must not silently choose the clipboard as the active source.
    assert_eq!(
        session.offer(&e, e.stamp()).snapshot(),
        e.selected_appearance(e.stamp()).ok()
    );
    session
        .choose(AppearanceSource::Copied, Some(ColorPalette::RedAccent))
        .unwrap();
    assert!(!session.is_current(original));
    let offer = session.offer(&e, e.stamp());
    let s = offer.snapshot().unwrap();
    assert_eq!(s.decimals, 3);
    assert_eq!(s.colors, ColorPalette::RedAccent.colors());
    let request = e.appearance_request(AppearanceScope::Selected, s, e.stamp()).unwrap();
    session.stage(request, offer).unwrap();
    assert_eq!(session.request(), Some(request));
    session.choose(AppearanceSource::Selected, None).unwrap();
    assert_eq!(session.request(), None);
    assert!(!session.is_current(offer));
    let mut no_selection = Editor::default();
    assert_eq!(session.offer(&no_selection, no_selection.stamp()).snapshot(), None);
    no_selection
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    assert!(session.offer(&no_selection, no_selection.stamp()).snapshot().is_some());
}

#[test]
fn exact_wire_limit_duplicate_fields_and_full_record_capacity_are_atomic() {
    let e = selected();
    let snapshot = changed(&e);
    let mut store = PresetStore::restore(Ok(None));
    store.add(&"x".repeat(32), snapshot).unwrap();
    store.add(" A ", snapshot).unwrap();
    assert_eq!(store.records()[1].name(), " A ");
    let text = store.export().unwrap();
    let before = store.version();
    let padded = format!("{text}{}", " ".repeat(MAX_PRESET_BYTES - text.len()));
    assert_eq!(padded.len(), MAX_PRESET_BYTES);
    store.import(&padded).unwrap();
    assert_ne!(store.version(), before);
    let before = store.export().unwrap();
    let invalids = [
        format!("{padded} "),
        text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1),
        text.replacen("\"records\":", "\"extra\":0,\"records\":", 1),
        text.replacen("\"name\":", "\"id\":1,\"name\":", 1),
        text.replacen("\"decimals\":3", "\"decimals\":3,\"decimals\":2", 1),
        text.replace("Numeric", "UnknownStyle"),
        text.replace("\"decimals\":3", "\"decimals\":4"),
        text.replace("#FFFFFF", "#000000"),
    ];
    for invalid in invalids {
        assert!(store.import(&invalid).is_err());
        assert_eq!(store.export().unwrap(), before);
    }
    let record = serde_json::to_value(&store.records()[0]).unwrap();
    let records = (0..17)
        .map(|i| {
            let mut r = record.clone();
            r["name"] = format!("Name{i}").into();
            r
        })
        .collect::<Vec<_>>();
    let text = serde_json::json!({"version":1,"records":records}).to_string();
    assert!(store.import(&text).is_err());
    assert_eq!(store.export().unwrap(), before);
    let duplicate = serde_json::json!({"version":1,"records":[record.clone(),record]}).to_string();
    assert_eq!(store.import(&duplicate), Err(PresetError::DuplicateName));
    assert_eq!(store.export().unwrap(), before);
}

#[test]
fn parsed_valid_and_invalid_json_and_retired_inspection_epochs_reject_appearance() {
    let mut e = selected();
    let snapshot = changed(&e);
    let request = e
        .appearance_request(AppearanceScope::AllWidgets, snapshot, e.stamp())
        .unwrap();
    let handle = e.open_json().unwrap();
    let json_request = e
        .appearance_request(AppearanceScope::AllWidgets, snapshot, e.stamp())
        .unwrap();
    let token = e.set_json_text(handle, e.export_json().unwrap()).unwrap();
    e.finish_json_parse(token).unwrap();
    let stamp = e.stamp();
    let history = e.history_len();
    let key = e.accepted_config_key();
    assert_eq!(e.apply(Edit::ApplyAppearance(request)), Err(EditorError::JsonPending));
    assert_eq!(
        e.appearance_request(AppearanceScope::Selected, snapshot, stamp),
        Err(EditorError::JsonPending)
    );
    let token = e.set_json_text(handle, "{ bad JSON".into()).unwrap();
    e.finish_json_parse(token).unwrap();
    assert_eq!(e.apply(Edit::ApplyAppearance(request)), Err(EditorError::JsonPending));
    assert_eq!(e.json_draft().unwrap().text(), "{ bad JSON");
    assert_eq!(
        (e.stamp(), e.history_len(), e.accepted_config_key()),
        (stamp, history, key)
    );
    e.discard_json().unwrap();
    // A target minted in this JSON inspection lifetime stays retired after Discard.
    assert_eq!(
        e.apply(Edit::ApplyAppearance(json_request)),
        Err(EditorError::StaleToken)
    );
    let request = e
        .appearance_request(AppearanceScope::Selected, snapshot, e.stamp())
        .unwrap();
    e.select(Selection {
        view: 1,
        widget: Some(2),
    })
    .unwrap();
    let before = state(&e);
    assert!(e.apply(Edit::ApplyAppearance(request)).is_err());
    assert_eq!(state(&e), before);
}

#[test]
fn invalid_typed_snapshot_and_empty_view_have_no_editor_effects() {
    let mut e = selected();
    let before = state(&e);
    let mut snapshot = changed(&e);
    snapshot.decimals = 4;
    assert!(
        e.appearance_request(AppearanceScope::AllWidgets, snapshot, e.stamp())
            .is_err()
    );
    assert_eq!(state(&e), before);
    snapshot = changed(&e);
    snapshot.colors.label = snapshot.colors.background;
    assert!(
        e.appearance_request(AppearanceScope::Selected, snapshot, e.stamp())
            .is_err()
    );
    assert_eq!(state(&e), before);
    e.apply(Edit::AddView { name: "Empty".into() }).unwrap();
    let request = e
        .appearance_request(AppearanceScope::ThisView, changed(&selected()), e.stamp())
        .unwrap();
    let preview = e.appearance_preview(request).unwrap();
    assert_eq!(preview.target_count, 0);
    assert!(!preview.changed);
    let before = state(&e);
    e.apply(Edit::ApplyAppearance(request)).unwrap();
    assert_eq!(state(&e), before);
}

#[test]
fn failed_explicit_reload_marks_storage_failed_without_losing_memory_records() {
    let e = selected();
    let mut store = PresetStore::restore(Ok(None));
    store.add("Mine", changed(&e)).unwrap();
    let request = store.begin_save().unwrap();
    store.finish_save(&request, Ok(())).unwrap();
    let before = store.export().unwrap();
    assert!(store.reload(Err(StorageFailure::Denied)).is_err());
    assert_eq!(store.status(), PresetStatus::Failed(StorageFailure::Denied));
    assert_eq!(store.export().unwrap(), before);
}

#[test]
fn captured_appearance_commands_cannot_acquire_post_discard_candidate_authority() {
    for command in [
        Command::CopyAppearance,
        Command::ApplyAppearance(AppearanceScope::Selected),
        Command::ApplyAppearance(AppearanceScope::ThisView),
        Command::ApplyAppearance(AppearanceScope::AllWidgets),
    ] {
        let mut e = selected();
        let handle = e.open_json().unwrap();
        let stamp = e.stamp();
        let session = AppearanceSession::default();
        let offer = session.offer(&e, stamp);
        assert!(offer.snapshot().is_some());
        let text = e.export_json().unwrap();
        let token = e.set_json_text(handle, text).unwrap();
        e.finish_json_parse(token).unwrap();
        e.discard_json().unwrap();
        assert_eq!(e.stamp(), stamp);
        assert!(session.is_current(offer));
        let before = state(&e);
        let mut workspace = WorkspaceState::default();
        assert!(
            dispatch_with_appearance(&mut e, &mut workspace, command, stamp, Some(offer)).is_err(),
            "old {command:?} acquired a new target"
        );
        assert_eq!(state(&e), before);
        let fresh = session.offer(&e, stamp);
        assert!(
            dispatch_with_appearance(&mut e, &mut workspace, command, stamp, Some(fresh)).is_ok(),
            "fresh {command:?} did not recover"
        );
        assert_eq!(state(&e), before);
    }
}

#[test]
fn failed_reload_observations_retire_saved_status_and_old_write_authority() {
    let e = selected();
    let mut original = PresetStore::restore(Ok(None));
    original.add("Mine", changed(&e)).unwrap();
    let valid = original.export().unwrap();
    for invalid in [
        "{}".into(),
        valid.replace("\"version\":1", "\"version\":99"),
        " ".repeat(MAX_PRESET_BYTES + 1),
    ] {
        for pending in [false, true] {
            let mut store = PresetStore::restore(Ok(Some(valid.clone())));
            let request = if pending {
                Some(store.begin_save().unwrap())
            } else {
                None
            };
            let before = store.export().unwrap();
            assert!(store.reload(Ok(Some(invalid.clone()))).is_err());
            assert!(
                matches!(store.status(), PresetStatus::Unavailable(_)),
                "failed observation retained {:?}",
                store.status()
            );
            assert_eq!(store.export().unwrap(), before);
            assert!(store.begin_save().is_err());
            if let Some(request) = request {
                assert!(!store.save_is_current(&request));
                assert_eq!(store.finish_save(&request, Ok(())), Err(PresetError::Stale));
            }
        }
    }
}

#[test]
fn denied_replace_observation_uses_the_real_producer_and_preserves_presets() {
    let e = selected();
    let mut original = PresetStore::restore(Ok(None));
    original.add("Mine", changed(&e)).unwrap();
    let valid = original.export().unwrap();
    for pending in [false, true] {
        let mut store = PresetStore::restore(Ok(Some(valid.clone())));
        let request = if pending {
            Some(store.begin_save().unwrap())
        } else {
            None
        };
        assert!(matches!(
            store.begin_replace_observation(Err(StorageFailure::Denied)),
            Err(PresetError::Storage(StorageFailure::Denied))
        ));
        assert_eq!(store.status(), PresetStatus::Failed(StorageFailure::Denied));
        assert_eq!(store.export().unwrap(), valid);
        assert!(store.begin_save().is_err());
        if let Some(request) = request {
            assert!(!store.save_is_current(&request));
            assert_eq!(store.check_write(&request, Some(&valid)), Err(PresetError::Stale));
        }
    }
}

#[test]
fn closing_preview_returns_original_scope_and_retires_stale_focus_returns() {
    for applied in [false, true] {
        for scope in AppearanceScope::ALL {
            let mut e = selected();
            let mut session = AppearanceSession::default();
            session.choose(AppearanceSource::Preset(changed(&e)), None).unwrap();
            let offer = session.offer(&e, e.stamp());
            let request = offer.request(scope).unwrap();
            session.stage(request, offer).unwrap();
            if applied {
                e.apply(Edit::ApplyAppearance(request)).unwrap();
            }
            assert_eq!(session.close_preview(request, offer), Ok(scope));
            assert_eq!(session.request(), None);
            assert!(!session.is_current(offer));
            let returned = session.offer(&e, e.stamp());
            assert!(session.can_return_focus(returned));
            assert_eq!(session.close_preview(request, offer), Err(PresetError::Stale));
            session.choose(AppearanceSource::Selected, None).unwrap();
            assert!(!session.can_return_focus(returned));
            let fresh = session.offer(&e, e.stamp());
            let newer = fresh.request(scope).unwrap();
            session.stage(newer, fresh).unwrap();
            assert!(!session.can_return_focus(fresh));
            assert_eq!(session.close_preview(request, offer), Err(PresetError::Stale));
            assert_eq!(session.request(), Some(newer));
        }
    }
    let e = selected();
    let mut session = AppearanceSession::default();
    let offer = session.offer(&e, e.stamp());
    let old = offer.request(AppearanceScope::Selected).unwrap();
    let newer = offer.request(AppearanceScope::AllWidgets).unwrap();
    session.stage(old, offer).unwrap();
    session.stage(newer, offer).unwrap();
    assert!(session.is_current(offer));
    assert_eq!(session.close_preview(old, offer), Err(PresetError::Stale));
    assert_eq!(session.request(), Some(newer));
    assert!(!session.can_return_focus(offer));
}
