//! [HD44780 display driver](https://www.sparkfun.com/datasheets/LCD/HD44780.pdf)

use crate::pio::{
    Common,
    Config,
    Direction,
    FifoJoin,
    Instance,
    Irq,
    LoadedProgram,
    PioPin,
    ShiftConfig,
    ShiftDirection,
    StateMachine,
};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;
use crate::{Peri, dma, interrupt};

/// Fixed-origin initialization program consuming `wait:24 | command:4 | zero:4` words, MSB first.
pub struct PioHD44780CommandWordProgram<'a, PIO: Instance> {
    /// Loaded fixed-origin instruction allocation.
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioHD44780CommandWordProgram<'a, PIO> {
    /// Loads the fixed-origin program through `common`; returns its allocation handle.
    /// Panics if any required instruction address is occupied.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = command_word_program();

        let prg = common.load_program(&prg);

        Self { prg }
    }
}

/// Returns the fixed-origin initialization program, including its completion IRQ instruction.
fn command_word_program() -> pio::Program<32> {
    pio::pio_asm!(
        r#"
                .side_set 1 opt
                .origin 20

                loop:
                    out x,     24
                delay:
                    jmp x--,   delay
                    out pins,  4     side 1
                    out null,  4     side 0
                    jmp !osre, loop
                irq 0 rel
            "#,
    )
    .program
}

/// Fixed-origin byte program consuming `!rs:1 | count_minus_one:7` headers followed by data bytes.
pub struct PioHD44780CommandSequenceProgram<'a, PIO: Instance> {
    /// Loaded fixed-origin instruction allocation.
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioHD44780CommandSequenceProgram<'a, PIO> {
    /// Loads the fixed-origin program through `common`; returns its allocation handle.
    /// Panics if any required instruction address is occupied.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        // many side sets are only there to free up a delay bit!
        let prg = pio::pio_asm!(
            r#"
                .origin 27
                .side_set 1

                .wrap_target
                pull     side 0
                out  x 1 side 0 ; !rs
                out  y 7 side 0 ; #data - 1

                ; rs/rw to e: >= 60ns
                ; e high time: >= 500ns
                ; e low time: >= 500ns
                ; read data valid after e falling: ~5ns
                ; write data hold after e falling: ~10ns

                loop:
                    pull                 side 0
                    jmp  !x       data   side 0
                command:
                    set  pins     0b00   side 0
                    jmp  shift           side 0
                data:
                    set  pins     0b01   side 0
                shift:
                    out  pins     4      side 1 [9]
                    nop                  side 0 [9]
                    out  pins     4      side 1 [9]
                    mov  osr      null   side 0 [7]
                    out  pindirs  4      side 0
                    set  pins     0b10   side 0
                busy:
                    nop                  side 1 [9]
                    jmp  pin      more   side 0 [9]
                    mov  osr      ~osr   side 1 [9]
                    nop                  side 0 [4]
                    out  pindirs  4      side 0
                    jmp  y--      loop   side 0
                .wrap
                more:
                    nop                  side 1 [9]
                    jmp busy             side 0 [9]
            "#
        );

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// Pio backed HD44780 driver
pub struct PioHD44780<'l, P: Instance, const S: usize> {
    /// Owned DMA channel feeding the display command FIFO.
    dma: dma::Channel<'l>,
    /// Enabled nibble transfer machine, polling DB7 for busy completion.
    sm: StateMachine<'l, P, S>,

    /// Two 16-character lines plus command/header bytes; initially spaces.
    buf: [u8; 40],
}

impl<'l, P: Instance, const S: usize> PioHD44780<'l, P, S> {
    /// Initializes `sm` through `word_prg`, then selects `seq_prg`; returns a two-line display writer.
    /// Registers `rs`/`rw` (consecutive), enable `e` and consecutive `db4`/`db5`/`db6`/`db7` through `common`.
    /// Owns `dma`; `dma_irq` proves its handler and `irq` receives initialization completion.
    /// Panics for pin/divider constraints; waits indefinitely for initialization or device busy release.
    /// Initialization signals the IRQ matching S. Cancellation drops the machine
    /// and DMA ownership but can leave the display partially initialized. DMA completion does not mean display idle.
    pub async fn new<D: dma::ChannelInstance>(
        common: &mut Common<'l, P>,
        mut sm: StateMachine<'l, P, S>,
        mut irq: Irq<'l, P, S>,
        dma: Peri<'l, D>,
        dma_irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'l,
        rs: Peri<'l, impl PioPin>,
        rw: Peri<'l, impl PioPin>,
        e: Peri<'l, impl PioPin>,
        db4: Peri<'l, impl PioPin>,
        db5: Peri<'l, impl PioPin>,
        db6: Peri<'l, impl PioPin>,
        db7: Peri<'l, impl PioPin>,
        word_prg: &PioHD44780CommandWordProgram<'l, P>,
        seq_prg: &PioHD44780CommandSequenceProgram<'l, P>,
    ) -> PioHD44780<'l, P, S> {
        let mut dma_ch = dma::Channel::new(dma, dma_irq);
        let rs = common.make_pio_pin(rs);
        let rw = common.make_pio_pin(rw);
        let e = common.make_pio_pin(e);
        let db4 = common.make_pio_pin(db4);
        let db5 = common.make_pio_pin(db5);
        let db6 = common.make_pio_pin(db6);
        let db7 = common.make_pio_pin(db7);

        sm.set_pin_dirs(Direction::Out, &[&rs, &rw, &e, &db4, &db5, &db6, &db7]);

        let mut cfg = Config::default();
        cfg.use_program(&word_prg.prg, &[&e]);

        // Target 1 MHz PIO clock (each cycle is 1µs)
        cfg.clock_divider = calculate_pio_clock_divider(1_000_000);

        cfg.set_out_pins(&[&db4, &db5, &db6, &db7]);
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            direction: ShiftDirection::Left,
            threshold: 32,
        };
        cfg.fifo_join = FifoJoin::TxOnly;
        sm.set_config(&cfg);

        sm.set_enable(true);
        // init to 8 bit thrice
        sm.tx().push((50000 << 8) | 0x30);
        sm.tx().push((5000 << 8) | 0x30);
        sm.tx().push((200 << 8) | 0x30);
        // init 4 bit
        sm.tx().push((200 << 8) | 0x20);
        // set font and lines
        sm.tx().push((50 << 8) | 0x20);
        sm.tx().push(0b1100_0000);

        irq.wait().await;
        sm.set_enable(false);

        let mut cfg = Config::default();
        cfg.use_program(&seq_prg.prg, &[&e]);

        // Target ~15.6 MHz PIO clock (~64ns/insn)
        cfg.clock_divider = calculate_pio_clock_divider(15_600_000);

        cfg.set_jmp_pin(&db7);
        cfg.set_set_pins(&[&rs, &rw]);
        cfg.set_out_pins(&[&db4, &db5, &db6, &db7]);
        cfg.shift_out.direction = ShiftDirection::Left;
        cfg.fifo_join = FifoJoin::TxOnly;
        sm.set_config(&cfg);

        sm.set_enable(true);

        // display on and cursor on and blinking, reset display
        sm.tx().dma_push(&mut dma_ch, &[0x81u8, 0x0F, 1], false).await;

        Self {
            dma: dma_ch,
            sm,
            buf: [0x20; 40],
        }
    }

    /// Moves the previous bottom line to the top and sends at most 16 bytes from `s` as the new bottom line.
    /// Pads with spaces, truncates longer input without UTF-8 interpretation, then positions the cursor at column 15.
    /// Completion means DMA has fed the FIFO, not LCD completion. Cancellation can leave partial display output
    /// while the software line buffer already contains the new line; there is no rollback or timeout.
    pub async fn add_line(
        &mut self,
        s: &[u8],
    ) {
        // move cursor to 0:0, prepare 16 characters
        self.buf[..3].copy_from_slice(&[0x80, 0x80, 15]);
        // move line 2 up
        self.buf.copy_within(22..38, 3);
        // move cursor to 1:0, prepare 16 characters
        self.buf[19..22].copy_from_slice(&[0x80, 0xC0, 15]);
        // file line 2 with spaces
        self.buf[22..38].fill(0x20);
        // copy input line
        let len = s.len().min(16);
        self.buf[22..22 + len].copy_from_slice(&s[0..len]);
        // set cursor to 1:15
        self.buf[38..].copy_from_slice(&[0x80, 0xCF]);

        self.sm.tx().dma_push(&mut self.dma, &self.buf, false).await;
    }
}

#[cfg(test)]
/// Assembled initialization IRQ routing and unchanged neighboring control-flow contracts.
mod tests {
    use pio::{InstructionOperands as Op, IrqIndexMode, JmpCondition, OutDestination};

    use super::*;

    /// The completion instruction must target the same IRQ as each generic state-machine owner.
    #[test]
    fn initialization_completion_targets_each_state_machine() {
        let program = command_word_program();
        let Op::IRQ {
            clear,
            wait,
            index,
            index_mode,
        } = Op::decode(*program.code.last().unwrap()).unwrap()
        else {
            panic!("initialization must finish with IRQ");
        };
        assert!(!clear && !wait);
        for sm in 0..4 {
            let flag = match index_mode {
                IrqIndexMode::DIRECT => index,
                IrqIndexMode::REL => (index & 4) | ((index + sm) & 3),
                _ => panic!("initialization must signal this PIO block"),
            };
            assert_eq!(flag, sm, "completion must reach the Irq<S> waiter");
        }
    }

    /// IRQ correction must preserve the fixed allocation, delay loop, nibble output and wrap flow.
    #[test]
    fn initialization_timing_and_control_flow_are_preserved() {
        let program = command_word_program();
        assert_eq!(program.origin, Some(20));
        assert_eq!(program.code.len(), 6);
        assert_eq!((program.wrap.target, program.wrap.source), (0, 5));
        let expected = [
            Op::OUT {
                destination: OutDestination::X,
                bit_count: 24,
            },
            Op::JMP {
                condition: JmpCondition::XDecNonZero,
                address: 1,
            },
            Op::OUT {
                destination: OutDestination::PINS,
                bit_count: 4,
            },
            Op::OUT {
                destination: OutDestination::NULL,
                bit_count: 4,
            },
            Op::JMP {
                condition: JmpCondition::OutputShiftRegisterNotEmpty,
                address: 0,
            },
        ];
        for (word, op) in program.code.iter().zip(expected) {
            assert_eq!(*word & 0xE0FF, op.encode());
        }
        assert_eq!(program.code[2] & 0x1800, 0x1800, "enable rises with the nibble");
        assert_eq!(program.code[3] & 0x1800, 0x1000, "enable falls after the nibble");
    }
}
