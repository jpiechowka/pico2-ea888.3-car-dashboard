use dashboard_helper_protocol::*;
use studio_helper::jobs::JobStore;
use studio_helper::jobs::ingress::*;
use studio_helper::jobs::logs::LogInput;
#[test]
fn process_channel_bounds_logs_without_blocking_control() {
    let (tx, rx) = process_log_channel();
    let log = LogInput {
        stage: StageId::Probe(ProbeStage::DeviceDetection),
        source: Source::Stdout,
        text: "safe".into(),
        progress: false,
    };
    for _ in 0..PROCESS_LOG_CAPACITY {
        assert!(tx.try_send(log.clone()).unwrap());
    }
    for _ in 0..1000 {
        assert!(!tx.try_send(log.clone()).unwrap());
    }
    let mut store = JobStore::new(SessionId::new("a".repeat(32)).unwrap());
    let id = JobId::new("b".repeat(32)).unwrap();
    store.start(id.clone(), OperationKind::DeviceScan, None).unwrap();
    store.cancel(&id).unwrap();
    assert!(store.summaries()[0].cancel_requested);
    let loss = rx.take_loss();
    assert_eq!(
        loss,
        LogLoss {
            chunks: 1000,
            bytes: 4000,
            counter_overflow: false
        }
    );
    store.account_log_loss(&id, loss).unwrap();
    assert_eq!(store.summaries()[0].output_dropped_bytes, 4000);
    assert_eq!(rx.take_loss(), LogLoss::default());
    for _ in 0..PROCESS_LOG_CAPACITY {
        assert_eq!(rx.try_recv().unwrap(), Some(log.clone()));
    }
    assert_eq!(rx.try_recv().unwrap(), None);
    drop(rx);
    assert_eq!(tx.try_send(log), Err(Error::Closed));
}
