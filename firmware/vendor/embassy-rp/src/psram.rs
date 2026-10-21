//! PSRAM driver for APS6404L and compatible devices
//!
//! This driver provides support for PSRAM (Pseudo-Static RAM) devices connected via QMI CS1.
//! It handles device verification, initialization, and memory-mapped access configuration.
//!
//! This driver is only available on RP235x chips as it requires the QMI CS1 peripheral.

// Credit: Initially based on https://github.com/Altaflux/gb-rp2350 (also licensed Apache 2.0 + MIT).
// Copyright (c) Altaflux

#![cfg(feature = "_rp235x")]

use embassy_hal_internal::drop::OnDrop;

use crate::pac;
use crate::qmi_cs1::QmiCs1;

/// PSRAM errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// PSRAM device is not detected or not supported
    DeviceNotFound,
    /// Invalid configuration
    InvalidConfig,
    /// Detected PSRAM size does not match the expected size
    SizeMismatch,
}

/// PSRAM device verification type.
#[derive(Clone, Copy)]
pub enum VerificationType {
    /// Skip device verification
    None,
    /// Verify as APS6404L device
    Aps6404l,
}

/// Memory configuration.
#[derive(Clone)]
pub struct Config {
    /// System clock frequency in Hz
    pub clock_hz: u32,
    /// Maximum memory operating frequency in Hz
    pub max_mem_freq: u32,
    /// Maximum CS assert time in microseconds (must be <= 8 us)
    pub max_select_us: u32,
    /// Minimum CS deassert time in nanoseconds (must be >= 18 ns)
    pub min_deselect_ns: u32,
    /// Cooldown period between operations (in SCLK cycles)
    pub cooldown: u8,
    /// Page break size for memory operations
    pub page_break: PageBreak,
    /// Clock divisor for direct mode operations during initialization
    pub init_clkdiv: u8,
    /// Enter Quad Mode command
    pub enter_quad_cmd: Option<u8>,
    /// Quad Read command (fast read with 4-bit data)
    pub quad_read_cmd: u8,
    /// Quad Write command (page program with 4-bit data)
    pub quad_write_cmd: Option<u8>,
    /// Number of dummy bits (a multiple of 4, up to 28); quad mode clocks four bits per cycle
    pub dummy_cycles: u8,
    /// Read format configuration
    pub read_format: FormatConfig,
    /// Write format configuration
    pub write_format: Option<FormatConfig>,
    /// Expected memory size in bytes
    pub mem_size: usize,
    /// Device verification type
    pub verification_type: VerificationType,
    /// Whether the memory is writable via XIP (e.g., PSRAM vs. read-only flash)
    pub xip_writable: bool,
}

/// Page break configuration for memory window operations.
#[derive(Clone, Copy)]
pub enum PageBreak {
    /// No page breaks
    None,
    /// Break at 256-byte boundaries
    _256,
    /// Break at 1024-byte boundaries
    _1024,
    /// Break at 4096-byte boundaries
    _4096,
}

/// Format configuration for read/write operations.
#[derive(Clone)]
pub struct FormatConfig {
    /// Width of command prefix phase
    pub prefix_width: Width,
    /// Width of address phase
    pub addr_width: Width,
    /// Width of command suffix phase
    pub suffix_width: Width,
    /// Width of dummy/turnaround phase
    pub dummy_width: Width,
    /// Width of data phase
    pub data_width: Width,
    /// Length of prefix (None or 8 bits)
    pub prefix_len: bool,
    /// Length of suffix (None or 8 bits)
    pub suffix_len: bool,
}

/// Interface width for different phases of SPI transfer.
#[derive(Clone, Copy)]
pub enum Width {
    /// Single-bit (standard SPI)
    Single,
    /// Dual-bit (2 data lines)
    Dual,
    /// Quad-bit (4 data lines)
    Quad,
}

impl Default for Config {
    /// Returns the APS6404L preset; caller must replace its assumed 125 MHz system clock.
    fn default() -> Self { Self::aps6404l() }
}

impl Config {
    /// Returns the 8 MiB APS6404L quad-I/O preset with an assumed 125 MHz system clock.
    /// This does not measure clocks or identify a device; update `clock_hz` before use.
    pub fn aps6404l() -> Self {
        Self {
            clock_hz: 125_000_000,        // Default to 125MHz
            max_mem_freq: 133_000_000,    // APS6404L max frequency
            max_select_us: 8,             // 8 microseconds max CS assert
            min_deselect_ns: 18,          // 18 nanoseconds min CS deassert
            cooldown: 1,                  // 1 SCLK cycle cooldown
            page_break: PageBreak::_1024, // 1024-byte page boundaries
            init_clkdiv: 10,              // Medium clock for initialization
            enter_quad_cmd: Some(0x35),   // Enter Quad Mode
            quad_read_cmd: 0xEB,          // Fast Quad Read
            quad_write_cmd: Some(0x38),   // Quad Page Program
            dummy_cycles: 24,             // 24 dummy bits, six quad-clock cycles
            read_format: FormatConfig {
                prefix_width: Width::Quad,
                addr_width: Width::Quad,
                suffix_width: Width::Quad,
                dummy_width: Width::Quad,
                data_width: Width::Quad,
                prefix_len: true,  // 8-bit prefix
                suffix_len: false, // No suffix
            },
            write_format: Some(FormatConfig {
                prefix_width: Width::Quad,
                addr_width: Width::Quad,
                suffix_width: Width::Quad,
                dummy_width: Width::Quad,
                data_width: Width::Quad,
                prefix_len: true,  // 8-bit prefix
                suffix_len: false, // No suffix
            }),
            mem_size: 8 * 1024 * 1024, // 8MB for APS6404L
            verification_type: VerificationType::Aps6404l,
            xip_writable: true, // PSRAM is writable
        }
    }

    /// Returns an unchecked configuration: `clock_hz` is actual clk_sys Hz;
    /// `max_mem_freq` caps SCLK Hz; `max_select_us`/`min_deselect_ns` bound CS timing;
    /// `cooldown` is SCLK cycles and `page_break` controls automatic burst splitting.
    /// `init_clkdiv` controls direct-command SCLK (`0` encodes 256).
    /// `enter_quad_cmd` optionally enters quad mode; `quad_read_cmd` and
    /// `quad_write_cmd` set mapping prefixes. `dummy_cycles` counts dummy bits,
    /// not clocks. `read_format`/`write_format` select phase widths; `mem_size` is
    /// bytes, `verification_type` chooses identity checking, and `xip_writable`
    /// enables memory-mapped writes. `Psram::new` validates representable timing.
    pub fn custom(
        clock_hz: u32,
        max_mem_freq: u32,
        max_select_us: u32,
        min_deselect_ns: u32,
        cooldown: u8,
        page_break: PageBreak,
        init_clkdiv: u8,
        enter_quad_cmd: Option<u8>,
        quad_read_cmd: u8,
        quad_write_cmd: Option<u8>,
        dummy_cycles: u8,
        read_format: FormatConfig,
        write_format: Option<FormatConfig>,
        mem_size: usize,
        verification_type: VerificationType,
        xip_writable: bool,
    ) -> Self {
        Self {
            clock_hz,
            max_mem_freq,
            max_select_us,
            min_deselect_ns,
            cooldown,
            page_break,
            init_clkdiv,
            enter_quad_cmd,
            quad_read_cmd,
            quad_write_cmd,
            dummy_cycles,
            read_format,
            write_format,
            mem_size,
            verification_type,
            xip_writable,
        }
    }
}

/// PSRAM driver.
pub struct Psram<'d> {
    /// Keeps exclusive CS1 ownership for the mapping lifetime.
    #[allow(dead_code)]
    qmi_cs1: QmiCs1<'d>,
    /// Configured byte size, identity-checked only when verification is enabled.
    size: usize,
}

impl<'d> Psram<'d> {
    /// Consumes `qmi_cs1`, validates `config` and returns an initialized CS1 mapping.
    /// Invalid timings fail before QMI access; identity rejection may already have
    /// exited device quad mode. Optional identity checks distinguish absent/unknown
    /// devices from size mismatch. Core1 is paused and local IRQs masked during
    /// direct mode; all other XIP users (including DMA) must already be quiescent.
    /// Hardware waits have no timeout. RISC-V direct operations panic as unsupported.
    /// A returned pointer does not establish Rust allocation or aliasing ownership.
    pub fn new(
        qmi_cs1: QmiCs1<'d>,
        config: Config,
    ) -> Result<Self, Error> {
        timing(&config)?; // Validate before pausing a core or touching QMI.
        let qmi = pac::QMI;
        let xip = pac::XIP_CTRL;

        // Verify PSRAM device if requested
        match config.verification_type {
            VerificationType::Aps6404l => {
                Self::verify_aps6404l(&qmi, config.mem_size)?;
                debug!("APS6404L PSRAM verified, size: {:#x}", config.mem_size);
            }
            VerificationType::None => {
                debug!("Skipping PSRAM verification, assuming size: {:#x}", config.mem_size);
            }
        }

        // Initialize PSRAM with proper timing
        Self::init_psram(&qmi, &xip, &config)?;

        Ok(Self {
            qmi_cs1,
            size: config.mem_size,
        })
    }

    /// Returns configured bytes; this is not a physical measurement when verification is disabled.
    pub fn size(&self) -> usize { self.size }

    /// Returns the CS1 XIP base pointer, 0x11000000. Dereferencing requires valid
    /// bounds, alignment, lifetime and exclusive mutable-access discipline; this
    /// pointer alone does not guarantee writable configuration or device presence.
    pub fn base_address(&self) -> *mut u8 { 0x1100_0000 as *mut u8 }

    /// Uses `qmi` to read identity and returns success only for a recognized size
    /// equal to `expected_size` bytes. Pauses core1/masks IRQs; unknown identity and
    /// mismatched capacity return distinct errors. Busy waits have no timeout.
    #[unsafe(link_section = ".data.ram_func")]
    #[inline(never)]
    fn verify_aps6404l(
        qmi: &pac::qmi::Qmi,
        expected_size: usize,
    ) -> Result<(), Error> {
        crate::multicore::pause_core1();
        let _resume = OnDrop::new(crate::multicore::resume_core1);
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        critical_section::with(|_| {
            let (kgd, eid) = unsafe { Self::read_aps6404l_kgd_eid(qmi) };
            verify_identity(kgd, eid, expected_size)
        })
    }

    #[unsafe(link_section = ".data.ram_func")]
    #[inline(never)]
    /// Returns the KGD/EID words read through `qmi`, after issuing exit-quad 0xF5.
    /// Busy waits have no timeout; RISC-V panics, native compilation returns zeros.
    /// # Safety
    /// Execute from RAM with both cores/IRQs and all other XIP users quiescent;
    /// `qmi` must address RP235x QMI with exclusive direct-mode ownership.
    unsafe fn read_aps6404l_kgd_eid(qmi: &pac::qmi::Qmi) -> (u32, u32) {
        /// Exit quad mode opcode (the retained identifier predates this clarification).
        const RESET_ENABLE_CMD: u8 = 0xF5;
        /// Single-lane manufacturer/device identification opcode.
        const READ_ID_CMD: u8 = 0x9F;

        #[allow(unused_assignments)]
        let mut kgd: u32 = 0;
        #[allow(unused_assignments)]
        let mut eid: u32 = 0;

        let qmi_base = qmi.as_ptr() as usize;

        #[cfg(target_arch = "arm")]
        core::arch::asm!(
        // Configure DIRECT_CSR: shift clkdiv (30) to bits 29:22 and set EN (bit 0)
        "movs {temp}, #30",
        "lsls {temp}, {temp}, #22",
        "orr {temp}, {temp}, #1",        // Set EN bit
        "str {temp}, [{qmi_base}]",

        // Poll for BUSY to clear before first operation
        "1:",
        "ldr {temp}, [{qmi_base}]",
        "lsls {temp}, {temp}, #30",      // Shift BUSY bit to sign position
        "bmi 1b",                        // Branch if negative (BUSY = 1)

        // Assert CS1N (bit 3)
        "ldr {temp}, [{qmi_base}]",
        "orr {temp}, {temp}, #8",        // Set ASSERT_CS1N bit (bit 3)
        "str {temp}, [{qmi_base}]",

        // Transmit RESET_ENABLE_CMD as quad
        // DIRECT_TX: OE=1 (bit 19), IWIDTH=2 (bits 17:16), DATA=RESET_ENABLE_CMD
        "movs {temp}, {reset_enable_cmd}",
        "orr {temp}, {temp}, #0x80000",  // Set OE (bit 19)
        "orr {temp}, {temp}, #0x20000",  // Set IWIDTH=2 (quad, bits 17:16)
        "str {temp}, [{qmi_base}, #4]",  // Store to DIRECT_TX

        // Wait for BUSY to clear
        "2:",
        "ldr {temp}, [{qmi_base}]",
        "lsls {temp}, {temp}, #30",
        "bmi 2b",

        // Read and discard RX data
        "ldr {temp}, [{qmi_base}, #8]",

        // Deassert CS1N
        "ldr {temp}, [{qmi_base}]",
        "bic {temp}, {temp}, #8",        // Clear ASSERT_CS1N bit
        "str {temp}, [{qmi_base}]",

        // Assert CS1N again
        "ldr {temp}, [{qmi_base}]",
        "orr {temp}, {temp}, #8",        // Set ASSERT_CS1N bit
        "str {temp}, [{qmi_base}]",

        // Read ID loop (7 iterations)
        "movs {counter}, #0",            // Initialize counter

        "3:",                            // Loop start
        "cmp {counter}, #0",
        "bne 4f",                        // If not first iteration, send 0xFF

        // First iteration: send READ_ID_CMD
        "movs {temp}, {read_id_cmd}",
        "b 5f",
        "4:",                            // Other iterations: send 0xFF
        "movs {temp}, #0xFF",
        "5:",
        "str {temp}, [{qmi_base}, #4]",  // Store to DIRECT_TX

        // Wait for TXEMPTY
        "6:",
        "ldr {temp}, [{qmi_base}]",
        "lsls {temp}, {temp}, #20",      // Shift TXEMPTY (bit 11) to bit 31
        "bpl 6b",                        // Branch if positive (TXEMPTY = 0)

        // Wait for BUSY to clear
        "7:",
        "ldr {temp}, [{qmi_base}]",
        "lsls {temp}, {temp}, #30",      // Shift BUSY bit to sign position
        "bmi 7b",                        // Branch if negative (BUSY = 1)

        // Read RX data
        "ldr {temp}, [{qmi_base}, #8]",
        "uxth {temp}, {temp}",           // Extract lower 16 bits

        // Store KGD or EID based on iteration
        "cmp {counter}, #5",
        "bne 8f",
        "mov {kgd}, {temp}",             // Store KGD
        "b 9f",
        "8:",
        "cmp {counter}, #6",
        "bne 9f",
        "mov {eid}, {temp}",             // Store EID

        "9:",
        "adds {counter}, #1",
        "cmp {counter}, #7",
        "blt 3b",                        // Continue loop if counter < 7

        // Disable direct mode: clear EN and ASSERT_CS1N
        "movs {temp}, #0",
        "str {temp}, [{qmi_base}]",

        // Memory barriers
        "dmb",
        "dsb",
        "isb",
        qmi_base = in(reg) qmi_base,
        temp = out(reg) _,
        counter = out(reg) _,
        kgd = out(reg) kgd,
        eid = out(reg) eid,
        reset_enable_cmd = const RESET_ENABLE_CMD as u32,
        read_id_cmd = const READ_ID_CMD as u32,
        options(nostack),
        );

        #[cfg(target_arch = "riscv32")]
        unimplemented!("APS6404L PSRAM verification not implemented for RISC-V");

        (kgd, eid)
    }

    /// Validates `config`, then configures `qmi` CS1 and `xip_ctrl` write permission.
    /// Returns InvalidConfig before changes when timing cannot fit; core1 resumes
    /// on scope exit. Direct commands have unbounded waits and RISC-V panics.
    #[unsafe(link_section = ".data.ram_func")]
    #[inline(never)]
    fn init_psram(
        qmi: &pac::qmi::Qmi,
        xip_ctrl: &pac::xip_ctrl::XipCtrl,
        config: &Config,
    ) -> Result<(), Error> {
        let timing = timing(config)?;

        crate::multicore::pause_core1();
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

        let _resume = OnDrop::new(crate::multicore::resume_core1);
        critical_section::with(|_| {
            if let Some(command) = config.enter_quad_cmd {
                unsafe { Self::direct_csr_send_init_command(config, command) };
            }

            write_mapping(qmi, xip_ctrl, config, &timing);
        });

        Ok(())
    }

    #[unsafe(link_section = ".data.ram_func")]
    #[inline(never)]
    /// Sends `init_cmd` using the initialization divider from `config`, then disables direct mode.
    /// Busy waits have no timeout; RISC-V panics and native compilation does nothing.
    /// # Safety
    /// Execute from RAM with QMI/XIP exclusive, both cores and IRQs quiescent,
    /// and DMA or other bus masters prevented from accessing XIP.
    unsafe fn direct_csr_send_init_command(
        config: &Config,
        init_cmd: u8,
    ) {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
        // Full memory barrier
        "dmb",
        "dsb",
        "isb",

        // Configure QMI Direct CSR register
        // Load base address of QMI (0x400D0000)
        "movw {base}, #0x0000",
        "movt {base}, #0x400D",

        // Load init_clkdiv and shift to bits 29:22
        "lsl {temp}, {clkdiv}, #22",

        // OR with EN (bit 0) and AUTO_CS1N (bit 7)
        "orr {temp}, {temp}, #0x81",

        // Store to DIRECT_CSR register
        "str {temp}, [{base}, #0]",

        // Memory barrier
        "dmb",

        // First busy wait loop
        "1:",
        "ldr {temp}, [{base}, #0]",      // Load DIRECT_CSR
        "tst {temp}, #0x2",              // Test BUSY bit (bit 1)
        "bne 1b",                        // Branch if busy

        // Write to Direct TX register
        "mov {temp}, {enter_quad_cmd}",

        // OR with NOPUSH (bit 20)
        "orr {temp}, {temp}, #0x100000",

        // Store to DIRECT_TX register (offset 0x4)
        "str {temp}, [{base}, #4]",

        // Memory barrier
        "dmb",

        // Second busy wait loop
        "2:",
        "ldr {temp}, [{base}, #0]",      // Load DIRECT_CSR
        "tst {temp}, #0x2",              // Test BUSY bit (bit 1)
        "bne 2b",                        // Branch if busy

        // Disable Direct CSR
        "mov {temp}, #0",
        "str {temp}, [{base}, #0]",      // Clear DIRECT_CSR register

        // Full memory barrier to ensure no prefetching
        "dmb",
        "dsb",
        "isb",
        base = out(reg) _,
        temp = out(reg) _,
        clkdiv = in(reg) config.init_clkdiv as u32,
        enter_quad_cmd = in(reg) u32::from(init_cmd),
        options(nostack),
        );

        #[cfg(target_arch = "riscv32")]
        unimplemented!("Direct CSR command sending is not implemented for RISC-V yet");
    }
}

/// Programs CS1 registers in `qmi` and write permission in `xip_ctrl` from `config`
/// and prevalidated `timing`. Caller must exclude concurrent QMI/XIP access.
/// Panics if dummy bits were not validated; absent write fields retain old registers.
fn write_mapping(
    qmi: &pac::qmi::Qmi,
    xip_ctrl: &pac::xip_ctrl::XipCtrl,
    config: &Config,
    timing: &Timing,
) {
    qmi.mem(1).timing().write(|w| {
        w.set_cooldown(config.cooldown);
        w.set_pagebreak(match config.page_break {
            PageBreak::None => pac::qmi::vals::Pagebreak::NONE,
            PageBreak::_256 => pac::qmi::vals::Pagebreak::_256,
            PageBreak::_1024 => pac::qmi::vals::Pagebreak::_1024,
            PageBreak::_4096 => pac::qmi::vals::Pagebreak::_4096,
        });
        w.set_max_select(timing.max_select);
        w.set_min_deselect(timing.min_deselect);
        w.set_rxdelay(timing.rxdelay);
        w.set_clkdiv(timing.divisor);
    });

    // Set PSRAM commands and formats
    qmi.mem(1).rfmt().write(|w| {
        let width_to_pac = |w: Width| {
            match w {
                Width::Single => pac::qmi::vals::PrefixWidth::S,
                Width::Dual => pac::qmi::vals::PrefixWidth::D,
                Width::Quad => pac::qmi::vals::PrefixWidth::Q,
            }
        };

        w.set_prefix_width(width_to_pac(config.read_format.prefix_width));
        w.set_addr_width(match config.read_format.addr_width {
            Width::Single => pac::qmi::vals::AddrWidth::S,
            Width::Dual => pac::qmi::vals::AddrWidth::D,
            Width::Quad => pac::qmi::vals::AddrWidth::Q,
        });
        w.set_suffix_width(match config.read_format.suffix_width {
            Width::Single => pac::qmi::vals::SuffixWidth::S,
            Width::Dual => pac::qmi::vals::SuffixWidth::D,
            Width::Quad => pac::qmi::vals::SuffixWidth::Q,
        });
        w.set_dummy_width(match config.read_format.dummy_width {
            Width::Single => pac::qmi::vals::DummyWidth::S,
            Width::Dual => pac::qmi::vals::DummyWidth::D,
            Width::Quad => pac::qmi::vals::DummyWidth::Q,
        });
        w.set_data_width(match config.read_format.data_width {
            Width::Single => pac::qmi::vals::DataWidth::S,
            Width::Dual => pac::qmi::vals::DataWidth::D,
            Width::Quad => pac::qmi::vals::DataWidth::Q,
        });
        w.set_prefix_len(if config.read_format.prefix_len {
            pac::qmi::vals::PrefixLen::_8
        } else {
            pac::qmi::vals::PrefixLen::NONE
        });
        w.set_suffix_len(if config.read_format.suffix_len {
            pac::qmi::vals::SuffixLen::_8
        } else {
            pac::qmi::vals::SuffixLen::NONE
        });
        w.set_dummy_len(match config.dummy_cycles {
            0 => pac::qmi::vals::DummyLen::NONE,
            4 => pac::qmi::vals::DummyLen::_4,
            8 => pac::qmi::vals::DummyLen::_8,
            12 => pac::qmi::vals::DummyLen::_12,
            16 => pac::qmi::vals::DummyLen::_16,
            20 => pac::qmi::vals::DummyLen::_20,
            24 => pac::qmi::vals::DummyLen::_24,
            28 => pac::qmi::vals::DummyLen::_28,
            _ => unreachable!("validated dummy bits"),
        });
    });

    qmi.mem(1).rcmd().write(|w| w.set_prefix(config.quad_read_cmd));

    if let Some(ref write_format) = config.write_format {
        qmi.mem(1).wfmt().write(|w| {
            w.set_prefix_width(match write_format.prefix_width {
                Width::Single => pac::qmi::vals::PrefixWidth::S,
                Width::Dual => pac::qmi::vals::PrefixWidth::D,
                Width::Quad => pac::qmi::vals::PrefixWidth::Q,
            });
            w.set_addr_width(match write_format.addr_width {
                Width::Single => pac::qmi::vals::AddrWidth::S,
                Width::Dual => pac::qmi::vals::AddrWidth::D,
                Width::Quad => pac::qmi::vals::AddrWidth::Q,
            });
            w.set_suffix_width(match write_format.suffix_width {
                Width::Single => pac::qmi::vals::SuffixWidth::S,
                Width::Dual => pac::qmi::vals::SuffixWidth::D,
                Width::Quad => pac::qmi::vals::SuffixWidth::Q,
            });
            w.set_dummy_width(match write_format.dummy_width {
                Width::Single => pac::qmi::vals::DummyWidth::S,
                Width::Dual => pac::qmi::vals::DummyWidth::D,
                Width::Quad => pac::qmi::vals::DummyWidth::Q,
            });
            w.set_data_width(match write_format.data_width {
                Width::Single => pac::qmi::vals::DataWidth::S,
                Width::Dual => pac::qmi::vals::DataWidth::D,
                Width::Quad => pac::qmi::vals::DataWidth::Q,
            });
            w.set_prefix_len(if write_format.prefix_len {
                pac::qmi::vals::PrefixLen::_8
            } else {
                pac::qmi::vals::PrefixLen::NONE
            });
            w.set_suffix_len(if write_format.suffix_len {
                pac::qmi::vals::SuffixLen::_8
            } else {
                pac::qmi::vals::SuffixLen::NONE
            });
        });
    }

    if let Some(quad_write_cmd) = config.quad_write_cmd {
        qmi.mem(1).wcmd().write(|w| w.set_prefix(quad_write_cmd));
    }

    if config.xip_writable {
        // Enable XIP writable mode for PSRAM
        xip_ctrl.ctrl().modify(|w| w.set_writable_m1(true));
    } else {
        // Disable XIP writable mode
        xip_ctrl.ctrl().modify(|w| w.set_writable_m1(false));
    }
}

/// Validated register-sized QMI timing values.
#[derive(Debug, PartialEq)]
struct Timing {
    /// System-clock cycles per SCLK period, 1..255.
    divisor: u8,
    /// Read sample delay in system-clock cycles, at most 7.
    rxdelay: u8,
    /// CS assertion budget in 64-system-clock units, 1..63; final burst reserved separately.
    max_select: u8,
    /// Additional deselect delay in system-clock cycles, 0..31.
    min_deselect: u8,
}

/// Returns SCLK cycles for `format` prefix, 24-bit address, suffix, `dummy_bits`
/// and one eight-byte XIP line, rounding each phase upward by its bus width.
fn transfer_clocks(
    format: &FormatConfig,
    dummy_bits: u8,
) -> u64 {
    let width = |value| {
        match value {
            Width::Single => 1u64,
            Width::Dual => 2,
            Width::Quad => 4,
        }
    };
    // QMI may complete a whole eight-byte XIP cache line after MAX_SELECT expires.
    (u64::from(format.prefix_len) * 8).div_ceil(width(format.prefix_width))
        + 24u64.div_ceil(width(format.addr_width))
        + (u64::from(format.suffix_len) * 8).div_ceil(width(format.suffix_width))
        + u64::from(dummy_bits).div_ceil(width(format.dummy_width))
        + 64u64.div_ceil(width(format.data_width))
}

/// Returns success when KGD `kgd` and extended ID `eid` encode recognized capacity
/// equal to `expected_size` bytes; distinguishes unknown identity from size mismatch.
fn verify_identity(
    kgd: u32,
    eid: u32,
    expected_size: usize,
) -> Result<(), Error> {
    if kgd != 0x5D || eid > 255 {
        return Err(Error::DeviceNotFound);
    }
    let size = if eid == 0x26 || eid >> 5 == 2 {
        8 << 20
    } else if eid >> 5 == 0 {
        2 << 20
    } else if eid >> 5 == 1 {
        4 << 20
    } else {
        return Err(Error::DeviceNotFound);
    };
    if size == expected_size {
        Ok(())
    } else {
        Err(Error::SizeMismatch)
    }
}

/// Returns representable timing for `config`, or InvalidConfig for zero clocks,
/// unsupported field/size/write combinations or insufficient CS transfer budget.
/// Uses wide arithmetic and reserves one final read/write burst; touches no hardware.
fn timing(config: &Config) -> Result<Timing, Error> {
    if config.clock_hz == 0
        || config.max_mem_freq == 0
        || !(1..=8).contains(&config.max_select_us)
        || config.min_deselect_ns < 18
        || config.cooldown > 3
        || config.dummy_cycles > 28
        || config.dummy_cycles % 4 != 0
        || !(1..=16 * 1024 * 1024).contains(&config.mem_size)
        || (config.xip_writable && (config.write_format.is_none() || config.quad_write_cmd.is_none()))
    {
        return Err(Error::InvalidConfig);
    }
    let clock = u64::from(config.clock_hz);
    let mut divisor = clock.div_ceil(u64::from(config.max_mem_freq));
    if divisor == 1 && clock > 100_000_000 {
        divisor = 2;
    }
    let rxdelay = divisor + u64::from(clock / divisor > 100_000_000);
    let min_deselect = (u64::from(config.min_deselect_ns) * clock)
        .div_ceil(1_000_000_000)
        .saturating_sub(divisor.div_ceil(2));
    let burst = transfer_clocks(&config.read_format, config.dummy_cycles).max(
        config
            .write_format
            .as_ref()
            .map_or(0, |format| transfer_clocks(format, 0)),
    );
    // Reserve a final transfer plus sample/hold margin before the device's limit.
    let budget = u64::from(config.max_select_us) * clock / 1_000_000;
    let max_select = budget
        .checked_sub(burst * divisor + rxdelay + 8)
        .ok_or(Error::InvalidConfig)?
        / 64;
    if divisor > 255 || rxdelay > 7 || min_deselect > 31 || !(1..=63).contains(&max_select) {
        return Err(Error::InvalidConfig);
    }
    Ok(Timing {
        divisor: divisor as u8,
        rxdelay: rxdelay as u8,
        max_select: max_select as u8,
        min_deselect: min_deselect as u8,
    })
}

/// Pure timing, identity and in-memory register contract regressions.
#[cfg(test)]
mod tests {
    use super::*;
    /// Rejects zero frequencies without dividing or touching hardware.
    #[test]
    fn zero_clock_and_frequency_are_controlled_errors() {
        let mut config = Config::default();
        config.clock_hz = 0;
        assert_eq!(timing(&config), Err(Error::InvalidConfig));
        config.clock_hz = 125_000_000;
        config.max_mem_freq = 0;
        assert_eq!(timing(&config), Err(Error::InvalidConfig));
    }
    /// Rejects truncated cooldown fields and nonrepresentable dummy-bit counts.
    #[test]
    fn hardware_field_limits_and_unsupported_formats_are_rejected() {
        let mut config = Config::default();
        config.cooldown = 4;
        assert_eq!(timing(&config), Err(Error::InvalidConfig));
        config.cooldown = 1;
        config.dummy_cycles = 7;
        assert_eq!(timing(&config), Err(Error::InvalidConfig));
    }
    /// Accepts a zero extra deselect field when the half-period already exceeds the minimum.
    #[test]
    fn long_sck_half_period_already_satisfies_deselect_requirement() {
        let mut config = Config::default();
        config.clock_hz = 125_000_000;
        config.max_mem_freq = 20_000_000;
        assert_eq!(timing(&config).unwrap().min_deselect, 0);
    }

    /// Checks calculated timing against independent physical-unit inequalities across supported clocks.
    #[test]
    fn supported_timings_respect_frequency_and_select_limits() {
        for clock in [48_000_000, 100_000_000, 125_000_000, 150_000_000, 280_000_000] {
            let config = Config {
                clock_hz: clock,
                ..Config::default()
            };
            let timing = timing(&config).unwrap();
            let divisor = u64::from(timing.divisor);
            assert!(u64::from(clock) <= u64::from(config.max_mem_freq) * divisor);
            let deselect = u64::from(timing.min_deselect) + divisor.div_ceil(2);
            assert!(deselect * 1_000_000_000 >= u64::from(config.min_deselect_ns) * u64::from(clock));
            let final_read = transfer_clocks(&config.read_format, config.dummy_cycles) * divisor;
            assert!(
                u64::from(timing.max_select) * 64 + final_read + u64::from(timing.rxdelay)
                    <= u64::from(config.max_select_us) * u64::from(clock) / 1_000_000
            );
        }
    }

    /// Rejects invalid boundaries and incomplete writable mappings before QMI access.
    #[test]
    fn impossible_or_unrepresentable_timings_fail_before_register_access() {
        let invalid = [
            Config {
                max_select_us: 0,
                ..Config::default()
            },
            Config {
                max_select_us: 9,
                ..Config::default()
            },
            Config {
                min_deselect_ns: 17,
                ..Config::default()
            },
            Config {
                min_deselect_ns: u32::MAX,
                ..Config::default()
            },
            Config {
                clock_hz: 1,
                ..Config::default()
            },
            Config {
                max_mem_freq: 1,
                ..Config::default()
            },
            Config {
                dummy_cycles: 32,
                ..Config::default()
            },
            Config {
                mem_size: 0,
                ..Config::default()
            },
            Config {
                mem_size: 16 * 1024 * 1024 + 1,
                ..Config::default()
            },
            Config {
                write_format: None,
                ..Config::default()
            },
            Config {
                quad_write_cmd: None,
                ..Config::default()
            },
        ];
        for config in invalid {
            assert_eq!(timing(&config), Err(Error::InvalidConfig));
        }
    }

    /// Separates recognized byte capacities, mismatches and unknown identity encodings.
    #[test]
    fn known_device_sizes_and_unknown_identity_remain_distinct() {
        for (eid, size) in [(0x00, 2 << 20), (0x20, 4 << 20), (0x26, 8 << 20), (0x40, 8 << 20)] {
            assert_eq!(verify_identity(0x5D, eid, size), Ok(()));
            assert_eq!(verify_identity(0x5D, eid, size + 1), Err(Error::SizeMismatch));
        }
        for (kgd, eid) in [(0, 0x26), (0xFF, 0xFF), (0x5D, 0xE0), (0x5D, 256)] {
            assert_eq!(verify_identity(kgd, eid, 8 << 20), Err(Error::DeviceNotFound));
        }
    }

    /// Checks mapping updates independently of initialization commands and clears retained XIP write permission.
    #[test]
    fn mapping_is_configured_without_an_enter_quad_command_and_read_only_clears_writes() {
        let mut qmi_registers = [0u32; 64];
        let mut xip_registers = [0u32; 16];
        let qmi = unsafe { pac::qmi::Qmi::from_ptr(qmi_registers.as_mut_ptr().cast()) };
        let xip = unsafe { pac::xip_ctrl::XipCtrl::from_ptr(xip_registers.as_mut_ptr().cast()) };
        let mut config = Config {
            enter_quad_cmd: None,
            ..Config::default()
        };
        write_mapping(&qmi, &xip, &config, &timing(&config).unwrap());
        assert_eq!(qmi.mem(1).rcmd().read().prefix(), 0xEB);
        assert_eq!(qmi.mem(1).wcmd().read().prefix(), 0x38);
        assert_eq!(qmi.mem(1).rfmt().read().dummy_len(), pac::qmi::vals::DummyLen::_24);
        assert!(xip.ctrl().read().writable_m1());
        config.xip_writable = false;
        config.write_format = None;
        config.quad_write_cmd = None;
        config.quad_read_cmd = 0x03;
        config.dummy_cycles = 0;
        write_mapping(&qmi, &xip, &config, &timing(&config).unwrap());
        assert_eq!(qmi.mem(1).rcmd().read().prefix(), 0x03);
        assert_eq!(qmi.mem(1).rfmt().read().dummy_len(), pac::qmi::vals::DummyLen::NONE);
        assert!(!xip.ctrl().read().writable_m1());
    }
}
