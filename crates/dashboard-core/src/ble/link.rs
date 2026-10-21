//! Shared bounded transport status. Ready never asserts vehicle qualification.
use super::session;

/// Current BLE owner phase; ready states qualify only their respective adapter/test protocol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkState {
    /// BLE configuration is disabled; default before startup.
    #[default]
    Disabled,
    /// Radio/host initialization is in progress.
    Starting,
    /// Connecting to the configured exact peer identity.
    Connecting,
    /// Establishing required link security.
    Securing,
    /// Discovering the selected profile's GATT endpoints.
    Discovering,
    /// Adapter initialization/identity transaction is in progress.
    Identifying,
    /// Adapter identified; live ECU requests remain unqualified.
    AdapterReady,
    /// Separate synthetic test-peer protocol is ready; no vehicle data is published.
    SyntheticReady,
    /// Waiting before a policy-authorized reconnect attempt.
    Backoff,
    /// Radio work is parked for coordinated flash maintenance.
    Maintenance,
    /// Owner cannot proceed without recovery/restart policy.
    Fault,
}

/// Bounded reason for an owner failure, separate from raw terminal-link evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkFault {
    /// Configuration failed admission.
    Configuration,
    /// Required encryption/authentication was unavailable or failed.
    Security,
    /// Required GATT endpoints/properties could not be admitted.
    Discovery,
    /// Controller/link I/O or abnormal termination failed.
    Transport,
    /// Receive path lost at least one required notification.
    NotificationLoss,
    /// Owner operation deadline expired.
    Timeout,
    /// Monotonic timing regressed.
    Clock,
    /// Nonreused identity or bounded monotonic counter exhausted.
    Exhausted,
    /// Session parser/transaction contract failed.
    Protocol(
        /// Exact bounded session error.
        session::Error,
    ),
}

/// Copyable diagnostic snapshot; not a qualification result for ECU measurement channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkSnapshot {
    /// Current owner lifecycle phase.
    pub state: LinkState,
    /// Current session epoch; zero before a connected session is issued.
    pub epoch: u64,
    /// Number of owner-reported completed protocol transactions.
    pub completed_transactions: u64,
    /// Cumulative notification-loss count reported by the receive boundary.
    pub lost_notifications: u64,
    /// Latest completed transaction latency in milliseconds, or unavailable.
    pub last_latency_ms: Option<u32>,
    /// Current bounded owner fault, absent when no failure is reported.
    pub fault: Option<LinkFault>,
}

/// Decide the outcome after controller-confirmed disconnection. Intentional
/// maintenance cancellation may surface as Transport from admission; suppress it
/// only with normal termination and no ingress fault. Specific errors survive.
/// Returns the final fault from `operation`, maintenance `parked`, optional HCI `reason`
/// and `local_error` ingress flag. Only parked, normal, ingress-clean teardown suppresses
/// a generic Transport fault; otherwise an absent operation fault becomes Transport.
pub fn closed_fault(
    parked: bool,
    operation: Option<LinkFault>,
    reason: Option<u8>,
    local_error: bool,
) -> Option<LinkFault> {
    if parked && super::termination::normal_reason(reason) && !local_error {
        operation.filter(|fault| *fault != LinkFault::Transport)
    } else {
        operation.or(Some(LinkFault::Transport))
    }
}

/// One bounded terminal record per connection. RX1 means retained local ingress
/// failure; ?? means the controller reason is unavailable, never success/zero.
/// Appends one bounded record to `store` at boot-relative `now_ms` milliseconds for
/// `epoch`, optional HCI `reason` and `local_error`. Returns unit; level is Info only
/// for a normal reason with no local error, otherwise Fault. Caller supplies locking context.
pub fn log_disconnect<M: embassy_sync::blocking_mutex::raw::RawMutex>(
    store: &crate::logging::LogStore<M>,
    now_ms: u64,
    epoch: u64,
    reason: Option<u8>,
    local_error: bool,
) {
    use crate::logging::Level;
    let level = if super::termination::normal_reason(reason) && !local_error {
        Level::Info
    } else {
        Level::Fault
    };
    let ingress = u8::from(local_error);
    match reason {
        Some(code) => store.push(level, now_ms, format_args!("BLE {epoch} END {code:02X} RX{ingress}")),
        None => store.push(level, now_ms, format_args!("BLE {epoch} END ?? RX{ingress}")),
    }
}

/// Record a changed BLE state outside the status lock. Codes retain every
/// state/reason in at most 39 ASCII bytes, including a full u64 epoch.
/// Appends `state`, optional `fault` and `epoch` to `store` at boot-relative `now_ms`
/// milliseconds, returning unit. Fault state or any fault uses Fault severity; other
/// states use Info. Call only on transitions to avoid per-frame logging.
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

/// Returns a bounded ASCII code preserving the exact session/parser category of `error`.
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
