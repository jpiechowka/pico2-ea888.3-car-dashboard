use dashboard_core::ble::link::{LinkFault, closed_fault, log_disconnect};
use dashboard_core::ble::termination::normal_reason;
use dashboard_core::logging::{Level, LogStore};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

#[test]
fn only_normal_terminal_reasons_can_admit_intentional_maintenance() {
    for reason in (0..=u8::MAX).map(Some).chain([None]) {
        let normal = matches!(reason, Some(0x13 | 0x16));
        assert_eq!(normal_reason(reason), normal);
        for local_error in [false, true] {
            let expected = if normal && !local_error {
                None
            } else {
                Some(LinkFault::Transport)
            };
            assert_eq!(closed_fault(true, None, reason, local_error), expected);
            // Admission cancellation reports Transport, but is suppressible only
            // for known-normal teardown without a retained local receive fault.
            assert_eq!(
                closed_fault(true, Some(LinkFault::Transport), reason, local_error),
                expected
            );
            assert_eq!(
                closed_fault(false, None, reason, local_error),
                Some(LinkFault::Transport)
            );
        }
    }
}

#[test]
fn maintenance_and_normal_teardown_never_erase_a_specific_operation_failure() {
    for fault in [
        LinkFault::Security,
        LinkFault::Discovery,
        LinkFault::Timeout,
        LinkFault::Configuration,
        LinkFault::NotificationLoss,
        LinkFault::Clock,
        LinkFault::Exhausted,
        LinkFault::Protocol(dashboard_core::ble::session::Error::Reply),
    ] {
        for parked in [false, true] {
            for reason in [None, Some(0x13), Some(0x16), Some(0x22)] {
                for ingress in [false, true] {
                    assert_eq!(closed_fault(parked, Some(fault), reason, ingress), Some(fault));
                }
            }
        }
    }
}

#[test]
fn termination_log_retains_raw_reason_unknown_and_epoch_without_truncation() {
    let store = LogStore::<CriticalSectionRawMutex>::new();
    for reason in (0..=u8::MAX).map(Some).chain([None]) {
        for ingress in [false, true] {
            log_disconnect(&store, 123, u64::MAX, reason, ingress);
            let entries = store.window(None).unwrap();
            let entry = &entries.entries.last().unwrap().entry;
            let code = reason.map_or_else(|| "??".to_owned(), |v| format!("{v:02X}"));
            assert_eq!(
                entry.message.as_str(),
                format!("BLE {} END {code} RX{}", u64::MAX, u8::from(ingress))
            );
            assert!(!entry.message.truncated());
            assert_eq!(
                entry.level,
                if normal_reason(reason) && !ingress {
                    Level::Info
                } else {
                    Level::Fault
                }
            );
            assert_eq!(entry.timestamp_ms, 123);
        }
    }
    assert_eq!(store.counters().truncated, 0);
}
