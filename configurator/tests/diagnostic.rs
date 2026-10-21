//! Candidate diagnostics and shared-renderer pixel regressions; no physical display qualification.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::preview::{Preview, PreviewOptions};

#[test]
/// Ambiguous widget IDs reject raw repair ownership and preserve each widget label instead of borrowing the first
/// match.
fn raw_labels_and_repairs_never_choose_the_first_duplicate_id_owner() {
    use dashboard_configurator::diagnostic::Draft;
    let mut config = default_config();
    config.widgets[1].id = config.widgets[0].id;
    let label = RawField::Widget(1, WidgetField::Label);
    let draft = Draft {
        config,
        raw: vec![RawInput {
            field: label,
            text: "Raw label".into(),
        }],
        error: EditorError::Parse(label),
    };
    let report = draft.validation_report();
    assert_eq!(report.raw_errors.len(), 1, "ambiguous raw owner must be rejected");
    assert_eq!(report.raw_errors[0].error, EditorError::AmbiguousTarget);
    assert_eq!(
        draft.label(&draft.config.widgets[0]),
        draft.config.widgets[0].label.as_str()
    );
    assert_eq!(
        draft.label(&draft.config.widgets[1]),
        draft.config.widgets[1].label.as_str()
    );
}

#[test]
/// Duplicate view IDs report ambiguity and cannot borrow a raw name from an arbitrary owner.
fn ambiguous_view_name_inspection_never_claims_the_first_owner_or_its_raw_name() {
    use dashboard_configurator::diagnostic::Draft;
    let mut config = default_config();
    config.views.push(config.views[0].clone()).unwrap();
    let draft = Draft {
        config,
        raw: vec![RawInput {
            field: RawField::ViewName(1),
            text: "Wrong owner".into(),
        }],
        error: EditorError::Parse(RawField::ViewName(1)),
    };
    let report = draft.validation_report();
    assert_eq!(report.raw_errors[0].error, EditorError::AmbiguousTarget);
    assert_eq!(draft.name(1), "AMBIGUOUS VIEW");
}

#[test]
/// Collecting validation retains 128 issues in validator order and counts the omitted remainder exactly.
fn collecting_report_is_bounded_with_an_exact_omitted_count() {
    let mut config = default_config();
    config.views.clear();
    config.widgets.clear();
    for view in 1..=8 {
        config
            .views
            .push(View {
                orientation: None,
                id: view,
                name: "MAX".try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8 {
            let mut w = default_widget(
                Channel::Oil,
                config.widgets.len() as u16 + 1,
                view,
                Placement {
                    column: cell % 4,
                    row: cell / 4,
                    span: Span::OneByOne,
                },
            );
            w.label.clear();
            w.display.decimals = 255;
            config.widgets.push(w).unwrap();
        }
    }
    config.history_seconds[0] = 0;
    let first = validate(config.clone()).unwrap_err();
    let report = dashboard_configurator::diagnostic::collect_candidate(&config);
    assert_eq!(report.issues.len(), 128);
    assert_eq!(report.omitted, 1);
    assert_eq!(report.issues[0].error, first);
    assert_eq!(report.first_config_error(), Some(ConfigError::Validation(first)));
    assert_eq!(
        report.describe(&config, &report.issues[1]),
        "/widgets/0/label (view 1, widget 1: \"\"): Text"
    );
}

#[test]
/// Raw parse, text-capacity and unavailable-field failures retain their own indices alongside semantic issues.
fn raw_parse_capacity_and_unavailable_errors_are_separate_from_semantic_errors() {
    use dashboard_configurator::diagnostic::{Draft, RawDiagnosticCategory};
    let mut config = default_config();
    config.widgets[1].display.decimals = 255;
    let field = RawField::Widget(1, WidgetField::Decimals);
    let draft = Draft {
        config,
        raw: vec![
            RawInput {
                field,
                text: "oops".into(),
            },
            RawInput {
                field: RawField::ViewName(1),
                text: "x".repeat(15),
            },
            RawInput {
                field: RawField::Widget(1, WidgetField::BandMinimum(0)),
                text: "oops".into(),
            },
        ],
        error: EditorError::Parse(field),
    };
    let report = draft.validation_report();
    assert_eq!(report.raw_errors.len(), 3);
    assert_eq!(report.raw_errors[0].category, RawDiagnosticCategory::Parse);
    assert_eq!(report.raw_errors[0].raw_index, 0);
    assert_eq!(report.raw_errors[1].category, RawDiagnosticCategory::Capacity);
    assert_eq!(report.raw_errors[2].category, RawDiagnosticCategory::Unavailable);
    assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
    assert_eq!(report.issues[0].location.widget_index(), Some(1));
    assert_eq!(report.omitted, 0);
    assert_eq!(draft.raw[0].text, "oops");
    assert_eq!(draft.config.widgets[0].display.decimals, 2);
}

#[test]
/// Raw failures consume the same 128-item report budget and contribute exact omitted counts.
fn raw_errors_share_the_retained_issue_budget() {
    use dashboard_configurator::diagnostic::Draft;
    let field = RawField::ControlHoldMilliseconds;
    let draft = Draft {
        config: default_config(),
        raw: (0..130)
            .map(|_| {
                RawInput {
                    field,
                    text: "oops".into(),
                }
            })
            .collect(),
        error: EditorError::Parse(field),
    };
    let report = draft.validation_report();
    assert_eq!(report.raw_errors.len(), 128);
    assert_eq!(report.issues.len(), 0);
    assert_eq!(report.omitted, 2);
}

#[test]
/// Zero reference rendering ignores an unused retained manual value without rewriting the rejected candidate or
/// accepted project.
fn review_diagnostic_zero_ignores_a_finite_out_of_domain_manual_reference() {
    let mut c = default_config();
    c.widgets.truncate(1);
    c.widgets[0].gauge.kind = GaugeKind::Horizontal;
    c.widgets[0].advanced.reference_mode = ReferenceMode::Zero;
    c.widgets[0].advanced.reference = 999.;
    let mut e = Editor::new(c).unwrap();
    e.select(Selection {
        view: 1,
        widget: Some(1),
    })
    .unwrap();
    e.set_raw(
        e.field_token(),
        RawField::Widget(1, WidgetField::SmoothingMilliseconds),
        "49".into(),
    )
    .unwrap();
    let d = e.diagnostic().unwrap();
    let accepted = e.preview().clone();
    let mut oracle = dashboard_configurator::diagnostic::Draft {
        config: d.config.clone(),
        raw: d.raw.clone(),
        error: d.error,
    };
    oracle.config.widgets[0].advanced.reference = 0.;
    let mut actual = Preview::new();
    let mut expected = Preview::new();
    actual
        .render_diagnostic(e.preview(), &d, 1, PreviewOptions::default())
        .unwrap();
    expected
        .render_diagnostic(e.preview(), &oracle, 1, PreviewOptions::default())
        .unwrap();
    assert!(
        actual.rgb565() == expected.rgb565(),
        "Zero marker must ignore the retained finite manual value"
    );
    assert_eq!(e.preview(), &accepted);
    assert_eq!(d.config.widgets[0].advanced.reference, 999.);
}

#[test]
/// A rejected shared history duration marks dependent widgets and preserves the attempted seconds in diagnostics.
fn invalid_typed_shared_history_marks_associated_widgets_and_lists_the_attempt() {
    let mut e = selected();
    assert!(
        e.apply(Edit::SetHistory {
            channel: Channel::Boost,
            seconds: 999
        })
        .is_err()
    );
    let d = e.diagnostic().unwrap();
    assert!(d.affected(e.preview(), 1).contains(&1));
    assert!(
        d.descriptions(e.preview())
            .iter()
            .any(|s| s.contains("999") && s.contains("history"))
    );
}

#[test]
/// Invalid reference, readout, timing and band settings retain visible tile content beneath the invalid banner.
fn invalid_advanced_fields_keep_the_rejected_tile_inspectable() {
    let mut editor = selected();
    let mut w = editor.preview().config().widgets[0].clone();
    w.gauge.kind = GaugeKind::Horizontal;
    w.advanced.reference_mode = ReferenceMode::Manual;
    w.advanced.reference = f32::NAN;
    w.advanced.readout = Readout::Fahrenheit;
    w.advanced.smoothing_ms = 1;
    w.advanced.peak_hold_ms = 1;
    w.advanced
        .set_bands(&[Band::new(999., 1000., Color(0xF800), "RED").unwrap()])
        .unwrap();
    assert!(editor.apply(Edit::UpdateWidget(w)).is_err());
    let mut preview = Preview::new();
    preview
        .render_diagnostic(
            editor.preview(),
            &editor.diagnostic().unwrap(),
            1,
            PreviewOptions::default(),
        )
        .unwrap();
    // The red invalid header alone is insufficient: the rejected tile must remain.
    let nonblack = (26..133)
        .flat_map(|y| (0..80).map(move |x| (y * 320 + x) * 2))
        .any(|at| preview.rgb565()[at..at + 2] != [0, 0]);
    assert!(nonblack, "rejected advanced widget disappeared");
}

/// Returns the default editor with widget 1 selected in view 1; panics if fixture selection fails.
fn selected() -> Editor {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    editor
}

#[test]
/// Rejected band colours remain visible, while nonfinite sample rendering preserves both previously published pixel
/// buffers.
fn invalid_band_contrast_bounds_and_overlap_keep_exact_raw_colour_and_failed_render_keeps_both_buffers() {
    let mut config = default_config();
    config.widgets.truncate(1);
    config.widgets[0].gauge.kind = GaugeKind::Horizontal;
    let mut e = Editor::new(config).unwrap();
    let before = e.preview().clone();
    for bounds in [[(0., 1.), (1., 2.)], [(999., 1000.), (0., 1.)], [(0., 2.), (1., 3.)]] {
        let mut w = e.preview().config().widgets[0].clone();
        w.advanced
            .set_bands(&bounds.map(|(a, b)| Band::new(a, b, Color(0x001F), "BLUE").unwrap()))
            .unwrap();
        assert!(e.apply(Edit::UpdateWidget(w)).is_err());
        let draft = e.diagnostic().unwrap();
        let mut p = Preview::new();
        p.render_diagnostic(e.preview(), &draft, 1, PreviewOptions::default())
            .unwrap();
        assert!(
            p.rgb565()[320 * 26 * 2..]
                .as_chunks::<2>()
                .0
                .iter()
                .any(|pixel| *pixel == 0x001Fu16.to_be_bytes()),
            "raw blue band vanished"
        );
        let rgb = p.rgb565().to_vec();
        let rgba = p.rgba().to_vec();
        let error = p.render_diagnostic(
            e.preview(),
            &draft,
            1,
            PreviewOptions {
                values: Some([f32::NAN; 8]),
                ..Default::default()
            },
        );
        assert!(error.is_err());
        assert_eq!(p.rgb565(), rgb);
        assert_eq!(p.rgba(), rgba);
        assert_eq!(e.preview(), &before);
        e.discard_raw().unwrap();
    }
}

#[test]
/// An off-grid 2x1 candidate is projected within the screen for inspection without changing accepted geometry.
fn rejected_out_of_grid_size_is_inspectable_at_a_bounded_projected_position() {
    let mut config = default_config();
    config.widgets.truncate(1);
    let mut e = Editor::new(config).unwrap();
    let before = e.preview().clone();
    let mut w = e.editing_config().widgets[0].clone();
    w.placement.column = 3;
    w.placement.span = Span::TwoByOne;
    w.gauge.kind = GaugeKind::Dial;
    assert!(e.apply(Edit::UpdateWidget(w)).is_err());
    let mut p = Preview::new();
    let draft = e.diagnostic().unwrap();
    p.render_diagnostic(e.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    // Rejected 2x1 at column4 projects to columns3/4, retaining its chosen style.
    assert!((160..320).any(|x| {
        let at = (80 * 320 + x) * 2;
        p.rgb565()[at..at + 2] != [0, 0]
    }));
    assert_eq!(e.preview(), &before);
}

#[test]
/// Oversized/control-character labels and invalid numbers stay inspectable, block export and cannot paint outside their
/// tile.
fn raw_overflow_and_unrenderable_numbers_remain_visible_and_never_serialize() {
    let mut editor = selected();
    let accepted = editor.export_binary().unwrap();
    for label in [
        "THIS LABEL IS TOO LONG".to_string(),
        "x".repeat(1024),
        "λ\n\tX".into(),
        "".into(),
    ] {
        editor
            .set_raw(
                editor.field_token(),
                RawField::Widget(1, WidgetField::Label),
                label.clone(),
            )
            .unwrap();
        editor
            .set_raw(
                editor.field_token(),
                RawField::Widget(1, WidgetField::Decimals),
                "NaN".into(),
            )
            .unwrap();
        let draft = editor.diagnostic().unwrap();
        assert_eq!(draft.label(&draft.config.widgets[0]), label);
        assert_eq!(draft.raw.len(), 2);
        assert!(draft.descriptions(editor.preview()).iter().any(|s| s.contains("NaN")));
        assert_eq!(draft.affected(editor.preview(), 1), [1]);
        let mut preview = Preview::new();
        preview
            .render_diagnostic(
                editor.preview(),
                &draft,
                1,
                PreviewOptions {
                    warnings: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(preview.warning_page(), None);
        let invalid_pixels = preview.rgb565().to_vec();
        assert!(editor.export_binary().is_err());
        assert!(editor.export_json().is_err());
        editor.discard_raw().unwrap();
        assert_eq!(editor.export_binary().unwrap(), accepted);
        preview.render(editor.preview(), 1, PreviewOptions::default()).unwrap();
        assert_ne!(
            &invalid_pixels[..26 * 320 * 2],
            &preview.rgb565()[..26 * 320 * 2],
            "invalid watermark missing"
        );
        // Invalid labels cannot damage any other cell or the header lane.
        for y in 26..240 {
            for x in 0..320 {
                if x < 80 && y < 133 {
                    continue;
                }
                let at = (y * 320 + x) * 2;
                assert_eq!(&invalid_pixels[at..at + 2], &preview.rgb565()[at..at + 2]);
            }
        }
    }
}

#[test]
/// Rejected overlap retains attempted size, marks all affected tiles, retires pending saves and clears on discard.
fn rejected_overlap_shows_attempt_and_marks_both_cells_then_discard_restores_pixels() {
    let mut editor = selected();
    let saved = editor.begin_save().unwrap();
    let accepted = editor.preview().clone();
    assert!(
        editor
            .apply(Edit::ResizeWidget {
                id: 1,
                span: Span::TwoByTwo
            })
            .is_err()
    );
    assert!(!editor.save_is_current(saved.token()));
    let draft = editor.diagnostic().unwrap();
    assert_eq!(draft.config.widgets[0].placement.span, Span::TwoByTwo);
    let affected = draft.affected(editor.preview(), 1);
    for id in [1, 2, 5, 6] {
        assert!(affected.contains(&id));
    }
    let mut preview = Preview::new();
    preview
        .render_diagnostic(editor.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    let attempted = preview.rgb565().to_vec();
    assert_eq!(editor.preview(), &accepted);
    editor.discard_raw().unwrap();
    preview.render(editor.preview(), 1, PreviewOptions::default()).unwrap();
    assert_ne!(attempted, preview.rgb565());
    assert!(editor.diagnostic().is_none());
    assert!(editor.export_binary().is_ok());
}

#[test]
/// Offscreen positions and invalid raw values remain diagnosable; out-of-range render time preserves prior pixels.
fn offscreen_geometry_and_invalid_settings_are_bounded_and_explicit() {
    let mut editor = selected();
    assert!(
        editor
            .apply(Edit::MoveWidget {
                id: 1,
                view: 1,
                column: 255,
                row: 255
            })
            .is_err()
    );
    let draft = editor.diagnostic().unwrap();
    assert!(
        draft
            .descriptions(editor.preview())
            .iter()
            .any(|s| s.contains("outside screen"))
    );
    let mut preview = Preview::new();
    preview
        .render_diagnostic(editor.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    editor.discard_raw().unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Decimals),
            "255".into(),
        )
        .unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::ViewName(1),
            "LONG REJECTED VIEW NAME".into(),
        )
        .unwrap();
    let draft = editor.diagnostic().unwrap();
    assert_eq!(draft.config.widgets[0].display.decimals, 255);
    assert_eq!(draft.name(1), "LONG REJECTED VIEW NAME");
    preview
        .render_diagnostic(editor.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    let pixels = preview.rgb565().to_vec();
    assert!(
        preview
            .render_diagnostic(
                editor.preview(),
                &draft,
                1,
                PreviewOptions {
                    sample_ms: u64::MAX,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(preview.rgb565(), pixels);
}

#[test]
/// Only complete raw-field repair clears diagnostics and restores export, retiring earlier field authority.
fn correcting_all_raw_fields_atomically_removes_diagnostics_and_retires_old_tokens() {
    let mut editor = selected();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Decimals),
            "NaN".into(),
        )
        .unwrap();
    let stale = editor.field_token();
    editor
        .set_raw(stale, RawField::Widget(1, WidgetField::Label), "OVERFLOW LABEL".into())
        .unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Decimals),
            "2".into(),
        )
        .unwrap();
    assert!(editor.diagnostic().is_some());
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Label),
            "BOOST".into(),
        )
        .unwrap();
    assert!(editor.diagnostic().is_none());
    assert_eq!(editor.preview().config().widgets[0].display.decimals, 2);
    assert_eq!(
        editor.set_raw(stale, RawField::Widget(1, WidgetField::Decimals), "NaN".into()),
        Err(EditorError::StaleToken)
    );
    assert!(editor.export_binary().is_ok());
}

#[test]
/// Invalid polling and fuel assumptions identify their dependent widgets and retain the attempted values.
fn rejected_global_choices_mark_dependent_widgets_and_list_attempted_values() {
    let mut editor = selected();
    let mut source = definition(DefinitionId::Disabled).source(Channel::Oil);
    editor
        .apply(Edit::SetSource {
            channel: Channel::Oil,
            source,
        })
        .unwrap();
    source.poll = Poll::EveryMs(500);
    assert!(
        editor
            .apply(Edit::SetSource {
                channel: Channel::Oil,
                source
            })
            .is_err()
    );
    let draft = editor.diagnostic().unwrap();
    assert!(draft.affected(editor.preview(), 1).contains(&5));
    assert!(
        draft
            .descriptions(editor.preview())
            .iter()
            .any(|s| s.contains("EveryMs(500)"))
    );
    editor.discard_raw().unwrap();
    assert!(
        editor
            .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: f32::NAN }))
            .is_err()
    );
    let draft = editor.diagnostic().unwrap();
    assert!(draft.affected(editor.preview(), 1).contains(&2));
    assert!(draft.descriptions(editor.preview()).iter().any(|s| s.contains("NaN")));
}

#[test]
/// Rejected label clipping and oversized placement match independently constructed text and border pixel expectations.
fn rejected_label_and_placement_pixels_match_independent_geometry() {
    use dashboard_core::colors::{BLACK, GRAY, WHITE};
    use dashboard_core::display::{FrameBuffer, St7789Renderer};
    use embedded_graphics::mono_font::MonoTextStyle;
    use embedded_graphics::mono_font::ascii::FONT_6X10;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    use embedded_graphics::text::Text;

    let mut editor = selected();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Label),
            "TOO LONG LABEL ATTEMPT".into(),
        )
        .unwrap();
    let mut preview = Preview::new();
    preview
        .render_diagnostic(
            editor.preview(),
            &editor.diagnostic().unwrap(),
            1,
            PreviewOptions::default(),
        )
        .unwrap();
    let mut oracle = Box::new(FrameBuffer::new());
    let mut target = St7789Renderer::new(&mut oracle);
    let lane = Rectangle::new(Point::new(2, 28), Size::new(76, 12));
    let mut clipped = target.clipped(&lane);
    clipped.clear(BLACK).unwrap();
    Text::with_text_style(
        "TOO LONG LABEL ATTEMPT",
        Point::new(40, 37),
        MonoTextStyle::new(&FONT_6X10, WHITE),
        dashboard_core::ui::CENTERED,
    )
    .draw(&mut clipped)
    .unwrap();
    for y in 28..40 {
        for x in 2..78 {
            let at = (y * 320 + x) * 2;
            assert_eq!(&preview.rgb565()[at..at + 2], &oracle[at..at + 2]);
        }
    }

    let mut config = default_config();
    config.widgets.truncate(2);
    let mut editor = Editor::new(config).unwrap();
    assert!(
        editor
            .apply(Edit::ResizeWidget {
                id: 1,
                span: Span::TwoByTwo
            })
            .is_err()
    );
    preview
        .render_diagnostic(
            editor.preview(),
            &editor.diagnostic().unwrap(),
            1,
            PreviewOptions::default(),
        )
        .unwrap();
    // The attempted widget's border extends into an otherwise empty bottom row.
    let at = (239 * 320 + 5) * 2;
    assert_eq!(&preview.rgb565()[at..at + 2], &GRAY.into_storage().to_be_bytes());
    editor.discard_raw().unwrap();
    preview.render(editor.preview(), 1, PreviewOptions::default()).unwrap();
    assert_eq!(&preview.rgb565()[at..at + 2], &BLACK.into_storage().to_be_bytes());
}

#[test]
/// Rejected low-contrast widget and warning colours remain visible beneath the invalid watermark until discarded.
fn invisible_widget_and_warning_colours_remain_inspectable_under_invalid_banner() {
    let mut editor = selected();
    let mut preview = Preview::new();
    // White background with white text: accepted document stays black.
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Color(ColorRole::Background)),
            "#FFFFFF".into(),
        )
        .unwrap();
    preview
        .render_diagnostic(
            editor.preview(),
            &editor.diagnostic().unwrap(),
            1,
            PreviewOptions::default(),
        )
        .unwrap();
    let at = (130 * 320 + 3) * 2;
    assert_eq!(&preview.rgb565()[at..at + 2], &[255, 255]);
    assert!(editor.export_json().is_err());
    editor.discard_raw().unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::WarningColor { foreground: true },
            "#FF0000".into(),
        )
        .unwrap();
    preview
        .render_diagnostic(
            editor.preview(),
            &editor.diagnostic().unwrap(),
            1,
            PreviewOptions::default(),
        )
        .unwrap();
    // The deliberately unreadable red/red warning sample remains visible.
    let at = (60 * 320 + 60) * 2;
    assert_eq!(&preview.rgb565()[at..at + 2], &[248, 0]);
    let at = (5 * 320 + 5) * 2;
    assert_eq!(&preview.rgb565()[at..at + 2], &[248, 0], "invalid watermark background");
    editor.discard_raw().unwrap();
    preview.render(editor.preview(), 1, PreviewOptions::default()).unwrap();
    assert!(editor.export_json().is_ok());
}

#[test]
/// An invalid widget warning palette retains that widget colour in diagnostics and blocks save.
fn invalid_widget_warning_pair_uses_its_own_diagnostic_colours() {
    let mut editor = selected();
    let mut preview = Preview::new();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Color(ColorRole::WarningBackground)),
            "#FFFFFF".into(),
        )
        .unwrap();
    let draft = editor.diagnostic().unwrap();
    assert_eq!(draft.affected(editor.preview(), 1), vec![1]);
    preview
        .render_diagnostic(editor.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    let at = (60 * 320 + 60) * 2;
    assert_eq!(&preview.rgb565()[at..at + 2], &[255, 255]);
    assert!(editor.begin_save().is_err());
}
#[test]
/// Error links retain positional raw-repair authority without changing selection or accepted state; successful repair
/// retires the link.
fn indexed_error_links_keep_actual_raw_authority_without_selection_or_acceptance() {
    use dashboard_configurator::field_registry::{Registry, TargetOwner};
    let mut editor = Editor::new(default_config()).unwrap();
    let selection = editor.selection();
    let index = 1;
    let id = editor.preview().config().widgets[index].id;
    let owner = editor
        .object_inspection_target(InspectionOwner::Widget { index })
        .unwrap();
    editor
        .set_located_raw(owner, RawField::Widget(id, WidgetField::Label), "bad\n".into())
        .unwrap();
    let before = (
        editor.stamp(),
        editor.history_len(),
        editor.raw_fields().to_vec(),
        editor.preview().config().clone(),
    );
    let links = Registry::error_targets(&editor);
    let raw = links
        .iter()
        .find(|link| {
            link.field
                .is_some_and(|field| field.owner() == TargetOwner::Widget(index))
                && editor.inspect(link.inspection.unwrap()).unwrap().raw_index == Some(0)
        })
        .expect("semantic-invalid retained raw input must keep authentic repair authority");
    assert_eq!(
        editor.inspect(raw.inspection.unwrap()).unwrap().widget_index,
        Some(index)
    );
    assert_eq!(editor.selection(), selection);
    assert_eq!(
        (
            editor.stamp(),
            editor.history_len(),
            editor.raw_fields().to_vec(),
            editor.preview().config().clone()
        ),
        before
    );
    editor.set_inspected_raw(raw.inspection.unwrap(), "Oil".into()).unwrap();
    assert!(editor.inspect(raw.inspection.unwrap()).is_err());
    assert_eq!(editor.selection(), selection);
}
