//! Pure bounded optional relay owner. Never retries an uncertain dispatched batch.
use std::collections::VecDeque;

use dashboard_helper_protocol::{BoundedVec, Text, diagnostics as wire, encode};

use crate::{events, workspace};
pub const QUEUE_EVENTS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    pub id: u64,
    pub epoch: u64,
    pub count: u8,
}
#[derive(Default)]
pub struct Queue {
    epoch: Option<u64>,
    enabled: bool,
    events: VecDeque<wire::Event>,
    gate: wire::RateGate,
    inflight: Option<Lease>,
    next: u64,
    pub dropped: u64,
    pub admitted: u64,
}
impl Queue {
    pub fn synchronize(
        &mut self,
        epoch: Option<u64>,
        enabled: bool,
    ) -> bool {
        let changed = self.epoch != epoch || self.enabled != enabled;
        if changed {
            self.dropped = self.dropped.saturating_add(self.events.len() as u64);
            self.events.clear();
        }
        self.epoch = epoch;
        self.enabled = enabled;
        // Retain an unsettled lease even across disconnect: never accumulate abandoned futures.
        changed
    }

    pub fn queued(&self) -> usize { self.events.len() }

    pub fn busy(&self) -> bool { self.inflight.is_some() }

    pub fn offer(
        &mut self,
        event: &events::Event,
        revision: u64,
        workspace: workspace::Workspace,
        now: u64,
    ) {
        if !self.enabled || self.epoch.is_none() {
            return;
        }
        let severity = match event.severity {
            events::Severity::Warn => wire::Severity::Warn,
            events::Severity::Error => wire::Severity::Error,
            _ => return,
        };
        let Some(code) = wire::Code::parse(event.code.get()) else {
            self.dropped = self.dropped.saturating_add(1);
            return;
        };
        if self.events.len() >= QUEUE_EVENTS || !self.gate.admit(now) {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let category = match event.category {
            events::Category::Editor => wire::Category::Editor,
            events::Category::Validation => wire::Category::Validation,
            events::Category::Storage => wire::Category::Storage,
            events::Category::ImportExport => wire::Category::ImportExport,
            events::Category::Simulation => wire::Category::Simulation,
            events::Category::Rendering => wire::Category::Rendering,
            events::Category::HostTools => wire::Category::HostTools,
            events::Category::Device => wire::Category::Device,
        };
        let workspace = match workspace {
            workspace::Workspace::Views => wire::Workspace::Views,
            workspace::Workspace::Device => wire::Workspace::Device,
            workspace::Workspace::DataWarnings => wire::Workspace::DataWarnings,
            workspace::Workspace::Diagnostics => wire::Workspace::Diagnostics,
            workspace::Workspace::BuildFlash => wire::Workspace::BuildFlash,
            workspace::Workspace::EcuReference => wire::Workspace::EcuReference,
        };
        self.events.push_back(wire::Event {
            severity,
            category,
            code,
            workspace,
            sequence: Text::new(event.seq.get().to_string()).expect("u64 fits"),
            client_elapsed_ms: Text::new(event.elapsed_ms.to_string()).expect("u64 fits"),
            accepted_revision: Text::new(revision.to_string()).expect("u64 fits"),
        });
    }

    pub fn begin(&mut self) -> Option<(Lease, Vec<u8>)> {
        if !self.enabled || self.inflight.is_some() || self.events.is_empty() {
            return None;
        }
        let epoch = self.epoch?;
        let next = self.next.checked_add(1)?;
        let count = self.events.len().min(wire::BATCH_EVENTS);
        let request = wire::Request {
            version: 1,
            events: BoundedVec::new(self.events.iter().take(count).cloned().collect()).ok()?,
        };
        let bytes = encode(&request).ok()?;
        self.events.drain(..count);
        self.next = next;
        let lease = Lease {
            id: next,
            epoch,
            count: count as u8,
        };
        self.inflight = Some(lease);
        Some((lease, bytes))
    }

    pub fn settle(
        &mut self,
        lease: Lease,
        result: Option<wire::Response>,
    ) -> bool {
        if self.inflight != Some(lease) {
            return false;
        }
        self.inflight = None;
        match result
            .filter(|r| r.version == 1 && u16::from(r.admitted) + u16::from(r.dropped) == u16::from(lease.count))
        {
            Some(r) => {
                self.admitted = self.admitted.saturating_add(u64::from(r.admitted));
                self.dropped = self.dropped.saturating_add(u64::from(r.dropped));
            }
            None => self.dropped = self.dropped.saturating_add(u64::from(lease.count)),
        }
        self.epoch == Some(lease.epoch) && self.enabled
    }
}
