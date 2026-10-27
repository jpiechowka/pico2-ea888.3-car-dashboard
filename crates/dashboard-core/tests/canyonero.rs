//! Synthetic CANyonero INFO regressions; no fixture is captured adapter traffic.

use dashboard_core::ble::canyonero::{
    CleanupOutcome,
    CleanupResult,
    DecodeError,
    DecodeStatus,
    Epoch,
    Event,
    FrameDecoder,
    Outcome,
    Reason,
    Rejected,
    Session,
    Stage,
    TransportDomain,
    TransportFault,
    parse_frame,
    validate_frame,
};

/// Synthetic operation identity with no peer address or private device data.
const EPOCH: Epoch = Epoch {
    connection: 1,
    operation: 1,
    maintenance: 0,
};

/// Starting an admitted identity attempt must enter bounded connection work.
#[test]
fn session_enters_connecting_once_started() {
    assert_eq!(Session::new().start(EPOCH, 100), Ok(Stage::Connecting));
}

/// Removing frame completion must fail this synthetic identity admission test.
#[test]
fn complete_info_frame_is_recognized() {
    let mut decoder = FrameDecoder::new();
    assert_eq!(
        decoder.push(b"\x1f\x91\x00\x09A\nB\nC\nD\nE"),
        Ok(DecodeStatus::Complete)
    );
}

/// Complete parsing borrows original field bytes and never accepts a complete-looking prefix with suffixes.
#[test]
fn complete_frame_parser_borrows_fields_and_requires_exact_input() {
    let bytes = frame(0x91, "Synthetic\nModel\nH2\nTEST\nβ1".as_bytes());
    let identity = parse_frame(&bytes).unwrap();
    assert_eq!(identity.vendor, Some("Synthetic"));
    assert_eq!(identity.firmware, Some("β1"));
    assert_eq!(identity.vendor.unwrap().as_ptr(), bytes[4..].as_ptr());
    assert!(!identity.trailing_lf);
    for end in 0..bytes.len() {
        assert_eq!(parse_frame(&bytes[..end]), Err(DecodeError::Truncated));
    }
    for suffix in [b"\x1f".as_slice(), b"garbage", bytes.as_slice()] {
        let mut combined = bytes.clone();
        combined.extend_from_slice(suffix);
        assert_eq!(parse_frame(&combined), Err(DecodeError::TrailingData));
    }
    let empty = parse_frame(b"\x1f\x91\0\x04\n\n\n\n").unwrap();
    assert_eq!(empty.firmware, None);
    assert!(!empty.trailing_lf);
    let extra = parse_frame(b"\x1f\x91\0\x05\n\n\n\n\n").unwrap();
    assert_eq!(extra.firmware, None);
    assert!(extra.trailing_lf);
}

/// Validation-only checks require EOF-complete framing and preserve suffix and explicit remote failures.
#[test]
fn validation_without_field_views_rejects_every_incomplete_prefix() {
    let bytes = frame(0x91, "Synthetic\nModel\nH2\nTEST\nβ1".as_bytes());
    assert_eq!(validate_frame(&bytes), Ok(()));
    for end in 0..bytes.len() {
        assert_eq!(validate_frame(&bytes[..end]), Err(DecodeError::Truncated));
    }
    for suffix in [b"\x1f".as_slice(), bytes.as_slice()] {
        let mut combined = bytes.clone();
        combined.extend_from_slice(suffix);
        assert_eq!(validate_frame(&combined), Err(DecodeError::TrailingData));
    }
    for code in [0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xEF] {
        assert_eq!(validate_frame(&[0x1F, code, 0, 9]), Err(DecodeError::Remote(code)));
    }
}

/// Header failures keep their original precedence, even when several malformed conditions coexist.
#[test]
fn complete_parser_preserves_hostile_header_and_text_errors() {
    for (bytes, error) in [
        (b"x\x91\xff\xff".as_slice(), DecodeError::Marker),
        (b"\x1f\xe5\xff\xff".as_slice(), DecodeError::UnsupportedSize),
        (b"\x1f\xe5\0\x09".as_slice(), DecodeError::Remote(0xE5)),
        (b"\x1f\x80\0\x09".as_slice(), DecodeError::UnexpectedType(0x80)),
        (b"\x1f\x91\0\x04bad!extra".as_slice(), DecodeError::TrailingData),
        (b"\x1f\x91\0\x04bad!".as_slice(), DecodeError::FieldCount),
        (b"\x1f\x91\0\x05\n\n\n\n\xff".as_slice(), DecodeError::InvalidUtf8),
        (b"\x1f\x91\0\x05\n\n\n\n\0".as_slice(), DecodeError::ControlCharacter),
        (
            b"\x1f\x91\0\x06\n\n\n\n\xc2\x85".as_slice(),
            DecodeError::ControlCharacter,
        ),
    ] {
        assert_eq!(parse_frame(bytes), Err(error));
        assert_eq!(validate_frame(bytes), Err(error));
    }
}

/// Complete parsing matches original push-plus-EOF admission across mutations and all valid split points.
#[test]
fn complete_parser_matches_incremental_input_boundaries_and_mutations() {
    let mut maximum = vec![b'A'; 64];
    for width in [64, 64, 44, 1] {
        maximum.push(b'\n');
        maximum.extend(vec![b'B'; width]);
    }
    for bytes in [
        frame(0x91, "V\nM\nH\nS\nβ".as_bytes()),
        frame(0x91, b"\n\n\n\n"),
        frame(0x91, b"A\nB\nC\nD\nE\n"),
        frame(0x91, &maximum),
    ] {
        for split in 0..=bytes.len() {
            equivalent_complete_parse(&bytes[..split]);
            let mut decoder = FrameDecoder::new();
            decoder.push(&bytes[..split]).unwrap();
            decoder.push(&bytes[split..]).unwrap();
            assert_eq!(parse_frame(&bytes), Ok(decoder.identity().unwrap()));
        }
        for offset in 0..bytes.len() {
            for byte in 0..=u8::MAX {
                let mut mutated = bytes.clone();
                mutated[offset] = byte;
                equivalent_complete_parse(&mutated);
            }
        }
    }
}

/// Checks borrowed parsing/validation against owned one-shot decoding and EOF for supplied `bytes`.
fn equivalent_complete_parse(bytes: &[u8]) {
    assert_eq!(validate_frame(bytes), parse_frame(bytes).map(|_| ()));
    let mut decoder = FrameDecoder::new();
    let result = decoder.push(bytes).and_then(|_| decoder.finish());
    match result {
        Ok(DecodeStatus::Complete) => assert_eq!(parse_frame(bytes), Ok(decoder.identity().unwrap())),
        Ok(DecodeStatus::NeedMore) => panic!("EOF returned incomplete progress"),
        Err(error) => assert_eq!(parse_frame(bytes), Err(error)),
    }
}

/// Returns a synthetic frame of `kind` with the declared length of `body` bytes.
/// Panics if the fixture body exceeds the wire's u16 length.
fn frame(
    kind: u8,
    body: &[u8],
) -> Vec<u8> {
    let mut bytes = vec![0x1F, kind];
    bytes.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

/// Header and UTF-8 boundaries must neither lose bytes nor alter field order.
#[test]
fn every_split_and_single_byte_chunks_preserve_identity() {
    let bytes = frame(0x91, "Synthetic\nModel\nH2\nTEST\nβ1".as_bytes());
    for split in 0..=bytes.len() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes[..split]).unwrap();
        assert_eq!(decoder.push(&bytes[split..]), Ok(DecodeStatus::Complete));
        let identity = decoder.identity().unwrap();
        assert_eq!(
            (
                identity.vendor,
                identity.model,
                identity.hardware,
                identity.serial,
                identity.firmware
            ),
            (Some("Synthetic"), Some("Model"), Some("H2"), Some("TEST"), Some("β1"))
        );
        assert!(!identity.trailing_lf);
        assert_eq!(decoder.raw(), bytes);
    }
    let mut decoder = FrameDecoder::new();
    for byte in &bytes {
        decoder.push(core::slice::from_ref(byte)).unwrap();
    }
    assert!(decoder.identity().is_some());
}

/// Empty firmware must not be confused with the optional sixth empty segment.
#[test]
fn empty_fields_and_one_terminal_lf_are_distinguished() {
    for (body, trailing) in [(b"\n\n\n\n".as_slice(), false), (b"\n\n\n\n\n".as_slice(), true)] {
        let mut decoder = FrameDecoder::new();
        assert_eq!(decoder.push(&frame(0x91, body)), Ok(DecodeStatus::Complete));
        let identity = decoder.identity().unwrap();
        assert_eq!(
            (
                identity.vendor,
                identity.model,
                identity.hardware,
                identity.serial,
                identity.firmware
            ),
            (None, None, None, None, None)
        );
        assert_eq!(identity.trailing_lf, trailing);
    }
}

/// Strict text policy must reject malformed bytes without marker resynchronization.
#[test]
fn malformed_text_is_explicit_and_sticky() {
    for (body, error) in [
        (b"".as_slice(), DecodeError::FieldCount),
        (b"A\nB\nC\nD", DecodeError::FieldCount),
        (b"A\nB\nC\nD\nE\nF", DecodeError::FieldCount),
        (b"A\nB\nC\nD\nE\n\n", DecodeError::FieldCount),
        (b"A\nB\nC\nD\n\xff", DecodeError::InvalidUtf8),
        (b"A\nB\nC\nD\n\0", DecodeError::ControlCharacter),
        (b"A\nB\nC\nD\n\x1f", DecodeError::ControlCharacter),
        (b"A\nB\nC\nD\n\x7f", DecodeError::ControlCharacter),
        (b"A\nB\nC\nD\n\xc2\x85", DecodeError::ControlCharacter),
    ] {
        let mut decoder = FrameDecoder::new();
        let bytes = frame(0x91, body);
        assert_eq!(parse_frame(&bytes), Err(error));
        assert_eq!(decoder.push(&bytes), Err(error));
        assert_eq!(decoder.push(b"anything"), Err(error));
        assert_eq!(decoder.raw(), bytes);
        assert!(decoder.identity().is_none());
    }
}

/// Field and overall caps are independent, with no silent truncation for oversized input.
#[test]
fn maximum_frame_and_field_bounds_are_enforced() {
    let mut body = vec![b'A'; 64];
    for width in [64, 64, 44, 1] {
        body.push(b'\n');
        body.extend(vec![b'B'; width]);
    }
    let bytes = frame(0x91, &body);
    assert_eq!(bytes.len(), 245);
    assert_eq!(parse_frame(&bytes).unwrap().vendor.unwrap().len(), 64);
    let mut decoder = FrameDecoder::new();
    assert_eq!(decoder.push(&bytes), Ok(DecodeStatus::Complete));
    assert_eq!(decoder.identity().unwrap().vendor.unwrap().len(), 64);
    assert!(!decoder.evidence_truncated());
    body.push(b'C');
    assert_eq!(parse_frame(&frame(0x91, &body)), Err(DecodeError::UnsupportedSize));
    let mut decoder = FrameDecoder::new();
    assert_eq!(decoder.push(&frame(0x91, &body)), Err(DecodeError::UnsupportedSize));
    assert!(decoder.evidence_truncated());
    assert_eq!(decoder.observed_bytes(), 246);
    assert_eq!(decoder.raw().len(), 245);
    let mut body = vec![b'A'; 65];
    body.extend_from_slice(b"\nB\nC\nD\nE");
    assert_eq!(parse_frame(&frame(0x91, &body)), Err(DecodeError::FieldTooLong));
    assert_eq!(
        FrameDecoder::new().push(&frame(0x91, &body)),
        Err(DecodeError::FieldTooLong)
    );
    let mut decoder = FrameDecoder::new();
    assert_eq!(decoder.push(b"\x1f\x91\xff\xff"), Err(DecodeError::UnsupportedSize));
    assert_eq!(decoder.declared_body_bytes(), Some(65535));
}

/// Explicit negative and unrelated messages must fail before waiting for their bodies.
#[test]
fn negative_and_wrong_reply_types_never_succeed() {
    for kind in [0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xEF] {
        let mut decoder = FrameDecoder::new();
        let bytes = frame(kind, b"opaque diagnostic");
        assert_eq!(decoder.push(&bytes), Err(DecodeError::Remote(kind)));
        assert_eq!(decoder.raw(), bytes);
        assert_eq!(FrameDecoder::new().push(&bytes[..4]), Err(DecodeError::Remote(kind)));
    }
    for kind in [0x00, 0x11, 0x80, 0x90, 0x92, 0xFF] {
        assert_eq!(
            FrameDecoder::new().push(&frame(kind, b"")),
            Err(DecodeError::UnexpectedType(kind))
        );
    }
}

/// A duplicate or malformed suffix cannot leave a previously decoded success available.
#[test]
fn duplicate_or_suffix_revokes_identity_at_every_split() {
    let bytes = frame(0x91, b"A\nB\nC\nD\nE");
    for suffix in [bytes.as_slice(), b"garbage".as_slice(), b"\x1f".as_slice()] {
        let mut combined = bytes.clone();
        combined.extend_from_slice(suffix);
        for split in 0..=combined.len() {
            let mut decoder = FrameDecoder::new();
            let _ = decoder.push(&combined[..split]);
            assert_eq!(decoder.push(&combined[split..]), Err(DecodeError::TrailingData));
            assert!(decoder.identity().is_none());
        }
    }
    assert_eq!(FrameDecoder::new().push(b"x\x1f\x91\0\0"), Err(DecodeError::Marker));
}

/// EOF at every incomplete position retains exactly the bytes that reached the decoder.
#[test]
fn eof_preserves_partial_evidence() {
    let bytes = frame(0x91, b"A\nB\nC\nD\nE");
    for cut in 0..bytes.len() {
        let mut decoder = FrameDecoder::new();
        assert_eq!(decoder.push(&bytes[..cut]), Ok(DecodeStatus::NeedMore));
        assert_eq!(decoder.finish(), Err(DecodeError::Truncated));
        assert_eq!(decoder.raw(), &bytes[..cut]);
    }
}

/// Returns a synthetic session at SendingInfo at 104 ms without taking its request.
fn sending() -> Session {
    let mut session = Session::new();
    session.start(EPOCH, 100).unwrap();
    for (time, event) in [
        (101, Event::Connected),
        (102, Event::Discovered),
        (103, Event::ParametersReady),
        (104, Event::ChannelOpened),
    ] {
        session.event(EPOCH, time, event).unwrap();
    }
    session
}

/// Returns a synthetic session at AwaitingInfo at 106 ms after taking/submitting one request.
fn awaiting() -> Session {
    let mut session = sending();
    let request = session.take_request(EPOCH, 105).unwrap();
    assert_eq!(request.epoch(), EPOCH);
    assert_eq!(request.bytes(), &[0x1F, 0x11, 0, 0]);
    session.event(EPOCH, 106, Event::InfoSent).unwrap();
    session
}

/// A valid reply is only pending evidence until normal cleanup, and cannot be resent.
#[test]
fn identity_success_requires_whole_send_and_normal_cleanup() {
    let mut session = awaiting();
    assert_eq!(
        session.take_request(EPOCH, 107).unwrap_err(),
        Rejected::RequestAlreadyTaken
    );
    assert_eq!(
        session.receive(EPOCH, 108, &frame(0x91, b"A\nB\nC\nD\nE")),
        Ok(DecodeStatus::Complete)
    );
    assert_eq!(session.stage(), Stage::Closing);
    assert_eq!(session.outcome(), None);
    assert_eq!(session.cleanup(), CleanupOutcome::Pending);
    assert_eq!(
        session.event(EPOCH, 109, Event::CleanupComplete(CleanupResult::Clean)),
        Ok(Stage::Finished)
    );
    assert_eq!(session.outcome(), Some(Outcome::Success));
    assert_eq!(
        session.receive(EPOCH, 110, &frame(0x91, b"X\nX\nX\nX\nX")),
        Err(Rejected::NotActive)
    );
    assert_eq!(session.decoder().identity().unwrap().vendor, Some("A"));
}

/// Fast reply may race send completion, but cannot conceal a failed or unfinished send.
#[test]
fn early_reply_waits_for_send_completion_and_send_failure_wins() {
    for fail in [false, true] {
        let mut session = sending();
        session.take_request(EPOCH, 105).unwrap();
        session.receive(EPOCH, 106, &frame(0x91, b"A\nB\nC\nD\nE")).unwrap();
        assert_eq!(session.stage(), Stage::SendingInfo);
        assert_eq!(session.outcome(), None);
        let event = if fail {
            Event::TransportFailed(TransportFault {
                domain: TransportDomain::Local,
                code: None,
                partial_send: true,
            })
        } else {
            Event::InfoSent
        };
        session.event(EPOCH, 107, event).unwrap();
        assert_eq!(session.stage(), Stage::Closing);
        session
            .event(EPOCH, 108, Event::CleanupComplete(CleanupResult::Clean))
            .unwrap();
        if fail {
            assert!(matches!(session.outcome(), Some(Outcome::Failed(_))));
        } else {
            assert_eq!(session.outcome(), Some(Outcome::Success));
        }
    }
}

/// Cached admission preserves early/late reply, EOF and cleanup decisions for valid,
/// partial, suffix-corrupted, invalid-text and every explicit remote-error frame.
#[test]
fn cached_session_completion_preserves_protocol_faults_and_cleanup() {
    let valid = frame(0x91, b"A\nB\nC\nD\nE");
    let mut cases: Vec<Vec<u8>> = (1..=valid.len()).map(|end| valid[..end].to_vec()).collect();
    let mut suffix = valid.clone();
    suffix.push(0x1F);
    cases.push(suffix);
    cases.push(frame(0x91, b"A\nB\nC\nD\n\xff"));
    cases.push(frame(0x91, b"A\nB\nC\nD\n\0"));
    for code in [0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xEF] {
        cases.push(frame(code, b""));
    }
    for bytes in cases {
        let parsed = parse_frame(&bytes);
        for early in [false, true] {
            for clean in [false, true] {
                let mut session = if early { sending() } else { awaiting() };
                if early {
                    session.take_request(EPOCH, 105).unwrap();
                }
                let _ = session.receive(EPOCH, 107, &bytes);
                assert_eq!(session.decoder().identity().is_some(), parsed.is_ok());
                if early && session.fault().is_none() {
                    session.event(EPOCH, 108, Event::InfoSent).unwrap();
                }
                let eof = session.event(EPOCH, 109, Event::Eof);
                assert_eq!(eof.is_ok(), parsed.is_ok());
                if let Err(error) = parsed {
                    assert_eq!(session.fault().unwrap().reason, Reason::Protocol(error));
                }
                assert_eq!(session.outcome(), None);
                let cleanup = if clean {
                    CleanupResult::Clean
                } else {
                    CleanupResult::Failed
                };
                session.event(EPOCH, 110, Event::CleanupComplete(cleanup)).unwrap();
                assert_eq!(session.outcome() == Some(Outcome::Success), parsed.is_ok() && clean);
                if let Err(error) = parsed {
                    assert_eq!(session.fault().unwrap().reason, Reason::Protocol(error));
                }
            }
        }
    }
}

/// A second frame during teardown must revoke success and preserve its raw evidence.
#[test]
fn late_duplicate_before_cleanup_is_a_failure() {
    let mut session = awaiting();
    let bytes = frame(0x91, b"A\nB\nC\nD\nE");
    session.receive(EPOCH, 107, &bytes).unwrap();
    assert_eq!(session.receive(EPOCH, 108, &bytes), Err(Rejected::Faulted));
    assert_eq!(
        session.fault().unwrap().reason,
        Reason::Protocol(DecodeError::TrailingData)
    );
    assert!(session.decoder().identity().is_none());
    assert_eq!(session.decoder().raw().len(), bytes.len() * 2);
    session
        .event(EPOCH, 109, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert!(matches!(session.outcome(), Some(Outcome::Failed(_))));
}

/// Each stage has a fixed deadline; ticks and reply bytes cannot refresh its budget.
#[test]
fn stage_deadlines_include_boundary_and_do_not_slide() {
    let mut session = Session::new();
    session.start(EPOCH, 100).unwrap();
    assert_eq!(session.event(EPOCH, 8_100, Event::Connected), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().stage, Stage::Connecting);
    let mut session = Session::new();
    session.start(EPOCH, 100).unwrap();
    session.event(EPOCH, 7_000, Event::Connected).unwrap();
    assert_eq!(session.deadline_ms(), Some(37_000));
    session.event(EPOCH, 36_999, Event::Tick).unwrap();
    assert_eq!(session.event(EPOCH, 37_000, Event::Discovered), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().stage, Stage::Discovering);
    let mut session = Session::new();
    session.start(EPOCH, 100).unwrap();
    session.event(EPOCH, 7_000, Event::Connected).unwrap();
    session.event(EPOCH, 36_999, Event::Discovered).unwrap();
    assert_eq!(session.deadline_ms(), Some(51_999));
    session.event(EPOCH, 51_998, Event::Tick).unwrap();
    assert_eq!(
        session.event(EPOCH, 51_999, Event::ParametersReady),
        Err(Rejected::Faulted)
    );
    assert_eq!(session.fault().unwrap().stage, Stage::TransportParameters);
    assert_eq!(session.fault().unwrap().reason, Reason::TimedOut);

    let mut session = Session::new();
    session.start(EPOCH, 100).unwrap();
    session.event(EPOCH, 101, Event::Connected).unwrap();
    session.event(EPOCH, 102, Event::Discovered).unwrap();
    session.event(EPOCH, 103, Event::ParametersReady).unwrap();
    assert_eq!(
        session.event(EPOCH, 5_103, Event::ChannelOpened),
        Err(Rejected::Faulted)
    );
    assert_eq!(session.fault().unwrap().stage, Stage::OpeningCoc);

    let mut session = sending();
    session.take_request(EPOCH, 2_103).unwrap();
    assert_eq!(session.event(EPOCH, 2_104, Event::InfoSent), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().stage, Stage::SendingInfo);

    let mut session = awaiting();
    assert_eq!(session.receive(EPOCH, 2_105, b"\x1f"), Ok(DecodeStatus::NeedMore));
    assert_eq!(session.deadline_ms(), Some(2_106));
    assert_eq!(session.receive(EPOCH, 2_106, b"\x91"), Err(Rejected::Faulted));
    assert_eq!(session.decoder().raw(), &[0x1F]);
    assert_eq!(session.fault().unwrap().stage, Stage::AwaitingInfo);
}

/// Cancellation and parking invalidate every await, including pending success cleanup.
#[test]
fn cancellation_and_parking_at_every_stage_never_turn_green() {
    for stop in [Event::Cancel, Event::Park] {
        for stage_index in 0..7 {
            let mut session = Session::new();
            session.start(EPOCH, 100).unwrap();
            for event in [
                Event::Connected,
                Event::Discovered,
                Event::ParametersReady,
                Event::ChannelOpened,
            ]
            .iter()
            .take(stage_index.min(4))
            {
                session.event(EPOCH, 101, *event).unwrap();
            }
            if stage_index >= 5 {
                session.take_request(EPOCH, 102).unwrap();
                session.event(EPOCH, 103, Event::InfoSent).unwrap();
            }
            if stage_index >= 6 {
                session.receive(EPOCH, 104, &frame(0x91, b"A\nB\nC\nD\nE")).unwrap();
            }
            let original_stage = session.stage();
            session.event(EPOCH, 105, stop).unwrap();
            let fault = session.fault().unwrap();
            assert_eq!(fault.stage, original_stage);
            assert_eq!(
                fault.reason,
                if stop == Event::Cancel {
                    Reason::Cancelled
                } else {
                    Reason::Parked
                }
            );
            assert_eq!(session.event(EPOCH, 106, Event::InfoSent), Err(Rejected::Faulted));
            assert_eq!(
                session.receive(EPOCH, 106, &frame(0x91, b"A\nB\nC\nD\nE")),
                Err(Rejected::Faulted)
            );
            assert_eq!(session.take_request(EPOCH, 106).unwrap_err(), Rejected::Faulted);
            session
                .event(EPOCH, 107, Event::CleanupComplete(CleanupResult::Clean))
                .unwrap();
            assert_eq!(session.outcome(), Some(Outcome::Failed(fault)));
        }
    }
}

/// Transport failure preserves its original domain/code and does not become a zero value.
#[test]
fn transport_failure_and_cleanup_failure_remain_separate() {
    let mut session = awaiting();
    session.receive(EPOCH, 107, b"\x1f\x91\0\x09A").unwrap();
    let error = TransportFault {
        domain: TransportDomain::Hci,
        code: Some(0x08),
        partial_send: false,
    };
    session.event(EPOCH, 108, Event::TransportFailed(error)).unwrap();
    let fault = session.fault().unwrap();
    assert_eq!(fault.reason, Reason::Transport(error));
    session
        .event(EPOCH, 109, Event::CleanupComplete(CleanupResult::Failed))
        .unwrap();
    assert_eq!(session.cleanup(), CleanupOutcome::Failed);
    assert_eq!(session.outcome(), Some(Outcome::Failed(fault)));
    assert_eq!(session.decoder().raw(), b"\x1f\x91\0\x09A");
    let next = Epoch {
        connection: 2,
        operation: 2,
        maintenance: 0,
    };
    assert_eq!(session.start(next, 110), Err(Rejected::CleanupUnconfirmed));
    session
        .event(EPOCH, 111, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert_eq!(session.cleanup(), CleanupOutcome::Clean);
    assert_eq!(session.outcome(), Some(Outcome::Failed(fault)));
    assert_eq!(session.start(next, 112), Ok(Stage::Connecting));
    assert!(session.decoder().raw().is_empty());
}

/// A missed cleanup deadline cannot be erased by a late clean observation or repeated fault.
#[test]
fn cleanup_timeout_is_terminal_but_late_cleanup_is_recorded() {
    let mut session = awaiting();
    session.receive(EPOCH, 107, &frame(0x91, b"A\nB\nC\nD\nE")).unwrap();
    assert_eq!(session.event(EPOCH, 5_107, Event::Tick), Err(Rejected::Faulted));
    assert_eq!(session.stage(), Stage::Finished);
    assert_eq!(session.cleanup(), CleanupOutcome::TimedOut);
    let fault = session.fault().unwrap();
    assert_eq!(fault.reason, Reason::CleanupTimedOut);
    session
        .event(EPOCH, 5_108, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert_eq!(session.outcome(), Some(Outcome::Failed(fault)));

    let mut session = awaiting();
    session.event(EPOCH, 107, Event::Cancel).unwrap();
    session.event(EPOCH, 4_000, Event::Park).unwrap();
    assert_eq!(session.deadline_ms(), Some(5_107));
    session
        .event(EPOCH, 5_107, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert_eq!(session.fault().unwrap().reason, Reason::Cancelled);
    assert_eq!(session.cleanup(), CleanupOutcome::Clean);
}

/// Backward time and arithmetic overflow fault rather than extending or wrapping deadlines.
#[test]
fn backwards_time_and_overflow_prevent_success() {
    let mut session = awaiting();
    assert_eq!(session.event(EPOCH, 105, Event::Tick), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().reason, Reason::TimeReversed);
    assert_eq!(session.deadline_ms(), Some(5_106));
    session
        .event(EPOCH, 107, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert!(matches!(session.outcome(), Some(Outcome::Failed(_))));
    let mut session = Session::new();
    assert_eq!(session.start(EPOCH, u64::MAX), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().reason, Reason::ClockOverflow);
    assert_eq!(session.cleanup(), CleanupOutcome::TimedOut);
    assert_eq!(session.stage(), Stage::Finished);
}

/// Stale generation callbacks cannot change time, decoder bytes, cleanup or the selected epoch.
#[test]
fn stale_events_and_epoch_reuse_are_rejected_without_mutation() {
    let mut session = awaiting();
    let stale = Epoch {
        connection: 1,
        operation: 1,
        maintenance: 1,
    };
    assert_eq!(
        session.event(stale, u64::MAX, Event::CleanupComplete(CleanupResult::Clean)),
        Err(Rejected::StaleEpoch)
    );
    assert_eq!(session.receive(stale, 0, b"bad"), Err(Rejected::StaleEpoch));
    assert_eq!(session.take_request(stale, 0).unwrap_err(), Rejected::StaleEpoch);
    assert_eq!(session.stage(), Stage::AwaitingInfo);
    assert_eq!(session.deadline_ms(), Some(2_106));
    assert_eq!(session.fault(), None);
    assert!(session.decoder().raw().is_empty());
    session.event(EPOCH, 107, Event::Cancel).unwrap();
    session
        .event(EPOCH, 108, Event::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert_eq!(session.start(EPOCH, 109), Err(Rejected::InvalidEpoch));
    let next = Epoch {
        connection: 2,
        operation: 2,
        maintenance: 1,
    };
    session.start(next, 110).unwrap();
    assert_eq!(session.event(EPOCH, 111, Event::Connected), Err(Rejected::StaleEpoch));
    assert_eq!(session.epoch(), Some(next));
    assert_eq!(session.stage(), Stage::Connecting);
}

/// Missing send permits, wrong-order completions, empty SDUs and EOF cannot satisfy identity.
#[test]
fn invalid_events_empty_sdu_and_eof_have_explicit_failures() {
    let mut session = sending();
    assert_eq!(session.event(EPOCH, 105, Event::InfoSent), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().reason, Reason::UnexpectedEvent);
    let mut session = awaiting();
    assert_eq!(session.receive(EPOCH, 107, b""), Err(Rejected::Faulted));
    assert_eq!(session.fault().unwrap().reason, Reason::UnexpectedEmptySdu);
    let mut session = awaiting();
    session.receive(EPOCH, 107, b"\x1f\x91").unwrap();
    assert_eq!(session.event(EPOCH, 108, Event::Eof), Err(Rejected::Faulted));
    assert_eq!(
        session.fault().unwrap().reason,
        Reason::Protocol(DecodeError::Truncated)
    );
    assert_eq!(session.decoder().raw(), b"\x1f\x91");
}

/// Reports actual native layouts for linked-image reviewers without asserting a host ABI on ARM.
#[test]
fn report_fixed_storage_sizes() {
    println!(
        "FrameDecoder={} BorrowedIdentity={} Session={} Epoch={} Fault={} InfoRequest={}",
        core::mem::size_of::<FrameDecoder>(),
        core::mem::size_of::<dashboard_core::ble::canyonero::Identity<'_>>(),
        core::mem::size_of::<Session>(),
        core::mem::size_of::<Epoch>(),
        core::mem::size_of::<dashboard_core::ble::canyonero::Fault>(),
        core::mem::size_of::<dashboard_core::ble::canyonero::InfoRequest>()
    );
}

/// Entering cleanup near the clock maximum must not retain the reply deadline.
#[test]
fn cleanup_deadline_overflow_finishes_without_confirmed_cleanup() {
    let mut session = Session::new();
    let start = u64::MAX - 45_000;
    session.start(EPOCH, start).unwrap();
    session.event(EPOCH, u64::MAX - 40_000, Event::Connected).unwrap();
    session.event(EPOCH, u64::MAX - 15_000, Event::Discovered).unwrap();
    session.event(EPOCH, u64::MAX - 6_000, Event::ParametersReady).unwrap();
    session.event(EPOCH, u64::MAX - 4_000, Event::ChannelOpened).unwrap();
    session.take_request(EPOCH, u64::MAX - 4_000).unwrap();
    session.event(EPOCH, u64::MAX - 4_000, Event::InfoSent).unwrap();
    assert_eq!(
        session.receive(EPOCH, u64::MAX - 3_999, &frame(0x91, b"A\nB\nC\nD\nE")),
        Err(Rejected::Faulted)
    );
    assert_eq!(session.fault().unwrap().reason, Reason::ClockOverflow);
    assert_eq!(session.stage(), Stage::Finished);
    assert_eq!(session.cleanup(), CleanupOutcome::TimedOut);
    assert_eq!(session.deadline_ms(), None);
}
