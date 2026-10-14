//! One owner for capability discovery, fair bounded polling and sample history.
//! The caller supplies known-DLC frames from a qualified transport epoch.
use dashboard_config::{SourceResolution, ValidatedConfig, resolve_source};

use super::CanFrame;
use super::client::{Client, ClientError, Event, Failure, TxAction, TxToken};
use super::obd::{self, Capabilities, Decoded, Support};
use super::request::Request;
use crate::acquisition::{AcquisitionModel, DashboardSnapshot};
use crate::metrics::{Distribution, Histogram};
use crate::samples::{Quality, Sample, SensorId, Source};

pub const QUERY_GAP_MS: u64 = 20;
pub const FAILURE_QUIET_MS: u64 = 1_000;
pub const PIDS: [u8; 7] = [0, 0x20, 0x40, 5, 0x0F, 0x42, 0x5C];
const PERIODS_MS: [u64; 7] = [0, 0, 0, 1_000, 1_000, 500, 1_500];
const CHANNELS: [(u8, SensorId); 4] = [
    (5, SensorId::Water),
    (0x0F, SensorId::Iat),
    (0x42, SensorId::Battery),
    (0x5C, SensorId::Oil),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    Client(ClientError),
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counters {
    pub requested: u32,
    pub transmitted: u32,
    pub abandoned: u32,
    pub completed: u32,
    pub negative: u32,
    pub timeouts: u32,
    pub malformed: u32,
    pub transport_faults: u32,
    pub replies: u32,
    pub last_reply_ms: Option<u64>,
    pub min_reply_ms: Option<u64>,
    pub max_reply_ms: Option<u64>,
}
#[derive(Clone, Copy)]
enum Outcome {
    Positive,
    Negative,
    Timeout,
    Malformed,
}
impl Counters {
    fn finish(
        &mut self,
        outcome: Outcome,
        reply_ms: Option<u64>,
    ) {
        let count = match outcome {
            Outcome::Positive => &mut self.completed,
            Outcome::Negative => &mut self.negative,
            Outcome::Timeout => &mut self.timeouts,
            Outcome::Malformed => &mut self.malformed,
        };
        *count = count.saturating_add(1);
        if let Some(ms) = reply_ms {
            self.replies = self.replies.saturating_add(1);
            self.last_reply_ms = Some(ms);
            self.min_reply_ms = Some(self.min_reply_ms.map_or(ms, |old| old.min(ms)));
            self.max_reply_ms = Some(self.max_reply_ms.map_or(ms, |old| old.max(ms)));
        }
    }
}
struct ActiveTiming {
    index: usize,
    request_tx: TxToken,
    sent_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelTelemetry {
    pub pid: u8,
    pub counters: Counters,
    pub reply_ms: Distribution,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Telemetry {
    pub ecu: u16,
    pub epoch: u64,
    pub at_ms: u64,
    pub channels: [ChannelTelemetry; 7],
}
#[derive(Clone, Copy, Default)]
struct Slot {
    due: u64,
    failures: u8,
    discovered: bool,
}

pub struct Session<'a> {
    client: Client,
    capabilities: Capabilities,
    model: AcquisitionModel<'a>,
    slots: [Slot; 7],
    cursor: usize,
    next_query: u64,
    last_ms: u64,
    counters: Counters,
    channels: [Counters; 7],
    timing: Option<ActiveTiming>,
    reply_ms: [Histogram<160>; 7],
}

/// Validate only the small transport fields before assembling a large owner.
/// Kept inside diagnostics so callers cannot bypass Session validation.
pub(super) struct PreparedSession {
    client: Client,
    capabilities: Capabilities,
}
impl PreparedSession {
    pub(super) fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, SessionError> {
        let capabilities = Capabilities::new(ecu, epoch).map_err(|_| SessionError::Client(ClientError::Address))?;
        let client = Client::new(ecu - 8, ecu, epoch).map_err(SessionError::Client)?;
        Ok(Self { client, capabilities })
    }

    pub(super) fn assemble(
        self,
        config: Option<&ValidatedConfig>,
    ) -> Session<'_> {
        let mut session = self.construction_base(config);
        session.initialize_measurements();
        session
    }

    // A valid construction value; the enclosing Session or Vehicle immediately
    // finishes its already-bound model before returning a public ready owner.
    pub(super) fn construction_base(
        self,
        config: Option<&ValidatedConfig>,
    ) -> Session<'_> {
        Session {
            client: self.client,
            capabilities: self.capabilities,
            model: AcquisitionModel::session_base(config),
            slots: [Slot::default(); 7],
            cursor: 0,
            next_query: 0,
            last_ms: 0,
            counters: Counters::default(),
            channels: [Counters::default(); 7],
            timing: None,
            reply_ms: const { [const { Histogram::new() }; 7] },
        }
    }
}
impl<'a> Session<'a> {
    pub(super) fn initialize_measurements(&mut self) { self.model.initialize_for_session(self.client.response_id()); }

    pub fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, SessionError> {
        Self::new_inner(ecu, epoch, None)
    }

    pub fn configured(
        ecu: u16,
        epoch: u64,
        config: &'a ValidatedConfig,
    ) -> Result<Self, SessionError> {
        Self::new_inner(ecu, epoch, Some(config))
    }

    pub fn configuration(&self) -> Option<&ValidatedConfig> { self.model.configuration() }

    fn new_inner(
        ecu: u16,
        epoch: u64,
        config: Option<&'a ValidatedConfig>,
    ) -> Result<Self, SessionError> {
        Ok(PreparedSession::new(ecu, epoch)?.assemble(config))
    }

    pub fn snapshot(&self) -> DashboardSnapshot { self.model.snapshot() }

    /// Presentation advances even when transport is offline or parked.
    pub fn tick_measurements(
        &mut self,
        now: u64,
    ) {
        self.model.tick(now);
    }

    pub const fn counters(&self) -> Counters { self.counters }

    /// Lifetime counters, including discovery pages, retained across epochs.
    pub fn channel(
        &self,
        pid: u8,
    ) -> Option<Counters> {
        PIDS.iter()
            .position(|&known| known == pid)
            .map(|index| self.channels[index])
    }

    /// Call outside latency-sensitive receive work; percentile scans are bounded.
    pub fn telemetry(&self) -> Telemetry {
        Telemetry {
            ecu: self.client.response_id(),
            epoch: self.client.epoch(),
            at_ms: self.last_ms,
            channels: core::array::from_fn(|index| {
                ChannelTelemetry {
                    pid: PIDS[index],
                    counters: self.channels[index],
                    reply_ms: self.reply_ms[index].summary(),
                }
            }),
        }
    }

    fn abandon(
        &mut self,
        fault: bool,
    ) {
        if let Some(active) = self.timing.take() {
            let channel = &mut self.channels[active.index];
            channel.abandoned = channel.abandoned.saturating_add(1);
            channel.transport_faults = channel.transport_faults.saturating_add(u32::from(fault));
            self.counters.abandoned = self.counters.abandoned.saturating_add(1);
        }
    }

    pub const fn requires_new_epoch(&self) -> bool { self.client.requires_new_epoch() }

    pub fn apply_warning_command(
        &mut self,
        command: crate::warning_state::WarningCommand,
        now: u64,
    ) -> crate::warning_state::CommandResult {
        self.model.apply_warning_command(command, now)
    }

    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        self.model.reset_statistics(generation)
    }

    fn fault_all(
        &mut self,
        quality: Quality,
        now: u64,
    ) {
        for id in SensorId::ALL {
            self.model.mark_fault(id, quality, now);
        }
    }

    fn clock(
        &mut self,
        now: u64,
    ) -> Result<(), SessionError> {
        if now < self.last_ms {
            self.abandon(true);
            self.client.invalidate_transport();
            self.fault_all(Quality::TransportError, self.last_ms);
            return Err(SessionError::Client(ClientError::Clock));
        }
        self.last_ms = now;
        Ok(())
    }

    fn client_result<T>(
        &mut self,
        result: Result<T, ClientError>,
        now: u64,
    ) -> Result<T, SessionError> {
        result.map_err(|error| {
            if matches!(
                error,
                ClientError::Clock | ClientError::Exhausted | ClientError::WireFault
            ) {
                self.last_ms = self.last_ms.max(now);
                self.abandon(true);
                self.client.invalidate_transport();
                self.fault_all(Quality::TransportError, self.last_ms);
            }
            SessionError::Client(error)
        })
    }

    pub fn transport_fault(
        &mut self,
        now: u64,
    ) -> Result<(), SessionError> {
        self.clock(now)?;
        self.abandon(true);
        self.client.invalidate_transport();
        self.fault_all(Quality::TransportError, now);
        self.counters.transport_faults = self.counters.transport_faults.saturating_add(1);
        Ok(())
    }

    /// Call only after the adapter owner establishes a new qualified boundary.
    /// Last-good history and request generations survive; old capabilities do not.
    pub fn change_epoch(
        &mut self,
        epoch: u64,
        now: u64,
    ) -> Result<(), SessionError> {
        // Reject an old boundary without advancing the current time or state.
        if epoch <= self.client.epoch() {
            return Err(SessionError::Client(ClientError::Epoch));
        }
        self.clock(now)?;
        self.client.change_epoch(epoch, now).map_err(SessionError::Client)?;
        self.abandon(false);
        self.capabilities = Capabilities::new(self.client.response_id(), epoch).unwrap();
        self.slots = [Slot::default(); 7];
        self.cursor = 0;
        self.next_query = now;
        self.fault_all(Quality::Unknown, now);
        Ok(())
    }

    fn eligible(
        &self,
        index: usize,
    ) -> bool {
        let pid = PIDS[index];
        if index < 3 {
            (3..PIDS.len()).any(|i| self.period(i).is_some())
                && !self.slots[index].discovered
                && (index == 0 || self.capabilities.supported(pid) == Support::Supported)
        } else {
            self.period(index).is_some() && self.capabilities.supported(pid) == Support::Supported
        }
    }

    fn period(
        &self,
        index: usize,
    ) -> Option<u64> {
        if index < 3 {
            return Some(0);
        }
        let Some(config) = self.configuration() else {
            return Some(PERIODS_MS[index]);
        };
        let (_, channel) = CHANNELS[index - 3];
        match resolve_source(
            config,
            channel.channel(),
            self.client.response_id() - 8,
            self.client.response_id(),
        ) {
            SourceResolution::Obd { pid, period_ms, .. } if pid == PIDS[index] => Some(u64::from(period_ms)),
            _ => None,
        }
    }

    /// Advance active transaction deadlines even when receive backlog prevents
    /// admitting a new request. This method never schedules a request.
    pub fn tick(
        &mut self,
        now: u64,
    ) -> Result<(), SessionError> {
        self.tick_measurements(now);
        self.tick_protocol(now)
    }

    // Vehicle batches measurement expiry after received inputs and commands.
    pub(crate) fn tick_protocol(
        &mut self,
        now: u64,
    ) -> Result<(), SessionError> {
        self.clock(now)?;
        let result = self.client.tick(now);
        let event = self.client_result(result, now)?;
        self.event(event, now);
        if self.client.requires_new_epoch() {
            return Err(SessionError::Client(ClientError::WireFault));
        }
        Ok(())
    }

    pub fn poll(
        &mut self,
        now: u64,
    ) -> Result<Option<TxAction>, SessionError> {
        self.tick_measurements(now);
        self.poll_protocol(now)
    }

    pub(crate) fn poll_protocol(
        &mut self,
        now: u64,
    ) -> Result<Option<TxAction>, SessionError> {
        self.tick_protocol(now)?;
        if self.client.active() || now < self.next_query {
            return Ok(None);
        }
        for offset in 0..PIDS.len() {
            let index = (self.cursor + offset) % PIDS.len();
            if self.eligible(index) && self.slots[index].due <= now {
                let result = self.client.start(Request::Obd(PIDS[index]), now);
                let tx = self.client_result(result, now)?;
                self.timing = Some(ActiveTiming {
                    index,
                    request_tx: tx.token,
                    sent_ms: None,
                });
                self.channels[index].requested = self.channels[index].requested.saturating_add(1);
                self.counters.requested = self.counters.requested.saturating_add(1);
                self.cursor = (index + 1) % PIDS.len();
                return Ok(Some(tx));
            }
        }
        Ok(None)
    }

    pub fn receive(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now: u64,
    ) -> Result<Option<TxAction>, SessionError> {
        self.clock(now)?;
        let result = self.client.receive(frame, epoch, now);
        let event = self.client_result(result, now)?;
        Ok(self.event(event, now))
    }

    pub fn tx_completed(
        &mut self,
        token: TxToken,
        now: u64,
    ) -> Result<Option<TxAction>, SessionError> {
        // The client's stale-token guard precedes its clock guard. Preserve that
        // property so an old completion cannot modify the current transaction.
        let result = self.client.tx_completed(token, now);
        let event = self.client_result(result, now)?;
        self.clock(now)?;
        // Only an accepted completion for the original request starts RTT.
        // Flow-control, duplicate and stale tokens never reset this timestamp.
        if let Some(active) = &mut self.timing
            && active.request_tx == token
            && active.sent_ms.is_none()
            && !matches!(event, Event::Failed { .. })
        {
            active.sent_ms = Some(now);
            self.channels[active.index].transmitted = self.channels[active.index].transmitted.saturating_add(1);
            self.counters.transmitted = self.counters.transmitted.saturating_add(1);
        }
        Ok(self.event(event, now))
    }

    fn finish(
        &mut self,
        request: Request,
        now: u64,
        failure: Option<Quality>,
        outcome: Outcome,
    ) {
        let Request::Obd(pid) = request else { return };
        let Some(index) = PIDS.iter().position(|&known| known == pid) else {
            return;
        };
        let reply_ms = self
            .timing
            .take()
            .filter(|active| active.index == index && matches!(outcome, Outcome::Positive | Outcome::Negative))
            .and_then(|active| active.sent_ms)
            .and_then(|sent| now.checked_sub(sent));
        self.channels[index].finish(outcome, reply_ms);
        self.counters.finish(outcome, reply_ms);
        if let Some(ms) = reply_ms {
            self.reply_ms[index].record(ms);
        }
        let period = self.period(index).unwrap_or(PERIODS_MS[index]);
        let configured = self.configuration().is_some();
        let slot = &mut self.slots[index];
        if let Some(quality) = failure {
            slot.failures = slot.failures.saturating_add(1);
            let backoff = 250u64 << u32::from(slot.failures.saturating_sub(1).min(5));
            slot.due = now.saturating_add(if configured { backoff.max(period) } else { backoff });
            self.next_query = now.saturating_add(FAILURE_QUIET_MS);
            if let Some((_, id)) = CHANNELS.iter().find(|(known, _)| *known == pid) {
                self.model.mark_fault(*id, quality, now);
            }
        } else {
            slot.failures = 0;
            slot.discovered = index < 3;
            slot.due = now.saturating_add(period);
            self.next_query = now.saturating_add(QUERY_GAP_MS);
        }
        if self.client.requires_new_epoch() {
            self.fault_all(Quality::TransportError, now);
        }
    }

    fn event(
        &mut self,
        event: Event,
        now: u64,
    ) -> Option<TxAction> {
        match event {
            Event::Transmit(tx) => return Some(tx),
            Event::Ignored | Event::Pending => {}
            Event::Positive(response) => {
                let Request::Obd(pid) = response.request else {
                    return None;
                };
                let valid = match obd::decode(pid, response.bytes.as_slice()) {
                    Ok(Decoded::Quantity(value)) => {
                        self.model
                            .ingest_sample(
                                Sample {
                                    value,
                                    received_at_ms: response.received_at_ms,
                                    sequence: response.generation,
                                    source: Source::Obd { ecu: response.ecu, pid },
                                },
                                now,
                            )
                            .is_ok()
                    }
                    Ok(Decoded::Supported { base, bits }) => {
                        let valid = self
                            .capabilities
                            .record_page(response.ecu, response.epoch, base, bits)
                            .is_ok();
                        if valid {
                            for (pid, id) in CHANNELS {
                                if self.capabilities.supported(pid) == Support::Unsupported {
                                    self.model.mark_fault(id, Quality::Unsupported, now);
                                }
                            }
                        }
                        valid
                    }
                    Err(_) => false,
                };
                self.finish(
                    response.request,
                    now,
                    (!valid).then_some(Quality::InvalidPayload),
                    if valid { Outcome::Positive } else { Outcome::Malformed },
                );
            }
            Event::Negative { request, .. } => {
                self.finish(request, now, Some(Quality::Unknown), Outcome::Negative);
            }
            Event::Failed { request, reason } => {
                let timeout = matches!(
                    reason,
                    Failure::TxTimeout
                        | Failure::ResponseTimeout
                        | Failure::TotalTimeout
                        | Failure::Transport(super::isotp::IsoError::Timeout)
                );
                let quality = if timeout {
                    Quality::TransportError
                } else {
                    Quality::InvalidPayload
                };
                self.finish(
                    request,
                    now,
                    Some(quality),
                    if timeout { Outcome::Timeout } else { Outcome::Malformed },
                );
            }
        }
        None
    }
}
