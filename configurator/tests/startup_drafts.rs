//! Retained startup timing drafts and stale asynchronous authority regressions.

use dashboard_config_tools::project::ExportMode;
use dashboard_configurator::editor::*;

/// Sets raw startup `field` to `text` in `e` under its current stamp; fixture rejection panics.
fn draft(
    e: &mut Editor,
    field: StartupNumber,
    text: &str,
) {
    e.set_startup_number(e.stamp(), field, text).unwrap();
}
#[test]
/// Malformed/overflow/out-of-range timing stays visible without altering accepted profile/config/history; snapshots
/// remain blocked.
fn invalid_text_is_retained_while_accepted_pixels_and_exports_stay_guarded() {
    let mut e = Editor::default();
    let profile = e.startup_profile();
    let config = e.preview().clone();
    let key = e.accepted_config_key();
    for text in ["", "-1", "oops", "2001", "65536"] {
        draft(&mut e, StartupNumber::StatusMilliseconds, text);
        assert_eq!(e.startup_timing_text(StartupNumber::StatusMilliseconds), text);
        assert!(!e.startup_draft_valid());
        assert!(e.startup_draft_label().unwrap().starts_with("INVALID"));
        assert!(e.apply_startup_timing(e.stamp()).is_err());
        assert_eq!(e.startup_profile(), profile);
        assert_eq!(e.preview(), &config);
        assert_eq!(e.accepted_config_key(), key);
        assert_eq!(e.history_len(), (0, 0));
        assert_eq!(e.export_binary(), Err(EditorError::StartupPending));
        assert!(e.export_project(ExportMode::Complete).is_err());
        assert!(e.begin_save().is_err());
        assert_eq!(e.save_status(), SaveStatus::InvalidDraft);
    }
}
#[test]
/// Multiple timing fields apply as one history entry; discard preserves accepted timing and prior history.
fn correction_applies_one_history_entry_and_discard_keeps_history() {
    let mut e = Editor::default();
    let old = e.startup_profile();
    let key = e.accepted_config_key();
    draft(&mut e, StartupNumber::StatusMilliseconds, "2000");
    draft(&mut e, StartupNumber::SplashMilliseconds, "10001");
    assert!(!e.startup_draft_valid());
    draft(&mut e, StartupNumber::SplashMilliseconds, "10000");
    assert!(e.startup_draft_valid());
    assert!(e.accepted_project().is_err());
    assert_eq!(e.undo(), Err(EditorError::StartupPending));
    assert_eq!(e.select(e.selection()), Err(EditorError::StartupPending));
    e.apply_startup_timing(e.stamp()).unwrap();
    assert_eq!(e.history_len(), (1, 0));
    assert_eq!(e.startup_profile().duration_ms, 10000);
    assert_eq!(e.accepted_config_key(), key);
    e.undo().unwrap();
    assert_eq!(e.startup_profile(), old);
    e.redo().unwrap();
    assert_eq!(e.startup_profile().status_ms, 2000);
    draft(&mut e, StartupNumber::SplashMilliseconds, "bad");
    e.discard_startup_timing(e.stamp()).unwrap();
    assert_eq!(e.history_len(), (1, 0));
    assert_eq!(e.startup_profile().duration_ms, 10000);
    assert!(e.accepted_project().is_ok());
}
#[test]
/// Timing edits retire prior save/import/stamp authority, and pending JSON independently blocks timing changes.
fn stale_import_save_and_draft_callbacks_cannot_join_after_edits_or_discard() {
    let mut e = Editor::default();
    let project = e.accepted_project().unwrap();
    let save = e.begin_save().unwrap();
    let import = e.begin_import().unwrap();
    let old = e.stamp();
    draft(&mut e, StartupNumber::StatusMilliseconds, "bad");
    let pending = e.stamp();
    assert!(!e.save_is_current(save.token()));
    assert!(!e.import_is_current(import));
    assert_eq!(e.apply_startup_timing(old), Err(EditorError::StaleToken));
    assert_eq!(e.discard_startup_timing(old), Err(EditorError::StaleToken));
    assert_eq!(
        e.set_startup_number(old, StartupNumber::StatusMilliseconds, "0"),
        Err(EditorError::StaleToken)
    );
    assert!(e.begin_import().is_err());
    assert!(e.open_json().is_err());
    e.discard_startup_timing(pending).unwrap();
    assert_eq!(
        e.complete_prepared_project_import(import, project),
        Err(EditorError::StaleToken)
    );
    assert_eq!(e.apply_startup_timing(pending), Err(EditorError::StaleToken));
    let json = e.open_json().unwrap();
    e.set_json_text(json, "{".into()).unwrap();
    assert_eq!(
        e.set_startup_number(e.stamp(), StartupNumber::StatusMilliseconds, "0"),
        Err(EditorError::JsonPending)
    );
}
#[test]
/// Overlong ASCII/Unicode input stays explicitly invalid despite display truncation; repaired zero durations admit.
fn oversized_and_unicode_drafts_are_bounded_and_cannot_accept_a_truncated_number() {
    let mut e = Editor::default();
    draft(&mut e, StartupNumber::StatusMilliseconds, &"0".repeat(33));
    assert_eq!(e.startup_timing_text(StartupNumber::StatusMilliseconds).len(), 32);
    assert!(!e.startup_draft_valid());
    assert!(e.apply_startup_timing(e.stamp()).is_err());
    draft(&mut e, StartupNumber::StatusMilliseconds, &"🦔".repeat(40));
    assert_eq!(
        e.startup_timing_text(StartupNumber::StatusMilliseconds).chars().count(),
        32
    );
    assert!(!e.startup_draft_valid());
    draft(&mut e, StartupNumber::StatusMilliseconds, "0");
    draft(&mut e, StartupNumber::SplashMilliseconds, "0");
    assert!(e.startup_draft_valid());
    e.apply_startup_timing(e.stamp()).unwrap();
    assert_eq!(e.startup_profile().total_duration(), 0);
}

#[test]
/// Timing entry cancels an active pointer, while identical Apply retires draft identity without an undo entry.
fn timing_edit_cancels_placement_and_same_value_apply_retires_the_draft() {
    let mut e = Editor::default();
    let widget = e.preview().config().widgets[0].clone();
    let token = e.begin_pointer(7, PointerTarget::Widget(widget.id)).unwrap();
    let value = e.startup_profile().status_ms.to_string();
    draft(&mut e, StartupNumber::StatusMilliseconds, &value);
    assert!(e.active_pointer().is_none());
    assert_eq!(
        e.commit_pointer(token, 7, widget.placement),
        Err(EditorError::StaleToken)
    );
    let pending = e.stamp();
    e.apply_startup_timing(pending).unwrap();
    assert_ne!(e.stamp(), pending);
    assert!(!e.has_startup_draft());
    assert_eq!(e.history_len(), (0, 0));
    assert!(e.accepted_project().is_ok());
}
