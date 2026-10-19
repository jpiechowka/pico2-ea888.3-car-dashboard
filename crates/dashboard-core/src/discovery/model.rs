use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioPhase {
    Starting,
    Scanning,
    Paused,
    Stopping,
    Connecting,
    Discovering,
    Disconnecting,
    Fault,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeError {
    None,
    Scan,
    Connect,
    Timeout,
    Discovery,
    Capacity,
    Disconnected,
    Cleanup,
    Transport,
    InvalidPeer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Scan(bool),
    Inspect(Peer),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub revision: u32,
    pub action: Action,
}
impl Request {
    pub const INITIAL: Self = Self {
        revision: 1,
        action: Action::Scan(true),
    };
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Report {
        revision: u32,
        report: Advertisement,
    },
    State {
        revision: u32,
        phase: RadioPhase,
        error: ProbeError,
    },
    Item {
        revision: u32,
        item: InspectionItem,
    },
    Complete {
        revision: u32,
        limited: bool,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Rotate(i32),
    Click,
    Details,
    Back,
    Recover,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    List,
    Details,
}
pub struct Probe {
    pub catalog: Catalog,
    pub inspection: Inspection,
    pub selected: usize,
    pub page: usize,
    pub screen: Screen,
    pub phase: RadioPhase,
    pub error: ProbeError,
    pub request: Request,
    pub dropped_reports: u32,
    pub malformed_reports: u32,
    pub encoder_connected: bool,
    pub diagnostics: ProbeDiagnostics,
}
impl Default for Probe {
    fn default() -> Self { Self::new() }
}
impl Probe {
    pub const fn new() -> Self {
        Self {
            catalog: Catalog::new(),
            inspection: Inspection::new(),
            selected: 0,
            page: 0,
            screen: Screen::List,
            phase: RadioPhase::Starting,
            error: ProbeError::None,
            request: Request::INITIAL,
            dropped_reports: 0,
            malformed_reports: 0,
            encoder_connected: false,
            diagnostics: ProbeDiagnostics {
                fps: None,
                work: [None; 2],
                ram: None,
                stacks: [None; 2],
            },
        }
    }

    pub fn selected_device(&self) -> Option<&Device> { self.catalog.devices().get(self.selected) }

    fn issue(
        &mut self,
        action: Action,
    ) -> Option<Request> {
        let Some(revision) = self.request.revision.checked_add(1) else {
            self.phase = RadioPhase::Fault;
            self.error = ProbeError::Transport;
            return None;
        };
        self.request = Request { revision, action };
        self.phase = RadioPhase::Stopping;
        self.error = ProbeError::None;
        Some(self.request)
    }

    pub fn input(
        &mut self,
        input: Input,
        now_ms: u64,
    ) -> Option<Request> {
        match input {
            Input::Rotate(delta) => {
                if self.screen == Screen::List {
                    self.selected = (self.selected as i64 + i64::from(delta))
                        .clamp(0, self.catalog.devices().len().saturating_sub(1) as i64)
                        as usize;
                } else {
                    self.page = (self.page as i64 + i64::from(delta)).clamp(0, 4095) as usize;
                }
                None
            }
            Input::Click => {
                if matches!(
                    self.phase,
                    RadioPhase::Connecting
                        | RadioPhase::Discovering
                        | RadioPhase::Disconnecting
                        | RadioPhase::Stopping
                        | RadioPhase::Fault
                ) {
                    return None;
                }
                let device = self.selected_device()?;
                if device.age_ms(now_ms) > 30_000 {
                    self.error = ProbeError::InvalidPeer;
                    return None;
                }
                let peer = device.peer;
                self.inspection = Inspection::new();
                self.screen = Screen::Details;
                self.page = 0;
                self.issue(Action::Inspect(peer))
            }
            Input::Details => {
                if self.screen == Screen::Details {
                    return self.input(Input::Click, now_ms);
                }
                self.selected_device()?;
                self.screen = Screen::Details;
                self.page = 0;
                self.inspection = Inspection::new();
                self.issue(Action::Scan(false))
            }
            Input::Back => {
                if self.screen == Screen::Details {
                    self.screen = Screen::List;
                    self.page = 0;
                    self.issue(Action::Scan(true))
                } else {
                    self.issue(Action::Scan(!matches!(self.request.action, Action::Scan(true))))
                }
            }
            Input::Recover => {
                self.catalog = Catalog::new();
                self.inspection = Inspection::new();
                self.selected = 0;
                self.page = 0;
                self.screen = Screen::List;
                self.issue(Action::Scan(true))
            }
        }
    }

    pub fn event(
        &mut self,
        event: Event,
    ) {
        let revision = match event {
            Event::Report { revision, .. }
            | Event::State { revision, .. }
            | Event::Item { revision, .. }
            | Event::Complete { revision, .. } => revision,
        };
        if revision != self.request.revision {
            return;
        }
        match event {
            Event::Report { report, .. } if self.request.action == Action::Scan(true) => self.catalog.observe(report),
            Event::State { phase, error, .. } => {
                self.phase = phase;
                self.error = error;
            }
            Event::Item { item, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                self.inspection.accept(item)
            }
            Event::Complete { limited, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                if limited {
                    self.inspection.omitted = self.inspection.omitted.saturating_add(1);
                }
                self.inspection.complete = !limited && self.inspection.omitted == 0 && self.inspection.malformed == 0;
            }
            _ => {}
        }
    }
}
