use dashboard_core::discovery::capture_ui::*;

fn context() -> Context {
    Context {
        save: SaveAvailability::Ready,
        revision: 7,
    }
}
fn open(
    ui: &mut CaptureUi,
    slots: [Slot; 4],
) {
    assert_eq!(ui.input(Input::XDown, 0, context()), None);
    let Action::Refresh { id } = ui.tick(2000).unwrap() else {
        panic!()
    };
    assert!(ui.complete(id, Completion::Listed(slots)));
    assert_eq!(ui.input(Input::XUp, 2001, context()), None);
}

#[test]
fn hold_opens_slots_once_and_short_x_only_fires_on_release() {
    let mut ui = CaptureUi::new();
    assert_eq!(ui.input(Input::XDown, 10, context()), None);
    assert_eq!(ui.tick(2009), None);
    assert_eq!(ui.input(Input::XUp, 2009, context()), Some(Action::LiveDetails));
    open(&mut ui, [Slot::Empty; 4]);
    assert!(matches!(ui.screen(), Screen::Slots));
    assert_eq!(ui.tick(6000), None);
    ui.input(Input::Turn(-1), 6001, context());
    assert_eq!(ui.selected(), 3);
    ui.input(Input::Turn(i16::MIN), 6002, context());
    assert_eq!(ui.selected(), 3);
}

#[test]
fn save_requires_current_revision_and_explicit_overwrite_then_stale_results_fail() {
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Saved { generation: 9 }; 4]);
    ui.input(Input::XDown, 2100, context());
    assert_eq!(ui.input(Input::XUp, 2200, context()), None);
    assert!(matches!(ui.screen(), Screen::ConfirmOverwrite));
    let changed = Context {
        revision: 8,
        ..context()
    };
    assert_eq!(ui.input(Input::Click, 2201, changed), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::Changed));
    ui.input(Input::Back, 2202, context());
    ui.input(Input::XDown, 2300, context());
    ui.input(Input::XUp, 2400, context());
    let Action::Save {
        id,
        slot,
        revision,
        overwrite,
    } = ui.input(Input::Click, 2401, context()).unwrap()
    else {
        panic!()
    };
    assert_eq!((slot, revision, overwrite), (0, 7, true));
    assert!(!ui.complete(id + 1, Completion::Saved { generation: 10 }));
    assert!(ui.complete(id, Completion::Saved { generation: 10 }));
    assert_eq!(ui.slots()[0], Slot::Saved { generation: 10 });
    assert!(!ui.complete(id, Completion::Failed(UiError::Storage)));
}

#[test]
fn empty_missing_busy_timeout_and_recovery_never_erase_implicitly() {
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Empty; 4]);
    assert_eq!(ui.input(Input::Click, 2100, context()), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::Empty));
    ui.input(Input::Back, 2101, context());
    let unavailable = Context {
        save: SaveAvailability::NoPeer,
        ..context()
    };
    ui.input(Input::XDown, 2200, unavailable);
    ui.input(Input::XUp, 2201, unavailable);
    assert_eq!(ui.screen(), Screen::Error(UiError::NoPeer));
    ui.input(Input::Back, 2202, context());
    ui.input(Input::XDown, 2300, context());
    let Action::Save { id, overwrite, .. } = ui.input(Input::XUp, 2301, context()).unwrap() else {
        panic!()
    };
    assert!(!overwrite);
    assert_eq!(ui.input(Input::Click, 2302, context()), None);
    assert_eq!(ui.tick(2301 + OPERATION_TIMEOUT_MS), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::Timeout));
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(id, Completion::Saved { generation: 1 }));
    assert_eq!(ui.screen(), Screen::Error(UiError::Timeout));
    assert_eq!(ui.input(Input::Recover, 40_000, context()), Some(Action::Recover));
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.slots()[0], Slot::Saved { generation: 1 });
}

#[test]
fn damaged_or_unknown_schema_needs_erase_confirmation_and_busy_io_stays_fenced() {
    for slot in [Slot::Corrupt, Slot::Unsupported] {
        let mut ui = CaptureUi::new();
        open(&mut ui, [slot; 4]);
        ui.input(Input::XDown, 2100, context());
        ui.input(Input::XUp, 2101, context());
        assert_eq!(ui.screen(), Screen::ConfirmReset);
        ui.input(Input::Back, 2102, context());
        assert_eq!(ui.slots()[0], slot);
        ui.input(Input::XDown, 2200, context());
        ui.input(Input::XUp, 2201, context());
        let Action::Reset { id, slot: 0 } = ui.input(Input::Click, 2202, context()).unwrap() else {
            panic!()
        };
        assert!(!ui.complete(id, Completion::Loaded { pages: 1 }));
        ui.input(Input::Back, 2203, context());
        assert_eq!(ui.pending(), Some(id));
        assert_eq!(ui.input(Input::Click, 2204, context()), None);
        assert!(ui.complete(id, Completion::Reset));
        assert_eq!(ui.slots()[0], Slot::Empty);
    }
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Unavailable; 4]);
    ui.input(Input::Click, 2100, context());
    assert_eq!(ui.screen(), Screen::Error(UiError::Unavailable));
}

#[test]
fn held_release_without_ticks_is_consumed_and_confirmation_expires() {
    let mut ui = CaptureUi::new();
    ui.input(Input::XDown, 0, context());
    let Action::Refresh { id } = ui.input(Input::XUp, 2000, context()).unwrap() else {
        panic!()
    };
    ui.complete(id, Completion::Listed([Slot::Saved { generation: 1 }; 4]));
    ui.input(Input::XDown, 2100, context());
    ui.input(Input::XUp, 2101, context());
    assert_eq!(ui.screen(), Screen::ConfirmOverwrite);
    assert_eq!(ui.input(Input::Click, 2101 + CONFIRM_TIMEOUT_MS, context()), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::Timeout));
}

#[test]
fn back_cancels_the_pending_x_gesture_without_opening_slots_or_forwarding_a_click() {
    let mut ui = CaptureUi::new();
    ui.input(Input::XDown, 0, context());
    assert_eq!(ui.input(Input::Back, 2000, context()), None);
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.input(Input::XUp, 2001, context()), None);
    assert_eq!(ui.pending(), None);
}

#[test]
fn abandoned_results_keep_original_slot_and_reject_wrong_operation_or_empty_page_count() {
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Empty; 4]);
    ui.input(Input::XDown, 2100, context());
    let Action::Save { id, slot: 0, .. } = ui.input(Input::XUp, 2101, context()).unwrap() else {
        panic!()
    };
    assert!(!ui.complete(id, Completion::Saved { generation: 0 }));
    ui.input(Input::Back, 2102, context());
    ui.input(Input::Turn(1), 2103, context());
    assert!(ui.complete(id, Completion::Saved { generation: 1 }));
    assert_eq!(ui.selected(), 1);
    assert_eq!(
        ui.slots(),
        &[Slot::Saved { generation: 1 }, Slot::Empty, Slot::Empty, Slot::Empty]
    );
    ui.input(Input::Turn(-1), 2104, context());
    let Action::Load { id, .. } = ui.input(Input::Click, 2105, context()).unwrap() else {
        panic!()
    };
    assert!(!ui.complete(id, Completion::Loaded { pages: 0 }));
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(id, Completion::Loaded { pages: usize::MAX }));
    ui.input(Input::Turn(-1), 2106, context());
    assert_eq!(ui.page(), usize::MAX - 1);
    ui.input(Input::Turn(1), 2107, context());
    assert_eq!(ui.page(), 0);
}

#[test]
fn semantic_x_uses_producer_hold_and_cannot_inspect_from_release_bounce() {
    use dashboard_core::input_model::DeferredButton;
    let mut button = DeferredButton::new();
    let mut state = CaptureUi::new();
    let mut actions = Vec::new();
    for (raw, at) in [
        (false, 0),
        (true, 10),
        (true, 60),
        (false, 2050),
        (false, 2060),
        (false, 2100),
    ] {
        let event = button.update(raw, at);
        if event.short
            && let Some(action) = state.input(Input::XShort, at, context())
        {
            actions.push(action);
        }
        if event.held
            && let Some(action) = state.input(Input::XHold, at, context())
        {
            actions.push(action);
        }
        assert_eq!(state.tick(at), None);
    }
    assert_eq!(actions, [Action::LiveDetails]);
    assert_eq!(state.screen(), Screen::Live);
    let Action::Refresh { id } = state.input(Input::XHold, 5000, context()).unwrap() else {
        panic!()
    };
    assert!(state.complete(id, Completion::Listed([Slot::Empty; 4])));
    let Action::Save { overwrite: false, .. } = state.input(Input::XShort, 5100, context()).unwrap() else {
        panic!()
    };
}

#[test]
fn loading_pages_back_confirmation_cancel_and_clock_reversal_are_bounded() {
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Saved { generation: 1 }; 4]);
    let Action::Load { id, slot: 0 } = ui.input(Input::Click, 2100, context()).unwrap() else {
        panic!()
    };
    assert!(ui.complete(id, Completion::Loaded { pages: 3 }));
    ui.input(Input::Turn(-1), 2200, context());
    assert_eq!(ui.page(), 2);
    ui.input(Input::Back, 2201, context());
    assert_eq!(ui.screen(), Screen::Slots);
    ui.input(Input::XDown, 2300, context());
    ui.input(Input::XUp, 2301, context());
    ui.input(Input::Back, 2302, context());
    assert_eq!(ui.screen(), Screen::Slots);
    ui.input(Input::XDown, 2400, context());
    assert_eq!(ui.input(Input::XUp, 2399, context()), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::Clock));
}
