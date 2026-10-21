//! Read-only capture USB command/framing admission with independent CRC fixture and fragmented input.

use dashboard_core::discovery::capture_store;
use dashboard_core::discovery::capture_wire::*;
/// Feeds `bytes` through persistent `parser`; returns every complete request/error, preserving partial-line state.
fn parse(
    parser: &mut RequestParser,
    bytes: &[u8],
) -> Vec<Result<Request, WireError>> {
    bytes.iter().filter_map(|&b| parser.feed(b)).collect()
}
/// Only LIST/GET 1..4 parse; line overflow discards the whole command and reset discards a partial prefix.
#[test]
fn only_list_and_numbered_get_are_accepted_and_overflow_discards_whole_line() {
    let mut p = RequestParser::new();
    assert_eq!(
        parse(&mut p, b"BLECAP LIST\nBLECAP GET 1\r\nBLECAP GET 4\n"),
        vec![Ok(Request::List), Ok(Request::Get(0)), Ok(Request::Get(3))]
    );
    for bad in [
        b"BLECAP GET 0\n".as_slice(),
        b"BLECAP GET 5\n",
        b"BLECAP ERASE 1\n",
        b"BLECAP SAVE\n",
        b"BLECAP CONNECT\n",
        b"BLECAP GET 01\n",
        b"BLECAP GET 1 x\n",
    ] {
        assert_eq!(parse(&mut p, bad), vec![Err(WireError::InvalidCommand)]);
    }
    let mut flood = vec![b'X'; 100];
    flood.extend_from_slice(b"BLECAP GET 1\nBLECAP LIST\n");
    assert_eq!(parse(&mut p, &flood), vec![Err(WireError::TooLong), Ok(Request::List)]);
    assert!(parse(&mut p, b"BLECAP GET ").is_empty());
    p.reset();
    assert_eq!(parse(&mut p, b"1\n"), vec![Err(WireError::InvalidCommand)]);
}
/// Header and payload checks bind metadata/content; every truncated header and single-byte header mutation is rejected.
#[test]
fn frames_bind_slot_generation_kind_length_and_payload_and_reject_truncation() {
    let h = FrameHeader::new(FrameKind::Capture, FrameStatus::Ok, Some(2), 19, b"abc").unwrap();
    let encoded = h.encode().unwrap();
    assert_eq!(FrameHeader::decode(&encoded).unwrap(), h);
    assert_eq!(h.validate_payload(b"abc"), Ok(()));
    assert!(h.validate_payload(b"abd").is_err());
    assert!(h.validate_payload(b"ab").is_err());
    for end in 0..HEADER_BYTES {
        assert!(FrameHeader::decode(&encoded[..end]).is_err());
    }
    for at in 0..HEADER_BYTES {
        let mut damaged = encoded;
        damaged[at] ^= 1;
        assert!(FrameHeader::decode(&damaged).is_err());
    }
    assert!(FrameHeader::new(FrameKind::Capture, FrameStatus::Ok, None, 1, b"a").is_err());
    assert!(FrameHeader::new(FrameKind::Capture, FrameStatus::Ok, Some(0), 0, b"a").is_err());
}

/// Independent Python struct/zlib bytes match encoding; valid CRC cannot bypass field semantics or the 8192-byte limit.
#[test]
fn frame_matches_independent_struct_zlib_fixture_and_enforces_semantics_after_crc() {
    // Independent Python struct.pack little-endian fields and zlib.crc32.
    let fixture = [
        0x42, 0x4C, 0x45, 0x43, 0x55, 0x53, 0x42, 0x31, 1, 0, 3, 1, 19, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0xC2, 0x41,
        0x24, 0x35, 0xC2, 0x64, 0xD6, 0x4F,
    ];
    let h = FrameHeader::decode(&fixture).unwrap();
    assert_eq!(h.encode().unwrap(), fixture);
    assert_eq!(h.validate_payload(b"abc"), Ok(()));
    for (at, value) in [(8, 2), (9, 255), (10, 5), (11, 255), (12, 0), (20, 0), (22, 1)] {
        let mut b = fixture;
        b[at] = value;
        let crc = capture_store::crc32(&b[..28]);
        b[28..].copy_from_slice(&crc.to_le_bytes());
        assert!(FrameHeader::decode(&b).is_err(), "field {at}");
    }
    assert!(FrameHeader::new(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &vec![0; 8193]).is_err());
    assert!(FrameHeader::new(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &vec![0; 8192]).is_ok());
}

/// List/error/End combinations retain distinct slot/generation/payload semantics, including empty/oversized length
/// rejection.
#[test]
fn list_and_error_frames_are_unambiguous_and_bound_payload_metadata() {
    let bytes = list_payload(8192, 0x12345678).unwrap();
    assert_eq!(parse_list_payload(&bytes), Ok((8192, 0x12345678)));
    let h = FrameHeader::new(FrameKind::ListEntry, FrameStatus::Ok, Some(3), 7, &bytes).unwrap();
    assert_eq!(h.validate_payload(&bytes), Ok(()));
    for status in [
        FrameStatus::Empty,
        FrameStatus::Corrupt,
        FrameStatus::Unsupported,
        FrameStatus::Busy,
        FrameStatus::StorageError,
    ] {
        for kind in [FrameKind::ListEntry, FrameKind::Error] {
            let h = FrameHeader::new(kind, status, Some(0), 0, &[]).unwrap();
            assert_eq!(FrameHeader::decode(&h.encode().unwrap()), Ok(h));
        }
    }
    assert!(FrameHeader::new(FrameKind::End, FrameStatus::Ok, None, 0, &[]).is_ok());
    assert!(FrameHeader::new(FrameKind::End, FrameStatus::Ok, Some(0), 0, &[]).is_err());
    assert!(FrameHeader::new(FrameKind::ListEntry, FrameStatus::InvalidRequest, Some(0), 0, &[]).is_err());
    assert!(list_payload(0, 0).is_err());
    assert!(list_payload(8193, 0).is_err());
    assert!(parse_list_payload(&[0; 8]).is_err());
}

/// Exact line-size boundaries and all non-ASCII bytes reject safely; every split of a valid GET preserves one request.
#[test]
fn command_boundary_and_each_non_ascii_byte_never_create_a_request() {
    for n in [31, 32, 33] {
        let mut p = RequestParser::new();
        let mut bytes = vec![b'X'; n];
        bytes.push(b'\n');
        assert_eq!(
            parse(&mut p, &bytes),
            vec![Err(if n > COMMAND_MAX {
                WireError::TooLong
            } else {
                WireError::InvalidCommand
            })]
        );
    }
    for b in 128..=255 {
        let mut p = RequestParser::new();
        assert_eq!(parse(&mut p, &[b, b'\n']), vec![Err(WireError::InvalidCommand)]);
    }
    for split in 0..=12 {
        let mut p = RequestParser::new();
        let command = b"BLECAP GET 2\n";
        let mut events = parse(&mut p, &command[..split]);
        events.extend(parse(&mut p, &command[split..]));
        assert_eq!(events, vec![Ok(Request::Get(1))]);
    }
}
