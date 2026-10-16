use dashboard_config_tools::project::Project;
use dashboard_configurator::config::*;
use dashboard_configurator::preview::Preview;
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_core::display::pipeline::{BufferId, FrameToken};
use dashboard_core::startup::{Exit, Phase, StartupProfile, StartupStatus};
fn project(profile: StartupProfile) -> Project {
    Project::new(&validate(default_config()).unwrap(), profile, None).unwrap()
}
fn token(sequence: u64) -> FrameToken {
    FrameToken {
        buffer: BufferId::A,
        sequence,
    }
}
fn present(
    sim: &mut ControlSimulator,
    preview: &mut Preview,
    sequence: u64,
) {
    let t = token(sequence);
    preview.render_simulation_submitted(sim, t).unwrap();
    assert!(sim.complete_frame(t));
    assert!(preview.finish_pending(t));
}
#[test]
fn full_replay_uses_matching_completion_durations_and_preserves_pending_pixels() {
    let project = project(StartupProfile {
        status_ms: 100,
        duration_ms: 200,
        ..Default::default()
    });
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.restart_project(&project).unwrap();
    let start = sim.now_ms();
    preview.render_simulation_submitted(&mut sim, token(1)).unwrap();
    let pixels = preview.rgba().to_vec();
    sim.advance_to(start + 80, None);
    assert!(!sim.complete_frame(token(2)));
    assert_eq!(sim.startup_phase(), Phase::Status);
    assert_eq!(preview.pending_pixels(&sim, token(1)).unwrap(), pixels);
    assert!(sim.complete_frame(token(1)));
    preview.finish_pending(token(1));
    sim.advance_to(start + 179, None);
    assert_eq!(sim.startup_phase(), Phase::Status);
    sim.advance_to(start + 180, None);
    assert_eq!(sim.startup_phase(), Phase::Splash);
    present(&mut sim, &mut preview, 2);
    sim.advance_to(start + 379, None);
    assert_eq!(sim.startup_phase(), Phase::Splash);
    sim.advance_to(start + 380, None);
    assert_eq!(sim.startup_phase(), Phase::Dashboard);
    assert_eq!(sim.startup_exit(), Some(Exit::Finished));
    present(&mut sim, &mut preview, 3);
}
#[test]
fn startup_skip_is_consumed_and_recovery_stays_protected() {
    let project = project(StartupProfile {
        status: StartupStatus::Off,
        duration_ms: 10000,
        ..Default::default()
    });
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.restart_project(&project).unwrap();
    sim.tap(ButtonId::A);
    assert_eq!(sim.startup_exit(), Some(Exit::Skip));
    assert!(!sim.session().boost_psi());
    sim.tap(ButtonId::A);
    assert!(sim.session().boost_psi());
    sim.restart_project(&project).unwrap();
    let view = sim.session().view_id();
    sim.rotate(1);
    assert_eq!(sim.startup_exit(), Some(Exit::Skip));
    assert_eq!(sim.session().view_id(), view);
    sim.restart_project(&project).unwrap();
    sim.recover();
    assert_eq!(sim.startup_exit(), Some(Exit::Recovery));
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Full
    );
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
}
#[test]
fn startup_warnings_keep_acquiring_without_hidden_acknowledgment() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 2.;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.on_ms = 1;
    let config = validate(raw).unwrap();
    let profile = StartupProfile {
        status: StartupStatus::Off,
        duration_ms: 10000,
        ..Default::default()
    };
    let project = Project::new(&config, profile, None).unwrap();
    let mut sim = ControlSimulator::new(config.clone());
    let mut preview = Preview::new();
    sim.restart_project(&project).unwrap();
    let start = sim.now_ms();
    sim.advance_to(start, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    sim.advance_to(start + 1, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    assert!(sim.snapshot().sequence > 0);
    present(&mut sim, &mut preview, 1);
    assert_eq!(sim.startup_phase(), Phase::Splash);
    assert_eq!(sim.completed_warning().rules, 0);
    sim.acknowledge_warnings();
    assert!(sim.pending_warning_command().is_none());
    let interrupt = Project::new(
        &config,
        StartupProfile {
            interrupt_on_attention: true,
            ..profile
        },
        None,
    )
    .unwrap();
    sim.restart_project(&interrupt).unwrap();
    let start = sim.now_ms();
    sim.advance_to(start, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    sim.advance_to(start + 1, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    assert_eq!(sim.startup_exit(), Some(Exit::Attention));
    sim.restart_project(&project).unwrap();
    sim.startup_fault();
    assert_eq!(sim.startup_exit(), Some(Exit::Fault));
}
#[test]
fn stalled_startup_frames_expire_without_forging_completion_and_old_replay_is_retired() {
    let project = project(StartupProfile {
        status: StartupStatus::Off,
        duration_ms: 100,
        ..Default::default()
    });
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.restart_project(&project).unwrap();
    preview.render_simulation_submitted(&mut sim, token(1)).unwrap();
    let start = sim.now_ms();
    sim.advance_to(start + 600, None);
    assert_eq!(sim.startup_exit(), Some(Exit::Finished));
    assert_eq!(sim.completed_frame(), None);
    assert!(preview.pending_pixels(&sim, token(1)).is_some());
    sim.restart_project(&project).unwrap();
    assert!(!sim.complete_frame(token(1)));
    assert!(preview.pending_pixels(&sim, token(1)).is_none());
    present(&mut sim, &mut preview, 2);
    assert_eq!(sim.startup_phase(), Phase::Splash);
    sim.advance_to(sim.now_ms() - 1, None);
    assert_eq!(sim.startup_exit(), Some(Exit::Clock));
}

#[test]
fn naturally_finished_startup_suppresses_a_held_buttons_release() {
    use dashboard_core::input_model::EncoderBatch;
    let project = project(StartupProfile {
        status: StartupStatus::Off,
        duration_ms: 100,
        ..Default::default()
    });
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.restart_project(&project).unwrap();
    let start = sim.now_ms();
    present(&mut sim, &mut preview, 1);
    sim.physical_at([true, false, false, false], EncoderBatch::default(), start + 10);
    sim.physical_at([true, false, false, false], EncoderBatch::default(), start + 60);
    sim.advance_to(start + 100, None);
    assert_eq!(sim.startup_exit(), Some(Exit::Finished));
    sim.physical_at([false; 4], EncoderBatch::default(), start + 101);
    sim.physical_at([false; 4], EncoderBatch::default(), start + 151);
    assert!(!sim.session().boost_psi());
    sim.tap(ButtonId::A);
    assert!(sim.session().boost_psi());
}

#[test]
fn replay_retires_delivery_but_preserves_acquisition_history_and_clock() {
    let project = project(StartupProfile::default());
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.advance_to(sim.now_ms() + 500, Some([2.5, 110., 95., 80., 30., 700., 13.8, 1.]));
    present(&mut sim, &mut preview, 1);
    let snapshot = sim.snapshot();
    let clock = sim.now_ms();
    let old_source = sim.source_key();
    let visible = sim.completed_warning();
    preview.render_simulation_submitted(&mut sim, token(2)).unwrap();
    sim.restart_project(&project).unwrap();
    assert_eq!(sim.snapshot(), snapshot);
    assert_eq!(sim.now_ms(), clock);
    assert_ne!(sim.source_key(), old_source);
    assert_eq!(sim.completed_warning(), visible);
    assert!(!sim.complete_frame(token(2)));
    assert!(preview.pending_pixels(&sim, token(2)).is_none());
    present(&mut sim, &mut preview, 3);
    assert_eq!(sim.snapshot(), snapshot);
    assert_eq!(sim.startup_phase(), Phase::Status);
}

#[test]
fn editing_a_captured_startup_cancels_only_presentation() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.restart_project(&project(StartupProfile::default())).unwrap();
    sim.advance_to(sim.now_ms() + 50, Some([2.5, 110., 95., 80., 30., 700., 13.8, 1.]));
    present(&mut sim, &mut preview, 1);
    let snapshot = sim.snapshot();
    let clock = sim.now_ms();
    let visible = sim.completed_warning();
    preview.render_simulation_submitted(&mut sim, token(2)).unwrap();
    let source = sim.source_key();
    sim.cancel_startup_replay();
    assert_eq!(sim.snapshot(), snapshot);
    assert_eq!(sim.now_ms(), clock);
    assert_eq!(sim.completed_warning(), visible);
    assert_ne!(sim.source_key(), source);
    assert_eq!(sim.startup_phase(), Phase::Dashboard);
    assert!(!sim.complete_frame(token(2)));
    assert!(preview.pending_pixels(&sim, token(2)).is_none());
}

#[test]
fn replay_uses_a_fresh_relative_clock_and_reports_visible_startup_phase() {
    use dashboard_configurator::timing::Playback;
    let project = project(StartupProfile::default());
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.advance_to(sim.now_ms() + 555, None);
    let before = sim.now_ms();
    sim.restart_project(&project).unwrap();
    assert!(sim.summary().starts_with("Startup status"));
    present(&mut sim, &mut preview, 1);
    let mut clock = Playback::new(sim.now_ms());
    clock.start(95191.);
    sim.scenario_step(clock.sample(95241.)).unwrap();
    assert_eq!(sim.now_ms(), before + 50);
    assert_eq!(sim.startup_phase(), Phase::Status);
    let old_source = sim.source_key();
    sim.restart_project(&project).unwrap();
    assert_ne!(sim.source_key(), old_source);
    present(&mut sim, &mut preview, 2);
    let mut clock = Playback::new(sim.now_ms());
    clock.start(100000.);
    sim.scenario_step(clock.sample(100600.)).unwrap();
    assert!(sim.summary().starts_with("Startup splash"));
    present(&mut sim, &mut preview, 3);
    sim.scenario_step(clock.sample(103100.)).unwrap();
    assert!(sim.summary().starts_with("Dashboard"));
}
