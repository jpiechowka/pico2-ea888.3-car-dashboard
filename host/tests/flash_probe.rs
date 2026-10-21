//! Scripted real JEDEC-read state machine; no QMI registers or physical flash are accessed.
/// Reviewed HAL probe algorithm, sharing the production register-order contract.
#[path = "../../firmware/vendor/embassy-rp/src/flash_probe.rs"]
mod probe;

use std::collections::VecDeque;

use probe::*;

/// One exact expected register-boundary operation in the synthetic bus transcript.
#[derive(Debug)]
enum Step {
    /// DIRECT_CSR control write, including clock/enable/CS bits.
    Control(
        /// Expected raw register value.
        u32,
    ),
    /// DIRECT_CSR polling read.
    Status(
        /// Raw status returned for this poll.
        u32,
    ),
    /// Next transmitted command/dummy byte.
    Write(
        /// Exact byte required by the scripted exchange.
        u8,
    ),
    /// Next received byte, including the discarded command-phase byte.
    Read(
        /// Byte supplied by the synthetic RX FIFO.
        u8,
    ),
}
/// Ordered destructive transcript; unexpected accesses panic at their first mismatch.
struct Wire(
    /// Remaining expected accesses, consumed from the front.
    VecDeque<Step>,
);
impl Io for Wire {
    /// Consumes the next expected control write and checks exact `value`; panics on order/value mismatch.
    fn control(
        &mut self,
        value: u32,
    ) {
        assert!(matches!(self.0.pop_front(), Some(Step::Control(want)) if value == want));
    }

    /// Returns/consumes the next scripted status word; panics if the next operation is not a status read.
    fn status(&mut self) -> u32 {
        match self.0.pop_front() {
            Some(Step::Status(value)) => value,
            other => panic!("unexpected status: {other:?}"),
        }
    }

    /// Consumes the next scripted TX operation and checks `byte`; panics on order/value mismatch.
    fn write(
        &mut self,
        byte: u8,
    ) {
        assert!(matches!(self.0.pop_front(), Some(Step::Write(want)) if byte == want));
    }

    /// Returns/consumes the next scripted RX byte; panics for a different operation or exhausted transcript.
    fn read(&mut self) -> u8 {
        match self.0.pop_front() {
            Some(Step::Read(value)) => value,
            other => panic!("unexpected read: {other:?}"),
        }
    }
}

/// Returns the complete JEDEC transcript with four received `bytes` (first is
/// discarded) and `delays` busy polls before each ready status, including final idle.
fn script(
    bytes: [u8; 4],
    delays: usize,
) -> Wire {
    let mut steps = VecDeque::from([Step::Control(TIMING | EN)]);
    /// Appends `delays` repeated `busy` statuses followed by one `ready` status
    /// to the mutable ordered `steps` transcript.
    fn wait(
        steps: &mut VecDeque<Step>,
        busy: u32,
        ready: u32,
        delays: usize,
    ) {
        steps.extend((0..delays).map(|_| Step::Status(busy)));
        steps.push_back(Step::Status(ready));
    }
    wait(&mut steps, BUSY | TXEMPTY | RXEMPTY, TXEMPTY | RXEMPTY, delays);
    steps.push_back(Step::Control(TIMING | EN | CS0));
    for (i, byte) in bytes.into_iter().enumerate() {
        wait(&mut steps, TXFULL | RXEMPTY, TXEMPTY | RXEMPTY, delays);
        steps.push_back(Step::Write(if i == 0 { 0x9F } else { 0 }));
        wait(&mut steps, BUSY | RXEMPTY, BUSY | TXEMPTY, delays);
        steps.push_back(Step::Read(byte));
    }
    wait(&mut steps, BUSY | TXEMPTY | RXEMPTY, TXEMPTY | RXEMPTY, delays);
    steps.push_back(Step::Control(TIMING));
    Wire(steps)
}

/// Exhausts byte values and delayed polling while checking command 0x9F, dummy writes and exactly three retained reply
/// bytes.
#[test]
fn exact_read_only_wire_order_discards_only_command_byte() {
    for byte in 0..=255 {
        for bytes in [[byte, 0xEF, 0x40, 0x16], [0, byte, byte, byte], [0, 0, 0, byte]] {
            for delays in [0, 1, 7] {
                let mut wire = script(bytes, delays);
                assert_eq!(
                    read_id(&mut wire, 100_000),
                    Some(u32::from_be_bytes([0, bytes[1], bytes[2], bytes[3]]))
                );
                assert!(wire.0.is_empty());
            }
        }
    }
}

/// A single poll budget covers the entire transaction; every insufficient budget leaves final CS/direct-mode release
/// unexecuted.
#[test]
fn one_budget_bounds_every_poll_and_never_deasserts_a_busy_chip_select() {
    for delays in [0, 1, 7] {
        let total = (10 * (delays + 1)) as u32;
        for budget in 0..=total {
            let mut wire = script([0, 0xEF, 0x40, 0x16], delays);
            let result = read_id(&mut wire, budget);
            assert_eq!(result, (budget == total).then_some(0xEF4016), "budget {budget}");
            if result.is_none() {
                assert!(
                    wire.0
                        .iter()
                        .any(|step| matches!(step, Step::Control(v) if *v == TIMING))
                );
            } else {
                assert!(wire.0.is_empty());
            }
        }
    }
}

/// Nonempty initial/final FIFOs fail without the success cleanup write, requiring the caller's RAM fail-stop policy.
#[test]
fn unexpected_initial_or_final_fifo_contents_require_ram_fail_stop() {
    for position in [1, 19] {
        for bad in [0, TXEMPTY, RXEMPTY] {
            let mut wire = script([0, 0xEF, 0x40, 0x16], 0);
            // Initial status is step 1; final status follows 4 complete byte exchanges.
            let index = if position == 1 { 1 } else { wire.0.len() - 2 };
            wire.0[index] = Step::Status(bad);
            assert_eq!(read_id(&mut wire, 100_000), None);
            assert!(
                wire.0
                    .iter()
                    .any(|step| matches!(step, Step::Control(v) if *v == TIMING))
            );
        }
    }
}
