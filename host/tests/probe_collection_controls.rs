//! Actual shared UI authority tests; synthetic advertisements never open hardware.
use dashboard_core::discovery::*;

/// Returns a fresh synthetic peer at boot10ms in an idle collector or ordinary image.
/// `enabled` selects collector and identity capabilities together; false keeps the ordinary policy.
fn probe(enabled: bool) -> Probe {
    let mut p = Probe::new();
    p.collection_enabled = enabled;
    p.identity_mode = enabled;
    p.phase = RadioPhase::Scanning;
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1, 2, 3, 4, 5, 6],
            random: false,
        },
        ReportKind::Connectable,
        -50,
        b"\x05\x09Test",
        10,
    ));
    p
}

/// Marks the exact current prompt as transferred at `at_ms`; never creates a radio request.
/// `p` is the live model with that prompt; missing prompt identity panics as a fixture failure.
fn presented(
    p: &mut Probe,
    at_ms: u64,
) {
    p.collection_presented(p.collection_prompt_id().unwrap(), at_ms);
}

/// Ordinary images cannot offer collection; collector Inspect still uses the adapter-only action.
#[test]
fn collection_requires_image_and_separate_local_confirmation() {
    let mut ordinary = probe(false);
    assert!(ordinary.input(Input::Collect, 20).is_none());
    assert_eq!(ordinary.screen, Screen::List);
    assert!(matches!(
        ordinary.input(Input::Click, 21).unwrap().action,
        Action::Inspect(_)
    ));
    let mut p = probe(true);
    assert!(p.input(Input::Collect, 20).is_none());
    assert_eq!(p.request, Request::INITIAL);
    assert_eq!(p.screen, Screen::CollectionConfirm);
    assert!(p.input(Input::Collect, 21).is_none());
    assert_eq!(p.request, Request::INITIAL);
    assert!(
        p.input(Input::Click, 21).is_none(),
        "queued click before a rendered prompt"
    );
    presented(&mut p, 22);
    assert!(!p.collection_click_allowed(21));
    assert!(!p.collection_click_allowed(22));
    assert!(
        p.input_observed(Input::Click, 21, 23).is_none(),
        "backlogged old click after presentation"
    );
    assert!(p.input_observed(Input::Click, 24, 23).is_none(), "future timestamp");
    assert!(matches!(p.input(Input::Click, 23).unwrap().action, Action::Collect(_)));
    assert!(p.collection_confirmation.is_none());
    assert_eq!(p.phase, RadioPhase::Stopping);
}

/// Expiry equality, backwards clock, generation/peer changes and stale advertisements revoke consent.
#[test]
fn confirmation_never_survives_changed_context_or_invalid_time() {
    for case in 0..6 {
        let mut p = probe(true);
        p.input(Input::Collect, 20);
        presented(&mut p, 20);
        let now = match case {
            0 => 15_020,
            1 => 19,
            2 => {
                p.request.revision += 1;
                21
            }
            3 => {
                p.selected = 1;
                21
            }
            4 => 30_011,
            _ => {
                p.phase = RadioPhase::Fault;
                21
            }
        };
        assert!(p.input(Input::Click, now).is_none(), "case{case}");
        assert!(!matches!(p.request.action, Action::Collect(_)));
        assert!(p.collection_confirmation.is_none());
    }
    let mut p = probe(true);
    assert!(p.input(Input::Collect, 9).is_none());
    assert!(p.collection_confirmation.is_none());
}

/// Timer expiry must consume clicks even before an expired/replacement frame can reach the panel.
#[test]
fn expired_visible_prompt_never_falls_through_to_inspect() {
    let mut p = probe(true);
    p.input(Input::Collect, 20);
    presented(&mut p, 21);
    let request = p.request;
    p.expire_collection(15_020);
    assert!(p.collection_confirmation.is_none());
    assert_eq!(p.screen, Screen::CollectionConfirm);
    assert!(p.input_observed(Input::Click, 15_021, 15_022).is_none());
    assert!(p.input(Input::Click, 15_023).is_none());
    assert_eq!(p.request, request);
    assert!(p.input(Input::Back, 15_024).is_none());
    assert_eq!(p.screen, Screen::List);
    assert!(matches!(
        p.input(Input::Click, 15_025).unwrap().action,
        Action::Inspect(_)
    ));
}

/// Navigation, capture entry and recovery invalidate the pending confirmation without collection.
#[test]
fn navigation_and_recovery_cancel_collection_consent() {
    for input in [
        Input::Back,
        Input::Rotate(1),
        Input::Details,
        Input::DismissCollection,
        Input::Recover,
    ] {
        let mut p = probe(true);
        p.input(Input::Collect, 20);
        let result = p.input(input, 21);
        assert!(!result.is_some_and(|r| matches!(r.action, Action::Collect(_))));
        assert!(p.collection_confirmation.is_none());
        assert_ne!(p.screen, Screen::CollectionConfirm);
    }
}

/// Busy operations and counter exhaustion cannot mint a collection generation.
#[test]
fn busy_and_exhausted_generation_reject_collection() {
    for phase in [
        RadioPhase::Starting,
        RadioPhase::Stopping,
        RadioPhase::Connecting,
        RadioPhase::Discovering,
        RadioPhase::Disconnecting,
        RadioPhase::Fault,
    ] {
        let mut p = probe(true);
        p.phase = phase;
        assert!(p.input(Input::Collect, 20).is_none());
        assert!(p.collection_confirmation.is_none());
    }
    let mut p = probe(true);
    p.request.revision = u32::MAX;
    p.input(Input::Collect, 20);
    presented(&mut p, 20);
    assert!(p.input(Input::Click, 21).is_none());
    assert_eq!(p.phase, RadioPhase::Fault);
    assert!(!matches!(p.request.action, Action::Collect(_)));
}

/// A completed old frame cannot grant consent to a new same-peer prompt; cancellation preserves evidence ownership.
#[test]
fn prompt_completion_identity_and_active_cancel_are_not_generation_replacements() {
    let mut p = probe(true);
    p.input(Input::Collect, 20);
    let old = p.collection_prompt_id().unwrap();
    p.input(Input::Back, 21);
    p.input(Input::Collect, 22);
    p.collection_presented(old, 23);
    assert!(p.input(Input::Click, 24).is_none());
    presented(&mut p, 25);
    let request = p.input(Input::Click, 26).unwrap();
    assert!(p.input(Input::Back, 27).is_none());
    assert_eq!(p.request, request);
    assert!(p.cancel_collection);
    assert_eq!(p.phase, RadioPhase::Disconnecting);
    p.inspection.stages = [true; 4];
    p.event(Event::Complete {
        revision: request.revision,
        limited: false,
    });
    assert!(!p.inspection.complete, "queued completion cannot undo cancellation");
    assert!(p.input(Input::Click, 28).is_none());
    p.event(Event::State {
        revision: request.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    assert_eq!(p.input(Input::Back, 29).unwrap().action, Action::Scan(true));
    assert!(!p.cancel_collection);
}
