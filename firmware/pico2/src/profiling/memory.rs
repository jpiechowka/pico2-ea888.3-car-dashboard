//! Conservative main-SRAM accounting and instantaneous stack-pointer observations.
/// Start of the admitted 512 KiB main SRAM bank range.
const RAM_START: u32 = 0x2000_0000;
/// Bytes admitted in main SRAM; the two separate 4 KiB banks are not counted.
const RAM_SIZE: u32 = 512 * 1024;
/// Exclusive end of the main SRAM range and initial core0 stack top.
const RAM_END: u32 = RAM_START + RAM_SIZE;

/// Observe this core's SP without reading the memory to which it points.
/// Returns the raw current stack address; this is not a continuous high-water mark.
pub fn stack_pointer() -> u32 {
    let stack_ptr: u32;
    unsafe {
        core::arch::asm!("mov {}, sp", out(reg) stack_ptr, options(nomem, nostack, preserves_flags));
    }
    stack_ptr
}

/// Bytes in one RGB565 framebuffer; two fixed buffers are allocated by the application.
pub const FRAMEBUFFER_SIZE: usize = crate::display::framebuffer::FRAME_BYTES;

/// Single definition used by the core 1 stack allocation and diagnostic bounds.
pub const CORE1_STACK_BYTES: u32 = 32 * 1024;

#[derive(Clone, Copy, Default)]
/// Main-SRAM static allocation and one stack sample, in bytes.
pub struct MemoryStats {
    /// Depth below core0 stack top, or `None` when SP is outside the admitted stack.
    pub stack_used: Option<u32>,
    /// Space from static allocation end to SRAM end; includes stack and free headroom.
    pub stack_total: u32,
    /// Linked initialized/uninitialized static bytes below the heap boundary.
    pub static_ram: u32,
    /// Conservative 512 KiB main-SRAM size, excluding the small auxiliary banks.
    pub ram_total: u32,
}

impl MemoryStats {
    /// Returns linker-derived static bounds and the current core0 stack sample.
    /// Panics if `__sheap` lies outside main SRAM; call on core0 for meaningful
    /// stack ownership. Does not scan memory or measure physical high-water usage.
    pub fn collect() -> Self {
        let stack_ptr = stack_pointer();
        // memory.x explicitly anchors this to aligned __euninit. The runtime's
        // default location counter is moved into FLASH by our boot-info section.
        unsafe extern "C" {
            /// Linker-provided static allocation end, anchored to aligned `__euninit`.
            static __sheap: u8;
        }
        let static_end = core::ptr::addr_of!(__sheap) as u32;
        assert!((RAM_START..=RAM_END).contains(&static_end), "invalid static RAM end");

        let stack_used = dashboard_core::stack_samples::StackSamples::new(static_end, RAM_END)
            .and_then(|mut sample| sample.observe(stack_ptr));

        let static_ram = static_end - RAM_START;
        let stack_total = RAM_SIZE - static_ram;

        Self {
            stack_used,
            stack_total,
            static_ram,
            ram_total: RAM_SIZE,
        }
    }
}
