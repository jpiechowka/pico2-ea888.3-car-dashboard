use dashboard_host_validation::diagnostics::CanFrame;
use dashboard_host_validation::diagnostics::isotp::{IsoError, IsoTpRx, RxEvent};
use dashboard_host_validation::diagnostics::longan::{BridgeError, Rx12};

fn frame(bytes: &[u8]) -> CanFrame { CanFrame::new(0x7E8, bytes).unwrap() }

#[test]
fn longan_requires_boundary_and_never_guesses_after_an_error() {
    let bytes = [0, 0, 7, 0xE8, 3, 0x41, 5, 0x46, 0, 0, 0, 0];
    for split in 0..=bytes.len() {
        let mut rx = Rx12::new();
        assert_eq!(rx.push(0), Err(BridgeError::Unsynchronized));
        let epoch = rx.confirm_boundary().unwrap();
        let mut records = Vec::new();
        for chunk in [&bytes[..split], &bytes[split..]] {
            for byte in chunk {
                if let Some(record) = rx.push(*byte).unwrap() {
                    records.push(record);
                }
            }
        }
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id(), 0x7E8);
        assert_eq!(records[0].data(), &bytes[4..]);
        assert_eq!(records[0].epoch(), epoch);
        rx.fault();
        for byte in bytes {
            assert_eq!(rx.push(byte), Err(BridgeError::Unsynchronized));
        }
        assert!(rx.confirm_boundary().unwrap() > epoch);
    }
    for cut in 0..12 {
        let mut rx = Rx12::new();
        rx.confirm_boundary().unwrap();
        for byte in &bytes[..cut] {
            assert_eq!(rx.push(*byte), Ok(None));
        }
        rx.fault();
        rx.confirm_boundary().unwrap();
        for (i, byte) in bytes.into_iter().enumerate() {
            assert_eq!(rx.push(byte).unwrap().is_some(), i == 11);
        }
    }
}

#[test]
fn raw_record_and_tx_widths_are_distinct_and_invalid_ids_desynchronize() {
    assert!(CanFrame::new(0x800, &[0]).is_err());
    assert!(CanFrame::new(0x7E8, &[0; 9]).is_err());
    let request = CanFrame::new(0x7E0, &[2, 1, 5, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        request.longan_tx().unwrap(),
        [0, 0, 7, 0xE0, 0, 0, 2, 1, 5, 0, 0, 0, 0, 0]
    );
    assert!(CanFrame::new(0x7E0, &[2, 1, 5]).unwrap().longan_tx().is_err());
    let mut rx = Rx12::new();
    rx.confirm_boundary().unwrap();
    for _ in 0..11 {
        assert_eq!(rx.push(0xFF), Ok(None));
    }
    assert_eq!(rx.push(0xFF), Err(BridgeError::Identifier));
    assert_eq!(rx.push(0), Err(BridgeError::Unsynchronized));
}

#[test]
fn single_frame_checks_dlc_pci_epoch_source_and_deadline() {
    let mut rx = IsoTpRx::new();
    rx.begin(0x7E8, 1, 10, 100).unwrap();
    assert_eq!(
        rx.on_frame(&CanFrame::new(0x7E9, &[3, 0x41, 5, 70]).unwrap(), 1, 11),
        Ok(RxEvent::Ignored)
    );
    assert_eq!(rx.on_frame(&frame(&[3, 0x41, 5, 70]), 2, 11), Ok(RxEvent::Ignored));
    let RxEvent::Complete(payload) = rx
        .on_frame(&frame(&[3, 0x41, 5, 70, 0xAA, 0xBB, 0, 0]), 1, 100)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(payload.as_slice(), &[0x41, 5, 70]);
    for data in [&[0][..], &[8, 0, 0, 0, 0, 0, 0, 0], &[3, 0x41, 5], &[0x30, 0, 0]] {
        rx.begin(0x7E8, 1, 10, 100).unwrap();
        assert!(rx.on_frame(&frame(data), 1, 11).is_err());
        assert!(!rx.active());
    }
    rx.begin(0x7E8, 1, 10, 100).unwrap();
    assert_eq!(rx.on_frame(&frame(&[3, 0x41, 5, 70]), 1, 101), Err(IsoError::Timeout));
    rx.begin(0x7E8, 1, 10, 100).unwrap();
    assert_eq!(rx.on_frame(&frame(&[3, 0x41, 5, 70]), 1, 9), Err(IsoError::Clock));
}

#[test]
fn segmented_reply_requires_flow_control_and_handles_sequence_wrap_and_blocks() {
    let payload: Vec<u8> = (0..200).collect();
    let mut rx = IsoTpRx::new();
    rx.begin(0x7E8, 7, 0, 1000).unwrap();
    let first = frame(&[0x10, 200, 0, 1, 2, 3, 4, 5]);
    assert_eq!(
        rx.on_frame(&first, 7, 0),
        Ok(RxEvent::FlowControl([0x30, 4, 5, 0, 0, 0, 0, 0]))
    );
    rx.flow_control_sent(1).unwrap();
    for (index, chunk) in payload[6..].chunks(7).enumerate() {
        let mut data = vec![0x20 | ((index + 1) as u8 & 15)];
        data.extend_from_slice(chunk);
        let now = 2 + index as u64 * 5;
        let event = rx.on_frame(&frame(&data), 7, now).unwrap();
        if index == 27 {
            let RxEvent::Complete(actual) = event else { panic!() };
            assert_eq!(actual.as_slice(), payload);
        } else if (index + 1) % 4 == 0 {
            assert!(matches!(event, RxEvent::FlowControl(_)));
            rx.flow_control_sent(now).unwrap();
        } else {
            assert_eq!(event, RxEvent::Pending);
        }
    }
    assert!(!rx.active());
}

#[test]
fn multi_frame_aborts_on_duplicate_sequence_missing_fc_truncation_and_timeouts() {
    for bad in [
        &[0x20, 6, 7, 8, 9][..],
        &[0x22, 6, 7, 8, 9],
        &[0x21, 6],
        &[0x10, 10, 0, 1, 2, 3, 4, 5],
    ] {
        let mut rx = IsoTpRx::new();
        rx.begin(0x7E8, 1, 0, 1000).unwrap();
        rx.on_frame(&frame(&[0x10, 10, 0, 1, 2, 3, 4, 5]), 1, 0).unwrap();
        rx.flow_control_sent(0).unwrap();
        assert!(rx.on_frame(&frame(bad), 1, 1).is_err());
        assert!(!rx.active());
    }
    let mut rx = IsoTpRx::new();
    rx.begin(0x7E8, 1, 0, 1000).unwrap();
    rx.on_frame(&frame(&[0x10, 10, 0, 1, 2, 3, 4, 5]), 1, 0).unwrap();
    assert_eq!(
        rx.on_frame(&frame(&[0x21, 6, 7, 8, 9]), 1, 1),
        Err(IsoError::FlowControl)
    );
    rx.begin(0x7E8, 1, 0, 1000).unwrap();
    rx.on_frame(&frame(&[0x10, 10, 0, 1, 2, 3, 4, 5]), 1, 0).unwrap();
    rx.flow_control_sent(0).unwrap();
    assert_eq!(rx.poll_timeout(1001), Err(IsoError::Timeout));
    assert!(!rx.active());
}

#[test]
fn transport_n_cr_allows_legal_delays_and_expires_after_one_second() {
    for now in [150, 1000, 1001] {
        let mut rx = IsoTpRx::new();
        rx.begin(0x7E8, 1, 0, 5000).unwrap();
        rx.on_frame(&frame(&[0x10, 10, 0, 1, 2, 3, 4, 5]), 1, 0).unwrap();
        rx.flow_control_sent(0).unwrap();
        let result = rx.on_frame(&frame(&[0x21, 6, 7, 8, 9]), 1, now);
        if now <= 1000 {
            assert!(matches!(result, Ok(RxEvent::Complete(_))), "legal N_Cr delay {now}");
        } else {
            assert_eq!(result, Err(IsoError::Timeout));
        }
    }
}

#[test]
fn oversized_reply_gets_overflow_flow_control_and_invalid_begin_is_atomic() {
    let mut rx = IsoTpRx::new();
    assert_eq!(rx.begin(0x800, 1, 0, 1), Err(IsoError::Identifier));
    assert_eq!(rx.begin(0x7E8, 1, 2, 1), Err(IsoError::Clock));
    rx.begin(0x7E8, 1, 0, 1000).unwrap();
    assert_eq!(rx.begin(0x7E8, 2, 0, 1000), Err(IsoError::Busy));
    assert_eq!(
        rx.on_frame(&frame(&[0x11, 1, 0, 0, 0, 0, 0, 0]), 1, 0),
        Ok(RxEvent::Overflow([0x32, 0, 0, 0, 0, 0, 0, 0]))
    );
    assert!(!rx.active());
    rx.begin(0x7E8, 2, u64::MAX - 1, u64::MAX).unwrap();
    assert!(rx.on_frame(&frame(&[1, 0x41]), 2, u64::MAX).is_ok());
}

#[test]
fn every_supported_payload_length_roundtrips_with_padding_and_all_sequence_values() {
    assert!(std::mem::size_of::<IsoTpRx>() <= 336);
    assert!(std::mem::size_of::<RxEvent>() <= 280);
    for length in 1..=256 {
        let expected: Vec<u8> = (0..length).map(|i| (i * 19 + 23) as u8).collect();
        let mut rx = IsoTpRx::new();
        rx.begin(0x7E8, 1, 0, 10000).unwrap();
        if length <= 7 {
            let mut bytes = [0xAA; 8];
            bytes[0] = length as u8;
            bytes[1..=length].copy_from_slice(&expected);
            let RxEvent::Complete(actual) = rx.on_frame(&frame(&bytes), 1, 0).unwrap() else {
                panic!()
            };
            assert_eq!(actual.as_slice(), expected);
        } else {
            let mut first = [0; 8];
            first[0] = 0x10 | (length >> 8) as u8;
            first[1] = length as u8;
            first[2..].copy_from_slice(&expected[..6]);
            assert!(matches!(rx.on_frame(&frame(&first), 1, 0), Ok(RxEvent::FlowControl(_))));
            rx.flow_control_sent(0).unwrap();
            let mut seen = 6;
            for (index, chunk) in expected[6..].chunks(7).enumerate() {
                let mut bytes = [0xAA; 8];
                bytes[0] = 0x20 | ((index + 1) as u8 & 15);
                bytes[1..1 + chunk.len()].copy_from_slice(chunk);
                seen += chunk.len();
                let now = (index + 1) as u64 * 5;
                match rx.on_frame(&frame(&bytes), 1, now).unwrap() {
                    RxEvent::Complete(actual) => {
                        assert_eq!(seen, length);
                        assert_eq!(actual.as_slice(), expected);
                    }
                    RxEvent::FlowControl(_) => {
                        rx.flow_control_sent(now).unwrap();
                        assert!(seen < length);
                    }
                    RxEvent::Pending => assert!(seen < length),
                    other => panic!("unexpected {other:?}"),
                }
            }
        }
        assert!(!rx.active());
    }
}
