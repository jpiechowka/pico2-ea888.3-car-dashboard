use dashboard_configurator::config::*;
use dashboard_configurator::simulator::ControlSimulator;

#[test]
fn gestures_navigate_preserve_choices_and_recover_from_off() {
    let config = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(config.clone());
    sim.tap(ButtonId::A);
    assert!(sim.session().boost_psi());
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    sim.hold(ButtonId::X);
    sim.advance(2500);
    assert_eq!(sim.session().backlight().compare(), 0);
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert!(sim.session().boost_psi());
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Full
    );
    assert!(sim.session().backlight().compare() > 0);
    sim.tap(ButtonId::B);
    assert!(sim.reset_pending());
    assert!(!matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Reset(_))
    ));
    sim.acknowledge_reset();
    assert!(matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Reset(_))
    ));
    sim.restart(config);
    assert!(!sim.session().boost_psi());
}

#[test]
fn invalid_maps_use_accepted_layout_and_cannot_disable_recovery() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::Logs;
    map.screens[2].buttons[3].hold = ControlAction::ToggleBoostUnits;
    sim.inspect_invalid(map);
    assert!(sim.invalid());
    sim.tap(ButtonId::A);
    assert_eq!(sim.session().page(), ControlPage::Logs);
    sim.rotate(-1);
    assert!(sim.session().log_anchor().is_some());
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
}

#[test]
fn incompatible_candidate_actions_are_counted_and_layout_is_accepted() {
    let accepted = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(accepted.clone());
    let mut map = accepted.config().controls;
    map.screens[0].buttons[0].tap = ControlAction::ScrollOlder;
    sim.inspect_invalid(map);
    sim.tap(ButtonId::A);
    assert_eq!(sim.ignored(), 1);
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(sim.config(), &accepted);
    sim.recover();
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Full
    );
}

#[test]
fn every_catalogue_action_runs_through_gestures_and_shared_session() {
    use dashboard_configurator::editor::{Edit, Editor};
    use dashboard_core::backlight::Brightness;
    use dashboard_core::render::FpsMode;
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Second".into() }).unwrap();
    for action in ControlAction::ALL {
        let page = if action.allowed_on(ControlPage::Dashboard) {
            ControlPage::Dashboard
        } else if action.allowed_on(ControlPage::Debug) {
            ControlPage::Debug
        } else {
            ControlPage::Logs
        };
        let mut sim = ControlSimulator::new(editor.preview().clone());
        if page != ControlPage::Dashboard {
            sim.tap(ButtonId::Y);
        }
        if page == ControlPage::Logs {
            sim.tap(ButtonId::Y);
        }
        if action == ControlAction::BrightnessUp {
            sim.rotate(-2);
        }
        if matches!(action, ControlAction::ScrollNewer | ControlAction::LogsLive) {
            sim.rotate(-2);
        }
        let brightness = sim.session().backlight().compare();
        let old_anchor = sim.session().log_anchor();
        let old_view = sim.session().view_id();
        let mut map = editor.preview().config().controls;
        map.screens[page.index()].buttons[0].tap = action;
        sim.inspect_invalid(map);
        sim.tap(ButtonId::A);
        if let Some(destination) = action.destination(page) {
            assert_eq!(sim.session().page(), destination, "{action:?}");
        } else {
            match action {
                ControlAction::None => assert_eq!(sim.session().page(), page),
                ControlAction::NextView | ControlAction::PreviousView => assert_ne!(sim.session().view_id(), old_view),
                ControlAction::ToggleBoostUnits => assert!(sim.session().boost_psi()),
                ControlAction::CycleFps => assert_eq!(sim.session().fps(), FpsMode::Instant),
                ControlAction::ResetStatistics => {
                    assert!(sim.reset_pending());
                    sim.acknowledge_reset();
                    assert!(matches!(
                        sim.session().popup(),
                        Some(dashboard_core::popup::Popup::Reset(_))
                    ));
                }
                ControlAction::CycleBrightness => assert_eq!(sim.session().backlight().brightness(), Brightness::Dim),
                ControlAction::ToggleBacklight => {
                    sim.advance(2500);
                    assert_eq!(sim.session().backlight().compare(), 0);
                }
                ControlAction::BrightnessUp => assert!(sim.session().backlight().compare() > brightness),
                ControlAction::BrightnessDown => assert!(sim.session().backlight().compare() < brightness),
                ControlAction::ScrollOlder | ControlAction::ScrollNewer => {
                    assert_ne!(sim.session().log_anchor(), old_anchor)
                }
                ControlAction::LogsLive => assert_eq!(sim.session().log_anchor(), None),
                _ => unreachable!("navigation handled above"),
            }
        }
    }
}

#[test]
fn off_feedback_clear_and_recovery_follow_presentation_rules() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.hold(ButtonId::X); // Full -> Dim
    sim.hold(ButtonId::X); // Dim -> Off, still presenting feedback
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Off
    );
    assert!(sim.session().backlight().compare() > 0);
    sim.advance(1700);
    assert!(sim.session().backlight().compare() > 0);
    sim.advance(400);
    assert_eq!(sim.session().backlight().compare(), 0);
    sim.recover();
    assert!(sim.session().backlight().compare() > 0);
    assert_eq!(sim.session().popup(), None);
}
