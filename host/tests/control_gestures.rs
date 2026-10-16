use dashboard_host_validation::controls::ControlGestures;
use dashboard_host_validation::view_config::{ControlAction as Action, ControlMap, ControlPage as Page};

const RELEASED: [bool; 4] = [false; 4];
const A: [bool; 4] = [true, false, false, false];
const Y: [bool; 4] = [false, false, false, true];

#[test]
fn page_change_release_bounce_cannot_rearm_boot_held_y() { page_change_bounce_trace(Y, true); }

#[test]
fn page_change_release_bounce_cannot_rearm_old_screen_ordinary_key() { page_change_bounce_trace(A, false); }

#[test]
fn page_change_preserves_pending_release_qualification_before_a_debounced_press() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    trace(&mut g, &map, Page::Dashboard, &[(0, RELEASED), (100, A)]);
    trace(
        &mut g,
        &map,
        Page::Debug,
        &[(110, RELEASED), (160, RELEASED), (200, A), (250, A)],
    );
    assert_eq!(
        g.update(&map, Page::Debug, A, 1000).actions[0].action,
        Action::BrightnessDown
    );
}

fn page_change_bounce_trace(
    levels: [bool; 4],
    boot_held: bool,
) {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    if boot_held {
        g.update(&map, Page::Dashboard, levels, 0);
    } else {
        trace(
            &mut g,
            &map,
            Page::Dashboard,
            &[(0, RELEASED), (10, levels), (60, levels)],
        );
    }
    let samples = [
        (100, RELEASED),
        (110, levels),
        (160, levels),
        (2110, levels),
        (2200, RELEASED),
        (2250, RELEASED),
    ];
    for batch in trace(&mut g, &map, Page::Debug, &samples) {
        assert!(
            batch.actions.is_empty(),
            "a 10 ms released bounce must not arm a new-screen action"
        );
        assert!(
            batch.recovery.is_none(),
            "a 10 ms released bounce must not arm recovery"
        );
    }
    trace(&mut g, &map, Page::Debug, &[(2300, levels), (2350, levels)]);
    let threshold = if boot_held { 4300 } else { 3100 };
    let batch = g.update(&map, Page::Debug, levels, threshold);
    if boot_held {
        assert!(batch.recovery.is_some());
    } else {
        assert_eq!(batch.actions[0].action, Action::BrightnessDown);
    }
}

fn trace(
    g: &mut ControlGestures,
    map: &ControlMap,
    page: Page,
    samples: &[(u64, [bool; 4])],
) -> Vec<dashboard_host_validation::controls::ControlBatch> {
    samples
        .iter()
        .map(|&(time, levels)| g.update(map, page, levels, time))
        .collect()
}

#[test]
fn exact_default_hold_and_y_recovery_deadlines() {
    for (levels, threshold) in [(A, 900), (Y, 2100)] {
        let map = ControlMap::default();
        let mut g = ControlGestures::new();
        for batch in trace(
            &mut g,
            &map,
            Page::Dashboard,
            &[(0, RELEASED), (100, levels), (150, levels), (threshold - 1, levels)],
        ) {
            assert!(batch.actions.is_empty() && batch.recovery.is_none());
        }
        let batch = g.update(&map, Page::Dashboard, levels, threshold);
        let observation = if levels == Y {
            batch.recovery.unwrap()
        } else {
            assert_eq!(batch.actions[0].action, Action::PreviousView);
            batch.actions[0].observation
        };
        assert_eq!(observation.observed_us, 100000);
        assert_eq!(observation.accepted_us, threshold * 1000);
        assert_eq!(observation.events, 1);
    }
}

#[test]
fn every_compatible_action_is_dispatched_for_tap_and_hold() {
    let actions = [
        Action::None,
        Action::NextScreen,
        Action::PreviousScreen,
        Action::Dashboard,
        Action::Debug,
        Action::Logs,
        Action::NextView,
        Action::PreviousView,
        Action::ToggleBoostUnits,
        Action::CycleFps,
        Action::ResetStatistics,
        Action::CycleBrightness,
        Action::ToggleBacklight,
        Action::BrightnessUp,
        Action::BrightnessDown,
        Action::ScrollOlder,
        Action::ScrollNewer,
        Action::LogsLive,
        Action::AcknowledgeWarnings,
    ];
    for page in Page::ALL {
        for action in actions {
            for hold in [false, true] {
                let mut map = ControlMap::default();
                map.screens[page.index()].buttons[0].tap = if hold { Action::None } else { action };
                map.screens[page.index()].buttons[0].hold = if hold { action } else { Action::None };
                let mut g = ControlGestures::new();
                trace(&mut g, &map, page, &[(0, RELEASED), (100, A), (150, A)]);
                let batch = if hold {
                    g.update(&map, page, A, 900)
                } else {
                    g.update(&map, page, RELEASED, 200);
                    g.update(&map, page, RELEASED, 250)
                };
                if action == Action::None {
                    assert!(batch.actions.is_empty());
                    assert_eq!(batch.discarded, 0);
                } else if action.allowed_on(page) {
                    assert_eq!(batch.actions[0].action, action);
                    assert_eq!(batch.discarded, 0);
                } else {
                    assert!(batch.actions.is_empty());
                    assert_eq!(batch.discarded, 1);
                }
            }
        }
    }
}

#[test]
fn candidate_bounce_same_timestamp_and_regression_do_not_advance_gestures() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    let batches = trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[
            (0, RELEASED),
            (100, A),
            (100, A),
            (120, RELEASED),
            (130, A),
            (179, A),
            (180, A),
            (200, RELEASED),
            (220, A),
            (100, RELEASED),
            (250, A),
            (929, A),
        ],
    );
    assert!(batches.iter().all(|batch| batch.actions.is_empty()));
    let batch = g.update(&map, Page::Dashboard, A, 930);
    assert_eq!(batch.actions[0].observation.observed_us, 130000);
    assert_eq!(batch.actions[0].action, Action::PreviousView);
    assert!(g.update(&map, Page::Dashboard, A, 930).actions.is_empty());
}

#[test]
fn release_before_threshold_debounced_after_threshold_remains_a_tap() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(0, RELEASED), (100, A), (150, A), (899, RELEASED)],
    );
    let batch = g.update(&map, Page::Dashboard, RELEASED, 949);
    assert_eq!(batch.actions[0].action, Action::ToggleBoostUnits);
}

#[test]
fn recovery_overrides_simultaneous_units_reset_and_disarms_other_held_keys() {
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].hold = Action::ToggleBoostUnits;
    map.screens[0].buttons[1].hold = Action::ResetStatistics;
    map.screens[0].buttons[3].hold = Action::Logs; // raw corruption cannot replace recovery
    let levels = [true, true, false, true];
    let mut g = ControlGestures::new();
    trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(0, RELEASED), (100, levels), (150, levels)],
    );
    let batch = g.update(&map, Page::Dashboard, levels, 2100);
    assert!(batch.recovery.is_some());
    assert!(batch.actions.is_empty());
    assert_eq!(batch.discarded, 2);
    assert_eq!(batch.transition(Page::Dashboard), None);
    assert_eq!(batch.transition(Page::Logs), Some(Page::Dashboard));
    for batch in trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(2200, levels), (2300, RELEASED), (2350, RELEASED)],
    ) {
        assert!(batch.actions.is_empty() && batch.recovery.is_none());
    }
    g.update(&map, Page::Dashboard, A, 2400);
    g.update(&map, Page::Dashboard, A, 2450);
    assert_eq!(
        g.update(&map, Page::Dashboard, A, 3200).actions[0].action,
        Action::ToggleBoostUnits
    );
}

#[test]
fn transition_wins_in_button_order_and_self_navigation_keeps_other_actions() {
    for self_navigation in [false, true] {
        let mut map = ControlMap::default();
        map.screens[0].buttons[0].tap = if self_navigation {
            Action::Dashboard
        } else {
            Action::Logs
        };
        map.screens[0].buttons[1].tap = if self_navigation {
            Action::ResetStatistics
        } else {
            Action::Debug
        };
        let levels = [true, true, true, false];
        let mut g = ControlGestures::new();
        trace(
            &mut g,
            &map,
            Page::Dashboard,
            &[(0, RELEASED), (100, levels), (150, levels), (200, RELEASED)],
        );
        let batch = g.update(&map, Page::Dashboard, RELEASED, 250);
        if self_navigation {
            assert_eq!(batch.actions.len(), 3);
            assert_eq!(batch.transition(Page::Dashboard), None);
            assert_eq!(batch.discarded, 0);
        } else {
            assert_eq!(batch.actions.len(), 1);
            assert_eq!(batch.actions[0].action, Action::Logs);
            assert_eq!(batch.transition(Page::Dashboard), Some(Page::Logs));
            assert_eq!(batch.discarded, 2);
        }
    }
}

#[test]
fn page_changes_cancel_old_releases_and_held_keys_until_release() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    trace(&mut g, &map, Page::Dashboard, &[(0, RELEASED), (100, A), (150, A)]);
    for batch in trace(
        &mut g,
        &map,
        Page::Debug,
        &[(200, A), (5000, A), (5100, RELEASED), (5150, RELEASED)],
    ) {
        assert!(batch.actions.is_empty());
    }
    trace(&mut g, &map, Page::Debug, &[(5200, A), (5250, A)]);
    assert_eq!(
        g.update(&map, Page::Debug, A, 6000).actions[0].action,
        Action::BrightnessDown
    );
}

#[test]
fn invalid_hold_bounds_saturate_and_microsecond_overflow_fails_closed() {
    for (duration, threshold) in [(0, 400), (u16::MAX, 1500)] {
        let map = ControlMap {
            hold_ms: duration,
            ..ControlMap::default()
        };
        let mut g = ControlGestures::new();
        trace(&mut g, &map, Page::Dashboard, &[(0, RELEASED), (100, A), (150, A)]);
        assert!(
            g.update(&map, Page::Dashboard, A, 100 + threshold - 1)
                .actions
                .is_empty()
        );
        assert_eq!(
            g.update(&map, Page::Dashboard, A, 100 + threshold).actions[0].action,
            Action::PreviousView
        );
    }
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    let start = u64::MAX - 3000;
    trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(start, RELEASED), (start + 100, Y), (start + 150, Y)],
    );
    let batch = g.update(&map, Page::Dashboard, Y, u64::MAX);
    assert!(batch.actions.is_empty() && batch.recovery.is_none());
    assert_eq!(batch.discarded, 1);
}

#[test]
fn recovery_wins_over_navigation_and_cancels_an_unfired_held_tap_on_dashboard() {
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].hold = Action::NextScreen;
    map.screens[0].buttons[1].hold = Action::None;
    let levels = [true, true, false, true];
    let mut g = ControlGestures::new();
    trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(0, RELEASED), (100, levels), (150, levels)],
    );
    let batch = g.update(&map, Page::Dashboard, levels, 2100);
    assert!(batch.recovery.is_some());
    assert_eq!(batch.discarded, 1);
    assert_eq!(batch.transition(Page::Dashboard), None);
    for batch in trace(&mut g, &map, Page::Dashboard, &[(2200, RELEASED), (2250, RELEASED)]) {
        assert!(batch.actions.is_empty() && batch.recovery.is_none());
    }
}

#[test]
fn a_returned_transition_immediately_cancels_other_old_page_releases() {
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].hold = Action::NextScreen;
    map.screens[0].buttons[1].hold = Action::None;
    let levels = [true, true, false, false];
    let mut g = ControlGestures::new();
    trace(
        &mut g,
        &map,
        Page::Dashboard,
        &[(0, RELEASED), (100, levels), (150, levels)],
    );
    assert_eq!(
        g.update(&map, Page::Dashboard, levels, 900).transition(Page::Dashboard),
        Some(Page::Debug)
    );
    // The caller can postpone installing the new page; old releases still cannot act.
    for batch in trace(&mut g, &map, Page::Dashboard, &[(1000, RELEASED), (1050, RELEASED)]) {
        assert!(batch.actions.is_empty());
    }
}

#[test]
fn tap_and_hold_use_first_observation_and_stable_debounce() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    for (t, levels) in [(0, RELEASED), (100, A), (150, A), (200, RELEASED)] {
        assert!(g.update(&map, Page::Dashboard, levels, t).actions.is_empty());
    }
    let tap = g.update(&map, Page::Dashboard, RELEASED, 250);
    assert_eq!(tap.actions.len(), 1);
    assert_eq!(tap.actions[0].action, Action::ToggleBoostUnits);
    assert_eq!(tap.actions[0].observation.observed_us, 100_000);
    assert_eq!(tap.actions[0].observation.accepted_us, 250_000);
    g.update(&map, Page::Dashboard, A, 300);
    g.update(&map, Page::Dashboard, A, 350);
    assert!(g.update(&map, Page::Dashboard, A, 1099).actions.is_empty());
    let hold = g.update(&map, Page::Dashboard, A, 1100);
    assert_eq!(hold.actions[0].action, Action::PreviousView);
    assert_eq!(hold.actions[0].observation.observed_us, 300_000);
    for (t, levels) in [(1200, A), (1300, RELEASED), (1350, RELEASED)] {
        assert!(g.update(&map, Page::Dashboard, levels, t).actions.is_empty());
    }
}

#[test]
fn held_boot_and_reset_require_stable_release_before_recovery() {
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    for (t, levels) in [
        (0, Y),
        (3000, Y),
        (3100, RELEASED),
        (3149, RELEASED),
        (3150, Y),
        (6000, Y),
    ] {
        assert!(g.update(&map, Page::Dashboard, levels, t).recovery.is_none());
    }
    g.update(&map, Page::Dashboard, RELEASED, 6100);
    g.update(&map, Page::Dashboard, RELEASED, 6150);
    g.update(&map, Page::Dashboard, Y, 6200);
    g.update(&map, Page::Dashboard, Y, 6250);
    assert!(g.update(&map, Page::Dashboard, Y, 8199).recovery.is_none());
    assert!(g.update(&map, Page::Dashboard, Y, 8200).recovery.is_some());
    g.reset();
    assert!(g.update(&map, Page::Dashboard, Y, 12000).recovery.is_none());
    assert!(g.update(&map, Page::Dashboard, Y, 15000).recovery.is_none());
}

#[test]
fn sampled_release_never_invents_hold_and_none_hold_keeps_long_tap() {
    for no_hold in [false, true] {
        let mut map = ControlMap::default();
        if no_hold {
            map.screens[0].buttons[0].hold = Action::None;
        }
        let mut g = ControlGestures::new();
        for (t, levels) in [(0, RELEASED), (100, A), (150, A), (5000, RELEASED)] {
            assert!(g.update(&map, Page::Dashboard, levels, t).actions.is_empty());
        }
        assert_eq!(
            g.update(&map, Page::Dashboard, RELEASED, 5050).actions[0].action,
            Action::ToggleBoostUnits
        );
    }
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    for (t, levels) in [(0, RELEASED), (100, Y), (150, Y), (8000, RELEASED), (8050, RELEASED)] {
        assert!(g.update(&map, Page::Dashboard, levels, t).recovery.is_none());
    }
}
