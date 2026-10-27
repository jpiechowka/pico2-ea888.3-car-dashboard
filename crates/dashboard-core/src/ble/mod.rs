//! Bounded BLE adapter protocol. Real ECU reads remain unqualified.
/// INFO-only CANyonero framing and single-flight diagnostic trial lifecycle.
pub mod canyonero;
/// Bounded ELM ASCII reply parsing and profile-qualified command handling.
pub mod elm;
/// Transport lifecycle/fault snapshots and bounded transition logging.
pub mod link;
/// Serialized completion-based polling cadence and transaction tickets.
pub mod poll;
/// Shared discovered-write, radio-clock and liveness contracts.
pub mod runtime;
/// Epoch-bound single-flight adapter/synthetic protocol sessions.
pub mod session;
/// HCI terminal reason classification without inventing missing success evidence.
pub mod termination;
