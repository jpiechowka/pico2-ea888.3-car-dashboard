use dashboard_configurator::config::default_config;
use dashboard_configurator::config::startup::{StartupProfile, StartupTheme};
use dashboard_configurator::editor::*;
#[test]
fn startup_is_an_accepted_undoable_project_edit_without_restarting_acquisition() {
    let mut editor = Editor::new(default_config()).unwrap();
    let key = editor.accepted_config_key();
    let stamp = editor.stamp();
    let profile = StartupProfile {
        theme: StartupTheme::Turbo,
        duration_ms: 7460,
        ..StartupProfile::default()
    };
    editor.set_startup_profile(profile).unwrap();
    assert_eq!(editor.startup_profile(), profile);
    assert_ne!(editor.stamp(), stamp);
    assert_eq!(editor.accepted_config_key(), key);
    assert_eq!(editor.accepted_project().unwrap().profile(), profile);
    editor.undo().unwrap();
    assert_eq!(editor.startup_profile(), StartupProfile::default());
    editor.redo().unwrap();
    assert_eq!(editor.startup_profile(), profile);
    assert_eq!(editor.accepted_config_key(), key);
    let handle = editor.open_json().unwrap();
    editor.set_json_text(handle, "{".into()).unwrap();
    assert_eq!(
        editor.set_startup_profile(StartupProfile::default()),
        Err(EditorError::JsonPending)
    );
    assert!(editor.accepted_project().is_err());
}
#[test]
fn project_import_and_storage_roundtrip_are_atomic_and_legacy_import_keeps_startup() {
    use dashboard_config_tools::project::ExportMode;
    let mut source = Editor::default();
    let profile = StartupProfile {
        theme: StartupTheme::Sanic,
        ..StartupProfile::default()
    };
    source.set_startup_profile(profile).unwrap();
    let request = source.begin_save().unwrap();
    let bytes = request.project().encode(ExportMode::Complete).unwrap();
    let restored = Editor::from_project_saved(&bytes, request.json()).unwrap();
    assert_eq!(restored.startup_profile(), profile);
    let mut target = Editor::default();
    let token = target.begin_import().unwrap();
    target.complete_project_import(token, &bytes).unwrap();
    assert_eq!(target.startup_profile(), profile);
    target.undo().unwrap();
    assert_eq!(target.startup_profile(), StartupProfile::default());
    target.redo().unwrap();
    assert_eq!(target.startup_profile(), profile);
    let binary = Editor::default().export_binary().unwrap();
    let token = target.begin_import().unwrap();
    target.complete_import(token, ImportFormat::Binary, &binary).unwrap();
    assert_eq!(target.startup_profile(), profile);
    let before = (target.stamp(), target.history_len());
    let token = target.begin_import().unwrap();
    assert!(target.complete_project_import(token, b"bad").is_err());
    assert_eq!((target.stamp(), target.history_len()), before);
    let token = target.begin_import().unwrap();
    target.apply(Edit::SetReducedMotion(true)).unwrap();
    assert_eq!(
        target.complete_project_import(token, &bytes),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn prepared_project_import_stays_blocked_until_current_admission_and_restores_envelope() {
    let mut editor = Editor::default();
    let mut source = Editor::default();
    source
        .set_startup_profile(StartupProfile {
            theme: StartupTheme::BoostBro,
            ..StartupProfile::default()
        })
        .unwrap();
    let prepared = source.accepted_project().unwrap();
    let token = editor.begin_import().unwrap();
    assert!(editor.accepted_project().is_err());
    assert!(editor.begin_save().is_err());
    editor.cancel_import(token).unwrap();
    assert_eq!(
        editor.complete_prepared_project_import(token, prepared.clone()),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.startup_profile(), StartupProfile::default());
    let token = editor.begin_import().unwrap();
    editor.complete_prepared_project_import(token, prepared).unwrap();
    let save = editor.begin_save().unwrap();
    let restored = Editor::from_prepared_project_saved(save.project().clone(), save.json()).unwrap();
    assert_eq!(restored.startup_profile(), source.startup_profile());
    assert!(Editor::from_prepared_project_saved(Editor::default().accepted_project().unwrap(), save.json()).is_err());
}
use std::sync::Arc;
fn backed_artwork(label: &str) -> Arc<dashboard_config_tools::project::Artwork> {
    backed_artwork_in(label, dashboard_config::Geometry::Landscape)
}
fn backed_artwork_in(
    label: &str,
    geometry: dashboard_config::Geometry,
) -> Arc<dashboard_config_tools::project::Artwork> {
    use dashboard_config_tools::project as p;
    let source: Arc<[u8]>=Arc::from(&b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b"[..]);
    let converted = dashboard_startup_tools::convert(
        &source,
        &dashboard_startup_tools::Options {
            geometry,
            kind: dashboard_startup_tools::SourceKind::Gif,
            ..Default::default()
        },
    )
    .unwrap();
    let recipe = p::Recipe {
        geometry,
        version: 2,
        converter: p::CONVERTER.into(),
        label: label.into(),
        source_kind: p::SourceKind::Gif,
        source_sha256: p::digest(&source),
        source_length: source.len() as u32,
        original_included: true,
        asset_sha256: p::digest(&converted.asset),
        fit: p::Fit::Contain,
        matte_rgb: [0; 3],
        sample_fps: 20,
        trim_start_ms: 0,
        trim_end_ms: 100,
        sheet: None,
        gif_background: p::GifBackground::Logical,
    };
    let art = p::Artwork::new(converted.asset.into(), recipe, Some(source.clone())).unwrap();
    Arc::new(
        art.externalized(Arc::new(
            p::OriginalReference::new(p::digest(&source), source.len()).unwrap(),
        ))
        .unwrap(),
    )
}
#[test]
fn artwork_handles_follow_history_and_captured_save_without_original_arrays() {
    let mut editor = Editor::default();
    let first = backed_artwork("first");
    let original = Arc::downgrade(first.original_reference().unwrap());
    let token = editor.begin_import().unwrap();
    assert!(editor.accepted_project().is_err());
    editor.complete_artwork_import(token, first).unwrap();
    let save = editor.begin_save().unwrap();
    for index in 0..34 {
        let token = editor.begin_import().unwrap();
        editor
            .complete_artwork_import(token, backed_artwork(&format!("replacement {index}")))
            .unwrap();
    }
    assert!(editor.startup_artwork().unwrap().original().is_none());
    assert!(original.upgrade().is_some(), "captured save pins an evicted source");
    drop(save);
    assert!(
        original.upgrade().is_none(),
        "evicted history releases its immutable backing pin"
    );
    editor.undo().unwrap();
    assert!(editor.startup_artwork().unwrap().has_original());
    assert!(editor.startup_artwork().unwrap().original().is_none());
    let save = editor.begin_save().unwrap();
    let restored = Editor::from_prepared_project_saved(save.project().clone(), save.json()).unwrap();
    assert_eq!(
        restored.startup_artwork().unwrap().identity(),
        editor.startup_artwork().unwrap().identity()
    );
    let token = editor.begin_import().unwrap();
    editor.cancel_import(token).unwrap();
    let before = editor.stamp();
    assert_eq!(
        editor.complete_artwork_import(token, backed_artwork("cancelled")),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.stamp(), before);
}

#[test]
fn actual_artwork_publication_scopes_feedback_and_stale_join_keeps_current_project() {
    use dashboard_configurator::artwork_draft::ProjectFeedback;
    let mut editor = Editor::default();
    let art = backed_artwork("owned source");
    let token = editor.begin_import().unwrap();
    editor.complete_artwork_import(token, art.clone()).unwrap();
    let feedback = ProjectFeedback::new(&editor, "Conversion accepted".into());
    assert_eq!(feedback.text(&editor), "Conversion accepted");
    assert_eq!(editor.startup_profile().theme, StartupTheme::Custom);
    assert_eq!(editor.startup_artwork().unwrap().identity(), art.identity());
    let pending = editor.begin_import().unwrap();
    let replacement = Editor::default().accepted_project().unwrap();
    editor.cancel_import(pending).unwrap();
    let current = editor.begin_import().unwrap();
    editor.complete_prepared_project_import(current, replacement).unwrap();
    let before = editor
        .export_project(dashboard_config_tools::project::ExportMode::ConvertedOnly)
        .unwrap();
    assert_eq!(feedback.text(&editor), "");
    assert_eq!(
        editor.complete_artwork_import(pending, art),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor
            .export_project(dashboard_config_tools::project::ExportMode::ConvertedOnly)
            .unwrap(),
        before
    );
    assert_eq!(feedback.text(&editor), "");
    editor.undo().unwrap();
    assert_eq!(editor.startup_profile().theme, StartupTheme::Custom);
    assert!(editor.startup_artwork().unwrap().has_original());
    assert_eq!(
        feedback.text(&editor),
        "",
        "history restoration cannot revive an old operation result"
    );
}

#[test]
fn orientation_mismatch_retains_artwork_blocks_snapshots_and_undo_restores_exact_project() {
    use dashboard_config_tools::project::ExportMode;
    use dashboard_configurator::config::Orientation;
    let mut e = Editor::default();
    let art = backed_artwork("retained");
    e.set_artwork_if_current(e.stamp(), art.clone()).unwrap();
    let before = e.export_project(ExportMode::ConvertedOnly).unwrap();
    let import = e.begin_import().unwrap();
    e.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    assert!(e.artwork_geometry_mismatch());
    assert_eq!(e.startup_artwork().unwrap().identity(), art.identity());
    assert!(e.startup_artwork().unwrap().has_original());
    assert!(e.begin_save().is_err());
    assert!(e.export_binary().is_err());
    assert!(e.accepted_project().is_err());
    assert_eq!(
        e.complete_artwork_import(import, art.clone()),
        Err(EditorError::StaleToken)
    );
    e.undo().unwrap();
    assert_eq!(e.export_project(ExportMode::ConvertedOnly).unwrap(), before);
    e.redo().unwrap();
    assert!(e.artwork_geometry_mismatch());
    let mut profile = e.startup_profile();
    profile.splash_orientation = Some(Orientation::LandscapeInverted);
    e.set_startup_profile(profile).unwrap();
    assert!(!e.artwork_geometry_mismatch());
    assert!(e.accepted_project().is_ok());
}

#[test]
fn stale_geometry_conversion_cannot_replace_project() {
    use dashboard_configurator::config::Orientation;
    let mut e = Editor::default();
    let token = e.begin_import().unwrap();
    let mut profile = e.startup_profile();
    profile.splash_orientation = Some(Orientation::Portrait);
    e.set_startup_profile(profile).unwrap();
    assert_eq!(
        e.complete_artwork_import(token, backed_artwork("obsolete")),
        Err(EditorError::StaleToken)
    );
    assert!(e.startup_artwork().is_none());
    // Even a fresh callback cannot admit wrong-shape pixels.
    assert!(
        e.set_artwork_if_current(e.stamp(), backed_artwork("wrong shape"))
            .is_err()
    );
    assert!(e.startup_artwork().is_none());
}

#[test]
fn matching_reconversion_is_atomic_and_undo_keeps_the_retained_original() {
    use dashboard_configurator::config::{Geometry, Orientation};
    let mut e = Editor::default();
    let old = backed_artwork("original");
    e.set_artwork_if_current(e.stamp(), old.clone()).unwrap();
    e.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    let converted = backed_artwork_in("original", Geometry::Portrait);
    let token = e.begin_import().unwrap();
    e.complete_artwork_import(token, converted.clone()).unwrap();
    assert!(!e.artwork_geometry_mismatch());
    assert!(e.accepted_project().is_ok());
    assert_eq!(e.startup_artwork().unwrap().identity(), converted.identity());
    assert!(e.startup_artwork().unwrap().has_original());
    e.undo().unwrap();
    assert!(e.artwork_geometry_mismatch());
    assert_eq!(e.startup_artwork().unwrap().identity(), old.identity());
    e.undo().unwrap();
    assert!(!e.artwork_geometry_mismatch());
    assert!(e.accepted_project().is_ok());
}

#[test]
fn history_commands_repair_retained_artwork_mismatch_without_admitting_exports() {
    use dashboard_config::{Geometry, Orientation};
    use dashboard_config_tools::project::ExportMode;
    use dashboard_configurator::workspace::{Command, ExportFormat, WorkspaceState, dispatch};

    let mut editor = Editor::default();
    let mut workspace = WorkspaceState::default();
    let mut profile = editor.startup_profile();
    profile.splash_orientation = Some(Orientation::Portrait);
    editor.set_startup_profile(profile).unwrap();
    let portrait = backed_artwork_in("original", Geometry::Portrait);
    editor.set_artwork_if_current(editor.stamp(), portrait.clone()).unwrap();
    profile = editor.startup_profile();
    profile.splash_orientation = Some(Orientation::Landscape);
    editor.set_startup_profile(profile).unwrap();
    let landscape = backed_artwork_in("original", Geometry::Landscape);
    let import = editor.begin_import().unwrap();
    editor.complete_artwork_import(import, landscape.clone()).unwrap();
    let converted = editor.export_project(ExportMode::ConvertedOnly).unwrap();
    let stamp = editor.stamp();
    dispatch(&mut editor, &mut workspace, Command::Undo, stamp).unwrap();
    assert!(editor.artwork_geometry_mismatch());
    assert_eq!(editor.startup_artwork().unwrap().identity(), portrait.identity());
    for command in [Command::Undo, Command::Redo] {
        assert_eq!(command.availability(&editor, editor.stamp()), None);
    }
    assert_eq!(editor.history_readiness().block, None);
    assert!(editor.export_project(ExportMode::ConvertedOnly).is_err());
    assert!(editor.export_binary().is_err());
    assert!(
        Command::Export(ExportFormat::Eav)
            .availability(&editor, editor.stamp())
            .is_some()
    );
    // Mismatch is the only exception: all draft and lifecycle blockers remain.
    let blocked = |editor: &mut Editor, workspace: &mut WorkspaceState, block| {
        assert_eq!(editor.history_readiness().block, Some(block));
        let before = (
            editor.stamp(),
            editor.history_len(),
            editor.startup_artwork().unwrap().identity(),
        );
        for command in [Command::Undo, Command::Redo] {
            let stamp = editor.stamp();
            assert!(command.availability(editor, stamp).is_some());
            assert!(dispatch(editor, workspace, command, stamp).is_err());
        }
        assert_eq!(
            (
                editor.stamp(),
                editor.history_len(),
                editor.startup_artwork().unwrap().identity()
            ),
            before
        );
    };
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    blocked(&mut editor, &mut workspace, ActionBlock::InvalidDraft);
    assert_eq!(editor.raw_fields()[0].text, "bad");
    editor.discard_raw().unwrap();
    let json = editor.open_json().unwrap();
    editor.set_json_text(json, "{".into()).unwrap();
    blocked(&mut editor, &mut workspace, ActionBlock::JsonPending);
    assert_eq!(editor.json_draft().unwrap().text(), "{");
    editor.discard_json().unwrap();
    editor
        .set_startup_number(editor.stamp(), StartupNumber::StatusMilliseconds, "bad")
        .unwrap();
    blocked(&mut editor, &mut workspace, ActionBlock::StartupPending);
    editor.discard_startup_timing(editor.stamp()).unwrap();
    let import = editor.begin_import().unwrap();
    blocked(&mut editor, &mut workspace, ActionBlock::ImportPending);
    assert!(editor.import_is_current(import));
    editor.cancel_import(import).unwrap();
    let pointer = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    blocked(&mut editor, &mut workspace, ActionBlock::PointerActive);
    assert_eq!(editor.active_pointer(), Some(pointer));
    editor.cancel_pointer(pointer).unwrap();
    assert_eq!(editor.history_readiness().block, None);
    let stamp = editor.stamp();
    dispatch(&mut editor, &mut workspace, Command::Redo, stamp).unwrap();
    assert_eq!(editor.export_project(ExportMode::ConvertedOnly).unwrap(), converted);
    let stamp = editor.stamp();
    dispatch(&mut editor, &mut workspace, Command::Undo, stamp).unwrap();
    let stamp = editor.stamp();
    dispatch(&mut editor, &mut workspace, Command::Undo, stamp).unwrap();
    assert!(!editor.artwork_geometry_mismatch());
    assert_eq!(editor.startup_profile().splash_orientation, Some(Orientation::Portrait));
    assert_eq!(editor.startup_artwork().unwrap().identity(), portrait.identity());
    assert!(editor.accepted_project().is_ok());
}
