const RAM_START: u32 = 0x2000_0000;
const RAM_SIZE: u32 = 512 * 1024;
const RAM_END: u32 = RAM_START + RAM_SIZE;

pub const FRAMEBUFFER_SIZE: usize = 320 * 240 * 2;
pub const TOTAL_FRAMEBUFFER_SIZE: usize = FRAMEBUFFER_SIZE * 2;

#[derive(Clone, Copy, Default)]
pub struct MemoryStats {
    pub stack_used: u32,
    pub stack_total: u32,
    pub static_ram: u32,
    pub ram_total: u32,
}

impl MemoryStats {
    pub fn collect() -> Self {
        let stack_ptr: u32;
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) stack_ptr);
        }

        let stack_used = if (RAM_START..=RAM_END).contains(&stack_ptr) {
            RAM_END.saturating_sub(stack_ptr)
        } else {
            0
        };

        let static_estimate = TOTAL_FRAMEBUFFER_SIZE as u32 + 32 * 1024;
        let stack_total = RAM_SIZE.saturating_sub(static_estimate);

        Self {
            stack_used,
            stack_total,
            static_ram: static_estimate,
            ram_total: RAM_SIZE,
        }
    }
}
