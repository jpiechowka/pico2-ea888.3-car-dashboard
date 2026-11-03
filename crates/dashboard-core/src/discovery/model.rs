//! Generation-checked discovery UI state; returns radio requests without performing I/O.
use super::*;

/// Last acknowledged radio-owner phase; UI requests initially enter `Stopping`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioPhase {
    /// Radio initialization has not yet acknowledged scanning.
    Starting,
    /// Advertising reports may be admitted for the current scan request.
    Scanning,
    /// Radio is idle with scanning paused.
    Paused,
    /// Prior operation must stop before the new request can run.
    Stopping,
    /// Explicit selected-peer connection is pending.
    Connecting,
    /// Connected peer's allowlisted inventory is being collected.
    Discovering,
    /// Connection teardown is pending acknowledgment.
    Disconnecting,
    /// Radio cannot accept ordinary inspection until recovery.
    Fault,
}
/// Coarse UI error; detailed operation and termination evidence is stored separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeError {
    /// No current operation error.
    None,
    /// Controller scan processing failed.
    Scan,
    /// Selected-peer connection attempt failed.
    Connect,
    /// Operation deadline expired.
    Timeout,
    /// GATT enumeration/read or declaration admission failed.
    Discovery,
    /// Bounded resources prevented complete evidence retention.
    Capacity,
    /// Link ended before a clean result was established.
    Disconnected,
    /// Radio teardown did not complete successfully.
    Cleanup,
    /// Controller transport or request-generation exhaustion prevented progress.
    Transport,
    /// Selected peer was stale or the connected identity did not match.
    InvalidPeer,
}
/// Desired radio-owner operation, acted on only after prior work is stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start or pause advertising scans.
    Scan(
        /// True starts scanning; false pauses it.
        bool,
    ),
    /// Connect to exactly the explicitly selected address and address type.
    Inspect(
        /// Immutable peer identity for this request.
        Peer,
    ),
    /// Separately confirmed fixed ECU reads; admitted only by the collector image.
    Collect(
        /// Fresh explicitly confirmed peer; never restored from a capture.
        Peer,
    ),
}
impl Action {
    /// Returns the selected peer for either explicit connection action, or None for scans.
    /// This accessor does not authorize collection or turn saved evidence into a request.
    pub const fn peer(self) -> Option<Peer> {
        match self {
            Self::Inspect(peer) | Self::Collect(peer) => Some(peer),
            Self::Scan(_) => None,
        }
    }
}
/// Monotonic request identity used to reject queued results from replaced operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// Nonzero generation; checked increments never wrap or reuse an old generation.
    pub revision: u32,
    /// Desired scan/inspection operation for this generation.
    pub action: Action,
}
impl Request {
    /// Boot request: start scanning at generation one, with no automatic connection.
    pub const INITIAL: Self = Self {
        revision: 1,
        action: Action::Scan(true),
    };
}
/// Radio-owner result; every variant carries the request generation that produced it.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Controller advertising report, admitted only while the matching request scans.
    Report {
        /// Originating request generation.
        revision: u32,
        /// Owned report including its own receipt timestamp.
        report: Advertisement,
    },
    /// Acknowledged owner phase and coarse error.
    State {
        /// Originating request generation.
        revision: u32,
        /// Owner's observed lifecycle state.
        phase: RadioPhase,
        /// Current owner error; `None` does not clear an already recorded detailed failure.
        error: ProbeError,
    },
    /// Candidate inventory item for an explicit inspection.
    Item {
        /// Originating request generation.
        revision: u32,
        /// Owned evidence validated again by inventory admission.
        item: InspectionItem,
    },
    /// Enumeration ended; clean completion still requires all stages and no losses/errors.
    Complete {
        /// Originating request generation.
        revision: u32,
        /// Producer hit a limit; adds one omission and prevents complete status.
        limited: bool,
    },
    /// Detailed operation failure; the first one is retained through teardown.
    Failure {
        /// Originating request generation.
        revision: u32,
        /// Failure stage/reason and optional query/timing evidence.
        failure: InspectionFailure,
    },
    /// Independent receive-task and termination evidence that may arrive after enumeration.
    FailureContext {
        /// Originating request generation.
        revision: u32,
        /// Local ingress error and controller disconnect reason, independently optional.
        context: ConnectionFailureContext,
    },
    /// Inspection lifecycle ended, with explicit cleanup and terminal-link evidence.
    InspectionEnd {
        /// Originating request generation.
        revision: u32,
        /// Boot-relative end time in milliseconds.
        at_ms: u64,
        /// Radio owner acknowledged that connection cleanup completed.
        cleanup_complete: bool,
        /// Controller HCI reason, absent when no terminal link event was observed.
        disconnect_reason: Option<u8>,
    },
}
/// Normalized UI input; physical hold/debounce recognition occurs outside this model.
#[derive(Clone, Copy, Debug)]
pub enum Input {
    /// Move selection/page with clamping, without wrapping.
    Rotate(
        /// Signed number of normalized encoder steps.
        i32,
    ),
    /// Request a fresh selected-peer GATT inspection, or confirm the currently
    /// visible, unexpired collector prompt; busy/stale contexts reject it.
    Click,
    /// Collector-only B hold: show confirmation; never sends or confirms by itself.
    Collect,
    /// Leave a pending collection confirmation before entering another UI workflow.
    DismissCollection,
    /// X: fresh advertisement or global scan diagnostics; Inspect from peer details.
    Details,
    /// Leave details and resume scanning, or toggle scan pause from the list.
    Back,
    /// Clear catalogue/inspection and request scanning; diagnostic fault evidence is retained.
    Recover,
}
/// Top-level probe display selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Stable-order peer list.
    List,
    /// Selected-peer evidence or global scan/fault diagnostics.
    Details,
    /// Local explicit consent for active fixed ECU reads; no radio command yet.
    CollectionConfirm,
}
/// Short-lived consent context, invalid after navigation, peer/revision change or clock reversal.
#[derive(Clone, Copy, Debug)]
pub struct CollectionConfirmation {
    /// Nonzero prompt instance, never reused within this model lifetime.
    id: u32,
    /// Successful transfer observation, absent before this exact prompt reaches the panel driver.
    presented_at_ms: Option<u64>,
    /// Exact selected address/type when the prompt was opened.
    peer: Peer,
    /// Radio generation whose visible peer selection prompted consent.
    revision: u32,
    /// Monotonic prompt-open time in boot milliseconds.
    started_at_ms: u64,
    /// Exclusive prompt deadline, fifteen seconds after opening; checked without wrapping.
    expires_at_ms: u64,
}
/// Single-owner discovery state shared by firmware controls and host tests/rendering.
pub struct Probe {
    /// True for the separately selected INFO or collector image. Display/evidence policy;
    /// this flag cannot authorize I/O, select a peer or qualify an adapter response.
    pub identity_mode: bool,
    /// Collector image exposes a separate confirmation. Radio also enforces the linked tool kind.
    pub collection_enabled: bool,
    /// Ephemeral local prompt; never serialized or restored as send authority.
    pub collection_confirmation: Option<CollectionConfirmation>,
    /// Checked prompt sequence; separate from radio revision because opening a prompt performs no I/O.
    collection_prompt_sequence: u32,
    /// Back requested cancellation without discarding this collection's terminal evidence generation.
    pub cancel_collection: bool,
    /// Bounded peers in first-admission order.
    pub catalog: Catalog,
    /// Current explicit inspection or empty fresh-advertisement view.
    pub inspection: Inspection,
    /// Zero-based catalogue selection; zero also represents an empty catalogue.
    pub selected: usize,
    /// Requested detail page, clamped to 0..=4095 by input; renderer handles actual page count.
    pub page: usize,
    /// Active list/details UI screen.
    pub screen: Screen,
    /// Last radio-owner phase, or locally entered stopping/fault state.
    pub phase: RadioPhase,
    /// Current coarse error; detailed failures survive later no-error state events.
    pub error: ProbeError,
    /// External fault latch snapshot; input recovery does not clear it here.
    pub fault_latch: FaultLatch,
    /// Observed/configured radio bus frequency in hertz, or unavailable.
    pub radio_bus_hz: Option<u32>,
    /// Retained boot/reset evidence, or unavailable; not a proven reset cause.
    pub boot: Option<crate::health::reset::BootEvidence>,
    /// First detailed failure for the current request, if any.
    pub failure: Option<InspectionFailure>,
    /// First observed local receive failure and first termination reason.
    pub failure_context: ConnectionFailureContext,
    /// Current generation and desired radio action.
    pub request: Request,
    /// Producer-supplied count of reports lost before model admission.
    pub dropped_reports: u32,
    /// Producer-supplied count of malformed reports.
    pub malformed_reports: u32,
    /// Global scan-error evidence retained across catalogue recovery.
    pub scan_diagnostics: ScanDiagnostics,
    /// Last supplied encoder connectivity status; not a presence probe.
    pub encoder_connected: bool,
    /// Current physical diagnostic observations; unavailable values stay optional.
    pub diagnostics: ProbeDiagnostics,
}
impl Default for Probe {
    /// Returns the initial scanning request with empty evidence and unavailable diagnostics.
    fn default() -> Self { Self::new() }
}
impl Probe {
    /// Returns boot state at generation one: empty list, starting phase, scan requested,
    /// no failures, and no fabricated hardware measurements. Performs no radio I/O.
    pub const fn new() -> Self {
        Self {
            identity_mode: false,
            collection_enabled: false,
            collection_confirmation: None,
            collection_prompt_sequence: 0,
            cancel_collection: false,
            catalog: Catalog::new(),
            inspection: Inspection::new(),
            selected: 0,
            page: 0,
            screen: Screen::List,
            phase: RadioPhase::Starting,
            error: ProbeError::None,
            fault_latch: FaultLatch::EMPTY,
            radio_bus_hz: None,
            boot: None,
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

    /// Returns `self`'s selected peer by bounds-checked index, or `None` for an empty/invalid selection.
    pub fn selected_device(&self) -> Option<&Device> { self.catalog.devices().get(self.selected) }

    /// Revokes any pending collection prompt without changing radio intent or retained evidence.
    /// Returns unit; entering slots or navigation must call this before accepting later clicks.
    pub fn dismiss_collection(&mut self) {
        self.collection_confirmation = None;
        if self.screen == Screen::CollectionConfirm {
            self.screen = Screen::List;
        }
    }

    /// Returns the current prompt instance for an immutable frame tag, or None outside confirmation.
    pub fn collection_prompt_id(&self) -> Option<u32> {
        self.collection_confirmation
            .as_ref()
            .filter(|_| self.screen == Screen::CollectionConfirm)
            .map(|c| c.id)
    }

    /// Records actual successful driver transfer for prompt `id` at boot-relative `at_ms`.
    /// Old/replaced/expired prompt frames grant no consent. This is not an LCD scanout measurement.
    pub fn collection_presented(
        &mut self,
        id: u32,
        at_ms: u64,
    ) {
        if self.screen == Screen::CollectionConfirm
            && let Some(c) = self.collection_confirmation.as_mut()
            && c.id == id
            && at_ms >= c.started_at_ms
            && at_ms < c.expires_at_ms
            && c.presented_at_ms.is_none()
        {
            c.presented_at_ms = Some(at_ms);
        }
    }

    /// Returns whether a click observed at `at_ms` strictly follows this prompt's transfer.
    /// Hardware queues preserve the edge timestamp so old queued clicks never authorize traffic.
    pub fn collection_click_allowed(
        &self,
        at_ms: u64,
    ) -> bool {
        self.collection_confirmation
            .as_ref()
            .is_some_and(|c| c.presented_at_ms.is_some_and(|shown| at_ms > shown) && at_ms < c.expires_at_ms)
    }

    /// Applies `input` observed at `observed_at_ms` using current boot time `now_ms`.
    /// A confirmation click queued before its exact frame completed, or from a future
    /// timestamp, is ignored. Returns only the same checked intent changes as input().
    pub fn input_observed(
        &mut self,
        input: Input,
        observed_at_ms: u64,
        now_ms: u64,
    ) -> Option<Request> {
        if self.screen == Screen::CollectionConfirm
            && matches!(input, Input::Click)
            && (observed_at_ms > now_ms || !self.collection_click_allowed(observed_at_ms))
        {
            return None;
        }
        self.input(input, now_ms)
    }

    /// Expires consent at exclusive deadline, backwards time, changed generation/peer or busy phase.
    /// `now_ms` is boot-monotonic milliseconds. No radio request or storage operation is produced.
    /// Keeps the expired confirmation screen until deliberate navigation so clicks on an
    /// old physical frame cannot fall through to ordinary Inspect before a repaint.
    pub fn expire_collection(
        &mut self,
        now_ms: u64,
    ) {
        if self.collection_confirmation.is_some_and(|confirmation| {
            !self.collection_enabled
                || now_ms < confirmation.started_at_ms
                || now_ms >= confirmation.expires_at_ms
                || self.request.revision != confirmation.revision
                || !matches!(self.phase, RadioPhase::Scanning | RadioPhase::Paused)
                || self.selected_device().is_none_or(|device| {
                    device.peer != confirmation.peer || now_ms < device.last_seen_ms || device.age_ms(now_ms) > 30_000
                })
        }) {
            self.collection_confirmation = None;
        }
    }

    /// Retains `failure` only if `self` has no earlier detailed failure, resetting its
    /// detail page then. Always invalidates inventory completion; returns unit.
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

    /// Returns a new checked generation for `action`, putting `self` into `Stopping`
    /// and clearing request-local errors. On generation exhaustion returns `None`
    /// and enters transport fault without reusing an identity or clearing earlier evidence.
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
        self.cancel_collection = false;
        self.phase = RadioPhase::Stopping;
        self.error = ProbeError::None;
        self.failure = None;
        self.failure_context = ConnectionFailureContext::default();
        Some(self.request)
    }

    /// Applies normalized `input` at boot-relative `now_ms` milliseconds to `self`.
    /// Returns a radio request only when scan/inspection/confirmed collection intent changes; navigation,
    /// ignored busy inputs, missing selection and peers older than 30 seconds return `None`.
    /// Ordinary inspection age saturates on time reversal; collection consent is revoked.
    /// A fresh inspection clears old inventory;
    /// recovery clears catalogue/inspection but retains global diagnostics and fault latch.
    /// Request overflow faults without wrapping; preceding UI mutations remain applied.
    pub fn input(
        &mut self,
        input: Input,
        now_ms: u64,
    ) -> Option<Request> {
        // An expired confirmation click must not fall through to ordinary Inspect.
        let was_confirmation = self.screen == Screen::CollectionConfirm;
        self.expire_collection(now_ms);
        if was_confirmation && matches!(input, Input::Click) {
            if !self.collection_click_allowed(now_ms) {
                return None;
            }
            let confirmation = self.collection_confirmation.take()?;
            self.inspection.clear();
            self.inspection.started_at_ms = Some(now_ms);
            self.screen = Screen::Details;
            self.page = 0;
            return self.issue(Action::Collect(confirmation.peer));
        }
        if was_confirmation && !matches!(input, Input::Collect) {
            self.dismiss_collection();
            if matches!(input, Input::Back) {
                return None;
            }
        }
        match input {
            Input::DismissCollection => {
                self.dismiss_collection();
                None
            }
            Input::Collect => {
                if !self.collection_enabled
                    || !matches!(self.phase, RadioPhase::Scanning | RadioPhase::Paused)
                    || self.collection_confirmation.is_some()
                {
                    return None;
                }
                let device = self.selected_device()?;
                if now_ms < device.last_seen_ms || device.age_ms(now_ms) > 30_000 {
                    self.error = ProbeError::InvalidPeer;
                    return None;
                }
                let id = self.collection_prompt_sequence.checked_add(1)?;
                let expires_at_ms = now_ms.checked_add(15_000)?;
                let peer = device.peer;
                self.collection_prompt_sequence = id;
                self.collection_confirmation = Some(CollectionConfirmation {
                    id,
                    presented_at_ms: None,
                    peer,
                    revision: self.request.revision,
                    started_at_ms: now_ms,
                    expires_at_ms,
                });
                self.screen = Screen::CollectionConfirm;
                None
            }
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
                self.inspection.clear();
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
                self.inspection.clear();
                self.issue(Action::Scan(false))
            }
            Input::Back => {
                if matches!(self.request.action, Action::Collect(_))
                    && !matches!(self.phase, RadioPhase::Paused | RadioPhase::Fault)
                {
                    self.cancel_collection = true;
                    self.inspection.complete = false;
                    self.phase = RadioPhase::Disconnecting;
                    // Keep this evidence generation until actual cleanup and terminal publication.
                    return None;
                }
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
                self.inspection.clear();
                self.selected = 0;
                self.page = 0;
                self.screen = Screen::List;
                self.issue(Action::Scan(true))
            }
        }
    }

    /// Applies owned `event` to `self` only for the current request generation, returning unit.
    /// Scan reports require active scan intent; inventory events require inspection intent.
    /// Clean enumeration requires all four stages, no error, and no omitted/malformed items.
    /// Late local/abnormal disconnect evidence revokes completion while retaining the first
    /// failure. Cleanup acknowledgment is separate; admitted GAP names may be cached only
    /// for the exact requested peer with a non-reversing inspection end timestamp.
    #[inline(always)]
    pub fn event(
        &mut self,
        event: Event,
    ) {
        self.event_ref(&event);
    }

    /// Applies borrowed `event` with the same generation and state checks as [`Self::event`].
    /// Copies only admitted payloads into resident state; the event borrow ends on return.
    /// Does not publish a request, retain queue storage or perform I/O.
    #[inline(always)]
    pub fn event_ref(
        &mut self,
        event: &Event,
    ) {
        let revision = match event {
            Event::Report { revision, .. }
            | Event::State { revision, .. }
            | Event::Item { revision, .. }
            | Event::Failure { revision, .. }
            | Event::FailureContext { revision, .. }
            | Event::InspectionEnd { revision, .. }
            | Event::Complete { revision, .. } => *revision,
        };
        if revision != self.request.revision {
            return;
        }
        match event {
            Event::Report { report, .. } if self.request.action == Action::Scan(true) => {
                self.catalog.observe_ref(report)
            }
            Event::State { phase, error, .. } => {
                self.phase = *phase;
                self.error = if *error == ProbeError::None {
                    self.failure.map_or(*error, InspectionFailure::error)
                } else {
                    *error
                };
                if self.error != ProbeError::None {
                    self.inspection.complete = false;
                }
            }
            Event::Failure { failure, .. } if self.request.action.peer().is_some() => {
                self.record_failure(*failure);
            }
            Event::Item { item, .. } if self.request.action.peer().is_some() => self.inspection.accept_ref(item),
            Event::FailureContext { context, .. } if self.request.action.peer().is_some() => {
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
            Event::Complete { limited, .. } if self.request.action.peer().is_some() => {
                if *limited {
                    self.inspection.omitted = self.inspection.omitted.saturating_add(1);
                }
                self.inspection.complete = !self.cancel_collection
                    && !*limited
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
            } if self.request.action.peer().is_some() => {
                if let Some(peer) = self.request.action.peer()
                    && self.inspection.started_at_ms.is_some_and(|start| start <= *at_ms)
                {
                    // Only admitted allowlisted reads from this request can name
                    // this exact peer. The read remains separately inspectable.
                    if let Some(value) = self.inspection.values.iter().find(|v| {
                        v.uuid == 0x2A00
                            && v.result == ReadResult::Ok
                            && !v.value.truncated
                            && !v.value.as_slice().is_empty()
                    }) {
                        self.catalog.remember_gap_name(peer, &value.value, *at_ms);
                    }
                }
                self.inspection.ended_at_ms = Some(*at_ms);
                self.inspection.cleanup_complete = *cleanup_complete;
                self.inspection.disconnect_reason = *disconnect_reason;
                self.failure_context.disconnect_reason = self.failure_context.disconnect_reason.or(*disconnect_reason);
                // No reason is expected for a failed/canceled create without a
                // link. A connected attempt requires terminal reason evidence.
                if *cleanup_complete
                    && (self.inspection.link.is_some() || disconnect_reason.is_some())
                    && !crate::ble::termination::normal_reason(*disconnect_reason)
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
