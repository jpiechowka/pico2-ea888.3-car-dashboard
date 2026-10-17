//! Reproducible state interleavings. Seeds and step numbers identify every failure.
//! These exercise software ownership and clocks, not DMA or physical scanout.
#[path = "../../test-support/generated.rs"]
mod generated;
use dashboard_config::Orientation;
use dashboard_core::backlight::Backlight;
use dashboard_core::display::damage::PanelHistory;
use dashboard_core::display::pipeline::{BufferId, BufferState, FrameLifecycle, FrameToken};
use dashboard_core::pages::Page;
use dashboard_core::startup::{Exit, Phase, Startup, StartupProfile, StartupStatus};

struct Generator(u64);
impl Generator {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(
        &mut self,
        bound: u64,
    ) -> u64 {
        self.next() % bound
    }
}

fn token(
    sequence: u64,
    buffer: BufferId,
) -> FrameToken {
    FrameToken { buffer, sequence }
}
fn states(lifecycle: &FrameLifecycle) -> [BufferState; 2] {
    [lifecycle.state(BufferId::A), lifecycle.state(BufferId::B)]
}

#[test]
fn stale_and_wrong_buffer_events_never_release_a_live_generation() {
    // Mutation target: removing token equality or freeing on rejected completion.
    // A legal two-buffer cycle is perturbed at every phase with fabricated,
    // previous-generation and wrong-buffer events. Its final ownership is literal.
    for seed in 1..=64 {
        let _case = generated::Case::new("runtime", seed, 0);
        let mut rng = Generator(seed);
        let mut owner = FrameLifecycle::new();
        let mut history = PanelHistory::new();
        let mut previous = None;
        for cycle in 0..64 {
            let _cycle = generated::Case::new("pipeline", seed, cycle);
            let buffer = if cycle % 2 == 0 { BufferId::A } else { BufferId::B };
            let orientation = [
                Orientation::Landscape,
                Orientation::Portrait,
                Orientation::LandscapeInverted,
                Orientation::PortraitInverted,
            ][rng.below(4) as usize];
            let current = owner.begin(buffer).unwrap();
            history.begin_render_in(current, orientation).unwrap();
            for phase in 0..4 {
                let _phase = generated::Case::new("phase", seed, phase);
                let before = states(&owner);
                let reference = history.reference();
                let panel = history.panel_orientation();
                for _ in 0..8 {
                    let wrong = match rng.below(3) {
                        0 => {
                            token(
                                current.sequence,
                                if buffer == BufferId::A {
                                    BufferId::B
                                } else {
                                    BufferId::A
                                },
                            )
                        }
                        1 => previous.unwrap_or(token(0, buffer)),
                        _ => token(current.sequence.saturating_add(1 + rng.below(100)), buffer),
                    };
                    assert_ne!(wrong, current);
                    assert!(owner.ready(wrong).is_err(), "seed={seed} cycle={cycle} phase={phase}");
                    assert!(owner.started(wrong).is_err());
                    assert!(owner.complete(wrong).is_err());
                    assert!(owner.discard(wrong).is_err());
                    assert!(owner.quarantine(wrong).is_err());
                    assert!(history.complete(wrong).is_err());
                    assert_eq!(states(&owner), before, "seed={seed} cycle={cycle} phase={phase}");
                    assert_eq!(history.reference(), reference);
                    assert_eq!(history.panel_orientation(), panel);
                }
                match phase {
                    0 => owner.ready(current).unwrap(),
                    1 => {
                        owner.queue(current).unwrap();
                        history.queue(current, None).unwrap();
                    }
                    2 => owner.started(current).unwrap(),
                    _ => {
                        owner.complete(current).unwrap();
                        history.complete(current).unwrap();
                    }
                }
            }
            assert_eq!(states(&owner), [BufferState::Free; 2]);
            assert_eq!(history.reference_for(orientation), Some(current));
            assert_eq!(history.panel_orientation(), Some(orientation));
            previous = Some(current);
        }
    }
}

#[test]
fn quarantine_is_absorbing_despite_arbitrary_late_events() {
    // Mutation target: a timeout/late success accidentally makes storage reusable.
    for seed in 1..=64 {
        let _case = generated::Case::new("runtime", seed, 0);
        let mut rng = Generator(seed);
        let mut owner = FrameLifecycle::new();
        let active = owner.begin(BufferId::A).unwrap();
        owner.ready(active).unwrap();
        owner.queue(active).unwrap();
        if seed % 2 == 0 {
            owner.started(active).unwrap();
        }
        owner.quarantine(active).unwrap();
        let other = owner.begin(BufferId::B).unwrap();
        owner.ready(other).unwrap();
        for step in 0..256 {
            let _step = generated::Case::new("quarantine", seed, step);
            let event = match rng.below(3) {
                0 => active,
                1 => other,
                _ => token(rng.next(), BufferId::A),
            };
            match rng.below(5) {
                0 => assert!(owner.complete(event).is_err()),
                1 => assert!(owner.started(event).is_err()),
                2 => assert!(owner.queue(event).is_err()),
                3 => assert!(owner.ready(event).is_err()),
                _ => assert!(owner.begin(BufferId::A).is_err()),
            }
            assert_eq!(
                owner.state(BufferId::A),
                BufferState::Quarantined(active),
                "seed={seed} step={step}"
            );
            assert_eq!(owner.state(BufferId::B), BufferState::Ready(other));
        }
    }
}

#[test]
fn off_requires_visible_feedback_then_its_own_clear_completion() {
    // Mutation target: starting the timer at click/submit, or accepting stale clear.
    for seed in 1..=256 {
        let _case = generated::Case::new("runtime", seed, 0);
        let mut rng = Generator(seed);
        let mut light = Backlight::new();
        let click = 100 + rng.below(10_000);
        let popup = light.encoder(Page::Dashboard, 0, 2, click).popup.unwrap();
        assert_eq!(light.compare(), 5000);
        let feedback = token(1, BufferId::A);
        let clear = token(2, BufferId::B);
        let visible = click + rng.below(30_000);
        light.frame_submitted(feedback);
        // Even a long unpresented interval cannot consume the visible two seconds.
        assert!(!light.expire_popup(popup, visible));
        light.frame_completed(token(0, BufferId::A), visible);
        assert!(!light.expire_popup(popup, visible));
        light.frame_completed(feedback, visible);
        for _ in 0..32 {
            let age = rng.below(2000);
            assert!(!light.expire_popup(popup, visible + age), "seed={seed} age={age}");
            assert_eq!(light.compare(), 5000);
        }
        assert!(light.expire_popup(popup, visible + 2000));
        assert_eq!(light.compare(), 5000);
        light.frame_submitted(clear);
        for _ in 0..32 {
            light.frame_completed(feedback, visible + 2000 + rng.below(10_000));
            assert_eq!(light.compare(), 5000);
        }
        if seed % 3 == 0 {
            light.wake_full();
            light.frame_completed(clear, visible + 12_000);
            assert_eq!(light.compare(), 10000, "recovery seed={seed}");
        } else if seed % 3 == 1 {
            light.interrupt_feedback();
            light.frame_completed(clear, visible + 12_000);
            assert_eq!(light.compare(), 5000, "replacement seed={seed}");
        } else {
            light.frame_completed(clear, visible + 12_000);
            assert_eq!(light.compare(), 0);
            light.encoder(Page::Dashboard, 0, 1, visible + 12_001);
            light.frame_completed(clear, visible + 12_002);
            assert_eq!(light.compare(), 10000);
        }
    }
}

#[test]
fn startup_deadlines_follow_completed_frames_across_random_poll_cadences() {
    // Mutation target: counting frames instead of elapsed time or accepting an
    // unrelated completion. Small deadlines also exercise the entry timeout.
    for seed in 1..=256 {
        let _case = generated::Case::new("runtime", seed, 0);
        let mut rng = Generator(seed);
        let duration = [1, 34, 500, 2500, 10_000][rng.below(5) as usize];
        let profile = StartupProfile {
            status: StartupStatus::Off,
            duration_ms: duration,
            ..StartupProfile::DEFAULT
        };
        let start = 1000;
        let delay = rng.below(500);
        let visible = start + delay;
        let mut state = Startup::new(profile, start);
        let first = token(1, BufferId::A);
        state.submitted(first);
        state.completed(token(1, BufferId::B), visible);
        assert_eq!(state.elapsed(visible + 100), 0);
        state.completed(first, visible);
        let mut now = visible;
        let deadline = visible + u64::from(duration);
        while now < deadline {
            state.advance(profile, now, false, true);
            assert_eq!(state.phase(), Phase::Splash, "seed={seed} now={now}");
            assert_eq!(state.elapsed(now), now - visible);
            now = (now + 1 + rng.below(97)).min(deadline);
        }
        state.advance(profile, deadline, false, true);
        assert_eq!((state.phase(), state.exit()), (Phase::Dashboard, Some(Exit::Finished)));
        // A late completion cannot resurrect startup or change its finished reason.
        state.completed(first, deadline + 1);
        state.advance(profile, u64::MAX, false, true);
        assert_eq!((state.phase(), state.exit()), (Phase::Dashboard, Some(Exit::Finished)));
    }
}

#[test]
fn stalled_startup_and_clock_faults_are_bounded_without_completion() {
    for duration in [0, 1, 500, 10_000] {
        let profile = StartupProfile {
            status: StartupStatus::Detailed,
            status_ms: 2000,
            duration_ms: duration,
            ..StartupProfile::DEFAULT
        };
        let mut state = Startup::new(profile, 100);
        state.submitted(token(1, BufferId::A));
        state.advance(profile, 2599, false, false);
        assert_eq!(state.phase(), Phase::Status);
        state.advance(profile, 2600, false, false);
        if duration > 0 {
            assert_eq!(state.phase(), Phase::Splash);
            state.submitted(token(2, BufferId::B));
            state.completed(token(1, BufferId::A), 2601);
            assert_eq!(state.elapsed(3000), 0);
            state.advance(profile, 2600 + u64::from(duration) + 500, false, false);
        }
        assert_eq!((state.phase(), state.exit()), (Phase::Dashboard, Some(Exit::Finished)));
        let mut backwards = Startup::new(profile, u64::MAX - 1);
        backwards.advance(profile, 0, false, false);
        assert_eq!(backwards.exit(), Some(Exit::Clock));
        let mut fault = Startup::new(profile, 100);
        fault.advance(profile, 100, true, false);
        assert_eq!(fault.exit(), Some(Exit::Fault));
    }
}
