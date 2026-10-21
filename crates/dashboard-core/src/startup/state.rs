//! Completion-gated startup phases with bounded fallback deadlines and one-shot input consumption.
use super::{StartupProfile, StartupStatus};
use crate::display::pipeline::FrameToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Startup screen selection; acquisition and framebuffer ownership remain external.
pub enum Phase {
    /// Optional brief or detailed initialization facts.
    Status,
    /// Optional procedural/custom artwork.
    Splash,
    /// Startup has ended or was disabled; normal selected screen is allowed.
    Dashboard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Why startup stopped; initially disabled startup has no recorded exit reason.
pub enum Exit {
    /// Configured display duration or bounded transfer-stall fallback elapsed.
    Finished,
    /// Ordinary startup button/encoder gesture consumed as skip.
    Skip,
    /// Protected recovery gesture exits startup while retaining recovery handling.
    Recovery,
    /// Invalid profile or explicit runtime fault bypassed optional startup.
    Fault,
    /// Attention interrupted startup because the profile enables this policy.
    Attention,
    /// A recognized completion or active-loop clock moved backward.
    Clock,
}

/// At most one identity waits for its first completed phase frame. No buffer owner.
pub struct Startup {
    /// Current presentation phase; Dashboard is terminal for normal advance calls.
    phase: Phase,
    /// Millisecond entry time for the current phase's duration-plus-500-ms hard deadline.
    entered: u64,
    /// Last accepted loop/completion clock, or explicit finish time, in milliseconds.
    last: u64,
    /// First matching completion clock of this phase, if observed; not physical scanout.
    visible: Option<u64>,
    /// First submitted frame token awaiting exact completion for this phase.
    first: Option<FrameToken>,
    /// Last explicit exit reason, absent before an exit or for initially disabled startup.
    exit: Option<Exit>,
}
impl Startup {
    /// Returns startup selection for `profile` at millisecond origin `now`, skipping
    /// zero-duration/disabled stages. Invalid profiles immediately finish with Fault.
    pub fn new(
        profile: StartupProfile,
        now: u64,
    ) -> Self {
        let phase = if !matches!(profile.status, StartupStatus::Off) && profile.status_ms > 0 {
            Phase::Status
        } else if profile.duration_ms > 0 {
            Phase::Splash
        } else {
            Phase::Dashboard
        };
        let mut state = Self {
            phase,
            entered: now,
            last: now,
            visible: None,
            first: None,
            exit: None,
        };
        if !profile.valid() {
            state.finish(Exit::Fault);
        }
        state
    }

    /// A corrupt optional profile must never prevent dashboard recovery.
    /// Returns decoded `bytes` and state at millisecond origin `now`; malformed
    /// encoding returns the default profile paired with an already fault-ended state.
    pub fn from_encoded(
        bytes: &[u8],
        now: u64,
    ) -> (StartupProfile, Self) {
        match StartupProfile::decode(bytes) {
            Some(profile) => (profile, Self::new(profile, now)),
            None => {
                let profile = StartupProfile::DEFAULT;
                let mut state = Self::new(profile, now);
                state.finish(Exit::Fault);
                (profile, state)
            }
        }
    }

    /// Returns the current startup/normal-screen phase.
    pub const fn phase(&self) -> Phase { self.phase }

    /// Returns whether startup still owns screen selection.
    pub const fn active(&self) -> bool { !matches!(self.phase, Phase::Dashboard) }

    /// Returns whether the active phase has neither a submitted first frame nor a completed one.
    pub const fn needs_first_frame(&self) -> bool { self.active() && self.visible.is_none() && self.first.is_none() }

    /// Returns the explicit exit reason, or `None` before exit/when initially disabled.
    pub const fn exit(&self) -> Option<Exit> { self.exit }

    /// Returns the last stored millisecond clock once inactive, otherwise `None`.
    pub const fn ended_at(&self) -> Option<u64> { if self.active() { None } else { Some(self.last) } }

    /// Ends startup with `reason` and stores `now` milliseconds as its input cutoff.
    /// This explicit override does not reject clock regression or a prior terminal state.
    pub fn finish_at(
        &mut self,
        reason: Exit,
        now: u64,
    ) {
        self.last = now;
        self.finish(reason);
    }

    /// Ends startup with `reason`, discarding the tracked first token while retaining
    /// the last clock. Does not cancel DMA/release framebuffer ownership or alter acquisition.
    pub fn finish(
        &mut self,
        reason: Exit,
    ) {
        self.phase = Phase::Dashboard;
        self.first = None;
        self.exit = Some(reason);
    }

    /// Consume startup gestures once, preserving protected recovery and rejecting late encoder observations.
    /// At `now` milliseconds, uses `buttons` and `encoder` to exit active startup;
    /// recovery takes priority, while skip clears ordinary button actions. Suppresses
    /// held inputs in `gestures` on either exit and drains the active encoder batch.
    /// After exit, removes only encoder observations at/before the recorded cutoff;
    /// recovery remains in `buttons` for the normal control owner to handle.
    pub fn consume_inputs(
        &mut self,
        gestures: &mut crate::controls::ControlGestures,
        buttons: &mut crate::controls::ControlBatch,
        encoder: &mut crate::input_model::EncoderBatch,
        now: u64,
    ) {
        if self.active() {
            if buttons.recovery.is_some() {
                self.finish_at(Exit::Recovery, now);
                gestures.suppress_held();
            } else if buttons.startup_skip || encoder.delta != 0 || encoder.presses != 0 {
                self.finish_at(Exit::Skip, now);
                gestures.suppress_held();
                buttons.actions.clear();
            }
            *encoder = Default::default();
        } else if let Some(ended) = self.ended_at() {
            let boundary = ended.saturating_mul(1000);
            if encoder.motion.is_some_and(|o| o.observed_us <= boundary) {
                encoder.delta = 0;
                encoder.motion = None;
            }
            if encoder.button.is_some_and(|o| o.observed_us <= boundary) {
                encoder.presses = 0;
                encoder.button = None;
            }
        }
    }

    /// Retains `token` only as the first submission of an active, not-yet-visible
    /// phase. Additional submissions do not replace its completion identity.
    pub fn submitted(
        &mut self,
        token: FrameToken,
    ) {
        if self.active() && self.visible.is_none() && self.first.is_none() {
            self.first = Some(token);
        }
    }

    /// Observes `token` completion at `now` milliseconds. Exact first-token match
    /// starts visible dwell and updates the monotonic clock; backward matching
    /// completion ends startup with Clock. Other/late tokens are inert.
    pub fn completed(
        &mut self,
        token: FrameToken,
        now: u64,
    ) {
        if self.first == Some(token) {
            if now < self.last {
                self.finish(Exit::Clock);
            } else {
                self.last = now;
                self.visible = Some(now);
                self.first = None;
            }
        }
    }

    /// Called once per actual loop; a stalled transfer never releases ownership.
    /// Advances under `profile` at monotonic `now` milliseconds, with `fault` taking
    /// priority over optionally enabled `attention` interruption. A phase expires
    /// after its first completion plus duration, or entry plus duration+500 ms even
    /// without completion. At most one phase transition occurs per call; inactive
    /// state is unchanged, and invalid profile/backward time ends startup.
    pub fn advance(
        &mut self,
        profile: StartupProfile,
        now: u64,
        fault: bool,
        attention: bool,
    ) {
        if !self.active() {
            return;
        }
        if now < self.last {
            self.finish(Exit::Clock);
            return;
        }
        self.last = now;
        if fault || !profile.valid() {
            self.finish(Exit::Fault);
            return;
        }
        if profile.interrupt_on_attention && attention {
            self.finish(Exit::Attention);
            return;
        }
        let duration = match self.phase {
            Phase::Status => profile.status_duration(),
            Phase::Splash => u64::from(profile.duration_ms),
            Phase::Dashboard => return,
        };
        let expired = self.visible.is_some_and(|start| now.saturating_sub(start) >= duration)
            || now.saturating_sub(self.entered) >= duration + 500;
        if expired {
            if self.phase == Phase::Status && profile.duration_ms > 0 {
                self.phase = Phase::Splash;
                self.entered = now;
                self.visible = None;
                self.first = None;
            } else {
                self.finish(Exit::Finished);
            }
        }
    }

    /// Returns milliseconds since this phase's first completion at `now`, saturating
    /// backward differences to zero; returns zero until completion. Finishing does
    /// not erase the stored completion, so callers normally use this only while active.
    pub fn elapsed(
        &self,
        now: u64,
    ) -> u64 {
        self.visible.map_or(0, |start| now.saturating_sub(start))
    }

    /// Appearance seeking has no physical-completion claim.
    /// Returns phase and phase-relative milliseconds for `profile` at overall
    /// `elapsed` milliseconds. Uses profile duration helpers without validating the
    /// raw profile; completed startup returns Dashboard and zero relative time.
    pub fn preview(
        profile: StartupProfile,
        elapsed: u64,
    ) -> (Phase, u64) {
        if elapsed < profile.status_duration() {
            (Phase::Status, elapsed)
        } else if elapsed < profile.total_duration() {
            (Phase::Splash, elapsed - profile.status_duration())
        } else {
            (Phase::Dashboard, 0)
        }
    }
}
