use dashboard_host_validation::diagnostics::CanFrame;
use dashboard_host_validation::diagnostics::client::{Client, ClientError, Event, Failure};
use dashboard_host_validation::diagnostics::request::Request;

fn frame(bytes: &[u8]) -> CanFrame { CanFrame::new(0x7E8, bytes).unwrap() }
fn started(request: Request) -> Client {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let tx = client.start(request, 0).unwrap();
    assert!(matches!(client.tx_completed(tx.token, 1), Ok(Event::Ignored)));
    client
}

#[test]
fn one_request_waits_for_current_tx_and_rejects_stale_completions() {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let tx = client.start(Request::Obd(5), 0).unwrap();
    assert_eq!(client.start(Request::Obd(0x42), 0), Err(ClientError::Busy));
    assert!(matches!(
        client.receive(&frame(&[3, 0x41, 5, 70]), 1, 0),
        Ok(Event::Ignored)
    ));
    client.tx_completed(tx.token, 1).unwrap();
    assert_eq!(client.tx_completed(tx.token, 1), Err(ClientError::StaleTx));
    let Event::Positive(reply) = client.receive(&frame(&[3, 0x41, 5, 70]), 1, 2).unwrap() else {
        panic!()
    };
    assert_eq!(reply.request, Request::Obd(5));
    assert_eq!(reply.bytes.as_slice(), &[70]);
    assert_eq!(reply.received_at_ms, 2);
    assert_eq!(reply.epoch, 1);
    assert_eq!(reply.ecu, 0x7E8);
    let next = client.start(Request::Obd(5), 3).unwrap();
    assert_eq!(client.tx_completed(tx.token, 3), Err(ClientError::StaleTx));
    client.tx_completed(next.token, 4).unwrap();
    let Event::Positive(later) = client.receive(&frame(&[3, 0x41, 5, 71]), 1, 5).unwrap() else {
        panic!()
    };
    assert!(later.generation > reply.generation);
}

#[test]
fn foreign_reply_does_not_extend_deadline_or_pollute_matching_payload() {
    let mut client = started(Request::Obd(5));
    for now in 2..=500 {
        assert_eq!(client.receive(&frame(&[3, 0x41, 0x0F, 70]), 1, now), Ok(Event::Ignored));
    }
    assert!(matches!(
        client.tick(502),
        Ok(Event::Failed {
            reason: Failure::ResponseTimeout,
            ..
        })
    ));
    assert_eq!(client.receive(&frame(&[3, 0x41, 5, 70]), 1, 503), Ok(Event::Ignored));
}

#[test]
fn late_foreign_single_frame_cannot_abort_an_active_segmented_reply_or_its_fc() {
    for acknowledge_first in [false, true] {
        let mut client = started(Request::ReadDid(0xF190));
        let Event::Transmit(fc) = client
            .receive(&frame(&[0x10, 10, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 2)
            .unwrap()
        else {
            panic!()
        };
        if acknowledge_first {
            client.tx_completed(fc.token, 3).unwrap();
        }
        assert_eq!(client.receive(&frame(&[3, 0x41, 5, 70]), 1, 4), Ok(Event::Ignored));
        assert_eq!(client.receive(&frame(&[3, 0x7F, 1, 0x78]), 1, 4), Ok(Event::Ignored));
        if !acknowledge_first {
            client.tx_completed(fc.token, 5).unwrap();
        }
        assert!(matches!(
            client.receive(&frame(&[0x21, 4, 5, 6, 7]), 1, 6),
            Ok(Event::Positive(_))
        ));
    }
}

#[test]
fn foreign_single_frame_does_not_extend_consecutive_frame_deadline() {
    let mut client = started(Request::ReadDid(0xF190));
    let Event::Transmit(fc) = client
        .receive(&frame(&[0x10, 10, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 2)
        .unwrap()
    else {
        panic!()
    };
    client.tx_completed(fc.token, 3).unwrap();
    assert_eq!(client.receive(&frame(&[3, 0x41, 5, 70]), 1, 1000), Ok(Event::Ignored));
    assert!(matches!(
        client.tick(1004),
        Ok(Event::Failed {
            reason: Failure::Transport(_),
            ..
        })
    ));
}

#[test]
fn request_and_previous_block_tokens_cannot_authorize_later_flow_control() {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let request = client.start(Request::ReadDid(0xF190), 0).unwrap();
    client.tx_completed(request.token, 1).unwrap();
    let Event::Transmit(first) = client
        .receive(&frame(&[0x10, 41, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 2)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(client.tx_completed(request.token, 10_000), Err(ClientError::StaleTx));
    client.tx_completed(first.token, 3).unwrap();
    for sequence in 1..=3 {
        assert_eq!(
            client.receive(&frame(&[0x20 | sequence, 1, 2, 3, 4, 5, 6, 7]), 1, 4),
            Ok(Event::Pending)
        );
    }
    let Event::Transmit(second) = client.receive(&frame(&[0x24, 1, 2, 3, 4, 5, 6, 7]), 1, 5).unwrap() else {
        panic!()
    };
    assert_ne!(first.token, second.token);
    assert_eq!(client.tx_completed(first.token, 10_000), Err(ClientError::StaleTx));
    assert_eq!(client.tx_completed(request.token, 10_000), Err(ClientError::StaleTx));
    client.tx_completed(second.token, 6).unwrap();
    assert!(matches!(
        client.receive(&frame(&[0x25, 1, 2, 3, 4, 5, 6, 7]), 1, 7),
        Ok(Event::Positive(_))
    ));
}

#[test]
fn pending_response_has_finite_count_and_cannot_move_the_hard_deadline() {
    let mut client = started(Request::ReadDid(0xF190));
    for now in [100, 900, 1700] {
        assert_eq!(
            client.receive(&frame(&[3, 0x7F, 0x22, 0x78]), 1, now),
            Ok(Event::Pending)
        );
    }
    assert!(matches!(
        client.receive(&frame(&[3, 0x7F, 0x22, 0x78]), 1, 1800),
        Ok(Event::Failed {
            reason: Failure::PendingLimit,
            ..
        })
    ));
    let mut client = started(Request::ReadDid(0xF190));
    assert_eq!(
        client.receive(&frame(&[3, 0x7F, 0x22, 0x78]), 1, 400),
        Ok(Event::Pending)
    );
    assert!(matches!(
        client.tick(1401),
        Ok(Event::Failed {
            reason: Failure::ResponseTimeout,
            ..
        })
    ));
    let mut client = started(Request::ReadDid(0xF190));
    let Event::Transmit(fc) = client
        .receive(&frame(&[0x10, 200, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 400)
        .unwrap()
    else {
        panic!()
    };
    client.tx_completed(fc.token, 401).unwrap();
    // Valid progress keeps N_Cr alive, but cannot extend the request's total budget.
    for sequence in 1..=5 {
        let now = u64::from(sequence) * 900;
        let event = client
            .receive(&frame(&[0x20 | sequence, 1, 2, 3, 4, 5, 6, 7]), 1, now)
            .unwrap();
        if sequence == 4 {
            let Event::Transmit(fc) = event else { panic!() };
            client.tx_completed(fc.token, now).unwrap();
        } else {
            assert_eq!(event, Event::Pending);
        }
    }
    assert!(matches!(
        client.tick(5001),
        Ok(Event::Failed {
            reason: Failure::TotalTimeout,
            ..
        })
    ));
}

#[test]
fn oversized_first_response_sends_overflow_before_finishing_and_tokens_stay_bounded() {
    assert!(std::mem::size_of::<Client>() <= 512);
    assert!(std::mem::size_of::<Event>() <= 320);
    let mut client = started(Request::ReadDid(0xF190));
    let Event::Transmit(fc) = client
        .receive(&frame(&[0x11, 1, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 490)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(fc.frame.data()[0], 0x32);
    assert!(matches!(
        client.tx_completed(fc.token, 550),
        Ok(Event::Failed {
            reason: Failure::Oversized,
            ..
        })
    ));
    assert_eq!(client.tx_completed(fc.token, 551), Err(ClientError::StaleTx));
    assert!(client.start(Request::Obd(5), 552).is_ok());
}

#[test]
fn flow_control_tokens_cannot_be_reused_after_abort_or_across_epochs() {
    let mut client = started(Request::ReadDid(0xF190));
    let Event::Transmit(fc) = client
        .receive(&frame(&[0x10, 10, 0x62, 0xF1, 0x90, 1, 2, 3]), 1, 2)
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        client.receive(&frame(&[0x21, 4, 5, 6, 7]), 1, 3),
        Ok(Event::Failed {
            reason: Failure::Transport(_),
            ..
        })
    ));
    client.change_epoch(2, 4).unwrap();
    let next = client.start(Request::ReadDid(0xF190), 4).unwrap();
    client.tx_completed(next.token, 5).unwrap();
    let Event::Transmit(new_fc) = client
        .receive(&frame(&[0x10, 10, 0x62, 0xF1, 0x90, 1, 2, 3]), 2, 6)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(client.tx_completed(fc.token, 7), Err(ClientError::StaleTx));
    client.tx_completed(new_fc.token, 7).unwrap();
    let Event::Positive(reply) = client.receive(&frame(&[0x21, 4, 5, 6, 7]), 2, 8).unwrap() else {
        panic!()
    };
    assert_eq!(reply.bytes.as_slice(), &[1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(client.change_epoch(2, 9), Err(ClientError::Epoch));
}

#[test]
fn tx_timeout_requires_new_transport_epoch_and_malformed_or_negative_replies_are_not_samples() {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let tx = client.start(Request::Obd(5), 0).unwrap();
    assert!(matches!(
        client.tick(101),
        Ok(Event::Failed {
            reason: Failure::TxTimeout,
            ..
        })
    ));
    assert_eq!(client.start(Request::Obd(5), 102), Err(ClientError::WireFault));
    assert_eq!(client.tx_completed(tx.token, 102), Err(ClientError::StaleTx));
    client.change_epoch(2, 103).unwrap();
    assert!(client.start(Request::Obd(5), 104).is_ok());
    let mut client = started(Request::Obd(5));
    assert!(matches!(
        client.receive(&frame(&[3, 0x7F, 1, 0x12]), 1, 2),
        Ok(Event::Negative { nrc: 0x12, .. })
    ));
    let mut client = started(Request::Obd(5));
    assert!(matches!(
        client.receive(&frame(&[4, 0x7F, 1, 0x78, 0]), 1, 2),
        Ok(Event::Failed {
            reason: Failure::Reply(_),
            ..
        })
    ));
    let mut client = started(Request::Obd(5));
    assert_eq!(client.tick(0), Err(ClientError::Clock));
    assert!(Client::new(0x800, 0x7E8, 1).is_err());
    assert!(Client::new(0x7E0, 0x7E8, 0).is_err());
}
