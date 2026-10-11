#[path = "../../firmware/vendor/embassy-rp/src/flash_probe.rs"]
mod probe;

use std::collections::VecDeque;

use probe::*;

#[derive(Debug)]
enum Step {
    Control(u32),
    Status(u32),
    Write(u8),
    Read(u8),
}
struct Wire(VecDeque<Step>);
impl Io for Wire {
    fn control(
        &mut self,
        value: u32,
    ) {
        assert!(matches!(self.0.pop_front(), Some(Step::Control(want)) if value == want));
    }

    fn status(&mut self) -> u32 {
        match self.0.pop_front() {
            Some(Step::Status(value)) => value,
            other => panic!("unexpected status: {other:?}"),
        }
    }

    fn write(
        &mut self,
        byte: u8,
    ) {
        assert!(matches!(self.0.pop_front(), Some(Step::Write(want)) if byte == want));
    }

    fn read(&mut self) -> u8 {
        match self.0.pop_front() {
            Some(Step::Read(value)) => value,
            other => panic!("unexpected read: {other:?}"),
        }
    }
}

fn script(
    bytes: [u8; 4],
    delays: usize,
) -> Wire {
    let mut steps = VecDeque::from([Step::Control(TIMING | EN)]);
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
