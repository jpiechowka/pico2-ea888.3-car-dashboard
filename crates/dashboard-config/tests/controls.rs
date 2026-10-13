use dashboard_config::*;

#[test]
fn defaults_match_reviewed_onboard_and_encoder_table() {
    use ControlAction::*;
    let map = ControlMap::default();
    assert_eq!(map.hold_ms, 800);
    let expected = [
        (
            [
                (ToggleBoostUnits, PreviousView),
                (ResetStatistics, NextView),
                (CycleFps, CycleBrightness),
                (NextScreen, None),
            ],
            RotationAction::Views,
            CycleBrightness,
        ),
        (
            [
                (None, BrightnessDown),
                (None, BrightnessUp),
                (None, ToggleBacklight),
                (NextScreen, None),
            ],
            RotationAction::Brightness,
            ToggleBacklight,
        ),
        (
            [
                (None, ScrollOlder),
                (None, ScrollNewer),
                (None, LogsLive),
                (NextScreen, None),
            ],
            RotationAction::Logs,
            None,
        ),
    ];
    for (screen, (buttons, rotation, click)) in map.screens.iter().zip(expected) {
        for (actual, (tap, hold)) in screen.buttons.iter().zip(buttons) {
            assert_eq!(*actual, ButtonBinding { tap, hold });
        }
        assert_eq!(screen.encoder_rotation, rotation);
        assert_eq!(screen.encoder_click, click);
    }
    assert_eq!(validate_controls(&map), Ok(()));
    assert!(core::mem::size_of::<ControlMap>() <= 64);
}

#[test]
fn compatibility_is_the_independent_approved_table() {
    use ControlAction::*;
    let cases = [
        (None, [true, true, true]),
        (NextScreen, [true, true, true]),
        (PreviousScreen, [true, true, true]),
        (Dashboard, [true, true, true]),
        (Debug, [true, true, true]),
        (Logs, [true, true, true]),
        (NextView, [true, false, false]),
        (PreviousView, [true, false, false]),
        (ToggleBoostUnits, [true, false, false]),
        (CycleFps, [true, false, false]),
        (ResetStatistics, [true, false, false]),
        (AcknowledgeWarnings, [true, false, false]),
        (CycleBrightness, [true, false, false]),
        (ToggleBacklight, [false, true, false]),
        (BrightnessUp, [false, true, false]),
        (BrightnessDown, [false, true, false]),
        (ScrollOlder, [false, false, true]),
        (ScrollNewer, [false, false, true]),
        (LogsLive, [false, false, true]),
    ];
    for (action, expected) in cases {
        for (page, allowed) in ControlPage::ALL.into_iter().zip(expected) {
            assert_eq!(action.allowed_on(page), allowed, "{action:?} on {page:?}");
        }
    }
    for (action, expected) in [
        (RotationAction::None, [true, true, true]),
        (RotationAction::Views, [true, false, false]),
        (RotationAction::Brightness, [false, true, false]),
        (RotationAction::Logs, [false, false, true]),
    ] {
        for (page, allowed) in ControlPage::ALL.into_iter().zip(expected) {
            assert_eq!(action.allowed_on(page), allowed);
        }
    }
}

#[test]
fn hold_boundaries_and_reserved_recovery_are_enforced() {
    for (hold_ms, expected) in [
        (399, Err(ControlError::HoldDuration)),
        (400, Ok(())),
        (800, Ok(())),
        (1500, Ok(())),
        (1501, Err(ControlError::HoldDuration)),
    ] {
        let map = ControlMap {
            hold_ms,
            ..ControlMap::default()
        };
        assert_eq!(validate_controls(&map), expected);
    }
    for page in ControlPage::ALL {
        let mut map = ControlMap::default();
        map.screens[page.index()].buttons[ButtonId::Y.index()].hold = ControlAction::Dashboard;
        assert_eq!(validate_controls(&map), Err(ControlError::ReservedRecovery { page }));
    }
}

#[test]
fn invalid_bindings_identify_the_actual_gesture() {
    for page in ControlPage::ALL {
        let invalid = if page == ControlPage::Dashboard {
            ControlAction::ScrollOlder
        } else {
            ControlAction::NextView
        };
        for button in ButtonId::ALL {
            for hold in [false, true] {
                if button == ButtonId::Y && hold {
                    continue;
                }
                let mut map = ControlMap::default();
                let binding = &mut map.screens[page.index()].buttons[button.index()];
                if hold {
                    binding.hold = invalid;
                } else {
                    binding.tap = invalid;
                }
                assert_eq!(
                    validate_controls(&map),
                    Err(ControlError::IncompatibleButton { page, button, hold })
                );
            }
        }
        let mut map = ControlMap::default();
        map.screens[page.index()].encoder_click = invalid;
        assert_eq!(
            validate_controls(&map),
            Err(ControlError::IncompatibleEncoderClick { page })
        );
        map = ControlMap::default();
        map.screens[page.index()].encoder_rotation = if page == ControlPage::Dashboard {
            RotationAction::Logs
        } else {
            RotationAction::Views
        };
        assert_eq!(
            validate_controls(&map),
            Err(ControlError::IncompatibleEncoderRotation { page })
        );
    }
}

#[test]
fn onboard_routes_and_wrapped_view_fallback_are_required() {
    let mut map = ControlMap::default();
    for screen in &mut map.screens {
        screen.buttons[3].tap = ControlAction::None;
    }
    assert_eq!(
        validate_controls(&map),
        Err(ControlError::UnreachableScreen {
            page: ControlPage::Debug
        })
    );
    map.screens[0].encoder_click = ControlAction::Debug;
    assert_eq!(
        validate_controls(&map),
        Err(ControlError::UnreachableScreen {
            page: ControlPage::Debug
        })
    );
    map.screens[0].buttons[3].tap = ControlAction::Debug;
    assert_eq!(
        validate_controls(&map),
        Err(ControlError::UnreachableScreen {
            page: ControlPage::Logs
        })
    );
    map.screens[1].buttons[0].tap = ControlAction::Logs;
    assert_eq!(validate_controls(&map), Ok(())); // Recovery makes one-way outbound routes usable.
    map.screens[0].buttons[0].hold = ControlAction::None;
    assert_eq!(validate_controls(&map), Ok(()));
    map.screens[0].buttons[1].hold = ControlAction::None;
    assert_eq!(validate_controls(&map), Err(ControlError::MissingViewNavigation));
    map.screens[0].buttons[0].tap = ControlAction::PreviousView;
    assert_eq!(validate_controls(&map), Ok(()));
    map.screens[0].buttons[3].tap = ControlAction::PreviousScreen;
    map.screens[2].buttons[0].tap = ControlAction::PreviousScreen;
    assert_eq!(validate_controls(&map), Ok(()));
}

#[test]
fn page_and_button_indices_and_navigation_are_stable() {
    assert_eq!(ControlPage::default(), ControlPage::Dashboard);
    for (index, page) in ControlPage::ALL.into_iter().enumerate() {
        assert_eq!(page.index(), index);
        assert_eq!(page.next(), ControlPage::ALL[(index + 1) % 3]);
        assert_eq!(page.previous(), ControlPage::ALL[(index + 2) % 3]);
    }
    assert_eq!(ControlPage::Dashboard.toggle(), ControlPage::Debug);
    assert_eq!(ControlPage::Debug.toggle(), ControlPage::Logs);
    assert_eq!(ControlPage::Logs.toggle(), ControlPage::Dashboard);
    for (index, button) in ButtonId::ALL.into_iter().enumerate() {
        assert_eq!(button.index(), index);
    }
}

#[cfg(feature = "json")]
#[test]
fn serde_rejects_unknown_actions_and_struct_fields() {
    let map = ControlMap::default();
    let value = serde_json::to_value(map).unwrap();
    assert_eq!(serde_json::from_value::<ControlMap>(value.clone()).unwrap(), map);
    for path in [0, 1, 2] {
        let mut bad = value.clone();
        match path {
            0 => {
                bad["unexpected"] = true.into();
            }
            1 => {
                bad["screens"][0]["unexpected"] = true.into();
            }
            _ => {
                bad["screens"][0]["buttons"][0]["unexpected"] = true.into();
            }
        }
        assert!(serde_json::from_value::<ControlMap>(bad).is_err());
    }
    let mut bad = value;
    bad["screens"][0]["buttons"][0]["tap"] = "ExecuteCommand".into();
    assert!(serde_json::from_value::<ControlMap>(bad).is_err());
}
