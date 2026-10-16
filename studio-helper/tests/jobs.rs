use dashboard_helper_protocol::*;
use studio_helper::jobs::logs::LogInput;
use studio_helper::jobs::*;
#[test]
fn saturated_ingress_drops_only_logs_and_requires_drain_before_seal() {
    let mut s = store();
    s.start(id(1), OperationKind::PrerequisiteCheck, None).unwrap();
    for _ in 0..256 {
        assert!(s.enqueue(&id(1), log("x")).unwrap());
    }
    assert!(!s.enqueue(&id(1), log("lost")).unwrap());
    s.cancel(&id(1)).unwrap();
    assert!(s.summaries()[0].cancel_requested);
    assert!(
        s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
            .is_err()
    );
    s.drain(&id(1)).unwrap();
    s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
        .unwrap();
    let p = poll(&s, 1);
    assert_eq!(
        (p.summary.output_dropped_chunks, p.summary.output_dropped_bytes),
        (1, 4)
    );
    assert_eq!(p.next, 257);
    assert_eq!(p.dropped, 0);
}
#[test]
fn timestamp_conversion_distinguishes_epoch_unavailable_and_overflow() {
    use std::time::{Duration, UNIX_EPOCH};

    use studio_helper::jobs::time::*;
    assert_eq!(utc_millis(UNIX_EPOCH), Some(0));
    assert_eq!(utc_millis(UNIX_EPOCH - Duration::from_millis(1)), None);
    assert_eq!(checked_millis(u128::from(u64::MAX) + 1), None);
    let mut anomaly = false;
    assert_eq!(monotonic_elapsed(100, Some(99), &mut anomaly), 100);
    assert!(anomaly);
}
fn id(n: u32) -> JobId { JobId::new(format!("{n:032x}")).unwrap() }
fn store() -> JobStore { JobStore::new(SessionId::new("a".repeat(32)).unwrap()) }
fn closure() -> OutputClosure {
    OutputClosure {
        workers_joined: true,
        tree_empty: true,
        output_incomplete: false,
        discarded_chunks: 0,
        discarded_bytes: 0,
    }
}
fn stage() -> StageId { StageId::Probe(ProbeStage::Prerequisites) }
fn log(s: &str) -> LogInput {
    LogInput {
        stage: stage(),
        source: Source::Stdout,
        text: s.to_owned(),
        progress: false,
    }
}
fn poll(
    s: &JobStore,
    n: u32,
) -> EventBatch {
    s.poll(&id(n), PollRequest { after: 0, limit: 128 }).unwrap()
}

#[test]
fn rolling_records_do_not_lose_terminal_summary_and_poll_cursor() {
    let mut s = store();
    s.start(id(1), OperationKind::PrerequisiteCheck, None).unwrap();
    for _ in 0..1100 {
        s.append(&id(1), log("line")).unwrap();
    }
    let p = poll(&s, 1);
    assert_eq!((p.oldest, p.next, p.dropped, p.records.len()), (77, 1101, 76, 128));
    assert_eq!(p.records.last().unwrap().sequence, 204);
    s.transition(&id(1), stage(), StageState::Running, None).unwrap();
    s.transition(&id(1), stage(), StageState::Succeeded, None).unwrap();
    s.finish(&id(1), TerminalCause::Succeeded, closure(), None, None)
        .unwrap();
    let p = poll(&s, 1);
    assert!(p.summary.events_closed);
    assert_eq!(p.summary.final_next, Some(p.next));
    assert_eq!(s.append(&id(1), log("late")), Err(Error::Closed));
    assert!(
        s.poll(
            &id(1),
            PollRequest {
                after: u64::MAX,
                limit: 128
            }
        )
        .unwrap()
        .records
        .is_empty()
    );
}

#[test]
fn seventeen_jobs_retain_sixteen_and_active_rejection_is_atomic() {
    let mut s = store();
    for n in 1..=17 {
        s.start(id(n), OperationKind::PrerequisiteCheck, None).unwrap();
        let before = s.summaries();
        assert!(matches!(s.start(id(100), OperationKind::Setup, None), Err(Error::Busy)));
        assert_eq!(s.summaries(), before);
        s.transition(&id(n), stage(), StageState::Running, None).unwrap();
        s.transition(&id(n), stage(), StageState::Succeeded, None).unwrap();
        s.finish(&id(n), TerminalCause::Succeeded, closure(), None, None)
            .unwrap();
    }
    assert_eq!(s.summaries().len(), 16);
    assert_eq!(s.summaries()[0].job_id, id(2));
    assert!(matches!(
        s.poll(&id(1), PollRequest { after: 0, limit: 1 }),
        Err(Error::Expired)
    ));
}

#[test]
fn illegal_transitions_and_unjoined_seal_preserve_state() {
    let mut s = store();
    s.start(id(1), OperationKind::Build, None).unwrap();
    let before = s.summaries();
    assert!(
        s.transition(
            &id(1),
            StageId::Firmware(FirmwareStage::Validation),
            StageState::Running,
            None
        )
        .is_err()
    );
    assert_eq!(s.summaries(), before);
    assert!(s.transition(&id(1), stage(), StageState::Running, None).is_err());
    assert_eq!(s.summaries(), before);
    let first = StageId::Firmware(FirmwareStage::Prerequisites);
    s.transition(&id(1), first, StageState::Running, None).unwrap();
    s.transition(&id(1), first, StageState::Failed, Some(Text::new("failure").unwrap()))
        .unwrap();
    assert!(
        s.summaries()[0].stages[1..]
            .iter()
            .all(|s| s.state == StageState::Skipped)
    );
    let mut c = closure();
    c.workers_joined = false;
    let before = s.summaries();
    assert!(s.finish(&id(1), TerminalCause::Failed, c, None, None).is_err());
    assert_eq!(s.summaries(), before);
    let mut c = closure();
    c.output_incomplete = true;
    s.finish(&id(1), TerminalCause::Failed, c, None, None).unwrap();
    assert!(s.summaries()[0].output_incomplete);
}

#[test]
fn transfer_cancel_waits_boundary_and_verified_reboot_failure_remains_verified() {
    for cancel in [true, false] {
        let mut s = store();
        s.start(id(1), OperationKind::Flash, None).unwrap();
        for stage in [
            FirmwareStage::Prerequisites,
            FirmwareStage::Validation,
            FirmwareStage::DeviceSelection,
        ] {
            s.transition(&id(1), StageId::Firmware(stage), StageState::Running, None)
                .unwrap();
            s.transition(&id(1), StageId::Firmware(stage), StageState::Succeeded, None)
                .unwrap();
        }
        let transfer = StageId::Firmware(FirmwareStage::Transfer);
        s.transition(&id(1), transfer, StageState::Running, None).unwrap();
        if cancel {
            s.cancel(&id(1)).unwrap();
            assert_eq!(s.summaries()[0].stages[3].state, StageState::Running);
        }
        s.transition(&id(1), transfer, StageState::Succeeded, None).unwrap();
        if cancel {
            s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
                .unwrap();
            assert_eq!(s.summaries()[0].write_outcome, WriteOutcome::TransferredUnverified);
        } else {
            let v = StageId::Firmware(FirmwareStage::Verification);
            s.transition(&id(1), v, StageState::Running, None).unwrap();
            s.transition(&id(1), v, StageState::Succeeded, None).unwrap();
            let r = StageId::Firmware(FirmwareStage::Reboot);
            s.transition(&id(1), r, StageState::Running, None).unwrap();
            s.transition(&id(1), r, StageState::Failed, Some(Text::new("failed").unwrap()))
                .unwrap();
            s.finish(&id(1), TerminalCause::Failed, closure(), None, None).unwrap();
            assert_eq!(s.summaries()[0].write_outcome, WriteOutcome::Verified);
        }
    }
}

#[test]
fn setup_aggregates_are_ledger_owned_and_cancellation_cannot_seal_active_step() {
    use dashboard_helper_protocol::setup::*;
    let step = SetupStepId {
        tool: ToolId::MsvcSdk,
        stage: SetupStage::Validate,
        command: SetupCommand::HostLinkSmoke,
    };
    let plan = SetupPlan::new(
        vec![SetupAssignment {
            step,
            executor: SetupExecutor::OriginalOwner,
        }],
        DelegationShape::Cold,
    )
    .unwrap();
    let mut s = store();
    s.start_setup(id(1), plan, None).unwrap();
    assert_eq!(s.summaries()[0].stages[0].state, StageState::Skipped);
    assert!(
        s.transition(&id(1), StageId::Setup(SetupStage::Check), StageState::Running, None)
            .is_err()
    );
    s.setup_start_next(&id(1), SetupExecutor::OriginalOwner, step).unwrap();
    assert_eq!(s.summaries()[0].active_setup_step, Some(step));
    s.cancel(&id(1)).unwrap();
    assert!(
        s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
            .is_err()
    );
    s.setup_finish_active(
        &id(1),
        SetupStepOutcome {
            step,
            state: StageState::Cancelled,
            failure: Some(SetupFailure::Cancelled),
            raw_exit: None,
            elapsed_ms: 1,
            output_incomplete: false,
        },
    )
    .unwrap();
    s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
        .unwrap();
    assert!(s.summaries()[0].events_closed);
}
#[test]
fn summaries_reject_forged_success_and_terminal_write_facts() {
    let mut s = store();
    s.start(id(1), OperationKind::Build, None).unwrap();
    let mut summary = s.summaries().remove(0);
    summary.terminal_cause = Some(TerminalCause::Succeeded);
    summary.events_closed = true;
    summary.final_next = Some(1);
    let mut stages = summary.stages.clone().into_vec();
    for stage in &mut stages {
        stage.state = StageState::Skipped;
        stage.reason = Some(Text::new("not run").unwrap());
    }
    summary.stages = BoundedVec::new(stages).unwrap();
    assert!(
        encode(&summary).is_err(),
        "a skipped build without artifact cannot claim success"
    );
}
#[test]
fn event_utf8_split_and_tombstone_expiry_are_exact() {
    let mut s = store();
    s.start(id(1), OperationKind::PrerequisiteCheck, None).unwrap();
    let input = "界\"".repeat(256);
    s.append(&id(1), log(&input)).unwrap();
    let p = poll(&s, 1);
    assert!(p.records.len() > 1);
    assert_eq!(p.records.iter().map(|r| r.text.as_str()).collect::<String>(), input);
    for e in p.records.iter() {
        assert!(encode(e).unwrap().len() <= 1024);
    }
    s.cancel(&id(1)).unwrap();
    s.finish(&id(1), TerminalCause::Cancelled, closure(), None, None)
        .unwrap();
    for n in 2..=34 {
        s.start(id(n), OperationKind::PrerequisiteCheck, None).unwrap();
        s.cancel(&id(n)).unwrap();
        s.finish(&id(n), TerminalCause::Cancelled, closure(), None, None)
            .unwrap();
    }
    assert!(matches!(
        s.poll(&id(1), PollRequest { after: 0, limit: 1 }),
        Err(Error::NotFoundOrUnavailable)
    ));
    assert!(matches!(
        s.poll(&id(18), PollRequest { after: 0, limit: 1 }),
        Err(Error::Expired)
    ));
}

#[test]
fn public_summary_list_and_poll_bounds_use_independent_envelopes() {
    let mut s = store();
    s.start(id(1), OperationKind::Build, None).unwrap();
    let mut summary = s.summaries().remove(0);
    summary.operation = OperationKind::Setup;
    summary.stages = BoundedVec::new(
        SETUP_STAGES
            .into_iter()
            .map(|stage| {
                StageSummary {
                    stage: StageId::Setup(stage),
                    state: StageState::Pending,
                    reason: None,
                    started_ms: None,
                    ended_ms: None,
                    tools: BoundedVec::new(
                        TOOLS
                            .into_iter()
                            .map(|tool| {
                                ToolSummary {
                                    tool,
                                    state: StageState::Pending,
                                    reason: Some(Text::new("x".repeat(512)).unwrap()),
                                }
                            })
                            .collect(),
                    )
                    .unwrap(),
                }
            })
            .collect(),
    )
    .unwrap();
    let summary_bytes = encode(&summary).unwrap();
    assert!(summary_bytes.len() < 65536);
    assert!(summary_bytes.len() > 40000);
    let summaries = (1..=16)
        .map(|n| {
            let mut v = summary.clone();
            v.job_id = id(n);
            v
        })
        .collect();
    let list = JobList {
        session_id: summary.session_id.clone(),
        jobs: BoundedVec::new(summaries).unwrap(),
    };
    let encoded = encode(&list).unwrap();
    assert!(encoded.len() > 65536);
    assert!(encoded.len() < JobList::MAX_BYTES);
    assert!(decode::<JobList>(&encoded).is_ok());
    let event = Event {
        sequence: 1,
        utc_ms: Some(u64::MAX),
        elapsed_ms: u64::MAX,
        stage: StageId::Setup(SetupStage::Check),
        kind: EventKind::Diagnostic,
        level: Level::Warning,
        source: Source::Internal,
        text: Text::new("x".repeat(500)).unwrap(),
        progress: None,
        tool: None,
        setup_step: None,
    };
    let records = (1..=128)
        .map(|n| {
            let mut e = event.clone();
            e.sequence = n;
            e
        })
        .collect();
    let batch = EventBatch {
        session_id: summary.session_id.clone(),
        job_id: summary.job_id.clone(),
        oldest: 1,
        next: 129,
        dropped: 0,
        records: BoundedVec::new(records).unwrap(),
        summary: summary.clone(),
    };
    assert!(encode(&batch).unwrap().len() < 262144);
    let mut stages = summary.stages.into_vec();
    for stage in &mut stages {
        let mut tools = stage.tools.clone().into_vec();
        for tool in &mut tools {
            tool.reason = Some(Text::new("\0".repeat(512)).unwrap());
        }
        stage.tools = BoundedVec::new(tools).unwrap();
    }
    summary.stages = BoundedVec::new(stages).unwrap();
    assert!(encode(&summary).is_err());
    println!(
        "80-cell summary={} bytes;16-summary list={} bytes",
        summary_bytes.len(),
        encoded.len()
    );
}

#[test]
fn immutable_flash_binding_and_unjoined_cleanup_fatal_survive_polling() {
    let mut s = store();
    s.start(id(1), OperationKind::Flash, None).unwrap();
    let artifact = ArtifactId::new("a".repeat(64)).unwrap();
    let device = DeviceId::new("b".repeat(32)).unwrap();
    s.bind_flash(&id(1), artifact.clone(), device.clone()).unwrap();
    assert!(
        s.bind_flash(&id(1), ArtifactId::new("c".repeat(64)).unwrap(), device.clone())
            .is_err()
    );
    for stage in OperationKind::Flash.layout() {
        s.transition(&id(1), stage, StageState::Running, None).unwrap();
        if stage == StageId::Firmware(FirmwareStage::Transfer) {
            break;
        }
        s.transition(&id(1), stage, StageState::Succeeded, None).unwrap();
    }
    s.note_cleanup_fatal(&id(1), true).unwrap();
    let summary = s.summaries().remove(0);
    assert_eq!(summary.terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
    assert_eq!(summary.write_outcome, WriteOutcome::Unknown);
    assert!(summary.output_incomplete);
    assert!(!summary.events_closed);
    assert_eq!(summary.final_next, None);
    assert_eq!(summary.artifact_id, Some(artifact.clone()));
    assert_eq!(summary.device_id, Some(device.clone()));
    assert!(s.start(id(2), OperationKind::Build, None).is_err());
    s.finish(&id(1), TerminalCause::OwnershipCleanupFatal, closure(), None, None)
        .unwrap();
    let summary = s.summaries().remove(0);
    assert_eq!(summary.artifact_id, Some(artifact));
    assert_eq!(summary.device_id, Some(device));
    assert!(summary.events_closed);
}

#[test]
fn setup_ledger_cleanup_fatal_cannot_be_downgraded_at_finalization() {
    use dashboard_helper_protocol::setup::*;
    let mut s = store();
    let steps = setup_steps();
    let plan = SetupPlan::new(
        steps[..2]
            .iter()
            .map(|&step| {
                SetupAssignment {
                    step,
                    executor: SetupExecutor::OriginalOwner,
                }
            })
            .collect(),
        DelegationShape::Cold,
    )
    .unwrap();
    s.start_setup(id(1), plan, None).unwrap();
    s.setup_start_next(&id(1), SetupExecutor::OriginalOwner, steps[0])
        .unwrap();
    s.cancel(&id(1)).unwrap();
    // Cleanup-fatal supersedes the earlier cancellation, through the ledger only.
    s.setup_finish_active(
        &id(1),
        SetupStepOutcome {
            step: steps[0],
            state: StageState::Failed,
            failure: Some(SetupFailure::OwnershipCleanupFatal),
            raw_exit: None,
            elapsed_ms: 1,
            output_incomplete: true,
        },
    )
    .unwrap();
    let before = s.summaries();
    for cause in [
        TerminalCause::Failed,
        TerminalCause::Cancelled,
        TerminalCause::Succeeded,
    ] {
        assert!(
            s.finish(&id(1), cause, closure(), None, None).is_err(),
            "must reject downgrade to {cause:?}"
        );
        assert_eq!(s.summaries(), before);
    }
    assert_eq!(before[0].terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
    assert!(!before[0].events_closed);
    assert_eq!(before[0].final_next, None);
    let mut unjoined = closure();
    unjoined.workers_joined = false;
    assert!(
        s.finish(&id(1), TerminalCause::OwnershipCleanupFatal, unjoined, None, None)
            .is_err()
    );
    assert_eq!(s.summaries(), before);
    s.finish(&id(1), TerminalCause::OwnershipCleanupFatal, closure(), None, None)
        .unwrap();
    let final_summary = s.summaries().remove(0);
    assert_eq!(final_summary.terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
    assert!(final_summary.events_closed);
}
