//! Memory profiling utilities for RP2350.
//!
//! Provides functions to query stack usage and estimate RAM consumption.
//!
//! # Memory Layout (RP2350)
//!
//! - RAM: 512KB at 0x20000000 (striped across SRAM0-7)
//! - SRAM4: 4KB at 0x20080000 (direct mapped)
//! - SRAM5: 4KB at 0x20081000 (direct mapped)
//!
//! # Stack
//!
//! Embassy uses a single main stack. Stack grows downward from the top of RAM.
//! We can measure usage by comparing MSP to the stack start address.

use cortex_m::register::msp;

/// RP2350 RAM configuration.
const RAM_START: u32 = 0x2000_0000;
const RAM_SIZE: u32 = 512 * 1024; // 512KB
const RAM_END: u32 = RAM_START + RAM_SIZE;

/// Known static allocations in this firmware.
///
/// These are large buffers we allocate statically:
/// - 2x framebuffers: 153,600 bytes each = 307,200 bytes total
pub const FRAMEBUFFER_SIZE: usize = 320 * 240 * 2; // 153,600 bytes
pub const TOTAL_FRAMEBUFFER_SIZE: usize = FRAMEBUFFER_SIZE * 2; // 307,200 bytes

/// Memory statistics snapshot.
#[derive(Clone, Copy, Default)]
pub struct MemoryStats {
    /// Current stack pointer value (MSP register).
    #[allow(dead_code)]
    pub stack_ptr: u32,
    /// Estimated stack usage in bytes.
    pub stack_used: u32,
    /// Total stack size (RAM end - heap end, approximate).
    pub stack_total: u32,
    /// Known static RAM usage (framebuffers + estimated overhead).
    pub static_ram: u32,
    /// Total RAM available.
    pub ram_total: u32,
}

impl MemoryStats {
    /// Collect current memory statistics.
    ///
    /// # Note
    /// Stack usage is measured from the current MSP value. The "total" stack
    /// size is estimated since we don't have precise linker symbol access.
    pub fn collect() -> Self {
        let stack_ptr = msp::read();

        // Stack grows down from RAM_END
        // Stack usage = RAM_END - current_SP
        let stack_used = RAM_END.saturating_sub(stack_ptr);

        // Estimate total stack size (RAM minus static allocations)
        // This is approximate - actual stack region depends on linker
        let static_estimate = TOTAL_FRAMEBUFFER_SIZE as u32 + 32 * 1024; // framebuffers + ~32KB other statics
        let stack_total = RAM_SIZE.saturating_sub(static_estimate);

        Self {
            stack_ptr,
            stack_used,
            stack_total,
            static_ram: static_estimate,
            ram_total: RAM_SIZE,
        }
    }

    /// Get stack usage as a percentage.
    #[allow(dead_code)]
    pub fn stack_percent(&self) -> u32 {
        if self.stack_total > 0 {
            (self.stack_used * 100) / self.stack_total
        } else {
            0
        }
    }

    /// Get static RAM usage as a percentage of total.
    #[allow(dead_code)]
    pub fn static_percent(&self) -> u32 {
        if self.ram_total > 0 {
            (self.static_ram * 100) / self.ram_total
        } else {
            0
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(FRAMEBUFFER_SIZE, 153_600);
        assert_eq!(TOTAL_FRAMEBUFFER_SIZE, 307_200);
        assert_eq!(RAM_SIZE, 524_288); // 512KB
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
        // 307200 / 524288 * 100 = ~58%
        assert_eq!(stats.static_percent(), 58);
    }
}
