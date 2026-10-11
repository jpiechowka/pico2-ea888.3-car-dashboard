//! One owner for capability discovery, fair bounded polling and sample history.
//! The caller supplies known-DLC frames from a qualified transport epoch.
use super::CanFrame;
use super::client::{Client, ClientError, Event, Failure, TxAction, TxToken};
use super::obd::{self, Capabilities, Decoded, Support};
use super::request::Request;
use crate::acquisition::{AcquisitionModel, DashboardSnapshot};
use crate::samples::{Quality, Sample, SensorId, Source};

pub const QUERY_GAP_MS: u64 = 20;
pub const FAILURE_QUIET_MS: u64 = 1_000;
const PIDS: [u8; 7] = [0, 0x20, 0x40, 5, 0x0F, 0x42, 0x5C];
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
    pub completed: u32,
    pub negative: u32,
    pub timeouts: u32,
    pub malformed: u32,
    pub transport_faults: u32,
}
#[derive(Clone, Copy, Default)]
struct Slot {
    due: u64,
    failures: u8,
    discovered: bool,
}

pub struct Session {
    client: Client,
    capabilities: Capabilities,
    model: AcquisitionModel,
    slots: [Slot; 7],
    cursor: usize,
    next_query: u64,
    last_ms: u64,
    counters: Counters,
}
impl Session {
    pub fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, SessionError> {
        let capabilities = Capabilities::new(ecu, epoch).map_err(|_| SessionError::Client(ClientError::Address))?;
        let client = Client::new(ecu - 8, ecu, epoch).map_err(SessionError::Client)?;
        Ok(Self {
            client,
            capabilities,
            model: AcquisitionModel::new(),
            slots: [Slot::default(); 7],
            cursor: 0,
            next_query: 0,
            last_ms: 0,
            counters: Counters::default(),
        })
    }

    pub fn snapshot(&self) -> DashboardSnapshot { self.model.snapshot() }

    pub const fn counters(&self) -> Counters { self.counters }

    pub const fn requires_new_epoch(&self) -> bool { self.client.requires_new_epoch() }

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
            !self.slots[index].discovered && (index == 0 || self.capabilities.supported(pid) == Support::Supported)
        } else {
            self.capabilities.supported(pid) == Support::Supported
        }
    }

    /// Advance active transaction deadlines even when receive backlog prevents
    /// admitting a new request. This method never schedules a request.
    pub fn tick(
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
        self.tick(now)?;
        if self.client.active() || now < self.next_query {
            return Ok(None);
        }
        for offset in 0..PIDS.len() {
            let index = (self.cursor + offset) % PIDS.len();
            if self.eligible(index) && self.slots[index].due <= now {
                let result = self.client.start(Request::Obd(PIDS[index]), now);
                let tx = self.client_result(result, now)?;
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
        Ok(self.event(event, now))
    }

    fn finish(
        &mut self,
        request: Request,
        now: u64,
        failure: Option<Quality>,
    ) {
        let Request::Obd(pid) = request else { return };
        let Some(index) = PIDS.iter().position(|&known| known == pid) else {
            return;
        };
        let slot = &mut self.slots[index];
        if let Some(quality) = failure {
            slot.failures = slot.failures.saturating_add(1);
            let backoff = 250u64 << u32::from(slot.failures.saturating_sub(1).min(5));
            slot.due = now.saturating_add(backoff);
            self.next_query = now.saturating_add(FAILURE_QUIET_MS);
            if let Some((_, id)) = CHANNELS.iter().find(|(known, _)| *known == pid) {
                self.model.mark_fault(*id, quality, now);
            }
        } else {
            slot.failures = 0;
            slot.discovered = index < 3;
            slot.due = now.saturating_add(PERIODS_MS[index]);
            self.next_query = now.saturating_add(QUERY_GAP_MS);
            self.counters.completed = self.counters.completed.saturating_add(1);
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
                if !valid {
                    self.counters.malformed = self.counters.malformed.saturating_add(1);
                }
                self.finish(response.request, now, (!valid).then_some(Quality::InvalidPayload));
            }
            Event::Negative { request, .. } => {
                self.counters.negative = self.counters.negative.saturating_add(1);
                self.finish(request, now, Some(Quality::Unknown));
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
                    self.counters.timeouts = self.counters.timeouts.saturating_add(1);
                    Quality::TransportError
                } else {
                    self.counters.malformed = self.counters.malformed.saturating_add(1);
                    Quality::InvalidPayload
                };
                self.finish(request, now, Some(quality));
            }
        }
        None
    }
}
