//! PIO instruction relocation within the 32-word instruction address space.
use pio::{Program, SideSet, Wrap};

/// Borrowed instruction stream translating JMP targets by a modular origin offset.
pub struct CodeIterator<'a, I>
where
    I: Iterator<Item = &'a u16>,
{
    /// Remaining original program words.
    iter: I,
    /// Relocation offset applied modulo 32 to five-bit JMP addresses.
    offset: u8,
}

impl<'a, I: Iterator<Item = &'a u16>> CodeIterator<'a, I> {
    /// Returns a lazy relocation view over `iter`, shifting JMP targets by `offset` modulo 32.
    pub fn new(
        iter: I,
        offset: u8,
    ) -> CodeIterator<'a, I> {
        CodeIterator { iter, offset }
    }
}

impl<'a, I> Iterator for CodeIterator<'a, I>
where
    I: Iterator<Item = &'a u16>,
{
    /// Original instruction with only JMP address bits potentially relocated.
    type Item = u16;

    /// Returns the next relocated instruction, or None when the source iterator is exhausted.
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|&instr| {
            if instr & 0b1110_0000_0000_0000 == 0 {
                // this is a JMP instruction -> add offset to address
                let address = (instr & 0b1_1111) as u8;
                let address = address.wrapping_add(self.offset) % 32;
                instr & (!0b11111) | address as u16
            } else {
                instr
            }
        })
    }
}

/// Borrowed PIO program with a chosen origin; no instruction memory is allocated or validated.
pub struct RelocatedProgram<'a, const PROGRAM_SIZE: usize> {
    /// Original code and wrap/side-set metadata.
    program: &'a Program<PROGRAM_SIZE>,
    /// Requested load origin; code/wrap arithmetic uses it modulo 32.
    origin: u8,
}

impl<'a, const PROGRAM_SIZE: usize> RelocatedProgram<'a, PROGRAM_SIZE> {
    /// Returns a borrowed `program` view at `origin`; caller must admit instruction-memory fit.
    pub fn new_with_origin(
        program: &Program<PROGRAM_SIZE>,
        origin: u8,
    ) -> RelocatedProgram<'_, PROGRAM_SIZE> {
        RelocatedProgram { program, origin }
    }

    /// Returns a lazy iterator relocating JMP addresses without copying the program.
    pub fn code(&'a self) -> CodeIterator<'a, core::slice::Iter<'a, u16>> {
        CodeIterator::new(self.program.code.iter(), self.origin)
    }

    /// Returns wrap source/target shifted by origin modulo 32.
    pub fn wrap(&self) -> Wrap {
        let wrap = self.program.wrap;
        let origin = self.origin;
        Wrap {
            source: wrap.source.wrapping_add(origin) % 32,
            target: wrap.target.wrapping_add(origin) % 32,
        }
    }

    /// Returns unchanged side-set encoding metadata.
    pub fn side_set(&self) -> SideSet { self.program.side_set }

    /// Returns the requested origin byte, without reducing it modulo 32.
    pub fn origin(&self) -> u8 { self.origin }
}
