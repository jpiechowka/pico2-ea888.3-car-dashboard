use dashboard_configurator::helper_client::*;
use dashboard_helper_protocol::*;

fn connection() -> Connection {
    Connection::decode(
        format!(
            r#"{{"protocol_version":1,"session_id":"{}","capability":"{}"}}"#,
            "a".repeat(32),
            "b".repeat(64)
        )
        .as_bytes(),
    )
    .unwrap()
}
fn summary() -> JobSummary {
    JobSummary {
        session_id: connection().session().clone(),
        job_id: JobId::new("c".repeat(32)).unwrap(),
        operation: OperationKind::PrerequisiteCheck,
        stages: BoundedVec::new(vec![StageSummary {
            stage: StageId::Probe(ProbeStage::Prerequisites),
            state: StageState::Running,
            reason: None,
            started_ms: Some(0),
            ended_ms: None,
            tools: BoundedVec::new(vec![]).unwrap(),
        }])
        .unwrap(),
        cancel_requested: false,
        raw_exit: None,
        write_outcome: WriteOutcome::NotStarted,
        artifact_id: None,
        device_id: None,
        retry_of: None,
        terminal_cause: None,
        final_next: None,
        events_closed: false,
        running_firmware_observed: false,
        output_incomplete: false,
        output_dropped_chunks: 0,
        output_dropped_bytes: 0,
        anomalies: Anomalies::default(),
        active_setup_step: None,
    }
}
fn batch(
    from: u64,
    count: u64,
    closed: bool,
) -> EventBatch {
    let mut summary = summary();
    let next = from + count;
    if closed {
        summary.stages = BoundedVec::new(vec![StageSummary {
            stage: StageId::Probe(ProbeStage::Prerequisites),
            state: StageState::Succeeded,
            reason: None,
            started_ms: Some(0),
            ended_ms: Some(next),
            tools: BoundedVec::new(vec![]).unwrap(),
        }])
        .unwrap();
        summary.terminal_cause = Some(TerminalCause::Succeeded);
        summary.events_closed = true;
        summary.final_next = Some(next);
    }
    EventBatch {
        session_id: summary.session_id.clone(),
        job_id: summary.job_id.clone(),
        oldest: from,
        next,
        dropped: from - 1,
        records: BoundedVec::new(
            (from..next)
                .map(|n| {
                    Event {
                        sequence: n,
                        utc_ms: None,
                        elapsed_ms: n,
                        stage: StageId::Probe(ProbeStage::Prerequisites),
                        kind: EventKind::Log,
                        level: Level::Info,
                        source: Source::Internal,
                        text: Text::new("check").unwrap(),
                        progress: None,
                        tool: None,
                        setup_step: None,
                    }
                })
                .collect(),
        )
        .unwrap(),
        summary,
    }
}
#[test]
fn connection_rejects_wrong_version_oversize_unknown_fields_and_non_hex_capability() {
    assert!(Connection::decode(b"{}").is_err());
    assert!(Connection::decode(&vec![b' '; 1025]).is_err());
    for (version, cap, extra) in [
        (2, "b".repeat(64), ""),
        (1, "X".repeat(64), ""),
        (1, "b".repeat(63), ""),
        (1, "b".repeat(64), ",\"extra\":true"),
    ] {
        let input = format!(
            r#"{{"protocol_version":{version},"session_id":"{}","capability":"{cap}"{extra}}}"#,
            "a".repeat(32)
        );
        assert!(Connection::decode(input.as_bytes()).is_err());
    }
    assert_eq!(connection().capability().len(), 64);
}
#[test]
fn job_feed_retains_bounded_events_tracks_gaps_and_rejects_replays() {
    let mut feed = JobFeed::default();
    feed.start(&connection(), summary()).unwrap();
    for n in 0..5 {
        feed.accept(batch(1 + n * 128, 128, false)).unwrap();
    }
    assert_eq!(feed.events.len(), 512);
    assert_eq!(feed.omitted, 128);
    assert_eq!(feed.after(), 640);
    assert!(feed.accept(batch(513, 128, false)).is_err());
    assert_eq!(feed.after(), 640);
    feed.accept(batch(645, 2, true)).unwrap();
    assert_eq!(feed.omitted, 134);
    assert!(feed.drained());
    assert!(!feed.active());
    let mut wrong = batch(647, 1, true);
    wrong.session_id = SessionId::new("d".repeat(32)).unwrap();
    assert!(feed.accept(wrong).is_err());
    assert_eq!(feed.after(), 646);
}
#[test]
fn closed_summary_is_not_drained_until_last_batch_arrives() {
    let mut feed = JobFeed::default();
    feed.start(&connection(), summary()).unwrap();
    let mut first = batch(1, 2, true);
    first.next = 5;
    first.summary.final_next = Some(5);
    feed.accept(first).unwrap();
    assert!(!feed.drained());
    feed.accept(batch(3, 2, true)).unwrap();
    assert!(feed.drained());
    let mut foreign = summary();
    foreign.session_id = SessionId::new("e".repeat(32)).unwrap();
    assert!(feed.start(&connection(), foreign).is_err());
    assert!(feed.drained());
}

#[test]
fn malformed_gaps_and_terminal_regressions_leave_prior_state_intact() {
    let mut feed = JobFeed::default();
    feed.start(&connection(), summary()).unwrap();
    let mut gap = batch(1, 2, false);
    gap.records = BoundedVec::new(vec![gap.records[1].clone()]).unwrap();
    assert!(feed.accept(gap).is_err());
    assert_eq!(feed.after(), 0);
    let mut empty = batch(1, 2, false);
    empty.records = BoundedVec::new(vec![]).unwrap();
    assert!(feed.accept(empty).is_err());
    assert!(feed.events.is_empty());
    feed.accept(batch(1, 2, true)).unwrap();
    assert!(feed.accept(batch(3, 1, false)).is_err());
    assert!(feed.drained());
}
