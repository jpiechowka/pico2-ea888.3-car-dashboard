const RAM_START: u32 = 0x2000_0000;
const RAM_SIZE: u32 = 512 * 1024;
const RAM_END: u32 = RAM_START + RAM_SIZE;

/// Observe this core's SP without reading the memory to which it points.
pub fn stack_pointer() -> u32 {
    let stack_ptr: u32;
    unsafe {
        core::arch::asm!("mov {}, sp", out(reg) stack_ptr, options(nomem, nostack, preserves_flags));
    }
    stack_ptr
}

pub const FRAMEBUFFER_SIZE: usize = crate::display::framebuffer::FRAME_BYTES;

/// Single definition used by the core 1 stack allocation and diagnostic bounds.
pub const CORE1_STACK_BYTES: u32 = 32 * 1024;

#[derive(Clone, Copy, Default)]
pub struct MemoryStats {
    pub stack_used: Option<u32>,
    pub stack_total: u32,
    pub static_ram: u32,
    pub ram_total: u32,
}

impl MemoryStats {
    pub fn collect() -> Self {
        let stack_ptr = stack_pointer();
        // memory.x explicitly anchors this to aligned __euninit. The runtime's
        // default location counter is moved into FLASH by our boot-info section.
        unsafe extern "C" {
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
