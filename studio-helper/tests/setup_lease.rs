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
