use super::{StartupProfile, StartupStatus};
use crate::display::pipeline::FrameToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Status,
    Splash,
    Dashboard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Finished,
    Skip,
    Recovery,
    Fault,
    Attention,
    Clock,
}

/// At most one identity waits for its first completed phase frame. No buffer owner.
pub struct Startup {
    phase: Phase,
    entered: u64,
    last: u64,
    visible: Option<u64>,
    first: Option<FrameToken>,
    exit: Option<Exit>,
}
impl Startup {
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

    pub const fn phase(&self) -> Phase { self.phase }

    pub const fn active(&self) -> bool { !matches!(self.phase, Phase::Dashboard) }

    pub const fn exit(&self) -> Option<Exit> { self.exit }

    pub const fn ended_at(&self) -> Option<u64> { if self.active() { None } else { Some(self.last) } }

    pub fn finish_at(
        &mut self,
        reason: Exit,
        now: u64,
    ) {
        self.last = now;
        self.finish(reason);
    }

    pub fn finish(
        &mut self,
        reason: Exit,
    ) {
        self.phase = Phase::Dashboard;
        self.first = None;
        self.exit = Some(reason);
    }

    pub fn submitted(
        &mut self,
        token: FrameToken,
    ) {
        if self.active() && self.visible.is_none() && self.first.is_none() {
            self.first = Some(token);
        }
    }

    pub fn completed(
        &mut self,
        token: FrameToken,
        now: u64,
    ) {
        if self.first == Some(token) {
            if now < self.last {
                self.finish(Exit::Clock);
            } else {
                self.visible = Some(now);
                self.first = None;
            }
        }
    }

    /// Called once per actual loop; a stalled transfer never releases ownership.
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

    pub fn elapsed(
        &self,
        now: u64,
    ) -> u64 {
        self.visible.map_or(0, |start| now.saturating_sub(start))
    }

    /// Appearance seeking has no physical-completion claim.
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
