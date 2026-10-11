use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use dashboard_host_validation::diagnostics::service::{IoError, LinkState, Port, Profile, ReceiveContract, Vehicle};
use dashboard_host_validation::samples::{Quality, SensorId};

struct Registers {
    rx: VecDeque<u8>,
    sent: Vec<u8>,
    writes: usize,
    reads: usize,
    discards: usize,
    limit: usize,
    busy: bool,
    flush_ready: bool,
    discard_fails: bool,
    rx_error: Option<IoError>,
    fault_after_copy: bool,
    invalid_read_count: bool,
}
impl Default for Registers {
    fn default() -> Self {
        Self {
            rx: VecDeque::new(),
            sent: Vec::new(),
            writes: 0,
            reads: 0,
            discards: 0,
            limit: 24,
            busy: false,
            flush_ready: true,
            discard_fails: false,
            rx_error: None,
            fault_after_copy: false,
            invalid_read_count: false,
        }
    }
}
#[derive(Clone, Default)]
struct Hardware(Rc<RefCell<Registers>>);
impl Hardware {
    fn feed(
        &self,
        bytes: &[u8],
    ) {
        self.0.borrow_mut().rx.extend(bytes);
    }

    fn take_tx(&self) -> Vec<u8> { std::mem::take(&mut self.0.borrow_mut().sent) }
}
impl Port for Hardware {
    fn poll_read(
        &mut self,
        _: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        let mut r = self.0.borrow_mut();
        r.reads += 1;
        if let Some(error) = r.rx_error {
            return Poll::Ready(Err(error));
        }
        if r.rx.is_empty() {
            return Poll::Pending;
        }
        let n = bytes.len().min(r.rx.len());
        for byte in &mut bytes[..n] {
            *byte = r.rx.pop_front().unwrap();
        }
        if r.fault_after_copy {
            r.rx_error = Some(IoError::Overrun);
        }
        Poll::Ready(Ok(if r.invalid_read_count { bytes.len() + 1 } else { n }))
    }

    fn poll_write(
        &mut self,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let mut r = self.0.borrow_mut();
        r.writes += 1;
        let n = bytes.len().min(r.limit);
        r.sent.extend_from_slice(&bytes[..n]);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        if self.0.borrow().flush_ready {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn busy(&self) -> bool { self.0.borrow().busy }

    fn rx_error(&self) -> Option<IoError> { self.0.borrow().rx_error }

    fn discard_rx(&mut self) -> Result<(), IoError> {
        let mut r = self.0.borrow_mut();
        r.discards += 1;
        if r.discard_fails {
            return Err(IoError::Overrun);
        }
        r.rx.clear();
        r.rx_error = None;
        Ok(())
    }
}
fn new(contract: ReceiveContract) -> (Vehicle<Hardware>, Hardware) {
    let hardware = Hardware::default();
    let vehicle = Vehicle::new(
        hardware.clone(),
        Profile {
            responder: 0x7E8,
            receive: contract,
        },
        0,
    )
    .unwrap();
    (vehicle, hardware)
}
fn tick(
    vehicle: &mut Vehicle<Hardware>,
    now: u64,
) {
    vehicle.step(&mut Context::from_waker(Waker::noop()), now);
}
fn record(
    id: u16,
    data: [u8; 8],
) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..4].copy_from_slice(&u32::from(id).to_be_bytes());
    bytes[4..].copy_from_slice(&data);
    bytes
}
fn handshake_until_exit(
    vehicle: &mut Vehicle<Hardware>,
    hw: &Hardware,
    base: u64,
) {
    tick(vehicle, base);
    tick(vehicle, base + 50);
    tick(vehicle, base + 51);
    assert_eq!(hw.take_tx(), b"+++");
    hw.feed(b"ENTER INTO SETTING MODE\r\n");
    tick(vehicle, base + 52);
    tick(vehicle, base + 53);
    assert_eq!(hw.take_tx(), b"AT\r\n");
    hw.feed(b"OK\r\n");
    tick(vehicle, base + 54);
    tick(vehicle, base + 55);
    assert_eq!(hw.take_tx(), b"AT+Q\r\n");
}
fn handshake(
    vehicle: &mut Vehicle<Hardware>,
    hw: &Hardware,
    base: u64,
) {
    handshake_until_exit(vehicle, hw, base);
    hw.feed(b"OK\r\nENTER DATA MODE\r\n");
    tick(vehicle, base + 56);
    tick(vehicle, base + 56); // separate pass observes drained RX at a boundary
}
fn coolant_request(
    vehicle: &mut Vehicle<Hardware>,
    hw: &Hardware,
) {
    handshake(vehicle, hw, 0);
    tick(vehicle, 57);
    assert_eq!(hw.take_tx(), [0, 0, 7, 0xE0, 0, 0, 2, 1, 0, 0, 0, 0, 0, 0]);
    hw.feed(&record(0x7E8, [6, 0x41, 0, 8, 0, 0, 0, 0]));
    tick(vehicle, 58);
    tick(vehicle, 78);
}

#[test]
fn unqualified_profile_records_raw_data_but_never_polls_can_or_publishes_values() {
    let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
    handshake(&mut vehicle, &hw, 0);
    assert_eq!(vehicle.link().state, LinkState::Unqualified);
    for (i, data) in [[6, 0x41, 0, 8, 0, 0, 0, 0], [3, 0x41, 5, 135, 0, 0, 0, 0]]
        .iter()
        .enumerate()
    {
        hw.feed(&record(0x7E8, *data));
        tick(&mut vehicle, 100 + i as u64);
    }
    tick(&mut vehicle, 30_000);
    assert!(hw.take_tx().is_empty());
    assert_eq!(vehicle.link().raw_records, 2);
    assert_eq!(vehicle.snapshot().quality(SensorId::Water, 30_000), Quality::Unknown);
    assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 30_000), None);
}

#[test]
fn qualified_session_discovers_support_and_reports_only_the_matching_value() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    coolant_request(&mut vehicle, &hw);
    tick(&mut vehicle, 79);
    assert_eq!(hw.take_tx(), [0, 0, 7, 0xE0, 0, 0, 2, 1, 5, 0, 0, 0, 0, 0]);
    hw.feed(&record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]));
    tick(&mut vehicle, 80);
    assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 80), Some(95.0));
    assert_eq!(vehicle.snapshot().quality(SensorId::Oil, 80), Quality::Unsupported);
    assert_eq!(vehicle.link().state, LinkState::Polling);
    assert_eq!(vehicle.link().diagnostics.completed, 2);
    vehicle.reset_statistics(1);
    assert_eq!(vehicle.snapshot().reset_generation, 1);
}

#[test]
fn buffered_response_is_not_consumed_until_tx_flush_and_busy_are_complete() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    coolant_request(&mut vehicle, &hw);
    hw.0.borrow_mut().flush_ready = false;
    tick(&mut vehicle, 79);
    hw.feed(&record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]));
    tick(&mut vehicle, 80);
    assert_eq!(hw.0.borrow().rx.len(), 12);
    hw.0.borrow_mut().flush_ready = true;
    hw.0.borrow_mut().busy = true;
    tick(&mut vehicle, 81);
    assert_eq!(hw.0.borrow().rx.len(), 12);
    assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 81), None);
    hw.0.borrow_mut().busy = false;
    tick(&mut vehicle, 82);
    assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 82), Some(95.0));
}

#[test]
fn receive_fault_before_or_during_copy_invalidates_values_before_publication() {
    for after_copy in [false, true] {
        let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
        coolant_request(&mut vehicle, &hw);
        tick(&mut vehicle, 79);
        hw.feed(&record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]));
        hw.0.borrow_mut().fault_after_copy = after_copy;
        if !after_copy {
            hw.0.borrow_mut().rx_error = Some(IoError::Overrun);
        }
        tick(&mut vehicle, 80);
        assert_eq!(vehicle.snapshot().quality(SensorId::Water, 80), Quality::TransportError);
        assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 80), None);
        assert_eq!(vehicle.link().state, LinkState::Recovering);
        assert_eq!(vehicle.link().raw_records, 1); // capability record only
    }
}

#[test]
fn receive_loss_discards_every_partial_record_before_a_new_epoch() {
    for prefix in 1..12 {
        let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
        handshake(&mut vehicle, &hw, 0);
        let bytes = record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]);
        hw.feed(&bytes[..prefix]);
        tick(&mut vehicle, 100);
        hw.0.borrow_mut().rx_error = Some(IoError::Framing);
        tick(&mut vehicle, 101);
        hw.feed(&bytes[prefix..]);
        handshake(&mut vehicle, &hw, 351);
        hw.feed(&bytes);
        tick(&mut vehicle, 410);
        assert_eq!(vehicle.link().raw_records, 1);
        assert_eq!(vehicle.raw().next().unwrap().record.data(), &bytes[4..]);
        assert!(vehicle.raw().next().unwrap().record.epoch() > 2);
    }
}

#[test]
fn silent_ecu_times_out_without_restarting_the_adapter_or_blocking_service() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    handshake(&mut vehicle, &hw, 0);
    tick(&mut vehicle, 57);
    hw.take_tx();
    tick(&mut vehicle, 558);
    assert_eq!(vehicle.link().diagnostics.timeouts, 1);
    assert_eq!(vehicle.link().state, LinkState::Polling);
    assert_eq!(hw.0.borrow().discards, 1);
    assert!(hw.take_tx().is_empty());
    tick(&mut vehicle, 1558);
    tick(&mut vehicle, 1559);
    assert_eq!(hw.take_tx().len(), 14);
}

#[test]
fn tx_stall_clock_failure_and_invalid_port_counts_stop_future_commands() {
    for mode in 0..3 {
        let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
        tick(&mut vehicle, 0);
        tick(&mut vehicle, 50);
        match mode {
            0 => {
                hw.0.borrow_mut().busy = true;
                tick(&mut vehicle, 51);
                tick(&mut vehicle, 151);
            }
            1 => tick(&mut vehicle, 49),
            _ => {
                hw.0.borrow_mut().limit = 0;
                tick(&mut vehicle, 51);
            }
        }
        assert_eq!(vehicle.link().state, LinkState::Quarantined);
        let writes = hw.0.borrow().writes;
        tick(&mut vehicle, 60_000);
        assert_eq!(hw.0.borrow().writes, writes);
    }
    let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
    handshake(&mut vehicle, &hw, 0);
    hw.0.borrow_mut().invalid_read_count = true;
    hw.feed(&[0; 12]);
    tick(&mut vehicle, 100);
    assert_eq!(vehicle.link().state, LinkState::Quarantined);
}

#[test]
fn recovery_retries_use_capped_backoff_and_never_spin_on_a_sticky_fault() {
    let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
    hw.0.borrow_mut().discard_fails = true;
    let mut now = 0;
    for (i, wait) in [250, 500, 1000, 2000, 4000, 8000, 16000, 30000, 30000]
        .iter()
        .enumerate()
    {
        tick(&mut vehicle, now);
        assert_eq!(hw.0.borrow().discards, i + 1);
        tick(&mut vehicle, now + wait - 1);
        assert_eq!(hw.0.borrow().discards, i + 1);
        assert_eq!(vehicle.link().state, LinkState::Recovering);
        now += wait;
    }
    assert!(hw.take_tx().is_empty());
}

#[test]
fn raw_trace_is_bounded_and_reports_overwrites_and_each_step_has_a_read_limit() {
    let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
    handshake(&mut vehicle, &hw, 0);
    for n in 0..32 {
        hw.feed(&record(0x7E8, [n; 8]));
    }
    for now in 100..112 {
        let before = hw.0.borrow().rx.len();
        tick(&mut vehicle, now);
        assert!(before - hw.0.borrow().rx.len() <= 32);
    }
    assert_eq!(vehicle.link().raw_records, 32);
    assert_eq!(vehicle.link().raw_overwrites, 16);
    assert_eq!(
        vehicle.raw().map(|v| v.record.data()[0]).collect::<Vec<_>>(),
        (16..32).collect::<Vec<_>>()
    );
}

#[test]
fn selected_responder_contract_violation_fails_closed() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    handshake(&mut vehicle, &hw, 0);
    tick(&mut vehicle, 57);
    hw.feed(&record(0x7E9, [6, 0x41, 0, 8, 0, 0, 0, 0]));
    tick(&mut vehicle, 58);
    assert_eq!(vehicle.link().state, LinkState::Recovering);
    assert_eq!(vehicle.link().raw_records, 1);
    assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 58), None);
}

#[test]
fn old_record_after_final_banner_cannot_complete_a_new_capability_query() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    handshake_until_exit(&mut vehicle, &hw, 0);
    hw.feed(b"OK\r\nENTER DATA MODE\r\n");
    hw.feed(&record(0x7E8, [6, 0x41, 0, 8, 0, 0, 0, 0]));
    tick(&mut vehicle, 56); // 21 banner bytes + 11 bytes of an old reply
    assert_eq!(hw.0.borrow().rx.len(), 1);
    tick(&mut vehicle, 57);
    assert!(hw.take_tx().is_empty(), "old bytes must drain before query TX");
    assert_eq!(vehicle.link().diagnostics.completed, 0);
    assert_eq!(vehicle.link().raw_records, 1);
    tick(&mut vehicle, 58); // observed empty read, complete record boundary
    tick(&mut vehicle, 59);
    assert_eq!(hw.take_tx().len(), 14);
    hw.feed(&record(0x7E8, [6, 0x41, 0, 8, 0, 0, 0, 0]));
    tick(&mut vehicle, 60);
    assert_eq!(vehicle.link().diagnostics.completed, 1);
}

#[test]
fn every_partial_record_must_drain_before_the_next_polling_request() {
    for prefix in 1..12 {
        let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
        coolant_request(&mut vehicle, &hw);
        tick(&mut vehicle, 79);
        hw.take_tx();
        hw.feed(&record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]));
        tick(&mut vehicle, 80);
        let old = record(0x7E8, [3, 0x41, 5, 155, 0, 0, 0, 0]);
        hw.feed(&old[..prefix]);
        tick(&mut vehicle, 1080); // next coolant poll is due
        tick(&mut vehicle, 1081); // empty port, but incomplete parser record
        hw.feed(&old[prefix..]);
        tick(&mut vehicle, 1082);
        assert!(hw.take_tx().is_empty());
        assert_eq!(vehicle.link().diagnostics.completed, 2);
        assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 1082), Some(95.0));
        tick(&mut vehicle, 1083);
        tick(&mut vehicle, 1084);
        assert_eq!(hw.take_tx()[8], 5);
        hw.feed(&record(0x7E8, [3, 0x41, 5, 130, 0, 0, 0, 0]));
        tick(&mut vehicle, 1085);
        assert_eq!(vehicle.link().diagnostics.completed, 3);
        assert_eq!(vehicle.snapshot().value_at(SensorId::Water, 1085), Some(90.0));
    }
}

#[test]
fn incomplete_record_deadline_is_fixed_inclusive_and_requires_a_new_epoch() {
    for prefix in 1..12 {
        let (mut vehicle, hw) = new(ReceiveContract::Unqualified);
        handshake(&mut vehicle, &hw, 0);
        let bytes = record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]);
        hw.feed(&bytes[..prefix]);
        tick(&mut vehicle, 100);
        if prefix < 11 {
            hw.feed(&bytes[prefix..prefix + 1]);
        }
        tick(&mut vehicle, 200); // more bytes cannot extend the first-byte age
        assert_eq!(vehicle.link().state, LinkState::Unqualified);
        tick(&mut vehicle, 201);
        assert_eq!(vehicle.link().state, LinkState::Recovering);
        assert_eq!(vehicle.link().raw_records, 0);
        hw.feed(&bytes[prefix..]);
        handshake(&mut vehicle, &hw, 451);
        hw.feed(&bytes);
        tick(&mut vehicle, 508);
        assert_eq!(vehicle.link().raw_records, 1);
        assert!(vehicle.raw().next().unwrap().record.epoch() > 2);
    }
}

#[test]
fn receive_backlog_cannot_starve_an_active_response_deadline() {
    let (mut vehicle, hw) = new(ReceiveContract::PaddedStandardDataDlc8);
    handshake(&mut vehicle, &hw, 0);
    tick(&mut vehicle, 57);
    hw.take_tx();
    for _ in 0..16 {
        hw.feed(&record(0x7E8, [3, 0x41, 5, 135, 0, 0, 0, 0]));
    }
    tick(&mut vehicle, 558);
    assert!(!hw.0.borrow().rx.is_empty());
    assert_eq!(vehicle.link().diagnostics.timeouts, 1);
    assert_eq!(vehicle.link().state, LinkState::Polling);
    assert!(hw.take_tx().is_empty());
}
