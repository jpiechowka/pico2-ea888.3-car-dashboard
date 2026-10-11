use dashboard_host_validation::diagnostics::CanFrame;
use dashboard_host_validation::diagnostics::client::{ClientError, TxAction};
use dashboard_host_validation::diagnostics::session::{Session, SessionError};
use dashboard_host_validation::samples::{Quality, SensorId};

fn reply(
    pid: u8,
    bytes: &[u8],
) -> CanFrame {
    let mut data = [0; 8];
    data[0] = bytes.len() as u8 + 2;
    data[1] = 0x41;
    data[2] = pid;
    data[3..3 + bytes.len()].copy_from_slice(bytes);
    CanFrame::new(0x7E8, &data).unwrap()
}
fn begin(
    session: &mut Session,
    at: u64,
    pid: u8,
) -> TxAction {
    let tx = session.poll(at).unwrap().expect("request due");
    assert_eq!(&tx.frame.data()[..3], &[2, 1, pid]);
    assert_eq!(tx.frame.id(), 0x7E0);
    session.tx_completed(tx.token, at + 1).unwrap();
    tx
}
fn exchange(
    session: &mut Session,
    at: u64,
    pid: u8,
    bytes: &[u8],
) {
    begin(session, at, pid);
    assert_eq!(session.receive(&reply(pid, bytes), 1, at + 2).unwrap(), None);
}
fn discover(session: &mut Session) {
    exchange(session, 0, 0, &0x0802_0001u32.to_be_bytes()); // 05, 0F, 20
    exchange(session, 22, 0x20, &1u32.to_be_bytes()); // 40
    exchange(session, 44, 0x40, &0x4000_0010u32.to_be_bytes()); // 42, 5C
}

#[test]
fn discovery_precedes_fair_supported_queries_and_samples_keep_sources() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    assert_eq!(session.snapshot().quality(SensorId::Water, 0), Quality::Unknown);
    discover(&mut session);
    exchange(&mut session, 66, 5, &[135]);
    exchange(&mut session, 88, 0x0F, &[70]);
    exchange(&mut session, 110, 0x42, &[0x36, 0xB0]);
    exchange(&mut session, 132, 0x5C, &[140]);
    let snapshot = session.snapshot();
    for (id, value) in [
        (SensorId::Water, 95.0),
        (SensorId::Iat, 30.0),
        (SensorId::Oil, 100.0),
        (SensorId::Battery, 14.0),
    ] {
        assert_eq!(snapshot.value_at(id, 134), Some(value));
    }
    for id in [SensorId::Boost, SensorId::Dsg, SensorId::Egt, SensorId::Lambda] {
        assert_eq!(snapshot.value_at(id, 134), None);
        assert_eq!(snapshot.quality(id, 134), Quality::Unknown);
    }
    assert_eq!(session.poll(611).unwrap(), None);
    begin(&mut session, 612, 0x42);
    assert_eq!(session.poll(613).unwrap(), None); // one outstanding
    assert_eq!(session.counters().completed, 7);
}

#[test]
fn absent_continuation_marks_only_proven_unsupported_channels() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    exchange(&mut session, 0, 0, &0x0800_0000u32.to_be_bytes()); // coolant only
    let snapshot = session.snapshot();
    for id in [SensorId::Iat, SensorId::Oil, SensorId::Battery] {
        assert_eq!(snapshot.quality(id, 2), Quality::Unsupported);
    }
    assert_eq!(snapshot.quality(SensorId::Water, 2), Quality::Unknown);
    exchange(&mut session, 22, 5, &[40]);
    assert_eq!(session.snapshot().value_at(SensorId::Water, 24), Some(0.0));
    for now in 25..1024 {
        assert_eq!(session.poll(now).unwrap(), None);
    }
    begin(&mut session, 1024, 5);
}

#[test]
fn failed_capability_page_does_not_starve_known_channels_or_claim_unsupported() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    exchange(&mut session, 0, 0, &0x0800_0001u32.to_be_bytes());
    begin(&mut session, 22, 0x20);
    assert_eq!(session.poll(524).unwrap(), None); // first-response timeout
    assert_eq!(session.snapshot().quality(SensorId::Oil, 524), Quality::Unknown);
    assert_eq!(session.poll(1523).unwrap(), None); // bounded quiet interval
    // A failed page cannot starve channels already advertised on earlier pages.
    exchange(&mut session, 1524, 5, &[130]);
    exchange(&mut session, 1546, 0x20, &0u32.to_be_bytes());
    assert_eq!(session.snapshot().value_at(SensorId::Water, 1548), Some(90.0));
    assert_eq!(session.counters().timeouts, 1);
}

#[test]
fn timeout_and_malformed_refresh_invalidate_without_corrupting_last_good_statistics() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    exchange(&mut session, 0, 0, &0x0800_0000u32.to_be_bytes());
    exchange(&mut session, 22, 5, &[130]);
    let before = session.snapshot();
    begin(&mut session, 1024, 5);
    session.poll(1526).unwrap();
    let failed = session.snapshot();
    assert_eq!(failed.value_at(SensorId::Water, 1526), None);
    assert_eq!(failed.quality(SensorId::Water, 1526), Quality::TransportError);
    assert_eq!(failed.sensor(SensorId::Water), before.sensor(SensorId::Water));
    begin(&mut session, 2526, 5);
    session.receive(&reply(5, &[140, 0]), 1, 2528).unwrap(); // bad decoder width
    assert_eq!(
        session.snapshot().quality(SensorId::Water, 2528),
        Quality::InvalidPayload
    );
    assert_eq!(
        session.snapshot().sensor(SensorId::Water),
        before.sensor(SensorId::Water)
    );
    exchange(&mut session, 3528, 5, &[140]);
    assert_eq!(session.snapshot().value_at(SensorId::Water, 3530), Some(100.0));
    assert_eq!(session.counters().malformed, 1);
}

#[test]
fn known_wire_loss_blocks_queries_until_a_new_epoch_and_late_replies_cannot_restore_data() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    exchange(&mut session, 0, 0, &0x0800_0000u32.to_be_bytes());
    exchange(&mut session, 22, 5, &[130]);
    let sequence = session.snapshot().metadata[SensorId::Water as usize].sequence;
    session.transport_fault(25).unwrap();
    assert_eq!(session.poll(30), Err(SessionError::Client(ClientError::WireFault)));
    assert_eq!(session.snapshot().value_at(SensorId::Water, 30), None);
    assert_eq!(
        session.change_epoch(1, 31),
        Err(SessionError::Client(ClientError::Epoch))
    );
    session.change_epoch(2, 32).unwrap();
    let tx = begin(&mut session, 32, 0);
    assert_eq!(
        session.tx_completed(tx.token, 34),
        Err(SessionError::Client(ClientError::StaleTx))
    );
    session
        .receive(&reply(0, &0x0800_0000u32.to_be_bytes()), 1, 35)
        .unwrap();
    assert_eq!(session.poll(35).unwrap(), None); // old epoch didn't complete request
    session
        .receive(&reply(0, &0x0800_0000u32.to_be_bytes()), 2, 36)
        .unwrap();
    begin(&mut session, 56, 5);
    session.receive(&reply(5, &[140]), 2, 58).unwrap();
    let after = session.snapshot();
    assert_eq!(after.value_at(SensorId::Water, 58), Some(100.0));
    assert!(after.metadata[SensorId::Water as usize].sequence > sequence);
    assert!(session.reset_statistics(1));
    assert!(!session.reset_statistics(1));
    assert_eq!(session.snapshot().reset_generation, 1);
}

#[test]
fn bad_source_negative_reply_clock_fault_and_tx_timeout_fail_closed() {
    assert!(Session::new(0x700, 1).is_err());
    assert!(Session::new(0x7E8, 0).is_err());
    let mut session = Session::new(0x7E8, 1).unwrap();
    begin(&mut session, 0, 0);
    let other = CanFrame::new(0x7E9, reply(0, &0u32.to_be_bytes()).data()).unwrap();
    session.receive(&other, 1, 2).unwrap();
    assert_eq!(session.poll(3).unwrap(), None);
    session
        .receive(&CanFrame::new(0x7E8, &[3, 0x7F, 1, 0x12]).unwrap(), 1, 4)
        .unwrap();
    assert_eq!(session.counters().negative, 1);
    assert_eq!(session.snapshot().quality(SensorId::Water, 4), Quality::Unknown);
    assert_eq!(session.poll(3), Err(SessionError::Client(ClientError::Clock)));
    assert_eq!(session.snapshot().quality(SensorId::Water, 4), Quality::TransportError);
    session.change_epoch(2, 5).unwrap();
    session.poll(5).unwrap().unwrap(); // no UART completion
    assert_eq!(session.poll(106), Err(SessionError::Client(ClientError::WireFault)));
    assert_eq!(session.snapshot().value_at(SensorId::Water, 106), None);
}

#[test]
fn regressed_tx_completion_invalidates_every_previously_valid_sample() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    exchange(&mut session, 0, 0, &0x0800_0000u32.to_be_bytes());
    exchange(&mut session, 22, 5, &[130]);
    let tx = session.poll(1024).unwrap().unwrap();
    assert_eq!(
        session.tx_completed(tx.token, 23),
        Err(SessionError::Client(ClientError::Clock))
    );
    assert_eq!(
        session.snapshot().quality(SensorId::Water, 1024),
        Quality::TransportError
    );
    assert_eq!(session.snapshot().value_at(SensorId::Water, 1024), None);
}

#[test]
fn sleeping_ecu_retries_are_bounded_and_never_invent_values() {
    let mut session = Session::new(0x7E8, 1).unwrap();
    let mut now = 0;
    for failure in 1..=12u32 {
        begin(&mut session, now, 0);
        now += 502;
        assert_eq!(session.poll(now).unwrap(), None);
        let delay = (250u64 << (failure - 1).min(5)).max(1000);
        assert_eq!(session.poll(now + delay - 1).unwrap(), None);
        now += delay;
        for id in SensorId::ALL {
            assert_eq!(session.snapshot().value_at(id, now), None);
        }
    }
    assert_eq!(session.counters().timeouts, 12);
    exchange(&mut session, now, 0, &0x0800_0000u32.to_be_bytes());
    exchange(&mut session, now + 22, 5, &[130]);
    assert_eq!(session.snapshot().value_at(SensorId::Water, now + 24), Some(90.0));
}
