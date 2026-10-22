//! Capture UI gestures and affine operation fences across cancellation, recovery, timeout and generation exhaustion.

use dashboard_core::discovery::capture_ui::*;
use dashboard_core::discovery::names::SavedIdentity;
use dashboard_core::discovery::{Payload, Peer};

/// Returns a named random-address fixture repeating `byte` in all address positions; no stable-address admission is
/// implied.
fn identity(byte: u8) -> SavedIdentity {
    SavedIdentity {
        peer: Peer {
            address: [byte; 6],
            random: true,
        },
        name: Payload::new(b"Saved device"),
    }
}

/// Returns save-ready live context for revision 7.
fn context() -> Context {
    Context {
        save: SaveAvailability::Ready,
        revision: 7,
    }
}
/// Opens `ui` with a two-second X hold and completes its directory refresh with `slots` and no identities.
/// Consumes the release at 2001ms; unexpected action/state/completion panics.
fn open(
    ui: &mut CaptureUi,
    slots: [Slot; 4],
) {
    assert_eq!(ui.input(Input::XDown, 0, context()), None);
    let Action::Refresh { id } = ui.tick(2000).unwrap() else {
        panic!()
    };
    assert!(ui.complete(
        id,
        Completion::Listed {
            slots,
            identities: [None; 4]
        }
    ));
    assert_eq!(ui.input(Input::XUp, 2001, context()), None);
}

/// Hold opens slots once; a short X action occurs on release, and signed extreme turns wrap within four slots.
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

/// Overwrite confirmation is fenced to the live revision, and only its exact pending operation can publish the new
/// generation.
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
    assert!(!ui.complete(
        id + 1,
        Completion::Saved {
            generation: 10,
            identity: identity(1)
        }
    ));
    assert!(ui.complete(
        id,
        Completion::Saved {
            generation: 10,
            identity: identity(1)
        }
    ));
    assert_eq!(ui.slots()[0], Slot::Saved { generation: 10 });
    assert!(!ui.complete(id, Completion::Failed(UiError::Storage)));
}

/// Empty/no-peer/busy/timeout paths never erase; timed-out save completion updates storage identity while retaining the
/// error screen.
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
    assert!(ui.complete(
        id,
        Completion::Saved {
            generation: 1,
            identity: identity(1)
        }
    ));
    assert_eq!(ui.screen(), Screen::Error(UiError::Timeout));
    assert_eq!(ui.input(Input::Recover, 40_000, context()), Some(Action::Recover));
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.slots()[0], Slot::Saved { generation: 1 });
}

/// Corrupt/unsupported slots require explicit Reset; wrong completion types and navigation cannot release outstanding
/// I/O ownership.
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

/// Release recognizes a hold even without ticks, and overwrite confirmation expires at its fixed deadline.
#[test]
fn held_release_without_ticks_is_consumed_and_confirmation_expires() {
    let mut ui = CaptureUi::new();
    ui.input(Input::XDown, 0, context());
    let Action::Refresh { id } = ui.input(Input::XUp, 2000, context()).unwrap() else {
        panic!()
    };
    ui.complete(
        id,
        Completion::Listed {
            slots: [Slot::Saved { generation: 1 }; 4],
            identities: [None; 4],
        },
    );
    ui.input(Input::XDown, 2100, context());
    ui.input(Input::XUp, 2101, context());
    assert_eq!(ui.screen(), Screen::ConfirmOverwrite);
    assert_eq!(ui.input(Input::Click, 2101 + CONFIRM_TIMEOUT_MS, context()), None);
    assert_eq!(ui.screen(), Screen::Error(UiError::ConfirmationExpired));
}

/// Expired overwrite/reset intent cannot issue I/O or consume an operation identity; a fresh confirmation can.
#[test]
fn confirmation_expiry_preserves_slots_and_requires_fresh_intent() {
    for slot in [Slot::Saved { generation: 1 }, Slot::SavedBlocked { generation: 1 }] {
        for (at, expired) in [(10, 10_010), (10, 10_011), (u64::MAX - 10_000, u64::MAX)] {
            let mut ui = CaptureUi::new();
            named_directory(&mut ui, slot);
            let before = *ui.identities();
            assert_eq!(ui.input(Input::XShort, at, context()), None);
            assert_eq!(ui.tick(at + 9_999), None);
            assert!(matches!(ui.screen(), Screen::ConfirmOverwrite | Screen::ConfirmReset));
            assert_eq!(ui.input(Input::Click, expired, context()), None);
            assert_eq!(ui.pending(), None);
            assert_eq!(ui.slots(), &[slot; 4]);
            assert_eq!(ui.identities(), &before);
            assert_eq!(ui.screen(), Screen::Error(UiError::ConfirmationExpired));
            for input in [Input::Click, Input::XShort, Input::Turn(1)] {
                assert_eq!(ui.input(input, expired, context()), None);
                assert_eq!(ui.pending(), None);
            }
            ui.input(Input::Back, expired, context());
            assert_eq!(ui.input(Input::XShort, expired, context()), None);
            let action = ui.input(Input::Click, expired, context());
            assert_eq!(
                action,
                Some(if matches!(slot, Slot::Saved { .. }) {
                    Action::Save {
                        id: 2,
                        slot: 0,
                        revision: 7,
                        overwrite: true,
                    }
                } else {
                    Action::Reset { id: 2, slot: 0 }
                })
            );
        }
    }
}

/// Confirmation may be accepted just before expiry; reversed time rejects both mutations as a clock fault.
#[test]
fn confirmation_deadline_and_clock_reversal_do_not_authorize_expired_mutation() {
    for slot in [Slot::Saved { generation: 1 }, Slot::SavedBlocked { generation: 1 }] {
        for now in [9, 10_009] {
            let mut ui = CaptureUi::new();
            named_directory(&mut ui, slot);
            ui.input(Input::XShort, 10, context());
            let result = ui.input(Input::Click, now, context());
            if now == 9 {
                assert_eq!(result, None);
                assert_eq!(ui.pending(), None);
                assert_eq!(ui.screen(), Screen::Error(UiError::Clock));
                assert_eq!(ui.slots(), &[slot; 4]);
            } else {
                assert!(matches!(result, Some(Action::Save { .. } | Action::Reset { .. })));
                assert_eq!(ui.pending(), Some(2));
            }
        }
    }
}

/// Back consumes a pending X gesture so its later release cannot open slots or forward a live click.
#[test]
fn back_cancels_the_pending_x_gesture_without_opening_slots_or_forwarding_a_click() {
    let mut ui = CaptureUi::new();
    ui.input(Input::XDown, 0, context());
    assert_eq!(ui.input(Input::Back, 2000, context()), None);
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.input(Input::XUp, 2001, context()), None);
    assert_eq!(ui.pending(), None);
}

/// Abandoned completion updates its original slot; invalid generation/page counts preserve ownership and maximum page
/// navigation wraps safely.
#[test]
fn abandoned_results_keep_original_slot_and_reject_wrong_operation_or_empty_page_count() {
    let mut ui = CaptureUi::new();
    open(&mut ui, [Slot::Empty; 4]);
    ui.input(Input::XDown, 2100, context());
    let Action::Save { id, slot: 0, .. } = ui.input(Input::XUp, 2101, context()).unwrap() else {
        panic!()
    };
    assert!(!ui.complete(
        id,
        Completion::Saved {
            generation: 0,
            identity: identity(1)
        }
    ));
    ui.input(Input::Back, 2102, context());
    ui.input(Input::Turn(1), 2103, context());
    assert!(ui.complete(
        id,
        Completion::Saved {
            generation: 1,
            identity: identity(1)
        }
    ));
    assert_eq!(ui.selected(), 1);
    assert_eq!(
        ui.slots(),
        &[Slot::Saved { generation: 1 }, Slot::Empty, Slot::Empty, Slot::Empty]
    );
    assert_eq!(ui.identities(), &[Some(identity(1)), None, None, None]);
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

/// Producer-debounced short/hold semantics prevent release bounce from creating an inspection or duplicate hold action.
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
    assert!(state.complete(
        id,
        Completion::Listed {
            slots: [Slot::Empty; 4],
            identities: [None; 4]
        }
    ));
    let Action::Save { overwrite: false, .. } = state.input(Input::XShort, 5100, context()).unwrap() else {
        panic!()
    };
}

/// Loaded pages wrap, Back cancels confirmations, and reversed gesture time produces an explicit Clock error.
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

/// Opens `ui` using semantic XHold and completes all four entries with `slot` plus distinct named identities.
/// Panics unless the refresh action and completion match.
fn named_directory(
    ui: &mut CaptureUi,
    slot: Slot,
) {
    let Action::Refresh { id } = ui.input(Input::XHold, 0, context()).unwrap() else {
        panic!()
    };
    assert!(ui.complete(
        id,
        Completion::Listed {
            slots: [slot; 4],
            identities: [
                Some(identity(1)),
                Some(identity(2)),
                Some(identity(3)),
                Some(identity(4))
            ],
        }
    ));
}

/// Refresh keeps names only for readable slots; stale IDs and wrong operation results preserve the previous directory.
#[test]
fn listing_keeps_identity_only_for_readable_slots_and_ignores_stale_replies() {
    let mut ui = CaptureUi::new();
    named_directory(&mut ui, Slot::Saved { generation: 1 });
    let previous = *ui.identities();
    let Action::Refresh { id } = ui.input(Input::XHold, 10, context()).unwrap() else {
        panic!()
    };
    let listed = Completion::Listed {
        slots: [
            Slot::Saved { generation: 2 },
            Slot::SavedBlocked { generation: 1 },
            Slot::Empty,
            Slot::Corrupt,
        ],
        identities: [
            Some(identity(8)),
            Some(identity(9)),
            Some(identity(10)),
            Some(identity(11)),
        ],
    };
    assert!(!ui.complete(id - 1, listed));
    assert_eq!(ui.identities(), &previous);
    assert!(!ui.complete(
        id,
        Completion::Saved {
            generation: 2,
            identity: identity(12)
        }
    ));
    assert_eq!(ui.identities(), &previous);
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(id, listed));
    assert_eq!(ui.identities(), &[Some(identity(8)), Some(identity(9)), None, None]);
}

/// Matching I/O failure clears only the original slot's identity, except refresh failure clears the whole directory.
#[test]
fn matching_io_failure_clears_only_affected_identity_and_refresh_failure_clears_directory() {
    for operation in [Operation::Load, Operation::Save, Operation::Reset, Operation::Refresh] {
        let mut ui = CaptureUi::new();
        named_directory(
            &mut ui,
            if operation == Operation::Reset {
                Slot::SavedBlocked { generation: 1 }
            } else {
                Slot::Saved { generation: 1 }
            },
        );
        ui.input(Input::Turn(2), 10, context());
        let action = match operation {
            Operation::Load => ui.input(Input::Click, 11, context()),
            Operation::Refresh => ui.input(Input::XHold, 11, context()),
            Operation::Save | Operation::Reset => {
                ui.input(Input::XShort, 11, context());
                ui.input(Input::Click, 12, context())
            }
        }
        .unwrap();
        let id = match action {
            Action::Load { id, slot: 2 }
            | Action::Save { id, slot: 2, .. }
            | Action::Reset { id, slot: 2 }
            | Action::Refresh { id } => id,
            other => panic!("unexpected {other:?}"),
        };
        let previous = *ui.identities();
        assert!(!ui.complete(id + 1, Completion::Failed(UiError::Storage)));
        assert_eq!(ui.identities(), &previous);
        ui.input(Input::Back, 13, context());
        ui.input(Input::Turn(1), 14, context());
        assert!(ui.complete(id, Completion::Failed(UiError::Storage)));
        assert_eq!(ui.screen(), Screen::Slots);
        assert_eq!(
            *ui.identities(),
            if operation == Operation::Refresh {
                [None; 4]
            } else {
                [Some(identity(1)), Some(identity(2)), None, Some(identity(4))]
            }
        );
    }
}

/// Recovery changes screens but retains I/O ownership; matching reset/save completion updates only its slot's name.
#[test]
fn reset_and_recovered_save_update_identity_only_when_the_owner_finishes() {
    let mut ui = CaptureUi::new();
    named_directory(&mut ui, Slot::SavedBlocked { generation: 1 });
    ui.input(Input::XShort, 10, context());
    let Action::Reset { id, slot: 0 } = ui.input(Input::Click, 11, context()).unwrap() else {
        panic!()
    };
    ui.input(Input::Recover, 12, context());
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(id, Completion::Reset));
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.slots()[0], Slot::Empty);
    assert_eq!(
        ui.identities(),
        &[None, Some(identity(2)), Some(identity(3)), Some(identity(4))]
    );
    let Action::Refresh { id } = ui.input(Input::XHold, 13, context()).unwrap() else {
        panic!()
    };
    ui.complete(
        id,
        Completion::Listed {
            slots: [Slot::Empty; 4],
            identities: [None; 4],
        },
    );
    let Action::Save { id, slot: 0, .. } = ui.input(Input::XShort, 14, context()).unwrap() else {
        panic!()
    };
    ui.input(Input::Recover, 15, context());
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(
        id,
        Completion::Saved {
            generation: 1,
            identity: identity(5)
        }
    ));
    assert_eq!(ui.screen(), Screen::Live);
    assert_eq!(ui.identities(), &[Some(identity(5)), None, None, None]);
}

/// Timeout hides the unconfirmed slot identity without releasing its completion fence or dismissing the timeout on late
/// success.
#[test]
fn timed_out_operation_hides_unconfirmed_identity_and_retains_its_completion_fence() {
    let mut ui = CaptureUi::new();
    named_directory(&mut ui, Slot::Saved { generation: 1 });
    ui.input(Input::XShort, 10, context());
    let Action::Save { id, slot: 0, .. } = ui.input(Input::Click, 11, context()).unwrap() else {
        panic!()
    };
    ui.tick(11 + OPERATION_TIMEOUT_MS);
    assert_eq!(
        ui.identities(),
        &[None, Some(identity(2)), Some(identity(3)), Some(identity(4))]
    );
    assert_eq!(ui.pending(), Some(id));
    assert!(ui.complete(
        id,
        Completion::Saved {
            generation: 2,
            identity: identity(6)
        }
    ));
    assert_eq!(ui.screen(), Screen::Error(UiError::Timeout));
    assert_eq!(
        ui.identities(),
        &[
            Some(identity(6)),
            Some(identity(2)),
            Some(identity(3)),
            Some(identity(4))
        ]
    );
}

/// A save reaching maximum generation stays readable and immediately switches replacement to explicit Reset without
/// directory refresh.
#[test]
fn final_generation_save_stays_readable_and_requires_reset_without_refresh() {
    for (before, after, expected_screen) in [
        (1, 2, Screen::ConfirmOverwrite),
        (u64::MAX - 2, u64::MAX - 1, Screen::ConfirmOverwrite),
        (u64::MAX - 1, u64::MAX, Screen::ConfirmReset),
    ] {
        let mut ui = CaptureUi::new();
        named_directory(&mut ui, Slot::Saved { generation: before });
        let originals = *ui.identities();
        ui.input(Input::Turn(2), 10, context());
        assert_eq!(ui.input(Input::XShort, 11, context()), None);
        assert_eq!(ui.screen(), Screen::ConfirmOverwrite);
        let Action::Save {
            id,
            slot: 2,
            overwrite: true,
            ..
        } = ui.input(Input::Click, 12, context()).unwrap()
        else {
            panic!()
        };
        assert!(ui.complete(
            id,
            Completion::Saved {
                generation: after,
                identity: identity(9)
            }
        ));
        assert_eq!(ui.screen(), Screen::Slots);
        // The final generation remains readable; overflow blocks only replacement.
        let Action::Load { id, slot: 2 } = ui.input(Input::Click, 13, context()).unwrap() else {
            panic!()
        };
        assert!(ui.complete(id, Completion::Loaded { pages: 2 }));
        ui.input(Input::Back, 14, context());
        assert_eq!(ui.input(Input::XShort, 15, context()), None);
        assert_eq!(ui.screen(), expected_screen, "generation {after}");
        assert_eq!(ui.pending(), None);
        // Cancelling must retain the newly saved identity and every other slot.
        ui.input(Input::Back, 16, context());
        assert_eq!(ui.identities()[2], Some(identity(9)));
        for index in [0, 1, 3] {
            assert_eq!(ui.slots()[index], Slot::Saved { generation: before });
            assert_eq!(ui.identities()[index], originals[index]);
        }
        if after == u64::MAX {
            ui.input(Input::XShort, 17, context());
            let Action::Reset { id, slot: 2 } = ui.input(Input::Click, 18, context()).unwrap() else {
                panic!()
            };
            assert!(ui.complete(id, Completion::Reset));
            assert_eq!(ui.slots()[2], Slot::Empty);
            assert_eq!(ui.identities()[2], None);
            for index in [0, 1, 3] {
                assert_eq!(ui.slots()[index], Slot::Saved { generation: before });
                assert_eq!(ui.identities()[index], originals[index]);
            }
        }
    }
}

/// Late maximum-generation save completes its original slot after Back/Recover/timeout without changing the current
/// screen or other slots.
#[test]
fn hidden_final_generation_completion_updates_original_slot_and_keeps_screen() {
    for hide in 0..3 {
        let mut ui = CaptureUi::new();
        named_directory(
            &mut ui,
            Slot::Saved {
                generation: u64::MAX - 1,
            },
        );
        ui.input(Input::XShort, 10, context());
        let Action::Save { id, slot: 0, .. } = ui.input(Input::Click, 11, context()).unwrap() else {
            panic!()
        };
        match hide {
            0 => {
                ui.input(Input::Back, 12, context());
                ui.input(Input::Turn(1), 13, context());
            }
            1 => {
                ui.input(Input::Recover, 12, context());
            }
            _ => {
                ui.tick(11 + OPERATION_TIMEOUT_MS);
            }
        }
        let screen = ui.screen();
        assert_eq!(ui.pending(), Some(id));
        assert!(ui.complete(
            id,
            Completion::Saved {
                generation: u64::MAX,
                identity: identity(8)
            }
        ));
        assert_eq!(ui.pending(), None);
        assert_eq!(ui.screen(), screen);
        assert_eq!(ui.slots()[0], Slot::SavedBlocked { generation: u64::MAX });
        assert_eq!(ui.identities()[0], Some(identity(8)));
        for index in 1..4 {
            assert_eq!(
                ui.slots()[index],
                Slot::Saved {
                    generation: u64::MAX - 1
                }
            );
            assert_eq!(ui.identities()[index], Some(identity(index as u8 + 1)));
        }
    }
}
