//! Strict metadata-only relay schema and rolling monotonic rate-limit regressions.
use dashboard_helper_protocol::diagnostics::*;
use dashboard_helper_protocol::{BoundedVec, Text, decode, encode};
/// Return bounded metadata with counters exceeding JavaScript's exact integer range.
fn event() -> Event {
    Event {
        severity: Severity::Warn,
        category: Category::Storage,
        code: Code::StorageFailed,
        workspace: Workspace::Diagnostics,
        sequence: Text::new(u64::MAX.to_string()).unwrap(),
        client_elapsed_ms: Text::new("9007199254740993").unwrap(),
        accepted_revision: Text::new("7").unwrap(),
    }
}
#[test]
/// Preserve large decimal IDs while rejecting payload injection, unknown codes, noncanonical numbers and bounds.
fn strict_context_preserves_large_ids_and_rejects_extra_text() {
    let request = Request {
        version: 1,
        events: BoundedVec::new(vec![event(); 4]).unwrap(),
    };
    let bytes = encode(&request).unwrap();
    assert!(bytes.len() < REQUEST_BYTES);
    let parsed = decode::<Request>(&bytes).unwrap();
    assert_eq!(parsed.events[0], event());
    let mut value = serde_json::to_value(&request).unwrap();
    value["events"][0]["message"] = "private data".into();
    assert!(decode::<Request>(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value = serde_json::to_value(&request).unwrap();
    value["events"][0]["code"] = "https://secret".into();
    assert!(decode::<Request>(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut bad = event();
    bad.sequence = Text::new("+1").unwrap();
    assert!(bad.validate().is_err());
    assert!(decode::<Request>(&vec![b' '; REQUEST_BYTES + 1]).is_err());
    assert!(BoundedVec::<Event, 4>::new(vec![event(); 5]).is_err());
}
#[test]
/// Ten admitted timestamps fill a rolling second; backward time cannot refill and exact expiry can.
fn rate_limit_is_rolling_and_backward_time_cannot_refill() {
    let mut gate = RateGate::default();
    for _ in 0..10 {
        assert!(gate.admit(500));
    }
    assert!(!gate.admit(499));
    assert!(!gate.admit(1499));
    assert!(gate.admit(1500));
}

/// Exhausted u64 time cannot create capacity through wraparound or repeated backward observations.
#[test]
fn rate_gate_retains_capacity_at_clock_exhaustion() {
    let mut gate = RateGate::default();
    for _ in 0..10 {
        assert!(gate.admit(u64::MAX - 1000));
    }
    assert!(!gate.admit(u64::MAX - 1));
    for _ in 0..10 {
        assert!(gate.admit(u64::MAX));
    }
    for now in [u64::MAX, 0, u64::MAX - 1000] {
        assert!(!gate.admit(now));
    }
}
