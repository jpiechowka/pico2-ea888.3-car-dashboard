use dashboard_configurator::diagnostic_relay::*;
use dashboard_configurator::events::*;
use dashboard_configurator::workspace::Workspace;
fn event() -> Event {
    let mut store = EventStore::default();
    let result = store
        .record(EventInput {
            category: Category::Storage,
            severity: Severity::Warn,
            code: "storage.failed",
            subject: Some("private subject"),
            message: "secret raw user text",
            elapsed_ms: 7,
            wall_clock_ms: None,
        })
        .unwrap();
    store.get(result.seq).unwrap().clone()
}
#[test]
fn opt_in_context_omits_text_and_never_retries_uncertain_delivery() {
    let mut q = Queue::default();
    let event = event();
    q.offer(&event, 9, Workspace::Diagnostics, 0);
    assert!(q.begin().is_none());
    q.synchronize(Some(1), true);
    q.offer(&event, 9, Workspace::Diagnostics, 0);
    let (lease, bytes) = q.begin().unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("secret"));
    assert!(!text.contains("private"));
    assert!(text.contains("storage.failed"));
    assert!(q.begin().is_none());
    q.synchronize(Some(2), true);
    q.offer(&event, 10, Workspace::Views, 1);
    assert!(q.begin().is_none());
    assert!(!q.settle(lease, None));
    assert_eq!(q.dropped, 1);
    assert_eq!(q.queued(), 1);
    assert!(q.begin().is_some());
}
#[test]
fn flood_is_bounded_and_disconnect_clears_queued_context() {
    let mut q = Queue::default();
    q.synchronize(Some(1), true);
    let event = event();
    for n in 0..100 {
        q.offer(&event, 1, Workspace::Views, n);
    }
    assert_eq!(q.queued(), 10);
    assert_eq!(q.dropped, 90);
    for n in 0..100 {
        q.offer(&event, 1, Workspace::Views, 1000 + n);
    }
    assert_eq!(q.queued(), QUEUE_EVENTS);
    q.synchronize(None, true);
    assert_eq!(q.queued(), 0);
    assert!(q.begin().is_none());
}

#[test]
fn toggle_retains_lease_and_old_ack_cannot_update_new_session_status() {
    let mut q = Queue::default();
    q.synchronize(Some(1), true);
    q.offer(&event(), 1, Workspace::Views, 0);
    let (lease, _) = q.begin().unwrap();
    q.synchronize(Some(1), false);
    q.synchronize(Some(1), true);
    q.offer(&event(), 2, Workspace::Views, 1);
    assert!(q.begin().is_none());
    let ack = dashboard_helper_protocol::diagnostics::Response {
        version: 1,
        admitted: 1,
        dropped: 0,
    };
    assert!(!q.settle(lease, Some(ack)));
    assert_eq!(q.admitted, 1);
    assert!(q.begin().is_some());
}
