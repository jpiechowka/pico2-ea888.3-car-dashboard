//! Pure bounded optional relay owner. Never retries an uncertain dispatched batch.
use std::collections::VecDeque;

use dashboard_helper_protocol::{BoundedVec, Text, diagnostics as wire, encode};

use crate::{events, workspace};
/// Maximum unsent warning/error events retained separately from the local diagnostic store.
pub const QUEUE_EVENTS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Sole outstanding relay batch identity; retained until its dispatched future settles.
pub struct Lease {
    /// Checked, nonwrapping dispatch issuance number.
    pub id: u64,
    /// Helper connection epoch at dispatch.
    pub epoch: u64,
    /// Number of removed queued events whose outcome must be accounted for.
    pub count: u8,
    /// Enable/connection generation distinguishing reconnects with reused epochs.
    generation: u64,
}
#[derive(Default)]
/// Bounded optional relay with one outstanding dispatch and no uncertain-delivery retries.
pub struct Queue {
    /// Current helper connection epoch, or `None` while disconnected.
    epoch: Option<u64>,
    /// Effective opt-in, disabled permanently when generation saturates.
    enabled: bool,
    /// Unsent allowlisted metadata only; never private event message/subject text.
    events: VecDeque<wire::Event>,
    /// Shared monotonic admission rate gate, retained across enable/connection changes.
    gate: wire::RateGate,
    /// Current dispatched batch, including after disconnect until settlement.
    inflight: Option<Lease>,
    /// Last issued dispatch ID; exhaustion prevents future batches.
    next: u64,
    /// Saturating connection/enable generation; maximum disables relay admission.
    generation: u64,
    /// Saturating count of queued discards and reported/uncertain dispatched losses.
    pub dropped: u64,
    /// Saturating count explicitly admitted by validated helper responses.
    pub admitted: u64,
}
impl Queue {
    /// Synchronizes helper `epoch` and requested `enabled` opt-in; returns whether either requested state changed.
    /// Changes discard queued events and advance generation, retaining any outstanding lease until settlement.
    /// Generation exhaustion disables admission even when enable was requested.
    pub fn synchronize(
        &mut self,
        epoch: Option<u64>,
        enabled: bool,
    ) -> bool {
        let changed = self.epoch != epoch || self.enabled != enabled;
        if changed {
            self.generation = self.generation.saturating_add(1);
            self.dropped = self.dropped.saturating_add(self.events.len() as u64);
            self.events.clear();
        }
        self.epoch = epoch;
        self.enabled = enabled && self.generation < u64::MAX;
        // Retain an unsettled lease even across disconnect: never accumulate abandoned futures.
        changed
    }

    /// Returns unsent event count, excluding any dispatched batch.
    pub fn queued(&self) -> usize { self.events.len() }

    /// Returns whether a dispatched batch still owns settlement, including across disconnects.
    pub fn busy(&self) -> bool { self.inflight.is_some() }

    /// Offers local `event` with accepted `revision`, active `workspace` and monotonic `now` milliseconds.
    /// Only enabled/connected Warn/Error allowlisted codes are queued; size/rate/code failures count as drops.
    /// Other severities and disabled/disconnected offers are ignored. Message and subject are never transmitted.
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

    /// Returns a new lease and encoded bounded batch, removing its events from the queue exactly once.
    /// Returns `None` while disabled/disconnected/busy/empty or if issuance/encoding fails; no automatic retry occurs
    /// after dispatch. Caller must eventually settle every returned lease, including cancellation/failure.
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
            generation: self.generation,
        };
        self.inflight = Some(lease);
        Some((lease, bytes))
    }

    /// Accounts for current `lease` using optional helper `result`; absent/malformed totals count the entire batch
    /// dropped. Returns true only if the settled lease still belongs to the enabled current connection/generation.
    /// Stale leases return false without touching another dispatch; current old-generation leases still update totals.
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
        self.epoch == Some(lease.epoch) && self.enabled && self.generation == lease.generation
    }
}
