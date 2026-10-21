//! OneWire pio driver

use crate::Peri;
use crate::clocks::clk_sys_freq;
use crate::gpio::Level;
use crate::pio::{
    Common,
    Config,
    Direction,
    Instance,
    LoadedProgram,
    Pin,
    PioPin,
    ShiftConfig,
    ShiftDirection,
    StateMachine,
};

/// Returns whether the eight MSB-aligned samples in `word` contain any low level.
/// Lower 24 bits are ignored; idle-high 0xff is not device presence.
fn presence_sample(word: u32) -> bool { (word >> 24) != 0xFF }

/// Returns the last-bit countdown for LSB-first `data`.
/// Panics unless nonempty, final bit is zero and eight times byte length fits u32.
fn pullup_counter(data: &[u8]) -> u32 {
    assert!(
        data.last().is_some_and(|byte| byte & 0x80 == 0),
        "strong pullup requires a nonempty command ending in a zero bit"
    );
    u32::try_from(data.len())
        .ok()
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_sub(1))
        .expect("OneWire command exceeds the bit counter")
}

/// This struct represents a onewire driver program
pub struct PioOneWireProgram<'a, PIO: Instance> {
    /// Fixed-origin bus timing instructions.
    prg: LoadedProgram<'a, PIO>,
    /// Entry point for reset pulse and presence sampling.
    reset_addr: u8,
    /// Entry point waiting for the next outgoing bit.
    next_bit_addr: u8,
}

impl<'a, PIO: Instance> PioOneWireProgram<'a, PIO> {
    /// Loads the OneWire program at origin zero through `common`; returns allocation and entry addresses.
    /// Panics if its fixed instruction range is occupied.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = pio::pio_asm!(
            r#"
                ; We need to use the pins direction to simulate open drain output
                ; This results in all the side-set values being swapped from the actual pin value
                .side_set 1 pindirs

                ; Set the origin to 0 so we can correctly use jmp instructions externally
                .origin 0

                ; Tick rate is 1 tick per 6us, so all delays should be calculated back to that
                ; All the instructions have a calculated delay XX in us as [(XX / CLK) - 1].
                ; The - 1 is for the instruction which also takes one clock cyle.
                ; The delay can be 0 which will result in just 6us for the instruction itself
                .define CLK 6

                ; Write the reset block after trigger
                public reset:
                        set x, 4                side 0 [(60 / CLK) - 1]     ; idle before reset
                    reset_inner:                                            ; Repeat the following 5 times, so 5*96us = 480us in total
                        nop                     side 1 [(90 / CLK) - 1]
                        jmp x--, reset_inner    side 1 [( 6 / CLK) - 1]
                        ; Fallthrough

                    ; Check for presence of one or more devices.
                    ; This samples 32 times with an interval of 12us after a 18us delay.
                    ; If any bit is zero in the end value, there is a detection
                    ; This whole function takes 480us
                        set x, 31               side 0 [(24 / CLK) - 1]     ; Loop 32 times -> 32*12us = 384us
                    presence_check:
                        in pins, 1              side 0 [( 6 / CLK) - 1]     ; poll pin and push to isr
                        jmp x--, presence_check side 0 [( 6 / CLK) - 1]
                        jmp next_bit            side 0 [(72 / CLK) - 1]

                    ; The low pulse was already done, we only need to delay and poll the bit in case we are reading
                    write_1:
                        jmp y--, continue_1     side 0 [( 6 / CLK) - 1]     ; Delay before sampling input. Always decrement y
                    continue_1:
                        in pins, 1              side 0 [(48 / CLK) - 1]     ; This writes the state of the pin into the ISR
                        ; Fallthrough

                ; This is the entry point when reading and writing data
                public next_bit:
                    .wrap_target
                        out x, 1                side 0 [(12 / CLK) - 1]     ; Stalls if no data available in TX FIFO and OSR
                        jmp x--, write_1        side 1 [( 6 / CLK) - 1]     ; Do the always low part of a bit, jump to write_1 if we want to write a 1 bit
                        jmp y--, continue_0     side 1 [(48 / CLK) - 1]     ; Do the remainder of the low part of a 0 bit
                        jmp pullup              side 1 [( 6 / CLK) - 1]     ; Remain low while jumping
                    continue_0:
                        in null, 1              side 1 [( 6 / CLK) - 1]     ; This writes 0 into the ISR so that the shift count stays in sync
                    .wrap

                    ; Assume that strong pullup commands always have MSB (the last bit) = 0,
                    ; since the rising edge can be used to start the operation.
                    ; That's the case for DS18B20 (44h and 48h).
                    pullup:
                        set pins, 1             side 1[( 6 / CLK) - 1]      ; Drive pin high output immediately.
                                                                            ; Strong pullup must be within 10us of rise.
                        in null, 1              side 1[( 6 / CLK) - 1]      ; Keep ISR in sync. Must occur after the y--.
                        out null, 8             side 1[( 6 / CLK) - 1]      ; Wait for write_bytes_pullup() delay to complete.
                                                                            ; The delay is hundreds of ms, so done externally.
                        set pins, 0             side 0[( 6 / CLK) - 1]      ; Back to open drain, pin low when driven
                        in null, 8              side 1[( 6 / CLK) - 1]      ; Inform write_bytes_pullup() it's ready
                        jmp next_bit            side 0[( 6 / CLK) - 1]      ; Continue
            "#
        );

        Self {
            prg: common.load_program(&prg.program),
            reset_addr: prg.public_defines.reset as u8,
            next_bit_addr: prg.public_defines.next_bit as u8,
        }
    }
}
/// PIO OneWire bus with direction-controlled open drain and optional actively driven strong pull-up.
/// Requires a suitable external pull-up and compatible electrical loading; no CRC or device-specific admission.
/// Canceled operations restore byte mode/release strong pull-up; start a new transaction with reset.
pub struct PioOneWire<'d, PIO: Instance, const SM: usize> {
    /// Owned bus state machine, normally waiting for output in byte mode.
    sm: StateMachine<'d, PIO, SM>,
    /// Cached configuration, temporarily changed to one-bit search mode.
    cfg: Config<'d, PIO>,
    /// Entry point for reset pulse and presence sampling.
    reset_addr: u8,
    /// Entry point waiting for the next outgoing bit.
    next_bit_addr: u8,
    /// Registered data pin restored to high impedance on cancellation.
    pin: Pin<'d, PIO>,
}

impl<'d, PIO: Instance, const SM: usize> PioOneWire<'d, PIO, SM> {
    /// Registers `pin` through `common`, configures `sm` with `program` and returns the running bus.
    /// Uses nominal six-microsecond instruction cycles; divider truncation can shorten them.
    /// Panics if the system clock cannot represent that divider; clock must remain unchanged.
    pub fn new(
        common: &mut Common<'d, PIO>,
        mut sm: StateMachine<'d, PIO, SM>,
        pin: Peri<'d, impl PioPin>,
        program: &PioOneWireProgram<'d, PIO>,
    ) -> Self {
        let pin = common.make_pio_pin(pin);

        sm.set_pin_dirs(Direction::In, &[&pin]);
        sm.set_pins(Level::Low, &[&pin]);

        let mut cfg = Config::default();
        cfg.use_program(&program.prg, &[&pin]);
        cfg.set_in_pins(&[&pin]);
        cfg.set_set_pins(&[&pin]);

        let shift_cfg = ShiftConfig {
            auto_fill: true,
            direction: ShiftDirection::Right,
            threshold: 8,
        };
        cfg.shift_in = shift_cfg;
        cfg.shift_out = shift_cfg;

        let divider = u64::from(clk_sys_freq()) * 6 * 256 / 1_000_000;
        assert!((256..=0x100_0000).contains(&divider));
        cfg.clock_divider = fixed::types::U24F8::from_bits(divider as u32);

        sm.set_config(&cfg);
        sm.clear_fifos();
        sm.restart();
        unsafe {
            sm.exec_jmp(program.next_bit_addr);
        }
        sm.set_enable(true);

        Self {
            sm,
            cfg,
            reset_addr: program.reset_addr,
            next_bit_addr: program.next_bit_addr,
            pin,
        }
    }

    /// Resets byte-mode state, emits the reset pulse and returns true if any presence sample is low.
    /// False means all 32 samples stayed high, not a bus diagnostic. Cancellation restores idle byte mode.
    pub async fn reset(&mut self) -> bool {
        self.restore_after_search();
        let mut guard = BusGuard {
            bus: self,
            active: true,
        };
        // The state machine immediately starts running when jumping to this address
        unsafe {
            guard.bus.sm.exec_jmp(guard.bus.reset_addr);
        }

        let rx = guard.bus.sm.rx();
        let mut found = false;
        for _ in 0..4 {
            if presence_sample(rx.wait_pull().await) {
                found = true;
            }
        }

        guard.active = false;
        found
    }

    /// Sends LSB-first `data`, draining echo samples after each byte; empty input still resets the pull-up counter.
    /// No timeout or device acknowledgment/CRC. Cancellation can expose a prefix, then restores byte mode.
    pub async fn write_bytes(
        &mut self,
        data: &[u8],
    ) {
        let mut guard = BusGuard {
            bus: self,
            active: true,
        };
        unsafe {
            guard.bus.sm.set_enable(false);
            guard.bus.sm.set_y(u32::MAX);
            guard.bus.sm.set_enable(true);
        }
        let (rx, tx) = guard.bus.sm.rx_tx();
        for b in data {
            tx.wait_push(*b as u32).await;

            // Empty the buffer that is being filled with every write
            let _ = rx.wait_pull().await;
        }
        guard.active = false;
    }

    /// Sends LSB-first `data`, actively drives high for at least the timer wait `pullup_time`, then releases.
    /// Panics before bus changes for empty input, final MSB set or bit-count overflow. A functioning
    /// time driver is required. Cancellation releases strong pull-up and requires a new reset transaction.
    pub async fn write_bytes_pullup(
        &mut self,
        data: &[u8],
        pullup_time: embassy_time::Duration,
    ) {
        let counter = pullup_counter(data);
        let mut guard = BusGuard {
            bus: self,
            active: true,
        };
        unsafe {
            guard.bus.sm.set_enable(false);
            guard.bus.sm.set_y(counter);
            guard.bus.sm.set_enable(true);
        };
        let (rx, tx) = guard.bus.sm.rx_tx();
        for b in data {
            tx.wait_push(*b as u32).await;

            // Empty the buffer that is being filled with every write
            let _ = rx.wait_pull().await;
        }

        // Perform the delay, usually hundreds of ms.
        embassy_time::Timer::after(pullup_time).await;

        // Signal that delay has completed
        tx.wait_push(0 as u32).await;
        // Wait until it's back at 0 low, open drain
        let _ = rx.wait_pull().await;
        guard.active = false;
    }

    /// Fills `data` with LSB-first samples by transmitting ones; empty input only resets the pull-up counter.
    /// Cancellation preserves a written prefix and restores bus state. No CRC, presence check or timeout.
    pub async fn read_bytes(
        &mut self,
        data: &mut [u8],
    ) {
        let mut guard = BusGuard {
            bus: self,
            active: true,
        };
        unsafe {
            guard.bus.sm.set_enable(false);
            guard.bus.sm.set_y(u32::MAX);
            guard.bus.sm.set_enable(true);
        };
        let (rx, tx) = guard.bus.sm.rx_tx();
        for b in data {
            // Write all 1's so that we can read what the device responds
            tx.wait_push(0xFF).await;

            *b = (rx.wait_pull().await >> 24) as u8;
        }
        guard.active = false;
    }

    /// Returns the next raw 64-bit ROM following `state` discrepancies, or None when no responder remains.
    /// Each call resets the bus and issues Search ROM; CRC is not checked. Successful completion updates
    /// `state`; cancellation does not commit a partial ROM and the guard restores byte mode.
    async fn search(
        &mut self,
        state: &mut PioOneWireSearch,
    ) -> Option<u64> {
        if !self.reset().await {
            // No device present, no use in searching
            state.finished = true;
            return None;
        }
        self.write_bytes(&[0xF0]).await; // 0xF0 is the search rom command

        self.prepare_search();
        let guard = BusGuard {
            bus: self,
            active: true,
        };
        let (rx, tx) = guard.bus.sm.rx_tx();

        let mut value = 0;
        let mut last_zero = 0;

        // The original Dallas app note used 1-based bit numbering with 0 being a
        // sentinel value (ie None).  This is important if you have sensors with
        // both 0 and 1 as LSB of the family code.
        for bit in 1..=64 {
            // Write 2 dummy bits to read a bit and its complement
            tx.wait_push(0x1).await;
            tx.wait_push(0x1).await;
            let in1 = rx.wait_pull().await;
            let in2 = rx.wait_pull().await;
            let push = match (in1, in2) {
                (0, 0) => {
                    // If both are 0, it means we have devices with 0 and 1 bits in this position
                    let write_value = if bit < state.last_discrepancy {
                        (state.last_rom & (1 << (bit - 1))) != 0
                    } else {
                        bit == state.last_discrepancy
                    };

                    if write_value {
                        1
                    } else {
                        last_zero = bit;
                        0
                    }
                }
                (0, 1) => 0, // Only devices with a 0 bit in this position
                (1, 0) => 1, // Only devices with a 1 bit in this position
                _ => {
                    // If both are 1, it means there is no device active and there is no point in continuing
                    state.finished = true;
                    return None;
                }
            };
            value >>= 1;
            if push == 1 {
                value |= 1 << 63;
            }
            tx.wait_push(push).await;
            let _ = rx.wait_pull().await; // Discard the result of the write action
        }

        state.last_discrepancy = last_zero;
        state.finished = last_zero == 0;
        state.last_rom = value;
        Some(value)
    }

    /// Switches FIFO thresholds to one-bit search mode and resumes at the bit entry point.
    fn prepare_search(&mut self) {
        self.cfg.shift_in.threshold = 1;
        self.cfg.shift_in.direction = ShiftDirection::Left;
        self.cfg.shift_out.threshold = 1;

        self.sm.set_enable(false);
        self.sm.set_config(&self.cfg);

        // set_config jumps to the wrong address so jump to the right one here
        unsafe {
            self.sm.exec_jmp(self.next_bit_addr);
        }
        self.sm.set_enable(true);
    }

    /// Discards FIFOs/shift state, releases the pin, restores byte thresholds and disables strong pull-up counting.
    /// Resumes at the data entry point; does not emit a reset pulse to attached devices.
    fn restore_after_search(&mut self) {
        self.cfg.shift_in.threshold = 8;
        self.cfg.shift_in.direction = ShiftDirection::Right;
        self.cfg.shift_out.threshold = 8;

        self.sm.set_enable(false);
        self.sm.set_config(&self.cfg);

        // Clear the state in case we aborted prematurely with some bits still in the shift registers
        self.sm.clear_fifos();
        self.sm.restart();
        self.sm.set_pin_dirs(Direction::In, &[&self.pin]);
        self.sm.set_pins(Level::Low, &[&self.pin]);

        // set_config jumps to the wrong address so jump to the right one here
        unsafe {
            self.sm.set_y(u32::MAX);
            self.sm.exec_jmp(self.next_bit_addr);
        }
        self.sm.set_enable(true);
    }
}

/// Restores bus state after incomplete operations and every search, without undoing device-side effects.
struct BusGuard<'a, 'd, PIO: Instance, const SM: usize> {
    /// Exclusively borrowed bus to recover on drop.
    bus: &'a mut PioOneWire<'d, PIO, SM>,
    /// True until normal byte/reset completion; searches deliberately leave recovery armed.
    active: bool,
}

impl<PIO: Instance, const SM: usize> Drop for BusGuard<'_, '_, PIO, SM> {
    /// Restores byte mode/high impedance when cleanup remains armed.
    fn drop(&mut self) {
        if self.active {
            self.bus.restore_after_search();
        }
    }
}

#[cfg(test)]
/// Pure presence-sample and strong-pull-up command admission regressions.
mod tests {
    extern crate std;
    use super::*;

    /// Distinguishes all-high sampling from any observed presence-low bit.
    #[test]
    fn idle_high_is_not_presence_and_a_sampled_low_is_presence() {
        assert!(!presence_sample(0xFF00_0000));
        assert!(presence_sample(0xFE00_0000));
        assert!(presence_sample(0));
    }

    /// Admits DS18B20-style final-zero commands and rejects empty/final-one input.
    #[test]
    fn pullup_requires_a_nonempty_command_ending_in_a_zero_bit() {
        assert_eq!(pullup_counter(&[0x44]), 7);
        assert_eq!(pullup_counter(&[0xCC, 0x48]), 15);
        assert!(std::panic::catch_unwind(|| pullup_counter(&[])).is_err());
        assert!(std::panic::catch_unwind(|| pullup_counter(&[0x80])).is_err());
    }
}

/// Onewire search state
pub struct PioOneWireSearch {
    /// Last complete raw ROM, used to revisit earlier discrepancy branches.
    last_rom: u64,
    /// One-based last zero branch (1..=64); zero means no remaining discrepancy.
    last_discrepancy: u8,
    /// True after no presence/responders or the final ROM branch.
    finished: bool,
}

impl PioOneWireSearch {
    /// Returns a fresh search before its first bus reset, with no previous ROM or discrepancy.
    pub fn new() -> Self {
        Self {
            last_rom: 0,
            last_discrepancy: 0,
            finished: false,
        }
    }

    /// Returns the next raw ROM using `pio`, or None without bus traffic if already finished.
    /// Does not validate ROM CRC; cancellation leaves any previously completed search state available for retry.
    pub async fn next<PIO: Instance, const SM: usize>(
        &mut self,
        pio: &mut PioOneWire<'_, PIO, SM>,
    ) -> Option<u64> {
        if self.finished { None } else { pio.search(self).await }
    }

    /// Returns whether traversal stopped, including no presence or an invalid/nonresponding search branch.
    pub fn is_finished(&self) -> bool { self.finished }
}
