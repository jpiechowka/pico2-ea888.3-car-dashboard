use std::collections::BTreeSet;

use dashboard_core::ble::link::{LinkFault, LinkState, log_transition};
use dashboard_core::ble::{elm, session};
use dashboard_core::logging::{LOG_ENTRIES, Level, LogStore};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
const STATES: [LinkState; 11] = [
    LinkState::Disabled,
    LinkState::Starting,
    LinkState::Connecting,
    LinkState::Securing,
    LinkState::Discovering,
    LinkState::Identifying,
    LinkState::AdapterReady,
    LinkState::SyntheticReady,
    LinkState::Backoff,
    LinkState::Maintenance,
    LinkState::Fault,
];
fn faults() -> Vec<Option<LinkFault>> {
    use session::Error as P;
    let mut faults = vec![
        None,
        Some(LinkFault::Configuration),
        Some(LinkFault::Security),
        Some(LinkFault::Discovery),
        Some(LinkFault::Transport),
        Some(LinkFault::NotificationLoss),
        Some(LinkFault::Timeout),
        Some(LinkFault::Clock),
        Some(LinkFault::Exhausted),
    ];
    faults.extend(
        [
            P::Disconnected,
            P::Epoch,
            P::Busy,
            P::Unqualified,
            P::Initialization,
            P::Token,
            P::NotificationLoss,
            P::Length,
            P::Overflow,
            P::Clock,
            P::Timeout,
            P::Exhausted,
            P::Unexpected,
            P::Reply,
        ]
        .map(|e| Some(LinkFault::Protocol(e))),
    );
    faults.extend(
        [
            elm::Error::Overflow,
            elm::Error::Ascii,
            elm::Error::Frame,
            elm::Error::Sequence,
            elm::Error::Foreign,
            elm::Error::Incomplete,
        ]
        .map(|e| Some(LinkFault::Protocol(P::Parse(e)))),
    );
    faults
}
#[test]
fn every_ble_transition_keeps_complete_epoch_state_and_reason_in_the_real_log_bound() {
    let store = LogStore::<CriticalSectionRawMutex>::new();
    let mut state_codes = BTreeSet::new();
    for state in STATES {
        let mut fault_codes = BTreeSet::new();
        for fault in faults() {
            for epoch in [u64::MAX, 0] {
                log_transition(&store, 123, state, fault, epoch);
                let window = store.window(None).unwrap();
                let entry = &window.entries.last().unwrap().entry;
                assert!(!entry.message.truncated(), "{state:?}/{fault:?}/{epoch}");
                assert_eq!(entry.timestamp_ms, 123);
                let fields: Vec<_> = entry.message.as_str().split(' ').collect();
                assert_eq!(fields.len(), 4, "one bounded event, epoch, state and reason");
                assert_eq!(fields[0], "BLE");
                assert_eq!(fields[1], epoch.to_string(), "correlation identity must be complete");
                assert!(!fields[2].is_empty());
                assert!(!fields[3].is_empty());
                if epoch == 0 {
                    assert!(
                        fault_codes.insert(fields[3].to_owned()),
                        "distinct failure reasons must survive"
                    );
                    if fault.is_none() {
                        assert!(state_codes.insert(fields[2].to_owned()), "distinct states must survive");
                    }
                }
            }
        }
    }
    assert_eq!(store.counters().truncated, 0);
}
#[test]
fn ble_failures_remain_in_the_fault_slot_after_normal_reconnect_and_info_eviction() {
    for fault in faults().into_iter().flatten() {
        let store = LogStore::<CriticalSectionRawMutex>::new();
        log_transition(&store, 77, LinkState::Backoff, Some(fault), u64::MAX);
        let retained = store.fault().expect("BLE failure must populate the protected slot");
        assert_eq!(retained.level, Level::Fault);
        assert_eq!(retained.timestamp_ms, 77);
        for n in 0..LOG_ENTRIES + 1 {
            log_transition(&store, n as u64 + 78, LinkState::Connecting, None, u64::MAX);
        }
        assert_eq!(store.fault(), Some(retained));
        assert_eq!(store.counters().fault_replacements, 0);
        assert_eq!(
            store.window(None).unwrap().entries.last().unwrap().entry.level,
            Level::Info
        );
    }
}
#[test]
fn ordinary_states_are_info_but_terminal_fault_without_detail_is_retained() {
    let store = LogStore::<CriticalSectionRawMutex>::new();
    for state in STATES.into_iter().filter(|state| *state != LinkState::Fault) {
        log_transition(&store, 1, state, None, 0);
        assert!(store.fault().is_none(), "ordinary {state:?} is not a failure");
    }
    log_transition(&store, 2, LinkState::Fault, None, 0);
    assert_eq!(store.fault().unwrap().level, Level::Fault);
}
