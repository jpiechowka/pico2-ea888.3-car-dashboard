//! Shared bounded transport status. Ready never asserts vehicle qualification.
use super::session;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkState {
    #[default]
    Disabled,
    Starting,
    Connecting,
    Securing,
    Discovering,
    Identifying,
    AdapterReady,
    SyntheticReady,
    Backoff,
    Maintenance,
    Fault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkFault {
    Configuration,
    Security,
    Discovery,
    Transport,
    NotificationLoss,
    Timeout,
    Clock,
    Exhausted,
    Protocol(session::Error),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkSnapshot {
    pub state: LinkState,
    pub epoch: u64,
    pub completed_transactions: u64,
    pub lost_notifications: u64,
    pub last_latency_ms: Option<u32>,
    pub fault: Option<LinkFault>,
}
