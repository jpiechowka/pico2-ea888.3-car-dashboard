//! Owned serial TX pump completion/cancellation boundaries without real UART traffic.
use dashboard_host_validation::diagnostics::serial_tx::{Completion, Error, Pump, Work};

/// Exhausts bounded payload/chunk widths: each byte is queued once and Sent requires flush then observed wire idle.
#[test]
fn every_partial_write_is_exactly_once_and_completion_requires_flush_and_wire_idle() {
    for length in 1..=24 {
        for width in 1..=length {
            let mut tx = Pump::new();
            let input: Vec<_> = (0..length as u8).collect();
            tx.start(7, &input, 0).unwrap();
            let mut sent = Vec::new();
            while let Work::Write(bytes) = tx.work(1).unwrap() {
                let n = bytes.len().min(width);
                sent.extend_from_slice(&bytes[..n]);
                tx.wrote(n, 1).unwrap();
            }
            assert_eq!(sent, input);
            assert_eq!(tx.work(2), Ok(Work::Flush));
            assert_eq!(tx.wire_idle(2), Err(Error::Phase));
            tx.flushed(2).unwrap();
            for now in 3..=100 {
                assert_eq!(tx.work(now), Ok(Work::WaitIdle));
            }
            assert_eq!(tx.wire_idle(100), Ok(Completion::Sent(7)));
            assert_eq!(tx.work(100), Ok(Work::Idle));
            assert_eq!(tx.wire_idle(100), Err(Error::Phase));
        }
    }
}

/// Bad lengths and busy starts leave the original token/payload intact through successful completion.
#[test]
fn invalid_lengths_and_overlapping_starts_preserve_the_original_transfer() {
    let mut tx = Pump::new();
    for data in [&[][..], &[0; 25][..]] {
        assert_eq!(tx.start(1, data, 0), Err(Error::Length));
    }
    tx.start(3, &[1, 2, 3], 0).unwrap();
    assert_eq!(tx.start(4, &[9], 50), Err(Error::Busy));
    assert_eq!(tx.work(0), Ok(Work::Write(&[1, 2, 3])));
    tx.wrote(3, 1).unwrap();
    tx.flushed(2).unwrap();
    assert_eq!(tx.wire_idle(3), Ok(Completion::Sent(3)));
}

/// Abort at every queued prefix stops further writes but still drains/flushes before releasing the pump without Sent.
#[test]
fn aborted_transfer_drains_queued_bytes_without_sending_the_remainder_or_acknowledging() {
    for written in 0..=14 {
        let mut tx = Pump::new();
        tx.start(5, &[0xA5; 14], 0).unwrap();
        if written > 0 {
            tx.wrote(written, 1).unwrap();
        }
        tx.abort(2).unwrap();
        assert_eq!(tx.work(2), Ok(Work::Flush));
        tx.abort(3).unwrap();
        tx.flushed(4).unwrap();
        tx.abort(5).unwrap();
        assert_eq!(tx.work(5), Ok(Work::WaitIdle));
        assert_eq!(tx.wire_idle(6), Ok(Completion::Aborted));
        tx.start(6, &[9], 7).unwrap();
        assert_eq!(tx.work(7), Ok(Work::Write(&[9])));
    }
}

/// Impossible write counts, timeout and backward time quarantine the pump against every subsequent operation.
#[test]
fn clock_timeout_zero_and_impossible_write_counts_quarantine_the_port() {
    for invalid in [0, 15, usize::MAX] {
        let mut tx = Pump::new();
        tx.start(1, &[0; 14], 0).unwrap();
        assert_eq!(tx.wrote(invalid, 1), Err(Error::WriteCount));
        assert_eq!(tx.start(2, &[1], 2), Err(Error::Quarantined));
        assert_eq!(tx.work(2), Err(Error::Quarantined));
    }
    let mut tx = Pump::new();
    tx.start(1, &[0; 14], 0).unwrap();
    tx.abort(90).unwrap();
    assert_eq!(tx.work(101), Err(Error::Timeout));
    assert_eq!(tx.abort(102), Err(Error::Quarantined));
    let mut tx = Pump::new();
    tx.start(1, &[0; 14], 10).unwrap();
    assert_eq!(tx.work(9), Err(Error::Clock));
    assert_eq!(tx.start(2, &[1], 20), Err(Error::Quarantined));
}

/// The inclusive 100ms transfer deadline remains usable near u64::MAX without deadline-addition overflow.
#[test]
fn near_maximum_time_uses_elapsed_duration_without_overflow() {
    let mut tx = Pump::new();
    tx.start(1, &[42], u64::MAX - 100).unwrap();
    tx.wrote(1, u64::MAX - 99).unwrap();
    tx.flushed(u64::MAX - 99).unwrap();
    assert_eq!(tx.wire_idle(u64::MAX), Ok(Completion::Sent(1)));
}
