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
