//! Bounded synthetic transport sequences; no vehicle or adapter I/O.
#[path = "../../test-support/generated.rs"]
mod generated;
use dashboard_core::diagnostics::CanFrame;
use dashboard_core::diagnostics::isotp::{IsoError, IsoTpRx, RxEvent};

fn frame(
    id: u16,
    bytes: &[u8],
) -> CanFrame {
    CanFrame::new(id, bytes).unwrap()
}

#[test]
fn every_payload_length_survives_foreign_traffic_and_sequence_wrap() {
    // Mutation targets: accepting another responder/epoch, stale bytes after
    // abort, sequence wrap and final-frame padding entering the payload.
    for length in 1..=256 {
        let _case = generated::Case::new("payload-length", 0, length);
        let payload: Vec<u8> = (0..length).map(|i| ((i * 173 + length * 31) & 255) as u8).collect();
        let mut receiver = IsoTpRx::new();
        receiver.begin(0x7E8, 17, 0, 60_000).unwrap();
        let mut frames = Vec::new();
        if length <= 7 {
            let mut first = vec![length as u8];
            first.extend_from_slice(&payload);
            first.resize(8, 0xAA);
            frames.push(first);
        } else {
            let mut first = vec![0x10 | ((length >> 8) as u8), length as u8];
            first.extend_from_slice(&payload[..6]);
            frames.push(first);
            for (number, chunk) in payload[6..].chunks(7).enumerate() {
                let mut next = vec![0x20 | ((number + 1) & 15) as u8];
                next.extend_from_slice(chunk);
                next.resize(8, 0xAA);
                frames.push(next);
            }
        }
        let mut completed = None;
        for (index, bytes) in frames.iter().enumerate() {
            let now = index as u64 * 10;
            // An invalid frame from elsewhere must not poison the active reply.
            assert_eq!(receiver.on_frame(&frame(0x7E9, &[0xFF]), 17, now), Ok(RxEvent::Ignored));
            assert_eq!(receiver.on_frame(&frame(0x7E8, &[0xFF]), 16, now), Ok(RxEvent::Ignored));
            match receiver.on_frame(&frame(0x7E8, bytes), 17, now).unwrap() {
                RxEvent::Complete(value) => {
                    assert_eq!(index + 1, frames.len(), "premature length={length} index={index}");
                    completed = Some(value);
                }
                RxEvent::FlowControl(control) => {
                    assert_eq!(control, [0x30, 4, 5, 0, 0, 0, 0, 0]);
                    receiver.flow_control_sent(now + 1).unwrap();
                }
                RxEvent::Pending => assert!(index + 1 < frames.len()),
                other => panic!("length={length} index={index}: {other:?}"),
            }
        }
        assert_eq!(completed.unwrap().as_slice(), payload, "length={length}");
        assert!(!receiver.active());
        assert_eq!(
            receiver.on_frame(&frame(0x7E8, &frames[0]), 17, 1000),
            Ok(RxEvent::Ignored)
        );
    }
}

#[test]
fn every_segment_failure_discards_partial_data_and_allows_a_fresh_transaction() {
    // Interruption at every CF, including wrapped sequence and FC boundaries.
    for cut in 0..36 {
        for fault in 0..4 {
            let _case = generated::Case::new("segment-fault", cut as u64, fault);
            let mut receiver = IsoTpRx::new();
            receiver.begin(0x7E8, 7, 0, 60_000).unwrap();
            assert!(matches!(
                receiver.on_frame(&frame(0x7E8, &[0x11, 0, 0, 1, 2, 3, 4, 5]), 7, 0),
                Ok(RxEvent::FlowControl(_))
            ));
            receiver.flow_control_sent(0).unwrap();
            for index in 0..cut {
                let mut data = [0x55; 8];
                data[0] = 0x20 | ((index + 1) & 15) as u8;
                let event = receiver.on_frame(&frame(0x7E8, &data), 7, index as u64 + 1).unwrap();
                if matches!(event, RxEvent::FlowControl(_)) {
                    receiver.flow_control_sent(index as u64 + 1).unwrap();
                } else {
                    assert_eq!(event, RxEvent::Pending);
                }
            }
            let now = cut as u64 + 1;
            let mut bad = [0x55; 8];
            bad[0] = 0x20 | ((cut + 1) & 15) as u8;
            let error = match fault {
                0 => {
                    bad[0] ^= 1;
                    receiver.on_frame(&frame(0x7E8, &bad), 7, now).unwrap_err()
                }
                1 => receiver.on_frame(&frame(0x7E8, &bad[..1]), 7, now).unwrap_err(),
                2 => receiver.poll_timeout(now + 1001).unwrap_err(),
                _ => receiver.flow_control_sent(now).unwrap_err(),
            };
            assert_eq!(
                error,
                [
                    IsoError::Sequence,
                    IsoError::Length,
                    IsoError::Timeout,
                    IsoError::FlowControl
                ][fault]
            );
            assert!(!receiver.active(), "cut={cut} fault={fault}");
            assert_eq!(
                receiver.on_frame(&frame(0x7E8, &bad), 7, now + 2000),
                Ok(RxEvent::Ignored)
            );
            receiver.begin(0x7E8, 8, now + 3000, now + 4000).unwrap();
            let RxEvent::Complete(value) = receiver
                .on_frame(&frame(0x7E8, &[3, 0x41, 5, 70]), 8, now + 3001)
                .unwrap()
            else {
                panic!("fresh transaction failed cut={cut} fault={fault}");
            };
            assert_eq!(value.as_slice(), [0x41, 5, 70]);
        }
    }
}
