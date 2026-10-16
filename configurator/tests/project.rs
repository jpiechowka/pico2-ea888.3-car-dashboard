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
    use dashboard_config_tools::project as p;
    let source: Arc<[u8]>=Arc::from(&b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b"[..]);
    let converted = dashboard_startup_tools::convert(
        &source,
        &dashboard_startup_tools::Options {
            kind: dashboard_startup_tools::SourceKind::Gif,
            ..Default::default()
        },
    )
    .unwrap();
    let recipe = p::Recipe {
        version: 1,
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
