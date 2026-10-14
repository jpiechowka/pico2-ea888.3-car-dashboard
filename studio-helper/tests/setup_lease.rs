use dashboard_helper_protocol::setup::*;
use dashboard_helper_protocol::*;
use studio_helper::jobs::*;
fn hash() -> Sha256 { Sha256::new("a".repeat(64)).unwrap() }
#[test]
fn lease_control_updates_the_only_store_ledger_and_rejection_keeps_active_truth() {
    let context = SetupInputContext {
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
    };
    let identity = SetupPeerIdentity {
        pid: 1,
        creation_filetime_100ns: 1,
        image_sha256: hash(),
        image_volume_serial: 1,
        image_file_id: FileId128::new("a".repeat(32)).unwrap(),
    };
    let nonce = Nonce::new(&"b".repeat(64)).unwrap();
    let attempt_id = AttemptId::new("a".repeat(32)).unwrap();
    let envelope = SetupLeaseEnvelope {
        schema_version: 1,
        attempt_id: attempt_id.clone(),
        action: SetupAction::MissingRequired,
        role: SetupRole::Continuation,
        peer_slot: 0,
        context,
        owner: identity.clone(),
        expected_peer_image_sha256: hash(),
        nonce: nonce.clone(),
    };
    let mut peer = SetupLeasePeer::new(envelope, identity.clone(), 0).unwrap();
    let step = setup_steps()[0];
    let plan = SetupPlan::new(
        vec![SetupAssignment {
            step,
            executor: SetupExecutor::Peer0,
        }],
        DelegationShape::Cold,
    )
    .unwrap();
    let mut store = JobStore::new(SessionId::new("a".repeat(32)).unwrap());
    let id = JobId::new("b".repeat(32)).unwrap();
    store.start_setup(id.clone(), plan, None).unwrap();
    for (i, (direction, body)) in [
        (
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::Hello {
                nonce: nonce.clone(),
                peer: identity,
            },
        ),
        (SetupDirection::OwnerToPeer, SetupLeaseMessage::HelloAck { nonce }),
        (SetupDirection::PeerToOwner, SetupLeaseMessage::AdmitStep { step }),
        (SetupDirection::OwnerToPeer, SetupLeaseMessage::StepAdmitted { step }),
    ]
    .into_iter()
    .enumerate()
    {
        let frame = SetupLeaseFrame {
            schema_version: 1,
            attempt_id: attempt_id.clone(),
            context_sha256: hash(),
            peer_slot: 0,
            message_no: i as u16 + 1,
            body,
        };
        store.setup_accept(&id, &mut peer, direction, &frame, i as u64).unwrap();
    }
    assert_eq!(store.summaries()[0].active_setup_step, Some(step));
    let replay = SetupLeaseFrame {
        schema_version: 1,
        attempt_id,
        context_sha256: hash(),
        peer_slot: 0,
        message_no: 1,
        body: SetupLeaseMessage::AdmitStep { step },
    };
    assert!(
        store
            .setup_accept(&id, &mut peer, SetupDirection::PeerToOwner, &replay, 4)
            .is_err()
    );
    assert!(peer.closed());
    assert_eq!(store.summaries()[0].active_setup_step, Some(step));
    assert!(!store.summaries()[0].events_closed);
    assert!(store.setup_start_next(&id, SetupExecutor::Peer0, step).is_err());
}

#[test]
fn abnormal_lease_close_projects_abort_skips_without_claiming_output_closure() {
    for disposition in [
        SetupCloseDisposition::Failed,
        SetupCloseDisposition::Cancelled,
        SetupCloseDisposition::CleanupFatal,
    ] {
        let (mut store, id, mut peer, attempt_id) = idle_lease();
        let frame = |message_no, body| {
            SetupLeaseFrame {
                schema_version: 1,
                attempt_id: attempt_id.clone(),
                context_sha256: hash(),
                peer_slot: 0,
                message_no,
                body,
            }
        };
        store
            .setup_accept(
                &id,
                &mut peer,
                SetupDirection::PeerToOwner,
                &frame(3, SetupLeaseMessage::Close { disposition }),
                2,
            )
            .unwrap();
        let summary = store.summaries().remove(0);
        assert!(summary.stages.iter().all(|s| s.state == StageState::Skipped));
        assert!(
            summary.stages[0]
                .reason
                .as_ref()
                .unwrap()
                .as_str()
                .contains("interrupted")
        );
        assert_eq!(summary.active_setup_step, None);
        assert!(!summary.events_closed);
        assert_eq!(summary.final_next, None);
        assert!(
            store
                .setup_start_next(&id, SetupExecutor::Peer0, setup_steps()[0])
                .is_err()
        );
        store
            .setup_accept(
                &id,
                &mut peer,
                SetupDirection::OwnerToPeer,
                &frame(4, SetupLeaseMessage::Closed { disposition }),
                3,
            )
            .unwrap();
        assert!(peer.closed());
        assert_eq!(store.summaries()[0], summary);
        if disposition == SetupCloseDisposition::CleanupFatal {
            assert_eq!(summary.terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
            assert!(store.finish(&id, TerminalCause::Failed, joined(), None, None).is_err());
            store
                .finish(&id, TerminalCause::OwnershipCleanupFatal, joined(), None, None)
                .unwrap();
        }
    }
}

#[test]
fn cleanup_fatal_rejected_acknowledgement_cannot_be_downgraded() {
    let (mut store, id, mut peer, attempt_id) = idle_lease();
    let frame = |message_no, body| {
        SetupLeaseFrame {
            schema_version: 1,
            attempt_id: attempt_id.clone(),
            context_sha256: hash(),
            peer_slot: 0,
            message_no,
            body,
        }
    };
    store
        .setup_accept(
            &id,
            &mut peer,
            SetupDirection::PeerToOwner,
            &frame(3, SetupLeaseMessage::AdmitStep { step: setup_steps()[0] }),
            2,
        )
        .unwrap();
    store
        .setup_accept(
            &id,
            &mut peer,
            SetupDirection::OwnerToPeer,
            &frame(
                4,
                SetupLeaseMessage::Rejected {
                    failure: SetupFailure::OwnershipCleanupFatal,
                },
            ),
            3,
        )
        .unwrap();
    assert!(peer.closed());
    let before = store.summaries();
    assert!(store.finish(&id, TerminalCause::Failed, joined(), None, None).is_err());
    assert_eq!(store.summaries(), before);
    assert_eq!(before[0].terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
    assert!(!before[0].events_closed);
    store
        .finish(&id, TerminalCause::OwnershipCleanupFatal, joined(), None, None)
        .unwrap();
}

fn joined() -> OutputClosure {
    OutputClosure {
        workers_joined: true,
        tree_empty: true,
        output_incomplete: false,
        discarded_chunks: 0,
        discarded_bytes: 0,
    }
}

fn idle_lease() -> (JobStore, JobId, SetupLeasePeer, AttemptId) {
    let context = SetupInputContext {
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
    };
    let identity = SetupPeerIdentity {
        pid: 1,
        creation_filetime_100ns: 1,
        image_sha256: hash(),
        image_volume_serial: 1,
        image_file_id: FileId128::new("a".repeat(32)).unwrap(),
    };
    let nonce = Nonce::new(&"b".repeat(64)).unwrap();
    let attempt_id = AttemptId::new("a".repeat(32)).unwrap();
    let envelope = SetupLeaseEnvelope {
        schema_version: 1,
        attempt_id: attempt_id.clone(),
        action: SetupAction::MissingRequired,
        role: SetupRole::Continuation,
        peer_slot: 0,
        context,
        owner: identity.clone(),
        expected_peer_image_sha256: hash(),
        nonce: nonce.clone(),
    };
    let mut peer = SetupLeasePeer::new(envelope, identity.clone(), 0).unwrap();
    let plan = SetupPlan::new(
        vec![SetupAssignment {
            step: setup_steps()[0],
            executor: SetupExecutor::Peer0,
        }],
        DelegationShape::Cold,
    )
    .unwrap();
    let mut store = JobStore::new(SessionId::new("a".repeat(32)).unwrap());
    let id = JobId::new("b".repeat(32)).unwrap();
    store.start_setup(id.clone(), plan, None).unwrap();
    for (i, (direction, body)) in [
        (
            SetupDirection::PeerToOwner,
            SetupLeaseMessage::Hello {
                nonce: nonce.clone(),
                peer: identity,
            },
        ),
        (SetupDirection::OwnerToPeer, SetupLeaseMessage::HelloAck { nonce }),
    ]
    .into_iter()
    .enumerate()
    {
        let frame = SetupLeaseFrame {
            schema_version: 1,
            attempt_id: attempt_id.clone(),
            context_sha256: hash(),
            peer_slot: 0,
            message_no: i as u16 + 1,
            body,
        };
        store.setup_accept(&id, &mut peer, direction, &frame, i as u64).unwrap();
    }
    (store, id, peer, attempt_id)
}
