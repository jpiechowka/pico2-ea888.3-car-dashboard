use dashboard_configurator::config::*;
use dashboard_configurator::json_diff::{DiffKind, PointerSide, diff_configs, diff_json, pointer_segment};

#[test]
fn equal_and_scalar_changes_compare_typed_values_at_real_pointers() {
    let before = default_config();
    assert!(diff_configs(&before, &before).unwrap().changes.is_empty());
    let mut after = before.clone();
    after.widgets[0].display.decimals = 3;
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.omitted, 0);
    assert_eq!(diff.changes.len(), 1);
    let entry = &diff.changes[0];
    assert_eq!(entry.pointer, "/widgets/0/display/decimals");
    assert_eq!(entry.kind, DiffKind::Changed);
    assert!(entry.object_label.as_deref().unwrap().contains("widget 1"));
    assert_eq!(entry.before.as_ref().unwrap().text, "2");
    assert_eq!(entry.after.as_ref().unwrap().text, "3");
}

#[test]
fn add_remove_and_shift_use_indexed_pointers_without_false_value_changes() {
    let before = default_config();
    let mut after = before.clone();
    after.widgets.remove(0);
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.changes.len(), 1);
    assert_eq!(diff.changes[0].kind, DiffKind::Removed);
    assert_eq!(diff.changes[0].pointer, "/widgets/0");
    assert!(diff.changes[0].after.is_none());
    let diff = diff_configs(&after, &before).unwrap();
    assert_eq!(diff.changes.len(), 1);
    assert_eq!(diff.changes[0].kind, DiffKind::Added);
    assert_eq!(diff.changes[0].pointer, "/widgets/0");
    assert!(diff.changes[0].before.is_none());
}

#[test]
fn unique_ids_match_reorders_separately_from_leaf_changes() {
    let mut before = default_config();
    before
        .views
        .push(View {
            orientation: None,
            id: 2,
            name: "SECOND".try_into().unwrap(),
        })
        .unwrap();
    let mut after = before.clone();
    after.widgets.swap(0, 1);
    after.views.swap(0, 1);
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.changes.len(), 2);
    assert!(diff.changes.iter().all(|entry| entry.kind == DiffKind::Reordered));
    assert_eq!(diff.changes[0].pointer, "/views");
    assert_eq!(diff.changes[1].pointer, "/widgets");
    after.widgets[1].display.decimals = 1;
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.changes.len(), 3);
    assert!(
        diff.changes
            .iter()
            .any(|entry| entry.pointer == "/widgets/1/display/decimals" && entry.kind == DiffKind::Changed)
    );
}

#[test]
fn duplicate_ids_fall_back_to_ambiguous_indexed_entries() {
    let before = default_config();
    let mut after = before.clone();
    after.widgets[1].id = after.widgets[0].id;
    after.widgets[1].display.decimals = 1;
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.changes.len(), 2);
    assert!(diff.changes.iter().all(|entry| entry.kind == DiffKind::Ambiguous));
    assert!(diff.changes.iter().any(|entry| entry.pointer == "/widgets/1/id"));
    assert!(
        diff.changes
            .iter()
            .any(|entry| entry.pointer == "/widgets/1/display/decimals")
    );
    assert!(diff_configs(&after, &after).unwrap().changes.is_empty());
}

#[test]
fn ambiguous_tail_pointers_identify_the_present_document_and_displayed_side() {
    let accepted = default_config();
    let mut shorter = accepted.clone();
    shorter.widgets.pop();
    let mut removed = shorter.clone();
    removed.widgets[1].id = removed.widgets[0].id;
    let mut added = accepted.clone();
    added.widgets[1].id = added.widgets[0].id;
    let pointer = format!("/widgets/{}", accepted.widgets.len() - 1);
    for (before, after, side, label) in [
        (&accepted, &removed, PointerSide::Accepted, "Pointer in accepted JSON"),
        (&shorter, &added, PointerSide::Candidate, "Pointer in candidate JSON"),
    ] {
        let before_json = serde_json::to_value(before).unwrap();
        let after_json = serde_json::to_value(after).unwrap();
        let diff = diff_configs(before, after).unwrap();
        let entry = diff.changes.iter().find(|entry| entry.pointer == pointer).unwrap();
        assert_eq!(
            entry.kind,
            DiffKind::Ambiguous,
            "duplicate IDs still require indexed ambiguity"
        );
        assert_eq!(entry.pointer_side(), side);
        assert_eq!(
            entry.pointer_side().label(),
            label,
            "UI consumes this exact shared label"
        );
        assert_eq!(entry.before.is_some(), before_json.pointer(&entry.pointer).is_some());
        assert_eq!(entry.after.is_some(), after_json.pointer(&entry.pointer).is_some());
        match side {
            PointerSide::Accepted => {
                assert!(entry.before.is_some());
                assert!(entry.after.is_none());
                assert!(before_json.pointer(&entry.pointer).unwrap().is_object());
                assert!(after_json.pointer(&entry.pointer).is_none());
            }
            PointerSide::Candidate => {
                assert!(entry.before.is_none());
                assert!(entry.after.is_some());
                assert!(before_json.pointer(&entry.pointer).is_none());
                assert!(after_json.pointer(&entry.pointer).unwrap().is_object());
            }
        }
    }
}

#[test]
fn pointer_segments_escape_tilde_and_slash_once() {
    assert_eq!(pointer_segment("a~/b"), "a~0~1b");
    assert_eq!(pointer_segment("~1/~0"), "~01~1~00");
    assert_eq!(pointer_segment(""), "");
}

#[test]
fn strict_original_byte_parse_precedes_diff_and_rejects_type_changes() {
    let before = default_config();
    let text = serde_json::to_string(&before).unwrap();
    for bad in [
        text.replacen("\"version\":4", "\"version\":4,\"version\":4", 1),
        text.replacen("\"decimals\":2", "\"decimals\":2,\"decimals\":3", 1),
        text.replacen("\"decimals\":2", "\"decimals\":\"2\"", 1),
        text.replacen("\"decimals\":2", "\"decimals\":2,\"unknown\":0", 1),
    ] {
        assert_ne!(bad, text);
        assert!(matches!(
            diff_json(&before, bad.as_bytes()),
            Err(ConfigError::Json { .. })
        ));
    }
    assert_eq!(
        diff_json(&before, &vec![b' '; MAX_JSON_BYTES + 1]),
        Err(ConfigError::Length)
    );
}

#[test]
fn large_subtree_summaries_are_bounded_and_flagged() {
    let mut before = default_config();
    let after = before.clone();
    before.widgets.remove(0);
    let diff = diff_configs(&before, &after).unwrap();
    let summary = diff.changes[0].after.as_ref().unwrap();
    assert!(summary.text.len() <= 256);
    assert!(summary.truncated);
    assert!(std::str::from_utf8(summary.text.as_bytes()).is_ok());
}

#[test]
fn retained_paths_stop_at_512_but_omitted_count_is_exact() {
    let mut before = default_config();
    let exemplar = before.widgets[0].clone();
    before.widgets.clear();
    for id in 1..=64 {
        let mut widget = exemplar.clone();
        widget.id = id;
        before.widgets.push(widget).unwrap();
    }
    let mut after = before.clone();
    for widget in &mut after.widgets {
        widget.display.decimals = 3;
        widget.display.graph = !widget.display.graph;
        widget.display.average = !widget.display.average;
        widget.display.extrema = !widget.display.extrema;
        widget.display.trend = !widget.display.trend;
        widget.warning.enabled = !widget.warning.enabled;
        widget.warning.advanced.trigger_high += 1.0;
        widget.placement.row = 1;
    }
    after.reduced_motion = true;
    assert!(serde_json::to_vec(&after).unwrap().len() <= MAX_JSON_BYTES);
    let diff = diff_configs(&before, &after).unwrap();
    assert_eq!(diff.changes.len(), 512);
    assert_eq!(diff.omitted, 1);
}
