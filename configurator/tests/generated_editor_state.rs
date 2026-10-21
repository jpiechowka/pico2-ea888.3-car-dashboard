//! Deterministic mixed editor state-transition corpus with seed/case failure context.

#[path = "../../test-support/generated.rs"]
/// Shared deterministic case reporter; includes seed/step in assertion failure context.
mod generated;
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

#[test]
/// 256 seeded orientation/raw/JSON/import/reduced-motion transactions preserve accepted
/// codec bytes, exact undo/redo, bounded history and rejection of retired async tokens.
fn seeded_editor_transactions_preserve_snapshots_and_retire_async_callbacks() {
    /// Frozen xorshift seed reproducing this editor operation sequence.
    const SEED: u32 = 0x17ED_1702;
    let mut rng = SEED;
    let mut editor = Editor::default();
    for step in 0..256 {
        let _case = generated::Case::new("editor", u64::from(SEED), step);
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let before = editor.export_binary().unwrap();
        match rng % 5 {
            0 => {
                let orientation = Orientation::ALL[(rng as usize / 5) % 4];
                editor.apply(Edit::SetOrientation(orientation)).unwrap();
                assert_eq!(editor.preview().config().orientation, orientation);
                let after = editor.export_binary().unwrap();
                if before != after {
                    editor.undo().unwrap();
                    assert_eq!(editor.export_binary().unwrap(), before);
                    editor.redo().unwrap();
                    assert_eq!(editor.export_binary().unwrap(), after);
                }
            }
            1 => {
                editor
                    .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
                    .unwrap();
                assert!(editor.has_invalid_draft());
                assert!(editor.export_binary().is_err());
                let mut bytes = [0; MAX_FILE_BYTES];
                assert_eq!(encode(editor.preview(), &mut bytes).unwrap(), before);
                editor
                    .set_raw(editor.field_token(), RawField::ViewName(1), format!("View{step}"))
                    .unwrap();
                let repaired = editor.export_binary().unwrap();
                editor.undo().unwrap();
                assert_eq!(editor.export_binary().unwrap(), before);
                editor.redo().unwrap();
                assert_eq!(editor.export_binary().unwrap(), repaired);
            }
            2 => {
                let save = editor.begin_save().unwrap();
                let import = editor.begin_import().unwrap();
                let handle = editor.open_json().unwrap();
                let old = editor.set_json_text(handle, "{".into()).unwrap();
                let current = editor.set_json_text(handle, "{}".into()).unwrap();
                assert_eq!(editor.finish_json_parse(old), Err(EditorError::StaleToken));
                editor.finish_json_parse(current).unwrap();
                assert!(editor.apply_json().is_err());
                editor.discard_json().unwrap();
                assert_eq!(
                    editor.complete_import(import, ImportFormat::Binary, &before),
                    Err(EditorError::StaleToken)
                );
                assert_eq!(
                    editor.acknowledge_save(save.token(), Ok(())),
                    Err(EditorError::StaleToken)
                );
                assert_eq!(editor.export_binary().unwrap(), before);
            }
            3 => {
                let import = editor.begin_import().unwrap();
                assert!(
                    editor
                        .complete_import(import, ImportFormat::Binary, &before[..before.len() - 1])
                        .is_err()
                );
                assert_eq!(editor.export_binary().unwrap(), before);
            }
            _ => {
                editor
                    .apply(Edit::SetReducedMotion(!editor.preview().config().reduced_motion))
                    .unwrap();
                let after = editor.export_binary().unwrap();
                assert_ne!(before, after);
                editor.undo().unwrap();
                assert_eq!(editor.export_binary().unwrap(), before);
                editor.redo().unwrap();
                assert_eq!(editor.export_binary().unwrap(), after);
            }
        }
        validate(editor.preview().config().clone()).unwrap();
        assert!(editor.history_len().0 <= 32, "seed={SEED:x} step={step}");
        let bytes = editor.export_binary().unwrap();
        assert_eq!(decode(&bytes).unwrap(), *editor.preview());
        assert_eq!(editor.action_readiness().block, None);
    }
}
