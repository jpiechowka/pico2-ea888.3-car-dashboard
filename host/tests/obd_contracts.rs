//! Read-only diagnostic wire grammar and generic OBD scaling; no exact-ECU qualification is inferred.
use dashboard_host_validation::diagnostics::obd::{Capabilities, DecodeError, Decoded, Support, decode};
use dashboard_host_validation::diagnostics::request::{Reply, ReplyError, Request};
use dashboard_host_validation::samples::Quantity;

/// Request frames use read services; replies require exact service/identifier and bounded positive/negative grammar.
#[test]
fn requests_only_expose_read_services_and_match_exact_response_identifiers() {
    assert_eq!(Request::Obd(5).frame(0x7E0).unwrap().data(), &[2, 1, 5, 0, 0, 0, 0, 0]);
    assert_eq!(
        Request::ReadDid(0xF190).frame(0x7E0).unwrap().data(),
        &[3, 0x22, 0xF1, 0x90, 0, 0, 0, 0]
    );
    for (request, positive) in [
        (Request::Obd(5), vec![0x41, 5, 70]),
        (Request::ReadDid(0xF190), vec![0x62, 0xF1, 0x90, 1, 2]),
    ] {
        assert!(matches!(request.reply(&positive), Ok(Reply::Positive(_))));
        assert_eq!(request.reply(&[0x7F, request.service(), 0x78]), Ok(Reply::Pending));
        assert_eq!(
            request.reply(&[0x7F, request.service(), 0x31]),
            Ok(Reply::Negative(0x31))
        );
        assert!(request.reply(&[]).is_err());
        assert!(request.reply(&[0x7F, request.service()]).is_err());
        assert!(request.reply(&[0x7F, request.service(), 0x78, 0]).is_err());
        assert_eq!(request.reply(&[0x7F, 0x2E, 0x78]), Err(ReplyError::Service));
        let mut bad = positive;
        bad[1] ^= 1;
        assert_eq!(request.reply(&bad), Err(ReplyError::Identifier));
    }
}

/// Every support bit maps to its PID, with page chaining, responder/epoch binding and immutable admitted pages.
#[test]
fn every_capability_bit_including_page_edges_maps_to_the_correct_pid() {
    for base in [0, 0x20, 0x40] {
        for offset in 1..=32u8 {
            let mut caps = Capabilities::new(0x7E8, 3).unwrap();
            if base >= 0x20 {
                caps.record_page(0x7E8, 3, 0, 1).unwrap();
            }
            if base >= 0x40 {
                caps.record_page(0x7E8, 3, 0x20, 1).unwrap();
            }
            caps.record_page(0x7E8, 3, base, 1u32 << (32 - offset)).unwrap();
            for check in 1..=32u8 {
                assert_eq!(
                    caps.supported(base + check),
                    if check == offset {
                        Support::Supported
                    } else {
                        Support::Unsupported
                    }
                );
            }
        }
    }
    let mut caps = Capabilities::new(0x7E8, 3).unwrap();
    assert_eq!(caps.supported(0x5C), Support::Unknown);
    let before = caps;
    assert!(caps.record_page(0x7E9, 3, 0, u32::MAX).is_err());
    assert!(caps.record_page(0x7E8, 2, 0, u32::MAX).is_err());
    assert_eq!(caps, before);
    assert!(caps.record_page(0x7E8, 3, 0x40, u32::MAX).is_err());
    caps.record_page(0x7E8, 3, 0, 0).unwrap();
    assert_eq!(caps.supported(0x5C), Support::Unsupported);
    assert!(
        caps.record_page(0x7E8, 3, 0, u32::MAX).is_err(),
        "capability changes need a new epoch"
    );
    assert!(caps.record_page(0x7E8, 3, 1, 0).is_err());
    assert!(Capabilities::new(0x7E0, 1).is_err());
}

/// Exhausts temperature bytes and voltage words, checks exact widths, and rejects unqualified PIDs.
#[test]
fn generic_temperature_and_module_voltage_scaling_cover_every_raw_value() {
    for pid in [5, 0x0F, 0x5C] {
        for byte in 0..=255u8 {
            let expected = f32::from(byte) - 40.0;
            let Decoded::Quantity(actual) = decode(pid, &[byte]).unwrap() else {
                panic!()
            };
            assert_eq!(actual.value(), expected);
            assert!(matches!(
                (pid, actual),
                (5, Quantity::CoolantC(_)) | (0x0F, Quantity::IntakeC(_)) | (0x5C, Quantity::EngineOilC(_))
            ));
        }
    }
    for raw in 0..=u16::MAX {
        assert_eq!(
            decode(0x42, &raw.to_be_bytes()).unwrap(),
            Decoded::Quantity(Quantity::ModuleVolts(f32::from(raw) / 1000.0))
        );
    }
    assert_eq!(
        decode(0x42, &[0x36, 0xB0]).unwrap(),
        Decoded::Quantity(Quantity::ModuleVolts(14.0))
    );
    for (pid, width) in [(5, 1), (0x0F, 1), (0x5C, 1), (0x42, 2), (0, 4), (0x20, 4), (0x40, 4)] {
        for size in 0..=7 {
            assert_eq!(decode(pid, &[0; 7][..size]).is_ok(), size == width);
        }
    }
    assert_eq!(
        decode(0, &[0x80, 0, 0, 1]),
        Ok(Decoded::Supported {
            base: 0,
            bits: 0x80000001
        })
    );
    for pid in [0x0B, 0x24, 0x34, 0x44, 0x3C, 0x78, 0xFF] {
        assert_eq!(decode(pid, &[0; 4]), Err(DecodeError::UnqualifiedPid));
    }
}
