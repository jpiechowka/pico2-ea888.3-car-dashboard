const RAM_START: u32 = 0x2000_0000;
const RAM_SIZE: u32 = 512 * 1024;
const RAM_END: u32 = RAM_START + RAM_SIZE;

pub const FRAMEBUFFER_SIZE: usize = 320 * 240 * 2;
pub const TOTAL_FRAMEBUFFER_SIZE: usize = FRAMEBUFFER_SIZE * 2;

#[derive(Clone, Copy, Default)]
pub struct MemoryStats {
    #[allow(dead_code)]
    pub stack_ptr: u32,
    pub stack_used: u32,
    pub stack_total: u32,
    pub static_ram: u32,
    pub ram_total: u32,
}

impl MemoryStats {
    pub fn collect() -> Self {
        let stack_ptr: u32;
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) stack_ptr);
        }
        #[cfg(not(target_arch = "arm"))]
        {
            stack_ptr = 0;
        }

        let stack_used = if (RAM_START..=RAM_END).contains(&stack_ptr) {
            RAM_END.saturating_sub(stack_ptr)
        } else {
            0
        };

        let static_estimate = TOTAL_FRAMEBUFFER_SIZE as u32 + 32 * 1024;
        let stack_total = RAM_SIZE.saturating_sub(static_estimate);

        Self {
            stack_ptr,
            stack_used,
            stack_total,
            static_ram: static_estimate,
            ram_total: RAM_SIZE,
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::manual_checked_ops)]
    pub fn stack_percent(&self) -> u32 {
        if self.stack_total > 0 {
            (self.stack_used * 100) / self.stack_total
        } else {
            0
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::manual_checked_ops)]
    pub fn static_percent(&self) -> u32 {
        if self.ram_total > 0 {
            (self.static_ram * 100) / self.ram_total
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(FRAMEBUFFER_SIZE, 153_600);
        assert_eq!(TOTAL_FRAMEBUFFER_SIZE, 307_200);
        assert_eq!(RAM_SIZE, 524_288);
    }

    #[test]
    fn test_memory_stats_default() {
        let stats = MemoryStats::default();
        assert_eq!(stats.stack_ptr, 0);
        assert_eq!(stats.stack_used, 0);
    }

    #[test]
    fn test_stack_percent() {
        let stats = MemoryStats {
            stack_ptr: 0,
            stack_used: 1000,
            stack_total: 10000,
            static_ram: 0,
            ram_total: 0,
        };
        assert_eq!(stats.stack_percent(), 10);
    }

    #[test]
    fn test_static_percent() {
        let stats = MemoryStats {
            stack_ptr: 0,
            stack_used: 0,
            stack_total: 0,
            static_ram: 307_200,
            ram_total: 524_288,
        };
        assert_eq!(stats.static_percent(), 58);
    }
}
