//! Accepted-resource accounting and identity-bound artifact evidence regressions.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::events::{Category, EventFilter, EventStore, Severity};
use dashboard_configurator::resources::*;

#[test]
/// Resource counts match actual JSON/EAV/saved-envelope bytes without changing save state; selection affects only
/// envelope identity/size.
fn actual_serializations_and_read_only_saved_envelope_share_bytes() {
    let mut editor = Editor::default();
    let before_status = editor.save_status();
    let before_revision = editor.revision();
    let saved = editor.saved_envelope_json().unwrap();
    assert_eq!(editor.save_status(), before_status);
    assert_eq!(editor.revision(), before_revision);
    let mut cache = ResourceCache::default();
    let report = cache.inspect(&editor).unwrap();
    assert_eq!(report.availability, ResourceAvailability::Current);
    let current = report.accepted.unwrap();
    assert_eq!(current.eav_bytes, editor.export_binary().unwrap().len());
    assert_eq!(current.json_bytes, editor.export_json().unwrap().len());
    assert_eq!(current.saved_bytes, saved.len());
    assert_eq!(current.saved_overhead, saved.len() - current.json_bytes);
    assert_eq!(current.limits.eav_bytes, MAX_FILE_BYTES);
    assert_eq!(current.limits.json_bytes, MAX_JSON_BYTES);
    assert_eq!(current.limits.saved_bytes, MAX_SAVED_BYTES);
    assert_eq!(editor.begin_save().unwrap().json(), saved);

    // Selection changes persisted envelope bytes and stamp, not EAV bytes.
    let eav = editor.export_binary().unwrap();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    let selected = cache.inspect(&editor).unwrap().accepted.unwrap();
    assert_eq!(editor.export_binary().unwrap(), eav);
    assert_ne!(selected.stamp, current.stamp);
    assert_ne!(selected.saved_bytes, current.saved_bytes);
}

#[test]
/// Pending JSON hides current exportability even when editor stamp and cached accepted report remain unchanged.
fn blocked_json_rechecks_readiness_even_with_unchanged_editor_stamp() {
    let mut editor = Editor::default();
    let mut cache = ResourceCache::default();
    let current = cache.inspect(&editor).unwrap().accepted.unwrap();
    let stamp = editor.stamp();
    let handle = editor.open_json().unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::Current
    );
    let text = editor.export_json().unwrap();
    let parse = editor.set_json_text(handle, text).unwrap();
    assert_eq!(editor.stamp(), stamp);
    let blocked = cache.inspect(&editor).unwrap();
    assert_eq!(
        blocked.availability,
        ResourceAvailability::LastAccepted(ActionBlock::JsonPending)
    );
    assert_eq!(blocked.accepted.unwrap().stamp, current.stamp);
    editor.finish_json_parse(parse).unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::LastAccepted(ActionBlock::JsonPending)
    );
    editor.discard_json().unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::Current
    );
}

#[test]
/// Invalid draft, active pointer and import block admission; storage failure leaves a valid exportable project.
fn invalid_pointer_import_and_storage_failure_are_distinct() {
    let mut editor = Editor::default();
    let mut cache = ResourceCache::default();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::NotExportable(ActionBlock::InvalidDraft)
    );
    editor.discard_raw().unwrap();
    let pointer = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::NotExportable(ActionBlock::PointerActive)
    );
    editor.cancel_pointer(pointer).unwrap();
    let import = editor.begin_import().unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::NotExportable(ActionBlock::ImportPending)
    );
    editor.cancel_import(import).unwrap();
    let save = editor.begin_save().unwrap();
    assert_eq!(
        editor.acknowledge_save(save.token(), Err(StorageFailure::Unavailable)),
        Err(EditorError::Storage(StorageFailure::Unavailable))
    );
    assert_eq!(editor.save_status(), SaveStatus::Failed(StorageFailure::Unavailable));
    assert_eq!(
        cache.inspect(&editor).unwrap().availability,
        ResourceAvailability::Current
    );
    assert!(editor.export_binary().is_ok());
}

#[test]
/// Maximum view/widget counts and mixed spans report occupied-cell masks rather than equating widgets with cells.
fn maximum_counts_and_mixed_spans_use_actual_occupied_cells() {
    let mut config = default_config();
    config.views.clear();
    config.widgets.clear();
    for view in 1..=8_u8 {
        config
            .views
            .push(View {
                orientation: None,
                id: view,
                name: format!("View {view}").as_str().try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8_u8 {
            let id = u16::from(view - 1) * 8 + u16::from(cell) + 1;
            config
                .widgets
                .push(default_widget(
                    Channel::Oil,
                    id,
                    view,
                    Placement {
                        column: cell % 4,
                        row: cell / 4,
                        span: Span::OneByOne,
                    },
                ))
                .unwrap();
        }
    }
    let editor = Editor::new(config).unwrap();
    let report = ResourceCache::default().inspect(&editor).unwrap().accepted.unwrap();
    assert_eq!((report.views_used, report.widgets_used), (MAX_VIEWS, MAX_WIDGETS));
    assert_eq!(report.views.len(), MAX_VIEWS);
    assert!(
        report
            .views
            .iter()
            .all(|view| view.occupied_mask == 0xFF && view.occupied_cells == 8 && view.widget_count == 8)
    );

    let mut config = default_config();
    config.widgets.clear();
    for (id, placement) in [
        (
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        ),
        (
            2,
            Placement {
                column: 2,
                row: 0,
                span: Span::TwoByOne,
            },
        ),
        (
            3,
            Placement {
                column: 2,
                row: 1,
                span: Span::OneByOne,
            },
        ),
    ] {
        config
            .widgets
            .push(default_widget(Channel::Oil, id, 1, placement))
            .unwrap();
    }
    let editor = Editor::new(config).unwrap();
    let view = ResourceCache::default()
        .inspect(&editor)
        .unwrap()
        .accepted
        .unwrap()
        .views[0]
        .clone();
    assert_eq!(view.occupied_mask, 0x7F);
    assert_eq!(view.occupied_cells, 7);
    assert_eq!(view.widget_count, 3);
}

#[test]
/// Source reports preserve catalogue evidence/unavailability and canonical units without claiming exact ECU support.
fn source_catalogue_and_unit_basis_are_explicitly_unqualified() {
    let editor = Editor::default();
    let report = ResourceCache::default().inspect(&editor).unwrap().accepted.unwrap();
    assert_eq!(report.sources.len(), Channel::ALL.len());
    let water = &report.sources[Channel::Water.index()];
    assert_eq!(water.channel, Channel::Water);
    assert_eq!(
        water.configured.evidence,
        definition(water.configured.definition).evidence
    );
    assert_eq!(water.status, SourceSelectionStatus::ExecutableSelectionUnqualified);
    assert_eq!(
        water.catalogue_references,
        definition(water.configured.definition).references()
    );
    assert!(water.catalogue_references.len() >= 2);
    let egt = &report.sources[Channel::Egt.index()];
    assert_eq!(egt.catalogue_unavailable, Some(UnavailableReason::SensorUnresolved));
    let boost = &report.sources[Channel::Boost.index()];
    assert_eq!(
        boost.status,
        SourceSelectionStatus::Unavailable(UnavailableReason::ScalingUnresolved)
    );
    let lambda = &report.sources[Channel::Lambda.index()];
    assert_eq!(
        lambda.status,
        SourceSelectionStatus::Unavailable(UnavailableReason::CandidateDecoder)
    );
    assert_eq!(canonical_unit(Channel::Boost), "bar");
    assert_eq!(canonical_unit(Channel::Lambda), "λ");
    assert_eq!(canonical_unit(Channel::Oil), "°C");
    assert_eq!(canonical_unit(Channel::Battery), "V");
    assert_eq!(boost_display_unit(true), "PSI");
    assert_eq!(report.stoich_ratio, editor.preview().stoich());
    assert_eq!(report.afr_for_lambda(1.0), report.stoich_ratio);
}

#[test]
/// Disabled source, requested polling off and unresolved transport route remain distinct resource statuses.
fn source_status_separates_disabled_poll_off_and_unresolved_route() {
    let mut config = default_config();
    config.sources[Channel::Water.index()] = definition(DefinitionId::Disabled).source(Channel::Water);
    config.sources[Channel::Oil.index()].poll = Poll::Off;
    config.sources[Channel::Battery.index()].route = Route::Unknown;
    let editor = Editor::new(config).unwrap();
    let sources = ResourceCache::default()
        .inspect(&editor)
        .unwrap()
        .accepted
        .unwrap()
        .sources;
    assert_eq!(sources[Channel::Water.index()].status, SourceSelectionStatus::Disabled);
    assert_eq!(sources[Channel::Oil.index()].status, SourceSelectionStatus::PollOff);
    assert_eq!(
        sources[Channel::Battery.index()].status,
        SourceSelectionStatus::RouteUnknown
    );
}

#[test]
/// Resource widget rows retain configured primary/secondary units while canonical channel units remain unchanged.
fn widget_rows_preserve_configured_readouts_and_secondary_units() {
    let mut config = default_config();
    config.widgets.clear();
    let mut boost = default_widget(
        Channel::Boost,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByOne,
        },
    );
    boost.advanced.readout = Readout::Kpa;
    boost.advanced.secondary = Secondary::Psi;
    config.widgets.push(boost).unwrap();
    let mut water = default_widget(
        Channel::Water,
        2,
        1,
        Placement {
            column: 2,
            row: 0,
            span: Span::OneByOne,
        },
    );
    water.advanced.readout = Readout::Fahrenheit;
    config.widgets.push(water).unwrap();
    config
        .widgets
        .push(default_widget(
            Channel::Lambda,
            3,
            1,
            Placement {
                column: 0,
                row: 1,
                span: Span::TwoByOne,
            },
        ))
        .unwrap();
    let editor = Editor::new(config).unwrap();
    let report = ResourceCache::default().inspect(&editor).unwrap().accepted.unwrap();
    assert_eq!(report.widgets[0].channel, WidgetChannel::Boost);
    assert_eq!(report.widgets[0].readout, Readout::Kpa);
    assert_eq!(report.widgets[0].secondary, Secondary::Psi);
    assert!(report.widgets[0].capabilities.secondary);
    assert_eq!(report.widgets[1].readout, Readout::Fahrenheit);
    assert_eq!(report.widgets[2].readout, Readout::LambdaAndAfr);
    assert_eq!(canonical_unit(report.widgets[2].channel.scalar().unwrap()), "λ");
}

/// Synthetic full option identity; fields independently participate in equality.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Options {
    /// Vehicle/demo selection label.
    mode: &'static str,
    /// Named clock profile identity.
    profile: &'static str,
    /// Requested CPU clock in MHz.
    cpu: u32,
    /// Requested SPI clock in MHz.
    spi: u32,
    /// Requested regulator voltage in millivolts.
    voltage_mv: u16,
    /// Cargo optimization profile label.
    optimization: &'static str,
    /// Enabled feature names in identity order.
    features: &'static [&'static str],
    /// Outlined-render feature selection.
    outline: bool,
    /// Parallel rendering feature selection.
    parallel: bool,
    /// Forced full-frame transfer feature selection.
    full_frame: bool,
    /// Frame tracing feature selection.
    frame_trace: bool,
    /// Tearing-effect observation tracing selection.
    te_trace: bool,
    /// Explicit Expert-overclock option acknowledgment.
    expert: bool,
}
/// Returns a complete synthetic build-option record used only for equality/identity tests.
fn options() -> Options {
    Options {
        mode: "Vehicle",
        profile: "stock",
        cpu: 150,
        spi: 37,
        voltage_mv: 1100,
        optimization: "release3",
        features: &["frame-trace", "te-trace"],
        outline: true,
        parallel: true,
        full_frame: false,
        frame_trace: true,
        te_trace: true,
        expert: false,
    }
}
/// Returns populated synthetic provenance identities, not hashes of a measured local build.
fn ids() -> IdentityView<'static> {
    IdentityView {
        source_inventory: Some("source-a"),
        compiler: Some("rustc-a"),
        toolchain: Some("nightly-a"),
        tools: Some("tools-a"),
    }
}
/// Returns synthetic RAM/flash/stack byte facts for artifact matcher admission tests.
fn facts() -> TargetMeasurements {
    TargetMeasurements {
        static_ram_bytes: Some(458_296),
        flash_bytes: Some(1_000_000),
        core0_stack_bytes: Some(8_192),
        core1_stack_bytes: Some(4_096),
    }
}
/// Returns all-required-gates-present-and-passed synthetic evidence for matcher tests.
fn gates() -> GateEvidence {
    GateEvidence {
        elf: Some(true),
        flash_layout: Some(true),
        static_ram: Some(true),
        stack: Some(true),
        other_required: Some(true),
    }
}

#[test]
/// Artifact matching requires live editor/session selection and all target measurements; equal bytes in another editor
/// are stale.
fn one_matcher_requires_complete_current_local_and_manifest_identity() {
    let editor = Editor::default();
    let selection = 1_u64;
    let opts = options();
    let current = CurrentEvidence {
        selection: Some(&selection),
        digest: Some("eav-sha256-a"),
        identities: ids(),
        options: Some(&opts),
    };
    let captured = CapturedAttempt {
        stamp: editor.stamp(),
        selection: &selection,
        digest: "eav-sha256-a",
        revision: editor.accepted_revision().get(),
        identities: ids(),
        options: &opts,
    };
    let artifact = ArtifactEvidence {
        digest: Some("eav-sha256-a"),
        revision: Some(editor.accepted_revision().get()),
        identities: ids(),
        options: Some(&opts),
        measurements: facts(),
        gates: gates(),
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, None),
        ArtifactEvidenceState::NotMeasured
    );
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&artifact)),
        ArtifactEvidenceState::Matching
    );
    let replacement = Editor::default();
    assert_eq!(replacement.accepted_revision(), editor.accepted_revision());
    assert_eq!(replacement.export_binary().unwrap(), editor.export_binary().unwrap());
    assert_eq!(
        match_artifact(&replacement, &current, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let mut blocked = Editor::default();
    let pointer = blocked.begin_pointer(1, PointerTarget::Widget(1)).unwrap();
    let blocked_capture = CapturedAttempt {
        stamp: blocked.stamp(),
        ..captured
    };
    assert_eq!(
        match_artifact(&blocked, &current, &blocked_capture, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    blocked.cancel_pointer(pointer).unwrap();
    let missing = ArtifactEvidence {
        measurements: TargetMeasurements::default(),
        ..artifact
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&missing)),
        ArtifactEvidenceState::Stale
    );
    let exhausted_selection = CurrentEvidence {
        selection: None,
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &exhausted_selection, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
}

#[test]
/// Each changed option/provenance/revision, missing target fact and failed/unknown gate independently invalidates a
/// match.
fn every_option_family_and_required_provenance_mismatch_is_stale() {
    let editor = Editor::default();
    let selection = 1_u64;
    let opts = options();
    let current = CurrentEvidence {
        selection: Some(&selection),
        digest: Some("digest"),
        identities: ids(),
        options: Some(&opts),
    };
    let captured = CapturedAttempt {
        stamp: editor.stamp(),
        selection: &selection,
        digest: "digest",
        revision: 0,
        identities: ids(),
        options: &opts,
    };
    let artifact = ArtifactEvidence {
        digest: Some("digest"),
        revision: Some(0),
        identities: ids(),
        options: Some(&opts),
        measurements: facts(),
        gates: gates(),
    };
    let check = |variant: Options| {
        let changed = CurrentEvidence {
            options: Some(&variant),
            ..current
        };
        assert_eq!(
            match_artifact(&editor, &changed, &captured, Some(&artifact)),
            ArtifactEvidenceState::Stale,
            "{variant:?}"
        );
    };
    let mut v = opts.clone();
    v.mode = "Demo";
    check(v);
    let mut v = opts.clone();
    v.profile = "280-70";
    check(v);
    let mut v = opts.clone();
    v.cpu = 280;
    check(v);
    let mut v = opts.clone();
    v.spi = 70;
    check(v);
    let mut v = opts.clone();
    v.voltage_mv = 1300;
    check(v);
    let mut v = opts.clone();
    v.optimization = "release2";
    check(v);
    let mut v = opts.clone();
    v.features = &["frame-trace"];
    check(v);
    let mut v = opts.clone();
    v.outline = false;
    check(v);
    let mut v = opts.clone();
    v.parallel = false;
    check(v);
    let mut v = opts.clone();
    v.full_frame = true;
    check(v);
    let mut v = opts.clone();
    v.frame_trace = false;
    check(v);
    let mut v = opts.clone();
    v.te_trace = false;
    check(v);
    let mut v = opts.clone();
    v.expert = true;
    check(v);
    let next_token = 2_u64;
    let changed = CurrentEvidence {
        selection: Some(&next_token),
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &changed, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let bad_digest = CurrentEvidence {
        digest: Some("other"),
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &bad_digest, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let bad_source = CurrentEvidence {
        identities: IdentityView {
            source_inventory: Some("source-b"),
            ..ids()
        },
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &bad_source, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let bad_compiler = CurrentEvidence {
        identities: IdentityView {
            compiler: Some("rustc-b"),
            ..ids()
        },
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &bad_compiler, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let bad_tools = CurrentEvidence {
        identities: IdentityView {
            tools: Some("tools-b"),
            ..ids()
        },
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &bad_tools, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let missing = CurrentEvidence {
        identities: IdentityView {
            toolchain: None,
            ..ids()
        },
        ..current
    };
    assert_eq!(
        match_artifact(&editor, &missing, &captured, Some(&artifact)),
        ArtifactEvidenceState::Stale
    );
    let failed = ArtifactEvidence {
        gates: GateEvidence {
            stack: Some(false),
            ..gates()
        },
        ..artifact
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&failed)),
        ArtifactEvidenceState::Stale
    );
    for unknown_gates in [
        GateEvidence { elf: None, ..gates() },
        GateEvidence {
            flash_layout: None,
            ..gates()
        },
        GateEvidence {
            static_ram: None,
            ..gates()
        },
        GateEvidence { stack: None, ..gates() },
        GateEvidence {
            other_required: None,
            ..gates()
        },
    ] {
        let incomplete = ArtifactEvidence {
            gates: unknown_gates,
            ..artifact
        };
        assert_eq!(
            match_artifact(&editor, &current, &captured, Some(&incomplete)),
            ArtifactEvidenceState::Stale
        );
    }
    let wrong_manifest = ArtifactEvidence {
        digest: Some("wrong"),
        ..artifact
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&wrong_manifest)),
        ArtifactEvidenceState::Stale
    );
    let wrong_revision = ArtifactEvidence {
        revision: Some(9_007_199_254_740_993),
        ..artifact
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&wrong_revision)),
        ArtifactEvidenceState::Stale
    );
    let empty_identity = ArtifactEvidence {
        identities: IdentityView {
            compiler: Some(""),
            ..ids()
        },
        ..artifact
    };
    assert_eq!(
        match_artifact(&editor, &current, &captured, Some(&empty_identity)),
        ArtifactEvidenceState::Stale
    );
    for missing_facts in [
        TargetMeasurements {
            static_ram_bytes: None,
            ..facts()
        },
        TargetMeasurements {
            flash_bytes: None,
            ..facts()
        },
        TargetMeasurements {
            core0_stack_bytes: None,
            ..facts()
        },
        TargetMeasurements {
            core1_stack_bytes: None,
            ..facts()
        },
    ] {
        let incomplete = ArtifactEvidence {
            measurements: missing_facts,
            ..artifact
        };
        assert_eq!(
            match_artifact(&editor, &current, &captured, Some(&incomplete)),
            ArtifactEvidenceState::Stale
        );
    }
}

#[test]
/// Host summaries enter bounded HostTools events with contextual severity, without embedding raw process stdout.
fn helper_summaries_enter_only_the_existing_bounded_common_event_sink() {
    let mut events = EventStore::default();
    record_host_summary(&mut events, HostSummary::Connected, None, 1, None).unwrap();
    record_host_summary(&mut events, HostSummary::StageFailed, Some("redacted job 7"), 2, None).unwrap();
    let records = events.snapshot(&EventFilter::default());
    assert_eq!(records.records().len(), 2);
    assert!(
        records
            .records()
            .iter()
            .all(|record| record.category == Category::HostTools)
    );
    assert_eq!(records.records()[1].severity, Severity::Error);
    assert_eq!(records.records()[1].subject.as_deref(), Some("redacted job 7"));
    assert!(!records.records()[1].message.contains("stdout"));
}

#[test]
/// Resource occupancy tracks each resolved view grid and restores inherited geometry through undo.
fn occupancy_reports_each_views_resolved_grid_after_override_and_undo() {
    let mut editor = Editor::default();
    let mut cache = ResourceCache::default();
    editor
        .apply(Edit::AddView {
            name: "Portrait".into(),
        })
        .unwrap();
    let view = editor.selection().view;
    editor
        .apply(Edit::SetViewOrientation {
            id: view,
            orientation: Some(Orientation::Portrait),
        })
        .unwrap();
    let report = cache.inspect(&editor).unwrap().accepted.unwrap();
    assert_eq!(report.views[0].geometry, Geometry::Landscape);
    assert_eq!(report.views[1].geometry, Geometry::Portrait);
    editor.undo().unwrap();
    assert_eq!(
        cache.inspect(&editor).unwrap().accepted.unwrap().views[1].geometry,
        Geometry::Landscape
    );
}
