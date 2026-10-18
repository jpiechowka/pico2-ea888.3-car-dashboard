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

/// Record a changed BLE state outside the status lock. Codes retain every
/// state/reason in at most 39 ASCII bytes, including a full u64 epoch.
pub fn log_transition<M: embassy_sync::blocking_mutex::raw::RawMutex>(
    store: &crate::logging::LogStore<M>,
    now_ms: u64,
    state: LinkState,
    fault: Option<LinkFault>,
    epoch: u64,
) {
    use crate::logging::Level;
    let level = if fault.is_some() || state == LinkState::Fault {
        Level::Fault
    } else {
        Level::Info
    };
    let state = match state {
        LinkState::Disabled => "OFF",
        LinkState::Starting => "START",
        LinkState::Connecting => "CONN",
        LinkState::Securing => "SEC",
        LinkState::Discovering => "DISC",
        LinkState::Identifying => "IDENT",
        LinkState::AdapterReady => "READY",
        LinkState::SyntheticReady => "SYNTH",
        LinkState::Backoff => "BACK",
        LinkState::Maintenance => "PARK",
        LinkState::Fault => "FAULT",
    };
    let reason = match fault {
        None => "OK",
        Some(LinkFault::Configuration) => "CONFIG",
        Some(LinkFault::Security) => "SECURITY",
        Some(LinkFault::Discovery) => "DISCOVER",
        Some(LinkFault::Transport) => "LINK",
        Some(LinkFault::NotificationLoss) => "RX-LOSS",
        Some(LinkFault::Timeout) => "TIMEOUT",
        Some(LinkFault::Clock) => "CLOCK",
        Some(LinkFault::Exhausted) => "EXHAUST",
        Some(LinkFault::Protocol(error)) => protocol_code(error),
    };
    store.push(level, now_ms, format_args!("BLE {epoch} {state} {reason}"));
}

fn protocol_code(error: session::Error) -> &'static str {
    use session::Error;

    use super::elm;
    match error {
        Error::Disconnected => "P-DISCON",
        Error::Epoch => "P-EPOCH",
        Error::Busy => "P-BUSY",
        Error::Unqualified => "P-UNQUAL",
        Error::Initialization => "P-INIT",
        Error::Token => "P-TOKEN",
        Error::NotificationLoss => "P-LOSS",
        Error::Length => "P-LENGTH",
        Error::Overflow => "P-OVER",
        Error::Clock => "P-CLOCK",
        Error::Timeout => "P-TIME",
        Error::Exhausted => "P-EXHAUS",
        Error::Unexpected => "P-UNEXP",
        Error::Reply => "P-REPLY",
        Error::Parse(error) => {
            match error {
                elm::Error::Overflow => "E-OVER",
                elm::Error::Ascii => "E-ASCII",
                elm::Error::Frame => "E-FRAME",
                elm::Error::Sequence => "E-SEQ",
                elm::Error::Foreign => "E-OTHER",
                elm::Error::Incomplete => "E-SHORT",
            }
        }
    }
}
