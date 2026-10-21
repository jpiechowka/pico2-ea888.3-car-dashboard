//! Importer raw-text rejection and accepted-project feedback identity regressions.
use dashboard_configurator::artwork_draft::{Number, Numbers};
use dashboard_startup_tools::{Options, Sheet, SourceKind};
#[test]
/// Assert malformed/out-of-range text stays inspectable and cannot reuse old options.
fn invalid_text_is_retained_and_never_falls_back_to_old_recipe() {
    let base = Options::default();
    let mut draft = Numbers::new(&base);
    for text in ["", "-1", "1e2", "NaN", "99999999999999999999999999999999", "0", "21"] {
        draft.set(Number::Fps, text);
        assert_eq!(draft.text(Number::Fps), text);
        assert!(draft.options(&base).is_err());
        assert_eq!(base.sample_fps, 20);
    }
    draft.set(Number::Fps, "10");
    assert_eq!(draft.options(&base).unwrap().sample_fps, 10);
    draft.set(Number::End, "0");
    assert!(draft.options(&base).is_err());
    draft.set(Number::End, "5000");
    draft.set(Number::Start, "5000");
    assert!(draft.options(&base).is_err());
    draft.set(Number::Start, "1000");
    assert_eq!(draft.options(&base).unwrap().trim_start_ms, 1000);
}
#[test]
/// Assert hidden sheet fields survive switching kinds and are validated when active.
fn hidden_grid_drafts_are_preserved_but_only_admitted_for_sheet_conversion() {
    let base = Options::default();
    let mut draft = Numbers::new(&base);
    draft.set(Number::Columns, "bad");
    assert!(draft.options(&base).is_ok());
    let sheet = Options {
        kind: SourceKind::SpriteSheet,
        sheet: Some(Sheet {
            columns: 1,
            rows: 1,
            frame_ms: 100,
        }),
        ..base
    };
    assert!(draft.options(&sheet).is_err());
    draft.set(Number::Columns, "0");
    assert!(draft.options(&sheet).is_err());
    draft.set(Number::Columns, "2");
    assert_eq!(draft.options(&sheet).unwrap().sheet.unwrap().columns, 2);
    draft.set(Number::Frame, &"9".repeat(1000));
    assert!(draft.text(Number::Frame).len() <= 32);
    assert!(draft.options(&sheet).is_err());
    let restored = Numbers::new(&sheet);
    assert_eq!(restored.options(&sheet).unwrap().sheet.unwrap().frame_ms, 100);
}

#[test]
/// Assert feedback survives draft activity but expires on accepted revision/owner changes.
fn feedback_tracks_accepted_project_and_own_publication_not_draft_activity() {
    use dashboard_configurator::artwork_draft::ProjectFeedback;
    use dashboard_configurator::config::startup::{StartupProfile, StartupTheme};
    use dashboard_configurator::editor::Editor;
    let mut editor = Editor::default();
    let previous = ProjectFeedback::new(&editor, "Conversion accepted".into());
    let token = editor.begin_import().unwrap();
    assert_eq!(previous.text(&editor), "Conversion accepted");
    editor.cancel_import(token).unwrap();
    editor
        .set_startup_profile(StartupProfile {
            theme: StartupTheme::Turbo,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(previous.text(&editor), "");
    let completed = ProjectFeedback::new(&editor, "Own operation accepted".into());
    assert_eq!(completed.text(&editor), "Own operation accepted");
    editor.undo().unwrap();
    assert_eq!(completed.text(&editor), "");
    assert_eq!(previous.text(&editor), "");
    editor.redo().unwrap();
    assert_eq!(completed.text(&editor), "");
    let current = ProjectFeedback::new(&editor, "Current project".into());
    let replacement = Editor::default().accepted_project().unwrap();
    let token = editor.begin_import().unwrap();
    editor.complete_prepared_project_import(token, replacement).unwrap();
    assert_eq!(current.text(&editor), "");
    let default_feedback = ProjectFeedback::new(&Editor::default(), "Other owner".into());
    assert_eq!(default_feedback.text(&Editor::default()), "");
}
