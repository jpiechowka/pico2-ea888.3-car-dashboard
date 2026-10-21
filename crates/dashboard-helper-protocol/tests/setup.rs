//! Pure setup transcript/ledger tests; fixtures do not create processes, pipes or install tools.
use dashboard_helper_protocol::setup::*;
use dashboard_helper_protocol::*;
/// Return the tuple from `tool`, `stage` and `command`; admission remains the tested implementation's job.
fn step(
    tool: ToolId,
    stage: SetupStage,
    command: SetupCommand,
) -> SetupStepId {
    SetupStepId { tool, stage, command }
}
/// Return 1 ms fixture evidence for `step`/`state`/`failure`, without a process exit or output loss.
fn outcome(
    step: SetupStepId,
    state: StageState,
    failure: Option<SetupFailure>,
) -> SetupStepOutcome {
    SetupStepOutcome {
        step,
        state,
        failure,
        raw_exit: None,
        elapsed_ms: 1,
        output_incomplete: false,
    }
}
#[test]
/// A later Validate step keeps that aggregate Running while ordered Install work proceeds; double-start is inert.
fn cold_prefix_can_validate_before_install_without_reopening_aggregate() {
    let a = step(ToolId::MsvcSdk, SetupStage::Validate, SetupCommand::HostLinkSmoke);
    let b = step(
        ToolId::NativeHelper,
        SetupStage::Install,
        SetupCommand::BuildNativeHelper,
    );
    let c = step(ToolId::Picotool, SetupStage::Validate, SetupCommand::HelpLoad);
    let plan = SetupPlan::new(
        vec![
            SetupAssignment {
                step: a,
                executor: SetupExecutor::OriginalOwner,
            },
            SetupAssignment {
                step: b,
                executor: SetupExecutor::OriginalOwner,
            },
            SetupAssignment {
                step: c,
                executor: SetupExecutor::Peer0,
            },
        ],
        DelegationShape::Cold,
    )
    .unwrap();
    let mut l = SetupLedger::new(plan);
    l.start_next(SetupExecutor::OriginalOwner, a).unwrap();
    l.finish_active(outcome(a, StageState::Succeeded, None)).unwrap();
    assert_eq!(l.projection()[4].state, StageState::Running);
    l.start_next(SetupExecutor::OriginalOwner, b).unwrap();
    assert_eq!(l.active(), Some(b));
    assert_eq!(l.projection()[3].state, StageState::Running);
    assert_eq!(l.projection()[4].state, StageState::Running);
    let before = l.clone();
    assert!(l.start_next(SetupExecutor::Peer0, c).is_err());
    assert_eq!(l, before);
    l.finish_active(outcome(b, StageState::Succeeded, None)).unwrap();
    l.start_next(SetupExecutor::Peer0, c).unwrap();
    l.finish_active(outcome(c, StageState::Succeeded, None)).unwrap();
    assert_eq!(l.projection()[4].state, StageState::Succeeded);
}
#[test]
/// Stop retains active work until terminal completion, blocks new starts and marks pending skips with the cause.
fn stop_retains_active_until_boundary_and_skips_with_truthful_cause() {
    let steps = setup_steps();
    assert_eq!(steps.len(), 82);
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
    let mut l = SetupLedger::new(plan);
    l.start_next(SetupExecutor::OriginalOwner, steps[0]).unwrap();
    l.stop(SetupFailure::Cancelled);
    assert_eq!(l.active(), Some(steps[0]));
    assert_eq!(l.projection()[0].state, StageState::Running);
    assert!(l.start_next(SetupExecutor::OriginalOwner, steps[1]).is_err());
    l.finish_active(outcome(steps[0], StageState::Cancelled, Some(SetupFailure::Cancelled)))
        .unwrap();
    assert_eq!(l.projection()[0].state, StageState::Cancelled);
    assert_eq!(l.projection()[1].failure, Some(SetupFailure::Cancelled));
}
/// Return an all-f grammar-valid digest, not a hash of real tool bytes.
fn hash() -> Sha256 { Sha256::new("f".repeat(64)).unwrap() }
/// Return widest fixed digest bindings for private codec/transcript tests.
fn context() -> SetupInputContext {
    SetupInputContext {
        workspace_sha256: hash(),
        context_sha256: hash(),
        report_sha256: hash(),
        source_sha256: hash(),
        tool_manifest_sha256: hash(),
        compiler_spec_sha256: hash(),
        root_lock_sha256: hash(),
        firmware_lock_sha256: hash(),
        helper_lock_sha256: hash(),
        options_sha256: hash(),
    }
}
/// Return maximal integer process/file identifiers without claiming any real operating-system process.
fn peer() -> SetupPeerIdentity {
    SetupPeerIdentity {
        pid: u32::MAX,
        creation_filetime_100ns: u64::MAX,
        image_sha256: hash(),
        image_volume_serial: u64::MAX,
        image_file_id: FileId128::new("f".repeat(32)).unwrap(),
    }
}
/// Return a cold continuation envelope for slot0 with a deterministic private test nonce.
fn envelope() -> SetupLeaseEnvelope {
    SetupLeaseEnvelope {
        schema_version: 1,
        attempt_id: AttemptId::new("f".repeat(32)).unwrap(),
        action: SetupAction::UpdatePinned,
        role: SetupRole::Continuation,
        peer_slot: 0,
        context: context(),
        owner: peer(),
        expected_peer_image_sha256: hash(),
        nonce: Nonce::new(&"e".repeat(64)).unwrap(),
    }
}
/// Return a slot0 frame with sequence `n` and typed `body`, bound to the fixture attempt/context.
fn frame(
    n: u16,
    body: SetupLeaseMessage,
) -> SetupLeaseFrame {
    SetupLeaseFrame {
        schema_version: 1,
        attempt_id: AttemptId::new("f".repeat(32)).unwrap(),
        context_sha256: hash(),
        peer_slot: 0,
        message_no: n,
        body,
    }
}
/// Return a cold plan assigning ordered `steps` to Peer0; panic if fixture tuples are invalid or duplicated.
fn assigned(steps: &[SetupStepId]) -> SetupPlan {
    SetupPlan::new(
        steps
            .iter()
            .map(|&step| {
                SetupAssignment {
                    step,
                    executor: SetupExecutor::Peer0,
                }
            })
            .collect(),
        DelegationShape::Cold,
    )
    .unwrap()
}
#[test]
/// A completed peer command blocks all new ledger work until its exact receipt is acknowledged.
fn private_peer_requires_receipt_before_any_next_command() {
    let steps = setup_steps();
    let mut ledger = SetupLedger::new(assigned(&steps[..2]));
    let e = envelope();
    let nonce = e.nonce.clone();
    let mut p = SetupLeasePeer::new(e, peer(), 0).unwrap();
    let mut send = |direction, body, now| {
        p.accept(direction, &frame(now as u16 + 1, body), now, &mut ledger)
            .unwrap();
    };
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::Hello {
            nonce: nonce.clone(),
            peer: peer(),
        },
        0,
    );
    send(SetupDirection::OwnerToPeer, SetupLeaseMessage::HelloAck { nonce }, 1);
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::AdmitStep { step: steps[0] },
        2,
    );
    send(
        SetupDirection::OwnerToPeer,
        SetupLeaseMessage::StepAdmitted { step: steps[0] },
        3,
    );
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::StepFinished {
            outcome: outcome(steps[0], StageState::Succeeded, None),
        },
        4,
    );
    assert!(
        ledger.start_next(SetupExecutor::Peer0, steps[1]).is_err(),
        "global ledger must block while finish receipt is outstanding"
    );
    p.accept(
        SetupDirection::OwnerToPeer,
        &frame(6, SetupLeaseMessage::StepReceived { step: steps[0] }),
        5,
        &mut ledger,
    )
    .unwrap();
    assert!(ledger.start_next(SetupExecutor::Peer0, steps[1]).is_ok());
}
#[test]
/// A full 82-step success transcript, wide result and cleanup-failure result fit actual serializer limits.
/// Output loss remains explicit even with successful commands; no OS cleanup behavior is simulated here.
fn private_serialized_maxima_and_full_82_step_transcript_fit_caps() {
    let steps = setup_steps();
    assert_eq!(steps.len(), 82);
    let mut ledger = SetupLedger::new(assigned(&steps));
    let e = envelope();
    let nonce = e.nonce.clone();
    let bytes = encode_private(&e).unwrap();
    assert!(bytes.len() <= 4096);
    assert!(decode_private::<SetupLeaseEnvelope>(&bytes).is_ok());
    let mut p = SetupLeasePeer::new(e, peer(), 0).unwrap();
    let mut no = 1u16;
    let mut max_frame = bytes.len();
    let mut send = |dir, body, ledger: &mut SetupLedger| {
        let f = frame(no, body);
        let b = encode_private(&f).unwrap();
        max_frame = max_frame.max(b.len());
        assert!(decode_private::<SetupLeaseFrame>(&b).is_ok());
        p.accept(dir, &f, u64::from(no), ledger).unwrap();
        no += 1;
    };
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::Hello {
            nonce: nonce.clone(),
            peer: peer(),
        },
        &mut ledger,
    );
    send(
        SetupDirection::OwnerToPeer,
        SetupLeaseMessage::HelloAck { nonce },
        &mut ledger,
    );
    for &s in &steps {
        send(
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::AdmitStep { step: s },
            &mut ledger,
        );
        send(
            SetupDirection::OwnerToPeer,
            SetupLeaseMessage::StepAdmitted { step: s },
            &mut ledger,
        );
        let mut o = outcome(s, StageState::Succeeded, None);
        o.elapsed_ms = u64::MAX;
        o.raw_exit = Some(RawExit {
            windows_u32: u32::MAX,
            interpreted_i32: -1,
        });
        send(
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::StepFinished { outcome: o },
            &mut ledger,
        );
        send(
            SetupDirection::OwnerToPeer,
            SetupLeaseMessage::StepReceived { step: s },
            &mut ledger,
        );
    }
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::ResultReady {
            result_sha256: hash(),
            result_bytes: 65536,
        },
        &mut ledger,
    );
    send(
        SetupDirection::OwnerToPeer,
        SetupLeaseMessage::ResultReceived { result_sha256: hash() },
        &mut ledger,
    );
    send(
        SetupDirection::PeerToOwner,
        SetupLeaseMessage::Close {
            disposition: SetupCloseDisposition::Completed,
        },
        &mut ledger,
    );
    send(
        SetupDirection::OwnerToPeer,
        SetupLeaseMessage::Closed {
            disposition: SetupCloseDisposition::Completed,
        },
        &mut ledger,
    );
    let (count, total) = p.counts();
    assert_eq!(count, 335);
    assert!(total < 1_392_640);
    assert!(p.closed());
    let result = SetupContinuationResult {
        schema_version: 1,
        attempt_id: AttemptId::new("f".repeat(32)).unwrap(),
        action: SetupAction::UpdatePinned,
        role: SetupRole::Continuation,
        peer_slot: 0,
        context: context(),
        native_exe_sha256: Some(hash()),
        resolved_tools_sha256: Some(hash()),
        compiler_identity_sha256: Some(hash()),
        stage_outcomes: ledger.projection(),
        step_outcomes: BoundedVec::new(ledger.outcomes().to_vec()).unwrap(),
        terminal: SetupResultState::Succeeded,
        failure: None,
        closure: SetupInternalClosure {
            tree: SetupTreeState::Empty,
            workers: SetupWorkerState::Joined,
            output_incomplete: true,
            observed_discarded_chunks: u64::MAX,
            observed_discarded_bytes: u64::MAX,
            counter_overflow: true,
        },
        publication: SetupPublication::PendingPublication,
    };
    let b = encode(&result).unwrap();
    assert!(b.len() <= 47360);
    assert!(decode::<SetupContinuationResult>(&b).is_ok());
    p.validate_result(&result, &ledger).unwrap();
    let mut failure_result = result.clone();
    failure_result.terminal = SetupResultState::CleanupFatal;
    failure_result.failure = Some(SetupFailure::PrerequisiteUnavailable);
    failure_result.closure.tree = SetupTreeState::NotEmpty;
    failure_result.closure.workers = SetupWorkerState::Unjoined;
    let mut failed = failure_result.step_outcomes.clone().into_vec();
    for o in &mut failed {
        o.state = StageState::Cancelled;
        o.failure = Some(SetupFailure::PrerequisiteUnavailable);
        o.output_incomplete = false;
        o.raw_exit = Some(RawExit {
            windows_u32: 2_147_483_648,
            interpreted_i32: i32::MIN,
        });
    }
    failure_result.stage_outcomes = aggregate(&failed);
    failure_result.step_outcomes = BoundedVec::new(failed).unwrap();
    let maximum_result = encode(&failure_result).unwrap().len();
    assert!(maximum_result <= 47360);
    println!("conservative widest result fixture: {maximum_result} bytes");
    println!(
        "82 steps: {count} actual records, {total} bytes; maximum exercised envelope/frame {max_frame}; result {} \
         bytes",
        b.len()
    );
}
#[test]
/// Every control variant and admitted tuple fits framing; conservative whole-transcript bounds cover them.
fn all_private_control_variants_and_step_tuples_fit_real_serializer() {
    let nonce = Nonce::new(&"a".repeat(64)).unwrap();
    let mut max = 0;
    for step in setup_steps() {
        let mut o = outcome(step, StageState::Cancelled, Some(SetupFailure::PrerequisiteUnavailable));
        o.elapsed_ms = u64::MAX;
        o.raw_exit = Some(RawExit {
            windows_u32: 2_147_483_648,
            interpreted_i32: i32::MIN,
        });
        for body in [
            SetupLeaseMessage::Hello {
                nonce: nonce.clone(),
                peer: peer(),
            },
            SetupLeaseMessage::HelloAck { nonce: nonce.clone() },
            SetupLeaseMessage::AdmitStep { step },
            SetupLeaseMessage::StepAdmitted { step },
            SetupLeaseMessage::StepFinished { outcome: o },
            SetupLeaseMessage::StepReceived { step },
            SetupLeaseMessage::PrepareContinuation { image_sha256: hash() },
            SetupLeaseMessage::ContinuationPrepared {
                peer_slot: 1,
                nonce: nonce.clone(),
            },
            SetupLeaseMessage::RegisterContinuation { peer: peer() },
            SetupLeaseMessage::ContinuationRegistered { peer_slot: 1 },
            SetupLeaseMessage::ResultReady {
                result_sha256: hash(),
                result_bytes: 65536,
            },
            SetupLeaseMessage::ResultReceived { result_sha256: hash() },
            SetupLeaseMessage::Close {
                disposition: SetupCloseDisposition::CleanupFatal,
            },
            SetupLeaseMessage::Closed {
                disposition: SetupCloseDisposition::CleanupFatal,
            },
            SetupLeaseMessage::Rejected {
                failure: SetupFailure::PrerequisiteUnavailable,
            },
        ] {
            let mut frame = frame(1024, body);
            frame.peer_slot = 1;
            let encoded = encode_private(&frame).unwrap();
            assert!(decode_private::<SetupLeaseFrame>(&encoded).is_ok());
            assert!(encoded.len() <= 4096);
            max = max.max(encoded.len());
        }
    }
    let conservative_records = setup_steps().len() * 4 + 12;
    assert_eq!(conservative_records, 340);
    assert!(conservative_records <= 1024);
    assert!(conservative_records * 4096 <= 4 * 1024 * 1024);
    println!(
        "15 control variants across all 82 tuples: largest frame {max} bytes; 340-record conservative cap 1392640 \
         bytes per peer"
    );
}
#[test]
/// Native delegation binds the prepared image, distinct nonce, inherited context/action and nonreusable slot.
fn native_second_peer_binds_prepared_image_nonce_context_and_action() {
    let steps = setup_steps();
    let plan = SetupPlan::new(
        vec![
            SetupAssignment {
                step: steps[0],
                executor: SetupExecutor::Peer0,
            },
            SetupAssignment {
                step: steps[1],
                executor: SetupExecutor::Peer1,
            },
        ],
        DelegationShape::Native,
    )
    .unwrap();
    let mut ledger = SetupLedger::new(plan);
    let mut peers = SetupLeasePeers::new(DelegationShape::Native);
    let mut e = envelope();
    e.role = SetupRole::Bootstrap;
    peers.register(e.clone(), peer(), 0).unwrap();
    let second_nonce = Nonce::new(&"c".repeat(64)).unwrap();
    for (i, (direction, body)) in [
        (
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::Hello {
                nonce: e.nonce.clone(),
                peer: peer(),
            },
        ),
        (
            SetupDirection::OwnerToPeer,
            SetupLeaseMessage::HelloAck { nonce: e.nonce.clone() },
        ),
        (
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::PrepareContinuation { image_sha256: hash() },
        ),
        (
            SetupDirection::OwnerToPeer,
            SetupLeaseMessage::ContinuationPrepared {
                peer_slot: 1,
                nonce: second_nonce.clone(),
            },
        ),
        (
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::RegisterContinuation { peer: peer() },
        ),
        (
            SetupDirection::OwnerToPeer,
            SetupLeaseMessage::ContinuationRegistered { peer_slot: 1 },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        peers
            .peer_mut(0)
            .unwrap()
            .accept(direction, &frame(i as u16 + 1, body), i as u64, &mut ledger)
            .unwrap();
    }
    let mut child = envelope();
    child.peer_slot = 1;
    child.nonce = second_nonce;
    let mut bad = child.clone();
    bad.action = SetupAction::MissingRequired;
    assert!(peers.register(bad, peer(), 7).is_err());
    let mut bad = child.clone();
    bad.nonce = e.nonce;
    assert!(peers.register(bad, peer(), 7).is_err());
    let mut bad = child.clone();
    bad.context.source_sha256 = Sha256::new("b".repeat(64)).unwrap();
    assert!(peers.register(bad, peer(), 7).is_err());
    peers.register(child.clone(), peer(), 7).unwrap();
    assert!(peers.register(child, peer(), 7).is_err());
}
#[test]
/// Wrong nonce or a 15001 ms handshake retires the peer; repeated/out-of-range slots and oversized prefixes fail.
fn invalid_nonce_or_late_handshake_retires_peer_without_success() {
    for late in [false, true] {
        let steps = setup_steps();
        let mut ledger = SetupLedger::new(assigned(&steps[..1]));
        let e = envelope();
        let mut p = SetupLeasePeer::new(e, peer(), 0).unwrap();
        let f = frame(
            1,
            SetupLeaseMessage::Hello {
                nonce: Nonce::new(&if late { "e".repeat(64) } else { "a".repeat(64) }).unwrap(),
                peer: peer(),
            },
        );
        assert!(
            p.accept(
                SetupDirection::PeerToOwner,
                &f,
                if late { 15001 } else { 1 },
                &mut ledger
            )
            .is_err()
        );
        assert!(p.closed());
        assert!(ledger.start_next(SetupExecutor::Peer0, steps[0]).is_err());
    }
    let mut peers = SetupLeasePeers::new(DelegationShape::Cold);
    peers.register(envelope(), peer(), 0).unwrap();
    assert!(peers.register(envelope(), peer(), 0).is_err());
    let mut e = envelope();
    e.peer_slot = 2;
    assert!(peers.register(e, peer(), 0).is_err());
    let bad = 4093u32.to_le_bytes();
    assert!(decode_private::<SetupLeaseFrame>(&bad).is_err());
}

#[test]
/// Abnormal Close stops pending work before acknowledgement while preserving prior successful/active evidence.
fn abnormal_close_immediately_stops_pending_work_and_preserves_prior_facts() {
    for (disposition, failure) in [
        (SetupCloseDisposition::Failed, SetupFailure::CommandFailed),
        (SetupCloseDisposition::Cancelled, SetupFailure::Cancelled),
        (SetupCloseDisposition::CleanupFatal, SetupFailure::OwnershipCleanupFatal),
    ] {
        for active_owner in [false, true] {
            let steps = setup_steps();
            let plan = SetupPlan::new(
                vec![
                    SetupAssignment {
                        step: steps[0],
                        executor: SetupExecutor::OriginalOwner,
                    },
                    SetupAssignment {
                        step: steps[1],
                        executor: SetupExecutor::Peer0,
                    },
                ],
                DelegationShape::Cold,
            )
            .unwrap();
            let mut ledger = SetupLedger::new(plan);
            ledger.start_next(SetupExecutor::OriginalOwner, steps[0]).unwrap();
            if !active_owner {
                ledger
                    .finish_active(outcome(steps[0], StageState::Succeeded, None))
                    .unwrap();
            }
            let prior = ledger.outcomes()[0].clone();
            let e = envelope();
            let nonce = e.nonce.clone();
            let mut peer_lease = SetupLeasePeer::new(e, peer(), 0).unwrap();
            peer_lease
                .accept(
                    SetupDirection::PeerToOwner,
                    &frame(
                        1,
                        SetupLeaseMessage::Hello {
                            nonce: nonce.clone(),
                            peer: peer(),
                        },
                    ),
                    0,
                    &mut ledger,
                )
                .unwrap();
            peer_lease
                .accept(
                    SetupDirection::OwnerToPeer,
                    &frame(2, SetupLeaseMessage::HelloAck { nonce }),
                    1,
                    &mut ledger,
                )
                .unwrap();
            peer_lease
                .accept(
                    SetupDirection::PeerToOwner,
                    &frame(3, SetupLeaseMessage::Close { disposition }),
                    2,
                    &mut ledger,
                )
                .unwrap();
            assert_eq!(
                ledger.stop_cause(),
                Some(failure),
                "Close must latch {disposition:?} before its acknowledgement"
            );
            assert_eq!(ledger.outcomes()[0], prior);
            assert_eq!(ledger.active(), active_owner.then_some(steps[0]));
            assert_eq!(ledger.outcomes()[1].state, StageState::Skipped);
            assert_eq!(ledger.outcomes()[1].failure, Some(failure));
            assert!(ledger.start_next(SetupExecutor::Peer0, steps[1]).is_err());
            peer_lease
                .accept(
                    SetupDirection::OwnerToPeer,
                    &frame(4, SetupLeaseMessage::Closed { disposition }),
                    3,
                    &mut ledger,
                )
                .unwrap();
            assert!(peer_lease.closed());
            assert_eq!(ledger.stop_cause(), Some(failure));
            assert_eq!(ledger.outcomes()[0], prior);
            assert!(ledger.start_next(SetupExecutor::Peer0, steps[1]).is_err());
        }
    }
}

/// Exact handshake deadline is accepted; replay, changed context, wrong direction and backward time retire it.
#[test]
fn transcript_identity_direction_and_time_fail_closed_without_publishing_candidate() {
    for fault in 0..4 {
        let steps = setup_steps();
        let mut ledger = SetupLedger::new(assigned(&steps[..1]));
        let envelope = envelope();
        let nonce = envelope.nonce.clone();
        let mut lease = SetupLeasePeer::new(envelope, peer(), 0).unwrap();
        let hello = frame(
            1,
            SetupLeaseMessage::Hello {
                nonce: nonce.clone(),
                peer: peer(),
            },
        );
        lease
            .accept(SetupDirection::PeerToOwner, &hello, 15000, &mut ledger)
            .unwrap();
        let accepted_counts = lease.counts();
        let mut next = frame(2, SetupLeaseMessage::HelloAck { nonce });
        let mut direction = SetupDirection::OwnerToPeer;
        let mut now = 15000;
        match fault {
            0 => next = hello,
            1 => next.context_sha256 = Sha256::new("a".repeat(64)).unwrap(),
            2 => direction = SetupDirection::PeerToOwner,
            _ => now -= 1,
        }
        assert!(lease.accept(direction, &next, now, &mut ledger).is_err());
        assert!(lease.closed());
        assert_eq!(
            lease.counts(),
            accepted_counts,
            "rejected record must not advance accounting"
        );
        assert_eq!(ledger.stop_cause(), Some(SetupFailure::InvalidState));
        assert_eq!(ledger.active(), None);
        assert_eq!(ledger.outcomes()[0].state, StageState::Skipped);
        assert!(ledger.start_next(SetupExecutor::Peer0, steps[0]).is_err());
    }
}

/// Every truncated prefix and trailing byte fails private framing before a value can escape admission.
#[test]
fn private_framing_rejects_truncation_and_trailing_bytes() {
    let valid = encode_private(&envelope()).unwrap();
    assert!(decode_private::<SetupLeaseEnvelope>(&valid).is_ok());
    for len in 0..valid.len() {
        assert!(
            decode_private::<SetupLeaseEnvelope>(&valid[..len]).is_err(),
            "prefix={len}"
        );
    }
    let mut trailing = valid.clone();
    trailing.push(b' ');
    assert!(decode_private::<SetupLeaseEnvelope>(&trailing).is_err());
    let mut mismatched = valid;
    mismatched[..4].copy_from_slice(&0u32.to_le_bytes());
    assert!(decode_private::<SetupLeaseEnvelope>(&mismatched).is_err());
}
