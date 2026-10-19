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
    Failure {
        revision: u32,
        failure: InspectionFailure,
    },
    FailureContext {
        revision: u32,
        context: ConnectionFailureContext,
    },
    InspectionEnd {
        revision: u32,
        at_ms: u64,
        cleanup_complete: bool,
        disconnect_reason: Option<u8>,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Rotate(i32),
    /// Encoder: request a fresh selected-peer GATT inspection when idle.
    Click,
    /// X: fresh advertisement or global scan diagnostics; Inspect from peer details.
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
    pub fault_latch: FaultLatch,
    pub radio_bus_hz: Option<u32>,
    pub failure: Option<InspectionFailure>,
    pub failure_context: ConnectionFailureContext,
    pub request: Request,
    pub dropped_reports: u32,
    pub malformed_reports: u32,
    pub scan_diagnostics: ScanDiagnostics,
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
            fault_latch: FaultLatch::EMPTY,
            radio_bus_hz: None,
            failure: None,
            failure_context: ConnectionFailureContext {
                local_error: None,
                disconnect_reason: None,
            },
            request: Request::INITIAL,
            dropped_reports: 0,
            malformed_reports: 0,
            scan_diagnostics: ScanDiagnostics::new(),
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

    fn record_failure(
        &mut self,
        failure: InspectionFailure,
    ) {
        // Preserve the operation's first cause through later teardown evidence.
        if self.failure.is_none() {
            self.failure = Some(failure);
            self.error = failure.error();
            self.page = 0;
        }
        self.inspection.complete = false;
    }

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
        self.failure = None;
        self.failure_context = ConnectionFailureContext::default();
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
                self.inspection.started_at_ms = Some(now_ms);
                self.screen = Screen::Details;
                self.page = 0;
                self.issue(Action::Inspect(peer))
            }
            Input::Details => {
                if self.screen == Screen::Details {
                    return self.input(Input::Click, now_ms);
                }
                if self.selected_device().is_none()
                    && self.scan_diagnostics.last.is_none()
                    && self.fault_latch == FaultLatch::EMPTY
                {
                    return None;
                }
                self.screen = Screen::Details;
                self.page = 0;
                // This is a fresh advertisement view, not a cached GATT result.
                // The renderer must not present this empty inspection as failed.
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
            | Event::Failure { revision, .. }
            | Event::FailureContext { revision, .. }
            | Event::InspectionEnd { revision, .. }
            | Event::Complete { revision, .. } => revision,
        };
        if revision != self.request.revision {
            return;
        }
        match event {
            Event::Report { report, .. } if self.request.action == Action::Scan(true) => self.catalog.observe(report),
            Event::State { phase, error, .. } => {
                self.phase = phase;
                self.error = if error == ProbeError::None {
                    self.failure.map_or(error, InspectionFailure::error)
                } else {
                    error
                };
                if self.error != ProbeError::None {
                    self.inspection.complete = false;
                }
            }
            Event::Failure { failure, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                self.record_failure(failure);
            }
            Event::Item { item, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                self.inspection.accept(item)
            }
            Event::FailureContext { context, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                self.failure_context.local_error = self.failure_context.local_error.or(context.local_error);
                self.failure_context.disconnect_reason =
                    self.failure_context.disconnect_reason.or(context.disconnect_reason);
                // The receive fault can arrive after the inventory's last await.
                // Even a normal local-host reason cannot hide that failure.
                if let Some(reason) = context.local_error.or_else(|| {
                    context
                        .disconnect_reason
                        .filter(|&code| !crate::ble::termination::normal_reason(Some(code)))
                        .map(FailureReason::DisconnectCode)
                }) {
                    self.record_failure(InspectionFailure::new(FailureStage::ConnectionEvents, reason));
                }
            }
            Event::Complete { limited, .. } if matches!(self.request.action, Action::Inspect(_)) => {
                if limited {
                    self.inspection.omitted = self.inspection.omitted.saturating_add(1);
                }
                self.inspection.complete = !limited
                    && self.error == ProbeError::None
                    && self.failure.is_none()
                    && self.inspection.stages.iter().all(|&complete| complete)
                    && self.inspection.omitted == 0
                    && self.inspection.malformed == 0;
            }
            Event::InspectionEnd {
                at_ms,
                cleanup_complete,
                disconnect_reason,
                ..
            } if matches!(self.request.action, Action::Inspect(_)) => {
                self.inspection.ended_at_ms = Some(at_ms);
                self.inspection.cleanup_complete = cleanup_complete;
                self.inspection.disconnect_reason = disconnect_reason;
                self.failure_context.disconnect_reason = self.failure_context.disconnect_reason.or(disconnect_reason);
                // No reason is expected for a failed/canceled create without a
                // link. A connected attempt requires terminal reason evidence.
                if cleanup_complete
                    && (self.inspection.link.is_some() || disconnect_reason.is_some())
                    && !crate::ble::termination::normal_reason(disconnect_reason)
                {
                    self.record_failure(InspectionFailure::new(
                        FailureStage::ConnectionEvents,
                        disconnect_reason.map_or(FailureReason::Disconnected, FailureReason::DisconnectCode),
                    ));
                }
            }
            _ => {}
        }
    }
}
