//! Bounded browser-side job history. Capabilities stay outside diagnostic state.
use std::collections::VecDeque;

use dashboard_helper_protocol::{Event, EventBatch, JobSummary, SessionId, Text, Wire};
use serde::Deserialize;

pub const MAX_RESPONSE_BYTES: usize = 262_144;
pub const MAX_VISIBLE_EVENTS: usize = 512;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectReply {
    protocol_version: u16,
    session_id: SessionId,
    capability: Text<64>,
}

/// Deliberately has no Debug or serialization implementation.
#[derive(Clone)]
pub struct Connection {
    session: SessionId,
    capability: String,
}
impl Connection {
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 1024 {
            return Err("Invalid helper connection response");
        }
        let reply: ConnectReply = serde_json::from_slice(bytes).map_err(|_| "Invalid helper connection response")?;
        let capability = reply.capability.as_str();
        if reply.protocol_version != 1
            || capability.len() != 64
            || !capability
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Incompatible helper connection response");
        }
        Ok(Self {
            session: reply.session_id,
            capability: capability.into(),
        })
    }

    pub fn session(&self) -> &SessionId { &self.session }

    pub fn capability(&self) -> &str { &self.capability }
}

#[derive(Clone, Default)]
pub struct JobFeed {
    pub summary: Option<JobSummary>,
    pub events: VecDeque<Event>,
    pub omitted: u64,
    after: u64,
    next: u64,
}
impl JobFeed {
    pub fn start(
        &mut self,
        connection: &Connection,
        summary: JobSummary,
    ) -> Result<(), &'static str> {
        summary.validate().map_err(|_| "Invalid helper job")?;
        if summary.session_id != *connection.session() {
            return Err("Helper session changed; reconnect");
        }
        *self = Self {
            summary: Some(summary),
            ..Self::default()
        };
        Ok(())
    }

    pub fn after(&self) -> u64 { self.after }

    pub fn accept(
        &mut self,
        batch: EventBatch,
    ) -> Result<(), &'static str> {
        batch.validate().map_err(|_| "Invalid helper event batch")?;
        let current = self.summary.as_ref().ok_or("No active helper job")?;
        if batch.session_id != current.session_id || batch.job_id != current.job_id {
            return Err("Helper job identity changed");
        }
        if batch.next < self.next || batch.records.iter().any(|event| event.sequence <= self.after) {
            return Err("Out-of-order helper events");
        }
        if current.events_closed && (!batch.summary.events_closed || current.final_next != batch.summary.final_next) {
            return Err("Closed helper job changed");
        }
        let mut expected = self.after.max(batch.oldest - 1).saturating_add(1);
        for event in batch.records.iter() {
            if event.sequence != expected {
                return Err("Missing helper events without a retention gap");
            }
            expected = expected.saturating_add(1);
        }
        if batch.records.is_empty() && expected < batch.next {
            return Err("Empty helper batch before pending events");
        }
        if batch.oldest > self.after.saturating_add(1) {
            self.omitted = self.omitted.saturating_add(batch.oldest - self.after - 1);
            self.after = batch.oldest - 1;
        }
        for event in batch.records.iter() {
            self.after = event.sequence;
            if self.events.len() == MAX_VISIBLE_EVENTS {
                self.events.pop_front();
                self.omitted = self.omitted.saturating_add(1);
            }
            self.events.push_back(event.clone());
        }
        self.next = batch.next;
        self.summary = Some(batch.summary);
        Ok(())
    }

    pub fn drained(&self) -> bool {
        self.summary
            .as_ref()
            .is_some_and(|s| s.events_closed && self.after.saturating_add(1) >= self.next && self.next != 0)
    }

    pub fn active(&self) -> bool { self.summary.as_ref().is_some_and(|s| !s.events_closed) }
}
