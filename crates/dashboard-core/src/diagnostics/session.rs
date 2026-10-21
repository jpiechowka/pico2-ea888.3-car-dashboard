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

/// Minimum milliseconds before the next request after successful completion.
pub const QUERY_GAP_MS: u64 = 20;
/// Whole-session quiet interval in milliseconds after any failed request.
pub const FAILURE_QUIET_MS: u64 = 1_000;
/// Fair scheduling order: three capability pages, coolant, intake, voltage and oil.
pub const PIDS: [u8; 7] = [0, 0x20, 0x40, 5, 0x0F, 0x42, 0x5C];
/// Unconfigured periods in milliseconds by PIDS index; zero means one-time discovery.
const PERIODS_MS: [u64; 7] = [0, 0, 0, 1_000, 1_000, 500, 1_500];
/// Reviewed Mode01 PID-to-canonical-channel mapping; no inferred manufacturer measurements.
const CHANNELS: [(u8, SensorId); 4] = [
    (5, SensorId::Water),
    (0x0F, SensorId::Iat),
    (0x42, SensorId::Battery),
    (0x5C, SensorId::Oil),
];

/// Transport/client admission failure requiring owner-level handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// Client error, including invalid address, stale completion or invalidated transport.
    Client(
        /// Original client classification.
        ClientError,
    ),
}
/// Saturating lifetime counts; valid response timings include positive and negative replies only.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counters {
    /// Requests admitted and assigned a UART ticket.
    pub requested: u32,
    /// Original request tickets accepted after UART completion; excludes FC.
    pub transmitted: u32,
    /// Active timing records discarded on transport fault or epoch replacement.
    pub abandoned: u32,
    /// Positive replies admitted by quantity/capability validation.
    pub completed: u32,
    /// Matching terminal ECU negative replies.
    pub negative: u32,
    /// TX, response, total or inter-frame timeouts.
    pub timeouts: u32,
    /// Malformed transport/application data, pending-limit or oversize failures.
    pub malformed: u32,
    /// Transport-fault occurrences; per-channel counts require an active request.
    pub transport_faults: u32,
    /// Positive/negative outcomes with a valid original-request UART completion timestamp.
    pub replies: u32,
    /// Latest measured UART-completion-to-reply milliseconds; None before a timed outcome.
    pub last_reply_ms: Option<u64>,
    /// Minimum measured reply milliseconds over retained lifetime evidence.
    pub min_reply_ms: Option<u64>,
    /// Maximum measured reply milliseconds over retained lifetime evidence.
    pub max_reply_ms: Option<u64>,
}
/// Terminal request classification used consistently by aggregate and per-PID metrics.
#[derive(Clone, Copy)]
enum Outcome {
    /// Positive response passed quantity/capability admission.
    Positive,
    /// Matching terminal negative response.
    Negative,
    /// A bounded transport/request deadline expired.
    Timeout,
    /// Non-timeout response or decode/admission failure.
    Malformed,
}
impl Counters {
    /// Saturating-increments `outcome`; optional `reply_ms` records valid elapsed
    /// milliseconds and updates last/min/max. None leaves timing evidence unchanged.
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
/// Correlates one scheduled PID with original UART completion, excluding FC timings.
struct ActiveTiming {
    /// Index into PIDS and parallel per-channel arrays.
    index: usize,
    /// Original request's exact completion token.
    request_tx: TxToken,
    /// Accepted original UART completion milliseconds, absent until sent.
    sent_ms: Option<u64>,
}
/// Lifetime request metrics for one scheduled PID, including capability pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelTelemetry {
    /// Mode01 PID from PIDS.
    pub pid: u8,
    /// Saturating request/outcome and exact min/max/last timing counters.
    pub counters: Counters,
    /// Bounded histogram summary of positive/negative reply milliseconds.
    pub reply_ms: Distribution,
}
/// Coherent copy of session identity and all seven PID metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Telemetry {
    /// Fixed eleven-bit OBD responder identifier.
    pub ecu: u16,
    /// Current qualified transport generation; counters may include older epochs.
    pub epoch: u64,
    /// Last admitted session time in monotonic milliseconds.
    pub at_ms: u64,
    /// PIDS-order metrics, including discovery requests.
    pub channels: [ChannelTelemetry; 7],
}
/// Scheduling state reset at every newly qualified transport epoch.
#[derive(Clone, Copy, Default)]
struct Slot {
    /// Earliest next admission time in monotonic milliseconds.
    due: u64,
    /// Saturating consecutive failures; backoff exponent is separately capped at five.
    failures: u8,
    /// Capability page has been successfully recorded in this epoch.
    discovered: bool,
}

/// One responder's bounded poll scheduler, capability evidence and canonical acquisition model.
/// Only advertised reviewed PIDs are polled; this type performs no physical I/O.
pub struct Session<'a> {
    /// Sole active request and ISO-TP transaction state.
    client: Client,
    /// Current-epoch capability bitmaps, never inferred from timeout.
    capabilities: Capabilities,
    /// Shared last-good values, quality, histories and warning state bound to configuration.
    model: AcquisitionModel<'a>,
    /// Due time/backoff/discovery state in PIDS order.
    slots: [Slot; 7],
    /// First index for the next bounded round-robin scan.
    cursor: usize,
    /// Earliest whole-session request time after completion/failure.
    next_query: u64,
    /// Last accepted protocol timestamp in monotonic milliseconds.
    last_ms: u64,
    /// Aggregate lifetime metrics, retained across epochs.
    counters: Counters,
    /// Per-PID lifetime counters in PIDS order.
    channels: [Counters; 7],
    /// Current request correlation; removed exactly once on outcome/abandonment.
    timing: Option<ActiveTiming>,
    /// Fixed-size lifetime reply histograms in milliseconds.
    reply_ms: [Histogram<160>; 7],
}

/// Validate only the small transport fields before assembling a large owner.
/// Kept inside diagnostics so callers cannot bypass Session validation.
pub(super) struct PreparedSession {
    /// Validated distinct eleven-bit request/response identifiers and nonzero epoch.
    client: Client,
    /// Empty pages bound to the same responder and epoch.
    capabilities: Capabilities,
}
impl PreparedSession {
    /// Returns validated small construction state for `ecu` in 0x7E8..=0x7EF and
    /// nonzero `epoch`, or a mapped address/epoch error. Request address is ecu-8.
    pub(super) fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, SessionError> {
        let capabilities = Capabilities::new(ecu, epoch).map_err(|_| SessionError::Client(ClientError::Address))?;
        let client = Client::new(ecu - 8, ecu, epoch).map_err(SessionError::Client)?;
        Ok(Self { client, capabilities })
    }

    /// Consumes validated inputs and returns an initialized session bound to optional
    /// immutable `config`; None selects legacy generic polling periods.
    pub(super) fn assemble(
        self,
        config: Option<&ValidatedConfig>,
    ) -> Session<'_> {
        if config.is_some() {
            self.assemble_variant::<true>(config)
        } else {
            self.assemble_variant::<false>(config)
        }
    }

    /// Returns the fully initialized owner for optional `config`; CONFIGURED must
    /// equal config presence or construction panics. Used only after branch dispatch.
    #[inline(always)]
    fn assemble_variant<const CONFIGURED: bool>(
        self,
        config: Option<&ValidatedConfig>,
    ) -> Session<'_> {
        let mut session = self.construction_base::<CONFIGURED>(config);
        session.initialize_measurements();
        session
    }

    /// Returns the bound owner storage for optional `config`, before measurement
    /// initialization. CONFIGURED must equal config presence or construction panics.
    /// The enclosing Session/Vehicle must call initialize_measurements before publication.
    #[inline(always)]
    pub(super) fn construction_base<const CONFIGURED: bool>(
        self,
        config: Option<&ValidatedConfig>,
    ) -> Session<'_> {
        Session {
            client: self.client,
            capabilities: self.capabilities,
            model: AcquisitionModel::session_base::<CONFIGURED>(config),
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
    /// Completes source/quality initialization for this bound responder; construction calls exactly once.
    pub(super) fn initialize_measurements(&mut self) { self.model.initialize_for_session(self.client.response_id()); }

    /// Returns an unconfigured session for `ecu` in 0x7E8..=0x7EF and nonzero `epoch`,
    /// or address/epoch error. Capability discovery precedes reviewed measurement requests.
    pub fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, SessionError> {
        Self::new_inner(ecu, epoch, None)
    }

    /// Returns a session bound for its lifetime to `config`, responder `ecu` and nonzero
    /// `epoch`; invalid responder/epoch returns an error. Only matching configured OBD
    /// sources are eligible; unsupported/unqualified sources remain unavailable.
    pub fn configured(
        ecu: u16,
        epoch: u64,
        config: &'a ValidatedConfig,
    ) -> Result<Self, SessionError> {
        Self::new_inner(ecu, epoch, Some(config))
    }

    /// Returns the immutable bound configuration, or None for legacy periods/source policy.
    pub fn configuration(&self) -> Option<&ValidatedConfig> { self.model.configuration() }

    /// Returns initialized state for responder `ecu`, nonzero `epoch` and optional
    /// immutable `config`; invalid addressing/epoch returns SessionError before assembly.
    fn new_inner(
        ecu: u16,
        epoch: u64,
        config: Option<&'a ValidatedConfig>,
    ) -> Result<Self, SessionError> {
        Ok(PreparedSession::new(ecu, epoch)?.assemble(config))
    }

    /// Returns a coherent copy of acquisition values, quality, statistics and warning state.
    pub fn snapshot(&self) -> DashboardSnapshot { self.model.snapshot() }

    /// Presentation advances even when transport is offline or parked.
    /// `now` is monotonic milliseconds passed to shared expiry/history/warning logic;
    /// no request is scheduled and no synthetic measurement is inserted.
    pub fn tick_measurements(
        &mut self,
        now: u64,
    ) {
        self.model.tick(now);
    }

    /// Returns aggregate lifetime counters, including previous transport epochs.
    pub const fn counters(&self) -> Counters { self.counters }

    /// Lifetime counters, including discovery pages, retained across epochs.
    /// Returns the counters for scheduled `pid`, or None for a PID outside PIDS.
    pub fn channel(
        &self,
        pid: u8,
    ) -> Option<Counters> {
        PIDS.iter()
            .position(|&known| known == pid)
            .map(|index| self.channels[index])
    }

    /// Call outside latency-sensitive receive work; percentile scans are bounded.
    /// Returns current identity/time and all seven lifetime histogram/counter summaries.
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

    /// Discards current timing once and increments abandoned counts; `fault` also
    /// increments that PID's transport-fault count. Aggregate fault counting is separate.
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

    /// Returns whether the client forbids requests until a newly qualified transport epoch.
    pub const fn requires_new_epoch(&self) -> bool { self.client.requires_new_epoch() }

    /// Applies producer-side `command` at monotonic `now` in milliseconds. Returns the
    /// shared warning engine's accepted/replayed/rejected result; does not schedule traffic.
    pub fn apply_warning_command(
        &mut self,
        command: crate::warning_state::WarningCommand,
        now: u64,
    ) -> crate::warning_state::CommandResult {
        self.model.apply_warning_command(command, now)
    }

    /// Applies strictly newer nonzero reset `generation`. Returns whether statistics
    /// were reset; raw values/quality and transport/capability identity are preserved.
    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        self.model.reset_statistics(generation)
    }

    /// Publishes `quality` for every sensor at monotonic `now` in milliseconds using
    /// shared status admission; last-good numbers remain distinct from unavailable quality.
    fn fault_all(
        &mut self,
        quality: Quality,
        now: u64,
    ) {
        for id in SensorId::ALL {
            self.model.mark_fault(id, quality, now);
        }
    }

    /// Admits monotonic `now` in milliseconds. Returns success and advances protocol time;
    /// reversal abandons active timing, invalidates transport and publishes TransportError
    /// at the last accepted time before returning Clock.
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

    /// Returns successful client `result` unchanged or maps its error. Clock/Exhausted/
    /// WireFault additionally abandon work and invalidate all quality at max(last time,
    /// `now`), in milliseconds; stale tokens and ordinary admission errors do not.
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

    /// Invalidates active work and all sample quality at monotonic `now` in milliseconds,
    /// incrementing the aggregate transport count. Returns success or a clock error;
    /// recovery requires change_epoch after an externally qualified new boundary.
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
    /// `epoch` must strictly increase and `now` is monotonic milliseconds. Returns
    /// success after clearing schedules/capabilities and marking quality Unknown;
    /// stale epoch rejects before any clock/state effect, reversal invalidates transport.
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

    /// Returns whether PIDS `index` is configured and advertised (or a needed,
    /// undiscovered capability page). Panics for an index outside the seven slots.
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

    /// Returns milliseconds for PIDS `index`: zero for discovery, legacy default
    /// without configuration, or the matching configured OBD period. None disables an
    /// incompatible source. Panics for an index outside the seven-slot contract.
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
    /// `now` is monotonic milliseconds. Returns protocol status after measurement
    /// expiry; terminal client faults invalidate quality and return an error.
    pub fn tick(
        &mut self,
        now: u64,
    ) -> Result<(), SessionError> {
        self.tick_measurements(now);
        self.tick_protocol(now)
    }

    /// Advances protocol deadlines at monotonic `now` in milliseconds without ticking
    /// measurements or scheduling a request. Returns an owner-handled error for clock/
    /// terminal wire failures; ordinary transaction failures update counters/backoff.
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

    /// Ticks measurements/protocol at monotonic `now` in milliseconds, then returns
    /// at most one due eligible request, None while busy/backing off, or an owner-handled
    /// error. A bounded seven-slot round-robin scan prevents unbounded catch-up.
    pub fn poll(
        &mut self,
        now: u64,
    ) -> Result<Option<TxAction>, SessionError> {
        self.tick_measurements(now);
        self.poll_protocol(now)
    }

    /// Returns at most one due eligible request at monotonic `now` in milliseconds
    /// after protocol checks, without measurement expiry. None means no admission;
    /// clock/terminal errors invalidate transport. Vehicle calls only with drained RX.
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

    /// Processes known-DLC `frame` from qualified `epoch` at monotonic `now` in
    /// milliseconds. Returns optional FC transmission or an owner-handled client error;
    /// valid positive outcomes update capabilities/samples, failures update quality/backoff.
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

    /// Accepts exact UART-idle `token` at monotonic `now` in milliseconds. Returns
    /// optional follow-up action or client/clock error; stale/duplicate tokens are rejected
    /// before clock effects. Only the original request's first completion starts reply timing.
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

    /// Records terminal `outcome` for scheduled OBD `request` at `now` milliseconds;
    /// optional `failure` quality selects capped exponential retry/backoff and channel fault.
    /// Success resets failures and schedules its period; unknown/non-OBD requests are ignored.
    /// Reply timing is measured only for positive/negative outcomes with a valid send timestamp.
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

    /// Applies client `event` at monotonic `now` in milliseconds. Returns only required
    /// transmit work; terminal replies decode/admit canonical samples or bitmaps before
    /// updating metrics, quality and schedules. No guessed PID/DID value is published.
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
