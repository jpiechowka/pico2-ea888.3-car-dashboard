//! Instructions controlling the PIO.
use pio::{InSource, InstructionOperands, JmpCondition, OutDestination, SetDestination};

use crate::pio::{Instance, StateMachine};

impl<'d, PIO: Instance, const SM: usize> StateMachine<'d, PIO, SM> {
    /// Queues `value` and injects OUT X, 32; returns before checking instruction completion.
    /// # Safety
    /// TX must have room and no older words, autopull must be enabled, and OSR must
    /// be ready to pull this word. Quiesce competing SM/DMA activity and ensure the
    /// injected instruction is valid in the current program state.
    pub unsafe fn set_x(
        &mut self,
        value: u32,
    ) {
        /// OUT encoding transfers all 32 OSR bits to the selected destination.
        const OUT: u16 = InstructionOperands::OUT {
            destination: OutDestination::X,
            bit_count: 32,
        }
        .encode();
        self.tx().push(value);
        self.exec_instr(OUT);
    }

    /// Injects IN X, 32 and returns one unchecked RX word.
    /// # Safety
    /// Enable autopush, ensure RX has room and no older words, and ensure the injected
    /// IN has completed before the RX read. Quiesce competing SM/DMA activity;
    /// otherwise the returned word may be stale or undefined.
    pub unsafe fn get_x(&mut self) -> u32 {
        /// IN encoding shifts all 32 scratch-register bits into ISR.
        const IN: u16 = InstructionOperands::IN {
            source: InSource::X,
            bit_count: 32,
        }
        .encode();
        self.exec_instr(IN);
        self.rx().pull()
    }

    /// Queues `value` and injects OUT Y, 32; returns before checking instruction completion.
    /// # Safety
    /// TX must have room and no older words, autopull must be enabled, and OSR must
    /// be ready to pull this word. Quiesce competing SM/DMA activity and ensure the
    /// injected instruction is valid in the current program state.
    pub unsafe fn set_y(
        &mut self,
        value: u32,
    ) {
        /// OUT encoding transfers all 32 OSR bits to the selected destination.
        const OUT: u16 = InstructionOperands::OUT {
            destination: OutDestination::Y,
            bit_count: 32,
        }
        .encode();
        self.tx().push(value);
        self.exec_instr(OUT);
    }

    /// Injects IN Y, 32 and returns one unchecked RX word.
    /// # Safety
    /// Enable autopush, ensure RX has room and no older words, and ensure the injected
    /// IN has completed before the RX read. Quiesce competing SM/DMA activity;
    /// otherwise the returned word may be stale or undefined.
    pub unsafe fn get_y(&mut self) -> u32 {
        /// IN encoding shifts all 32 scratch-register bits into ISR.
        const IN: u16 = InstructionOperands::IN {
            source: InSource::Y,
            bit_count: 32,
        }
        .encode();
        self.exec_instr(IN);

        self.rx().pull()
    }

    /// Injects SET PINDIRS with five-bit direction mask `data` (1 = output); panics above 31.
    /// # Safety
    /// `data` must fit five bits. SET pin mapping and the program's current state must
    /// permit these direction changes; coordinate other pin/SM users.
    pub unsafe fn set_pindir(
        &mut self,
        data: u8,
    ) {
        let set: u16 = InstructionOperands::SET {
            destination: SetDestination::PINDIRS,
            data,
        }
        .encode();
        self.exec_instr(set);
    }

    /// Injects SET PINS with five-bit level mask `data` (1 = high); panics above 31.
    /// # Safety
    /// `data` must fit five bits. SET pin mapping and the program's current state must
    /// permit these output changes; coordinate other pin/SM users.
    pub unsafe fn set_pin(
        &mut self,
        data: u8,
    ) {
        let set: u16 = InstructionOperands::SET {
            destination: SetDestination::PINS,
            data,
        }
        .encode();
        self.exec_instr(set);
    }

    /// Queues `data` and injects OUT PINS, 32 using configured OUT pin mapping.
    /// # Safety
    /// TX must have room and no older words; enable autopull with OSR ready to pull
    /// this word. Quiesce competing SM/DMA users and permit the resulting pin levels.
    pub unsafe fn set_out_pin(
        &mut self,
        data: u32,
    ) {
        /// OUT encoding transfers all 32 OSR bits to the selected destination.
        const OUT: u16 = InstructionOperands::OUT {
            destination: OutDestination::PINS,
            bit_count: 32,
        }
        .encode();
        self.tx().push(data);
        self.exec_instr(OUT);
    }

    /// Queues `data` and injects OUT PINDIRS, 32 (1 bits select outputs).
    /// # Safety
    /// TX must have room and no older words; enable autopull with OSR ready to pull
    /// this word. Quiesce competing SM/DMA users and permit the resulting directions.
    pub unsafe fn set_out_pindir(
        &mut self,
        data: u32,
    ) {
        /// OUT encoding transfers all 32 OSR bits to the selected destination.
        const OUT: u16 = InstructionOperands::OUT {
            destination: OutDestination::PINDIRS,
            bit_count: 32,
        }
        .encode();
        self.tx().push(data);
        self.exec_instr(OUT);
    }

    /// Injects an unconditional jump to instruction-memory address `to_addr`; panics above 31.
    /// # Safety
    /// `to_addr` must be in 0..32 and refer to valid loaded code for the current SM
    /// configuration; coordinate any running program and externally stalled instruction.
    pub unsafe fn exec_jmp(
        &mut self,
        to_addr: u8,
    ) {
        let jmp: u16 = InstructionOperands::JMP {
            address: to_addr,
            condition: JmpCondition::Always,
        }
        .encode();
        self.exec_instr(jmp);
    }
}
