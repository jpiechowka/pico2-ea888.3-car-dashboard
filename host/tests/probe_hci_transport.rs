//! Synthetic legacy advertising frames through the locked HCI transport and report decoders.
//! This covers host-side framing, not CYW bus/ring execution or physical RF behavior.
use bt_hci::event::EventKind;
use bt_hci::event::le::{LeAdvertisingReport, LeEventKind, LeEventPacket};
use bt_hci::param::LeAdvReports;
use bt_hci::{ControllerToHostPacket, FromHciBytes, FromHciBytesError, PacketKind};
use bt_hci_transport::PacketToHost;

/// Layer at which the synthetic transport-to-report path rejected a packet.
#[derive(Debug, PartialEq)]
enum FrameError {
    /// Packet-kind/header/payload read failed after all input was consumed.
    Transport,
    /// Transport left unconsumed bytes; takes precedence over any simultaneous read error.
    TrailingBytes,
    /// Complete transport event had a malformed LE/report envelope.
    Envelope(
        /// Exact dependency size/value category.
        FromHciBytesError,
    ),
    /// Parsed packet, event or LE subevent was not a legacy advertising report.
    UnexpectedEvent,
}

/// Returns one wire record with raw event `kind`, `length` data bytes and RSSI -94 dBm.
/// `seed` identifies the address and starts wrapping payload bytes; panics if `length` exceeds 255.
fn record(
    kind: u8,
    length: usize,
    seed: u8,
) -> Vec<u8> {
    let mut bytes = vec![kind, 0, seed, 2, 3, 4, 5, 6, u8::try_from(length).unwrap()];
    bytes.extend((0..length).map(|i| (i as u8).wrapping_add(seed)));
    bytes.push(0xA2);
    bytes
}

/// Returns a framed HCI LE legacy-report event containing counted `records` in order.
/// Panics when record count or total event parameters exceed their one-byte fields.
fn event(records: &[Vec<u8>]) -> Vec<u8> {
    let mut params = vec![2, u8::try_from(records.len()).unwrap()];
    for record in records {
        params.extend(record);
    }
    let mut bytes = vec![4, 0x3E, u8::try_from(params.len()).unwrap()];
    bytes.extend(params);
    bytes
}

/// Decodes `wire` through the locked transport into caller-owned `buffer`.
/// Returns reports borrowing that buffer, or the exact rejecting layer; unread bytes take precedence.
/// Failed decoding may partially overwrite scratch bytes. Inner records are checked lazily by the returned iterator.
fn decode<'a>(
    wire: &[u8],
    buffer: &'a mut [u8],
) -> Result<LeAdvReports<'a>, FrameError> {
    let mut reader = wire;
    // Match BtDriver::read: real PacketToHost decoding, then reject unconsumed
    // transport bytes. LE dispatch then removes the subevent and report count.
    let packet =
        PacketKind::read(&mut reader).and_then(|kind| ControllerToHostPacket::read_hci(kind, &mut reader, buffer));
    if !reader.is_empty() {
        return Err(FrameError::TrailingBytes);
    }
    let ControllerToHostPacket::Event(event) = packet.map_err(|_| FrameError::Transport)? else {
        return Err(FrameError::UnexpectedEvent);
    };
    if event.kind != EventKind::Le {
        return Err(FrameError::UnexpectedEvent);
    }
    let le = LeEventPacket::from_hci_bytes_complete(event.data).map_err(FrameError::Envelope)?;
    if le.kind != LeEventKind::LeAdvertisingReport {
        return Err(FrameError::UnexpectedEvent);
    }
    Ok(LeAdvertisingReport::from_hci_bytes_complete(le.data)
        .map_err(FrameError::Envelope)?
        .reports)
}

/// All 40,960 length/kind/RSSI combinations preserve raw data through the actual locked decoder path.
#[test]
fn all_single_lengths_kinds_and_rssi_patterns_survive_actual_transport_and_report_decoders() {
    let mut cases = 0;
    for length in 0..=31 {
        for kind in 0..=4 {
            for rssi in 0..=u8::MAX {
                let mut record = record(kind, length, 0x41);
                *record.last_mut().unwrap() = rssi;
                let wire = event(&[record.clone()]);
                assert_eq!(wire.len(), 15 + length);
                let mut buffer = [0; 259];
                let reports = decode(&wire, &mut buffer).unwrap();
                assert_eq!(reports.len(), 1);
                let mut iter = reports.iter();
                let actual = iter.next().unwrap().unwrap();
                assert_eq!(actual.data, &record[9..9 + length]);
                assert_eq!(actual.rssi as u8, rssi);
                assert_eq!(actual.addr.raw(), &[0x41, 2, 3, 4, 5, 6]);
                assert!(iter.next().is_none());
                cases += 1;
            }
        }
    }
    // 32 data lengths x 5 report kinds x all 256 RSSI encodings; these include
    // reserved RSSI encodings accepted by the decoder, not 256 legal RF values.
    assert_eq!(cases, 40_960);
}

/// Mixed reports and a maximum 255-parameter-byte event retain record order and exact payload lengths.
#[test]
fn mixed_batches_and_maximum_event_preserve_lengths_order_and_payloads() {
    for lengths in [
        vec![0, 31],
        vec![31; 6],
        vec![0; 25],
        vec![31, 0, 1, 30, 2],
        vec![31, 31, 31, 31, 31, 28, 0],
    ] {
        let records: Vec<_> = lengths
            .iter()
            .enumerate()
            .map(|(i, &length)| record((i % 5) as u8, length, i as u8))
            .collect();
        let wire = event(&records);
        if lengths == [31, 31, 31, 31, 31, 28, 0] {
            assert_eq!((wire[2], wire.len()), (255, 258));
        }
        let mut buffer = [0; 259];
        let reports = decode(&wire, &mut buffer).unwrap();
        assert_eq!(reports.len(), lengths.len());
        for (i, actual) in reports.iter().enumerate() {
            let actual = actual.unwrap();
            assert_eq!(actual.data, &records[i][9..9 + lengths[i]]);
            assert_eq!(actual.addr.raw()[0], i as u8);
        }
    }
}

/// Every truncated record inside a truthful outer envelope produces one terminal inner size error.
#[test]
fn every_truncated_single_record_is_an_inner_size_error() {
    for length in 0..=31 {
        let record = record(0, length, 1);
        for keep in 0..record.len() {
            let wire = event(&[record[..keep].to_vec()]);
            let mut buffer = [0; 259];
            // The outer length truthfully describes this short record.
            let reports = decode(&wire, &mut buffer).unwrap();
            let mut iter = reports.iter();
            assert_eq!(iter.next().unwrap().unwrap_err(), FromHciBytesError::InvalidSize);
            assert!(iter.next().is_none());
        }
    }
}

/// A truncated third record retains two valid prefixes and terminates the iterator after one error.
#[test]
fn malformed_batch_retains_prefix_and_terminates_once() {
    for keep in 0..41 {
        let records = [record(0, 0, 1), record(2, 31, 2), record(4, 31, 3)[..keep].to_vec()];
        let wire = event(&records);
        let mut buffer = [0; 259];
        let reports = decode(&wire, &mut buffer).unwrap();
        let mut iter = reports.iter();
        for seed in [1, 2] {
            assert_eq!(iter.next().unwrap().unwrap().addr.raw()[0], seed);
        }
        assert_eq!(iter.next().unwrap().unwrap_err(), FromHciBytesError::InvalidSize);
        assert!(iter.next().is_none());
    }
}

/// Outer truncation, padding, length mismatch and short buffers reject before any report iterator is returned.
#[test]
fn outer_truncation_padding_and_wrong_lengths_do_not_reach_the_report_iterator() {
    for length in 0..=31 {
        let wire = event(&[record(0, length, 1)]);
        for keep in 0..wire.len() {
            // Slice read_exact leaves a short remainder untouched. BtDriver's
            // trailing-byte guard can therefore take precedence over read EOF.
            assert!(matches!(
                decode(&wire[..keep], &mut [0; 259]),
                Err(FrameError::Transport | FrameError::TrailingBytes)
            ));
        }
        for padding in 1..=4 {
            let mut extra = wire.clone();
            extra.extend(vec![0; padding]);
            assert!(matches!(decode(&extra, &mut [0; 259]), Err(FrameError::TrailingBytes)));
        }
        let mut short_length = wire.clone();
        short_length[2] -= 1;
        assert!(matches!(
            decode(&short_length, &mut [0; 259]),
            Err(FrameError::TrailingBytes)
        ));
        let mut long_length = wire.clone();
        long_length[2] += 1;
        assert!(matches!(
            decode(&long_length, &mut [0; 259]),
            Err(FrameError::TrailingBytes)
        ));
        let mut exact_buffer = vec![0; usize::from(wire[2])];
        assert!(decode(&wire, &mut exact_buffer).is_ok());
        exact_buffer.pop();
        assert!(matches!(
            decode(&wire, &mut exact_buffer),
            Err(FrameError::TrailingBytes)
        ));
    }
    // Complete event framing can still lack the LE subevent or report count.
    for wire in [&[4, 0x3E, 0][..], &[4, 0x3E, 1, 2][..]] {
        assert!(matches!(
            decode(wire, &mut [0; 259]),
            Err(FrameError::Envelope(FromHciBytesError::InvalidSize))
        ));
    }
}

/// Characterizes dependency acceptance of oversized data and ignored trailing records without claiming wire validity.
#[test]
fn current_parser_permissiveness_is_explicit_not_mistaken_for_wire_validation() {
    // Characterize the locked decoder; these are not legal legacy data lengths.
    for length in [32, 40, 243] {
        let wire = event(&[record(0, length, 1)]);
        let mut buffer = [0; 259];
        assert_eq!(
            decode(&wire, &mut buffer)
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .unwrap()
                .data
                .len(),
            length
        );
    }
    for count in [0, 1] {
        let mut wire = event(&[record(0, 0, 1), record(4, 0, 2)]);
        wire[4] = count;
        let mut buffer = [0; 259];
        assert_eq!(decode(&wire, &mut buffer).unwrap().iter().count(), usize::from(count));
    }
    let mut wire = event(&[record(0, 0, 1)]);
    wire[4] = 2;
    let mut buffer = [0; 259];
    let reports = decode(&wire, &mut buffer).unwrap();
    let result: Vec<_> = reports.iter().collect();
    assert_eq!(result.len(), 2);
    assert!(result[0].is_ok());
    assert_eq!(result[1].as_ref().unwrap_err(), &FromHciBytesError::InvalidSize);
}

/// Declared-length truncation is a size error, while an invalid event kind retains its value-error category.
#[test]
fn declared_data_length_and_invalid_kind_keep_inner_error_categories() {
    for declared in 0..=u8::MAX {
        let mut record = record(0, 31, 1);
        record[8] = declared;
        let wire = event(&[record]);
        let mut buffer = [0; 259];
        let reports = decode(&wire, &mut buffer).unwrap();
        let result = reports.iter().next().unwrap();
        if declared <= 31 {
            assert_eq!(result.unwrap().data.len(), usize::from(declared));
        } else {
            assert_eq!(result.unwrap_err(), FromHciBytesError::InvalidSize);
        }
    }
    let wire = event(&[record(5, 0, 1)]);
    let mut buffer = [0; 259];
    let reports = decode(&wire, &mut buffer).unwrap();
    assert_eq!(
        reports.iter().next().unwrap().unwrap_err(),
        FromHciBytesError::InvalidValue
    );
}
