//! OBD-II Dashboard Firmware for Raspberry Pi Pico 2 (RP2350)
//!
//! Displays the OBD-II dashboard on the Pimoroni PIM715 Display Pack 2.8".
//!
//! # Architecture
//!
//! Uses double buffering for parallel render/flush:
//! - Main task: Renders to buffer A, signals flush, swaps to buffer B, continues rendering
//! - Flush task: Waits for signal, flushes completed buffer via DMA
//!
//! # Button Controls
//!
//! - **X**: Toggle FPS display (Dashboard only)
//! - **Y**: Cycle through pages (Dashboard → Debug → Logs → Dashboard)
//! - **A**: Toggle boost unit BAR/PSI (Dashboard only)
//! - **B**: Reset min/max/avg statistics (Dashboard only)

#![no_std]
#![no_main]
// Crate-level lints (match lib.rs for consistency)
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

// Modules only used in the binary (not testable on host)
mod animations;
mod display;
mod log_buffer;
mod screens;
mod st7789;
mod styles;
mod widgets;

// Re-export testable modules from library for local use
// (These are defined in lib.rs with host-testable code)
mod colors {
    pub use dashboard_pico2::colors::*;
}
mod config {
    pub use dashboard_pico2::config::*;
}
mod cpu_cycles {
    pub use dashboard_pico2::cpu_cycles::*;
}
mod memory {
    pub use dashboard_pico2::memory::*;
}
mod pages {
    pub use dashboard_pico2::pages::*;
}
mod render {
    pub use dashboard_pico2::render::*;
}
mod sensor_state {
    pub use dashboard_pico2::sensor_state::*;
}
mod thresholds {
    pub use dashboard_pico2::thresholds::*;
}

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::spi::Spi;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::prelude::*;
use {defmt_rtt as _, panic_probe as _};

use crate::animations::ColorTransition;
use crate::colors::{BLACK, BLUE, DARK_TEAL, GREEN, ORANGE, RED};
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::pages::Page;
use crate::render::{RenderState, cell_idx};
use crate::sensor_state::SensorState;
use crate::st7789::{DoubleBuffer, St7789Flusher, St7789Renderer};
use crate::thresholds::{
    AFR_LEAN_CRITICAL,
    AFR_OPTIMAL_MAX,
    AFR_RICH,
    AFR_RICH_AF,
    BATT_CRITICAL,
    BATT_WARNING,
    BOOST_EASTER_EGG_BAR,
    BOOST_EASTER_EGG_PSI,
    EGT_DANGER_MANIFOLD,
};
use crate::widgets::{
    SensorDisplayData,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_danger_manifold_popup,
    draw_dividers,
    draw_fps_toggle_popup,
    draw_header,
    draw_reset_popup,
    draw_temp_cell,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    is_low_temp_oil,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};

// =============================================================================
// Double Buffering Synchronization
// =============================================================================

/// Signal to notify flush task which buffer to flush (buffer index).
static FLUSH_SIGNAL: Signal<CriticalSectionRawMutex, usize> = Signal::new();

/// Signal to notify main task that flush is complete.
static FLUSH_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Atomic counter for buffer swaps (for profiling).
static BUFFER_SWAPS: AtomicU32 = AtomicU32::new(0);

/// Atomic counter for times main task waited for flush (for profiling).
static BUFFER_WAITS: AtomicU32 = AtomicU32::new(0);

/// Current buffer being flushed (for profiling display).
static FLUSH_BUFFER_IDX: AtomicUsize = AtomicUsize::new(0);

/// Last flush time in microseconds (for profiling).
static LAST_FLUSH_TIME_US: AtomicU32 = AtomicU32::new(0);

/// Display flush task - runs in parallel with rendering.
///
/// Waits for signal from main task, then flushes the completed buffer to display.
/// This allows the main task to continue rendering to the other buffer.
#[embassy_executor::task]
async fn display_flush_task(flusher: &'static mut St7789Flusher<'static>) {
    info!("Display flush task started");

    loop {
        // Wait for signal with buffer index to flush
        let buffer_idx = FLUSH_SIGNAL.wait().await;
        FLUSH_BUFFER_IDX.store(buffer_idx, Ordering::Relaxed);

        let flush_start = Instant::now();

        // SAFETY: Main task is rendering to the OTHER buffer, so this one is safe to read
        let buffer = unsafe {
            if buffer_idx == 0 {
                &*core::ptr::addr_of!(crate::st7789::FRAMEBUFFER_A)
            } else {
                &*core::ptr::addr_of!(crate::st7789::FRAMEBUFFER_B)
            }
        };

        // Flush buffer to display via DMA
        flusher.flush_buffer(buffer).await;

        LAST_FLUSH_TIME_US.store(flush_start.elapsed().as_micros() as u32, Ordering::Relaxed);

        // Signal completion
        FLUSH_DONE.signal(());
    }
}

// =============================================================================
// Demo Sensor Values (generated by separate async task)
// =============================================================================

/// All demo sensor values generated by the demo task.
#[derive(Clone, Copy, Default)]
pub struct DemoSensorValues {
    pub boost: f32,
    pub oil_temp: f32,
    pub water_temp: f32,
    pub dsg_temp: f32,
    pub iat_temp: f32,
    pub egt_temp: f32,
    pub batt_voltage: f32,
    pub afr: f32,
}

/// Watch channel for sharing demo sensor values between tasks.
/// The demo task writes, the render task reads the latest values.
/// Initialized at compile time (Watch::new() is const).
static DEMO_VALUES: Watch<CriticalSectionRawMutex, DemoSensorValues, 2> = Watch::new();

/// Demo values generation task - runs concurrently with rendering.
/// Generates simulated sensor values using micromath sine waves.
#[embassy_executor::task]
async fn demo_values_task(
    sender: embassy_sync::watch::DynSender<'static, DemoSensorValues>,
    start_time: Instant,
) {
    info!("Demo values task started");

    loop {
        // Time-based animation (independent of frame rate)
        let elapsed_ms = start_time.elapsed().as_millis() as u32;
        let t = elapsed_ms as f32 / 1000.0;

        // Generate demo values using micromath sine waves
        let values = DemoSensorValues {
            boost: 0.5 + 1.5 * micromath::F32(t * 0.5).sin().0.abs(),
            oil_temp: 60.0 + 55.0 * micromath::F32(t * 0.3).sin().0,
            water_temp: 88.0 + 7.0 * micromath::F32(t * 0.4).sin().0,
            dsg_temp: 75.0 + 40.0 * micromath::F32(t * 0.35).sin().0,
            iat_temp: 30.0 + 40.0 * micromath::F32(t * 0.25).sin().0,
            egt_temp: 200.0 + 1000.0 * micromath::F32(t * 0.04).sin().0.abs(),
            batt_voltage: 12.0 + 2.5 * micromath::F32(t * 0.15).sin().0,
            afr: 14.0 + 4.0 * micromath::F32(t * 0.45).sin().0,
        };

        // Send latest values (overwrites previous if not consumed)
        sender.send(values);

        // Generate values at ~100 Hz (faster than render to ensure fresh data)
        Timer::after_millis(10).await;
    }
}

use crate::display::{display_spi_config, get_actual_spi_freq};
use crate::screens::{ProfilingData, draw_logs_page, draw_profiling_page, show_loading_screen, show_welcome_screen};

// =============================================================================
// Popup State Management
// =============================================================================

/// Duration that popups remain visible on screen.
const POPUP_DURATION: Duration = Duration::from_secs(3);

/// Active popup with its start time.
#[derive(Clone, Copy, Debug)]
enum Popup {
    /// "MIN/AVG/MAX RESET" popup.
    Reset(Instant),
    /// "FPS ON/OFF" popup.
    Fps(Instant),
    /// "BOOST: BAR/PSI" popup.
    BoostUnit(Instant),
}

impl Popup {
    /// Get the start time of this popup.
    #[inline]
    const fn start_time(&self) -> Instant {
        match self {
            Self::Reset(t) | Self::Fps(t) | Self::BoostUnit(t) => *t,
        }
    }

    /// Check if this popup has expired.
    #[inline]
    fn is_expired(&self) -> bool { self.start_time().elapsed() >= POPUP_DURATION }

    /// Get the popup kind as a u8 discriminant for RenderState tracking.
    #[inline]
    const fn kind(&self) -> u8 {
        match self {
            Self::Reset(_) => 0,
            Self::Fps(_) => 1,
            Self::BoostUnit(_) => 2,
        }
    }
}

// =============================================================================
// Button Debounce
// =============================================================================

/// Debounce duration in milliseconds.
const DEBOUNCE_MS: u64 = 50;

/// Button debounce state with time-based edge detection.
struct ButtonState {
    was_pressed: bool,
    last_change: Option<Instant>,
}

impl ButtonState {
    const fn new() -> Self {
        Self {
            was_pressed: false,
            last_change: None,
        }
    }

    /// Returns true only on the falling edge (button just pressed).
    /// Buttons are active-low, so `is_low()` means pressed.
    /// Includes debounce logic to prevent multiple triggers from contact bounce.
    fn just_pressed(
        &mut self,
        is_low: bool,
    ) -> bool {
        // Check if state changed
        if is_low != self.was_pressed {
            // Apply debounce: only accept change if enough time has passed
            if let Some(last) = self.last_change
                && last.elapsed() < Duration::from_millis(DEBOUNCE_MS)
            {
                return false;
            }

            self.was_pressed = is_low;
            self.last_change = Some(Instant::now());

            // Return true only on press (falling edge, is_low == true)
            return is_low;
        }

        false
    }
}

// Program metadata for `picotool info`
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2-dashboard"),
    embassy_rp::binary_info::rp_program_description!(c"OBD-II Dashboard for EA888.3 on PIM715 Display"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

// Make framebuffers accessible for the flush task
pub use crate::st7789::{FRAMEBUFFER_A, FRAMEBUFFER_B};

// Ensure only one overclock feature is enabled at a time
#[cfg(any(
    all(
        feature = "cpu250-spi62-1v10",
        any(
            feature = "cpu280-spi70-1v30",
            feature = "cpu300-spi75-1v30",
            feature = "cpu320-spi80-1v40",
            feature = "cpu340-spi85-1v40"
        )
    ),
    all(
        feature = "cpu280-spi70-1v30",
        any(
            feature = "cpu300-spi75-1v30",
            feature = "cpu320-spi80-1v40",
            feature = "cpu340-spi85-1v40"
        )
    ),
    all(
        feature = "cpu300-spi75-1v30",
        any(feature = "cpu320-spi80-1v40", feature = "cpu340-spi85-1v40")
    ),
    all(feature = "cpu320-spi80-1v40", feature = "cpu340-spi85-1v40")
))]
compile_error!(
    "Only one overclock feature can be enabled at a time. Choose one of: cpu250-spi62-1v10, cpu280-spi70-1v30, \
     cpu300-spi75-1v30, cpu320-spi80-1v40, cpu340-spi85-1v40"
);

// =============================================================================
// VREG Voltage Control (RP2350)
// =============================================================================
// Register addresses from RP2350 datasheet and pico-sdk:
// - VREG_CTRL: 0x40100004 - unlock bit at bit 13
// - VREG: 0x4010000c - VSEL in bits [8:4]
// Voltage formula: V = 0.55 + (VSEL × 0.05), range 0.55V to 3.30V
// Reference: https://github.com/nspsck/RP2350_Micropython_voltage_control

/// Read current VREG voltage from hardware registers.
///
/// Returns voltage in millivolts (e.g., 1100 for 1.10V).
#[cfg(target_arch = "arm")]
fn read_vreg_voltage_mv() -> u32 {
    const VREG: *const u32 = 0x4010_000C as *const u32;
    // SAFETY: Reading hardware register
    let vreg_val = unsafe { core::ptr::read_volatile(VREG) };
    let vsel = (vreg_val >> 4) & 0x1F; // VSEL is bits [8:4]
    // V = 0.55 + (VSEL × 0.05), convert to mV
    550 + (vsel * 50)
}

/// Placeholder for non-ARM targets (tests).
#[cfg(not(target_arch = "arm"))]
fn read_vreg_voltage_mv() -> u32 {
    1100 // Default 1.10V for tests
}

/// Set VREG voltage by directly writing to hardware registers.
///
/// The RP2350 VREG is locked by default to prevent accidental voltage changes.
/// This function:
/// 1. Unlocks the VREG by writing the magic value to VREG_CTRL
/// 2. Sets VSEL in the VREG register with magic value
///
/// # Safety
/// - Higher voltages increase power consumption and heat
/// - VREG remains unlocked until power cycle
///
/// # Voltage Formula
/// `voltage = 0.55V + (VSEL × 0.05V)`
/// For 1.40V: VSEL = (1.40 - 0.55) / 0.05 = 17
#[cfg(any(feature = "cpu320-spi80-1v40", feature = "cpu340-spi85-1v40"))]
unsafe fn set_vreg_voltage(vsel: u32) {
    const VREG_CTRL: *mut u32 = 0x4010_0004 as *mut u32;
    const VREG: *mut u32 = 0x4010_000C as *mut u32;
    const VREG_UNLOCK_MAGIC: u32 = 0x5AFE_0000;

    // SAFETY: Writing to VREG registers to set voltage
    // Rust 2024 requires explicit unsafe blocks even inside unsafe fn
    unsafe {
        // Step 1: Unlock VREG control interface
        core::ptr::write_volatile(VREG_CTRL, 0x5AFE_2100);

        // Step 2: Read current value, clear VSEL bits, set new VSEL with magic
        let current = core::ptr::read_volatile(VREG);
        let new_val = (current & 0xFFFF_FE0F) | ((vsel << 4) & 0x1F0) | VREG_UNLOCK_MAGIC;
        core::ptr::write_volatile(VREG, new_val);
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("OBD-II Dashboard starting...");

    // Initialize with optional overclocking
    // cpu250-spi62-1v10: 250 MHz @ 1.10V for 62.5 MHz SPI (250/4)
    #[cfg(feature = "cpu250-spi62-1v10")]
    let p = {
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        const FREQ_HZ: u32 = 250_000_000; // 250 MHz / 4 = 62.5 MHz SPI
        const VOLTAGE: CoreVoltage = CoreVoltage::V1_10; // 1.10V (default, safe)

        let mut config = Config::default();
        config.clocks = ClockConfig::system_freq(FREQ_HZ).expect("Invalid overclock frequency");
        config.clocks.core_voltage = VOLTAGE;
        info!("Overclock: 250 MHz @ 1.10V (SPI 62.5 MHz)");
        embassy_rp::init(config)
    };

    // cpu280-spi70-1v30: 280 MHz @ 1.30V for 70 MHz SPI (280/4)
    #[cfg(feature = "cpu280-spi70-1v30")]
    let p = {
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        const FREQ_HZ: u32 = 280_000_000; // 280 MHz / 4 = 70 MHz SPI
        const VOLTAGE: CoreVoltage = CoreVoltage::V1_30; // 1.30V for stability

        let mut config = Config::default();
        config.clocks = ClockConfig::system_freq(FREQ_HZ).expect("Invalid overclock frequency");
        config.clocks.core_voltage = VOLTAGE;
        info!("Overclock: 280 MHz @ 1.30V (SPI 70 MHz)");
        embassy_rp::init(config)
    };

    // cpu300-spi75-1v30: 300 MHz @ 1.30V for 75 MHz SPI (300/4)
    #[cfg(feature = "cpu300-spi75-1v30")]
    let p = {
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        const FREQ_HZ: u32 = 300_000_000; // 300 MHz / 4 = 75 MHz SPI
        const VOLTAGE: CoreVoltage = CoreVoltage::V1_30; // 1.30V for stability

        let mut config = Config::default();
        config.clocks = ClockConfig::system_freq(FREQ_HZ).expect("Invalid overclock frequency");
        config.clocks.core_voltage = VOLTAGE;
        info!("Overclock: 300 MHz @ 1.30V (SPI 75 MHz)");
        embassy_rp::init(config)
    };

    // cpu320-spi80-1v40: 320 MHz @ 1.40V for 80 MHz SPI (320/4)
    // WARNING: 1.40V requires manual VREG unlock (beyond embassy's 1.30V limit)
    // We set voltage BEFORE init to ensure stability at 320 MHz
    #[cfg(feature = "cpu320-spi80-1v40")]
    let p = {
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        // CRITICAL: Set voltage to 1.40V BEFORE initializing at 320 MHz
        // VSEL 17 = 0.55V + (17 × 0.05V) = 1.40V
        unsafe {
            set_vreg_voltage(17);
        }

        const FREQ_HZ: u32 = 320_000_000; // 320 MHz / 4 = 80 MHz SPI

        let mut config = Config::default();
        config.clocks = ClockConfig::system_freq(FREQ_HZ).expect("Invalid overclock frequency");
        // Embassy will try to set 1.30V, but we already set 1.40V above
        config.clocks.core_voltage = CoreVoltage::V1_30;

        info!("Overclock: 320 MHz @ 1.40V (SPI 80 MHz)");
        embassy_rp::init(config)
    };

    // cpu340-spi85-1v40: 340 MHz @ 1.40V for 85 MHz SPI (340/4)
    // WARNING: 1.40V requires manual VREG unlock (beyond embassy's 1.30V limit)
    // We set voltage BEFORE init to ensure stability at 340 MHz
    #[cfg(feature = "cpu340-spi85-1v40")]
    let p = {
        use embassy_rp::clocks::{ClockConfig, CoreVoltage};
        use embassy_rp::config::Config;

        // CRITICAL: Set voltage to 1.40V BEFORE initializing at 340 MHz
        // VSEL 17 = 0.55V + (17 × 0.05V) = 1.40V
        unsafe {
            set_vreg_voltage(17);
        }

        const FREQ_HZ: u32 = 340_000_000; // 340 MHz / 4 = 85 MHz SPI

        let mut config = Config::default();
        config.clocks = ClockConfig::system_freq(FREQ_HZ).expect("Invalid overclock frequency");
        // Embassy will try to set 1.30V, but we already set 1.40V above
        config.clocks.core_voltage = CoreVoltage::V1_30;

        info!("Overclock: 340 MHz @ 1.40V (SPI 85 MHz)");
        embassy_rp::init(config)
    };

    #[cfg(not(any(
        feature = "cpu250-spi62-1v10",
        feature = "cpu280-spi70-1v30",
        feature = "cpu300-spi75-1v30",
        feature = "cpu320-spi80-1v40",
        feature = "cpu340-spi85-1v40"
    )))]
    let p = embassy_rp::init(Default::default());

    // Initialize DWT cycle counter for CPU utilization measurement
    let cpu_freq_hz = if cfg!(feature = "cpu340-spi85-1v40") {
        340_000_000
    } else if cfg!(feature = "cpu320-spi80-1v40") {
        320_000_000
    } else if cfg!(feature = "cpu300-spi75-1v30") {
        300_000_000
    } else if cfg!(feature = "cpu280-spi70-1v30") {
        280_000_000
    } else if cfg!(feature = "cpu250-spi62-1v10") {
        250_000_000
    } else {
        150_000_000
    };
    cpu_cycles::init(cpu_freq_hz);
    info!("DWT cycle counter initialized at {} MHz", cpu_freq_hz / 1_000_000);

    // Initialize RGB LED (active-low: Low = ON)
    // PIM715: Red=26, Green=27, Blue=28
    let mut _led_r = Output::new(p.PIN_26, Level::High); // Off
    let mut _led_g = Output::new(p.PIN_27, Level::High); // Off
    let mut led_b = Output::new(p.PIN_28, Level::High); // Off (used for heartbeat)

    // Initialize display pins
    // PIM715 pinout: CS=17, DC=16, CLK=18, MOSI=19, Backlight=20
    let cs = Output::new(p.PIN_17, Level::High);
    let dc = Output::new(p.PIN_16, Level::Low);
    let mut _backlight = Output::new(p.PIN_20, Level::High); // Turn on backlight

    // Initialize async SPI with DMA (TX-only, display doesn't need MISO)
    let spi = Spi::new_txonly(p.SPI0, p.PIN_18, p.PIN_19, p.DMA_CH0, display_spi_config());

    // Initialize display flusher and hardware
    let mut flusher = St7789Flusher::new(spi, dc, cs);
    flusher.init().await;

    log_info!("Display initialized");

    // Initialize double buffer
    // SAFETY: Only one DoubleBuffer instance exists
    let mut double_buffer = unsafe { DoubleBuffer::new() };

    // Clear both framebuffers before boot screens to prevent grainy noise
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
    double_buffer.swap();
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(1) }).await;
    double_buffer.swap(); // Back to buffer 0

    // Show boot screens using single-buffer mode for simplicity
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        let mut renderer = St7789Renderer::new(buffer);
        show_loading_screen(&mut renderer).await;
        flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
        show_welcome_screen(&mut renderer).await;
        flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
    }

    // Move flusher to static for task (Embassy tasks need 'static lifetime)
    use static_cell::StaticCell;
    static FLUSHER: StaticCell<St7789Flusher<'static>> = StaticCell::new();
    let flusher: &'static mut St7789Flusher<'static> = FLUSHER.init(flusher);

    // Spawn flush task (takes &'static mut reference, no unsafe needed)
    spawner.spawn(display_flush_task(flusher)).unwrap();
    info!("Display flush task spawned");

    // Initialize buttons (active-low with internal pull-up)
    // PIM715: A=12, B=13, X=14, Y=15
    let btn_a = Input::new(p.PIN_12, Pull::Up);
    let btn_b = Input::new(p.PIN_13, Pull::Up);
    let btn_x = Input::new(p.PIN_14, Pull::Up);
    let btn_y = Input::new(p.PIN_15, Pull::Up);

    // Button debounce state
    let mut btn_a_state = ButtonState::new();
    let mut btn_b_state = ButtonState::new();
    let mut btn_x_state = ButtonState::new();
    let mut btn_y_state = ButtonState::new();

    info!("Buttons initialized!");

    // UI state
    let mut current_page = Page::Dashboard;
    let mut clear_frames_remaining: u8 = 2;
    let mut show_fps = false;
    let mut show_boost_psi = false;
    let mut active_popup: Option<Popup> = None;
    let mut prev_egt_danger_active = false;
    let mut reset_requested = false;

    // Render state
    let mut render_state = RenderState::new();
    let mut frame_count = 0u32;
    let mut current_fps = 0.0f32;
    let mut fps_frame_count = 0u32;
    let mut last_fps_calc = Instant::now();

    // Profiling: track render and flush times (microseconds)
    let mut render_time_us = 0u32;
    let mut flush_time_us = 0u32;
    let mut total_frame_time_us = 0u32;
    let mut last_profile_log = Instant::now();

    // CPU cycle tracking
    let mut frame_cycles_used = 0u32;
    let mut cpu_util_percent = 0u32;

    // Track if flush is in progress (for first frame)
    let mut flush_in_progress = false;

    // Demo sensor values (defaults until first update from demo task)
    let mut boost = 0.5f32;
    let mut oil_temp = 60.0f32;
    let mut water_temp = 88.0f32;
    let mut dsg_temp = 75.0f32;
    let mut iat_temp = 30.0f32;
    let mut egt_temp = 200.0f32;
    let mut batt_voltage = 12.0f32;
    let mut afr = 14.0f32;

    // Sensor states
    let mut oil_state = SensorState::new();
    let mut water_state = SensorState::new();
    let mut dsg_state = SensorState::new();
    let mut iat_state = SensorState::new();
    let mut egt_state = SensorState::new();
    let mut batt_state = SensorState::new();
    let mut afr_state = SensorState::new();

    // Max tracking
    let mut boost_max = 0.0f32;
    let mut oil_max = 0.0f32;
    let mut water_max = 0.0f32;
    let mut dsg_max = 0.0f32;
    let mut iat_max = f32::MIN;
    let mut egt_max = 0.0f32;
    let mut batt_min = f32::MAX;
    let mut batt_max = 0.0f32;

    log_info!("Main loop starting");

    // Color transitions for smooth background changes
    let mut color_transitions = ColorTransition::new();

    // Time-based animation (independent of frame rate)
    let animation_start = Instant::now();

    // Get sender/receiver from static Watch channel (initialized at compile time)
    let mut demo_receiver = DEMO_VALUES.dyn_receiver().unwrap();
    let demo_sender = DEMO_VALUES.dyn_sender();

    // Spawn demo values task on second core (Embassy handles core assignment)
    spawner.spawn(demo_values_task(demo_sender, animation_start)).unwrap();
    info!("Demo values task spawned");

    loop {
        let frame_start = Instant::now();
        let frame_cycles_start = cpu_cycles::read();

        // Time-based blink cycle (200ms per state)
        let elapsed_ms = animation_start.elapsed().as_millis() as u32;
        let blink_on = (elapsed_ms / 200).is_multiple_of(2);

        // Handle button presses
        if btn_x_state.just_pressed(btn_x.is_low()) && current_page == Page::Dashboard {
            show_fps = !show_fps;
            active_popup = Some(Popup::Fps(Instant::now()));
            clear_frames_remaining = 2; // Clear both buffers when FPS toggles
            info!("FPS: {}", if show_fps { "ON" } else { "OFF" });
        }

        if btn_y_state.just_pressed(btn_y.is_low()) {
            current_page = current_page.toggle();
            clear_frames_remaining = 2; // Clear both double buffers on page switch
            active_popup = None;
            log_info!(
                "Page: {}",
                match current_page {
                    Page::Dashboard => "Dashboard",
                    Page::Debug => "Debug",
                    Page::Logs => "Logs",
                }
            );
        }

        if btn_a_state.just_pressed(btn_a.is_low()) && current_page == Page::Dashboard {
            show_boost_psi = !show_boost_psi;
            active_popup = Some(Popup::BoostUnit(Instant::now()));
            info!("Boost: {}", if show_boost_psi { "PSI" } else { "BAR" });
        }

        if btn_b_state.just_pressed(btn_b.is_low()) && current_page == Page::Dashboard {
            reset_requested = true;
            active_popup = Some(Popup::Reset(Instant::now()));
            info!("Reset requested");
        }

        // Check popup expiration
        if let Some(ref popup) = active_popup
            && popup.is_expired()
        {
            active_popup = None;
            clear_frames_remaining = 2; // Clear both buffers when popup closes
        }

        // Update render state (include danger popup in combined visibility)
        let popup_kind = if active_popup.is_some() {
            active_popup.as_ref().map(Popup::kind)
        } else if prev_egt_danger_active {
            Some(3u8) // Danger popup kind
        } else {
            None
        };
        render_state.update_popup(popup_kind);

        // Get demo values from async task (generated on second core)
        // Use try_get() for non-blocking access to latest values
        if let Some(demo_values) = demo_receiver.try_get() {
            boost = demo_values.boost;
            oil_temp = demo_values.oil_temp;
            water_temp = demo_values.water_temp;
            dsg_temp = demo_values.dsg_temp;
            iat_temp = demo_values.iat_temp;
            egt_temp = demo_values.egt_temp;
            batt_voltage = demo_values.batt_voltage;
            afr = demo_values.afr;
        }

        // Handle reset
        if reset_requested {
            oil_state.reset_average();
            oil_state.reset_graph();
            oil_state.reset_peak();
            water_state.reset_average();
            water_state.reset_graph();
            water_state.reset_peak();
            dsg_state.reset_average();
            dsg_state.reset_graph();
            dsg_state.reset_peak();
            iat_state.reset_average();
            iat_state.reset_graph();
            iat_state.reset_peak();
            egt_state.reset_average();
            egt_state.reset_graph();
            egt_state.reset_peak();
            batt_state.reset_average();
            batt_state.reset_graph();
            batt_state.reset_peak();
            afr_state.reset_average();
            afr_state.reset_graph();
            afr_state.reset_peak();

            boost_max = boost;
            oil_max = oil_temp;
            water_max = water_temp;
            dsg_max = dsg_temp;
            iat_max = iat_temp;
            egt_max = egt_temp;
            batt_min = batt_voltage;
            batt_max = batt_voltage;

            reset_requested = false;
            log_info!("Stats reset");
        }

        // Boost easter egg detection
        let show_boost_easter_egg = if show_boost_psi {
            boost * 14.5038 >= BOOST_EASTER_EGG_PSI
        } else {
            boost >= BOOST_EASTER_EGG_BAR
        };

        // Update max values
        let oil_updated = oil_temp > oil_max;
        let water_updated = water_temp > water_max;
        let dsg_updated = dsg_temp > dsg_max;
        let iat_updated = iat_temp > iat_max;
        let egt_updated = egt_temp > egt_max;
        let batt_updated = batt_voltage > batt_max || batt_voltage < batt_min;

        boost_max = boost_max.max(boost);
        oil_max = oil_max.max(oil_temp);
        water_max = water_max.max(water_temp);
        dsg_max = dsg_max.max(dsg_temp);
        iat_max = iat_max.max(iat_temp);
        egt_max = egt_max.max(egt_temp);
        batt_min = batt_min.min(batt_voltage);
        batt_max = batt_max.max(batt_voltage);

        // Update sensor states
        oil_state.update(oil_temp, oil_updated);
        water_state.update(water_temp, water_updated);
        dsg_state.update(dsg_temp, dsg_updated);
        iat_state.update(iat_temp, iat_updated);
        egt_state.update(egt_temp, egt_updated);
        batt_state.update(batt_voltage, batt_updated);
        afr_state.update(afr, false);

        // FPS calculation
        fps_frame_count += 1;
        if last_fps_calc.elapsed() >= Duration::from_secs(1) {
            current_fps = fps_frame_count as f32 / last_fps_calc.elapsed().as_millis() as f32 * 1000.0;
            fps_frame_count = 0;
            last_fps_calc = Instant::now();
        }

        // Calculate EGT danger state (persists across page switches)
        let egt_danger_active = egt_temp >= EGT_DANGER_MANIFOLD;

        // Calculate target colors and update transitions
        // AFR color based on value
        let afr_target = if afr < AFR_RICH_AF {
            BLUE
        } else if afr < AFR_RICH {
            DARK_TEAL
        } else if afr < AFR_OPTIMAL_MAX {
            GREEN
        } else if afr <= AFR_LEAN_CRITICAL {
            ORANGE
        } else {
            RED
        };
        color_transitions.set_target(cell_idx::AFR, afr_target);

        // Battery color based on voltage
        let batt_target = if batt_voltage < BATT_CRITICAL {
            RED
        } else if batt_voltage < BATT_WARNING {
            ORANGE
        } else {
            BLACK
        };
        color_transitions.set_target(cell_idx::BATTERY, batt_target);

        // Temperature cells - get color from color functions
        let (water_target, _) = temp_color_water(water_temp);
        let (oil_target, _) = temp_color_oil_dsg(oil_temp);
        let (dsg_target, _) = temp_color_oil_dsg(dsg_temp);
        let (iat_target, _) = temp_color_iat(iat_temp);
        let (egt_target, _) = temp_color_egt(egt_temp);

        color_transitions.set_target(cell_idx::COOLANT, water_target);
        color_transitions.set_target(cell_idx::OIL, oil_target);
        color_transitions.set_target(cell_idx::DSG, dsg_target);
        color_transitions.set_target(cell_idx::IAT, iat_target);
        color_transitions.set_target(cell_idx::EGT, egt_target);

        // Update color transitions (time-based interpolation for FPS independence)
        color_transitions.update(Instant::now());

        // Profiling: start render timing
        let render_start = Instant::now();

        // Get current render buffer and create renderer
        let buffer = unsafe { double_buffer.render_buffer() };
        let mut display = St7789Renderer::new(buffer);

        // Clear display when needed (both buffers need clearing on page switch or popup close)
        if render_state.is_first_frame() || render_state.popup_just_closed() || clear_frames_remaining > 0 {
            display.clear(BLACK).ok();
            render_state.mark_display_cleared(); // Always mark when cleared

            // When popup closes via render_state, ensure BOTH double buffers get cleared
            // by setting clear_frames_remaining = 1 for the next frame
            if render_state.popup_just_closed() && clear_frames_remaining == 0 {
                clear_frames_remaining = 1;
            } else {
                clear_frames_remaining = clear_frames_remaining.saturating_sub(1);
            }
        }

        // Render based on current page
        match current_page {
            Page::Dashboard => {
                // Draw header
                if render_state.check_header_dirty(show_fps, current_fps) {
                    draw_header(&mut display, show_fps, current_fps);
                }

                // Draw cells
                draw_boost_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    boost,
                    boost_max,
                    show_boost_psi,
                    show_boost_easter_egg,
                    blink_on,
                    0,
                );

                draw_afr_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    afr,
                    &to_display_data(&afr_state),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::AFR)),
                );

                draw_batt_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    batt_voltage,
                    batt_min,
                    batt_max,
                    &to_display_data(&batt_state),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::BATTERY)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "COOL",
                    water_temp,
                    water_max,
                    &to_display_data(&water_state),
                    temp_color_water,
                    is_critical_water,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::COOLANT)),
                );

                draw_temp_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "OIL",
                    oil_temp,
                    oil_max,
                    &to_display_data(&oil_state),
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    Some(is_low_temp_oil),
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::OIL)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "DSG",
                    dsg_temp,
                    dsg_max,
                    &to_display_data(&dsg_state),
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::DSG)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "IAT",
                    iat_temp,
                    iat_max,
                    &to_display_data(&iat_state),
                    temp_color_iat,
                    is_critical_iat,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::IAT)),
                );

                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "EGT",
                    egt_temp,
                    egt_max,
                    &to_display_data(&egt_state),
                    temp_color_egt,
                    is_critical_egt,
                    None::<fn(f32) -> bool>,
                    blink_on,
                    0,
                    Some(color_transitions.get_current(cell_idx::EGT)),
                );

                // Draw dividers
                if render_state.need_dividers() {
                    draw_dividers(&mut display);
                    render_state.mark_dividers_drawn();
                }

                // Render popup (user popup takes priority over danger warning)
                if let Some(ref popup) = active_popup {
                    match popup {
                        Popup::Reset(_) => draw_reset_popup(&mut display),
                        Popup::Fps(_) => draw_fps_toggle_popup(&mut display, show_fps),
                        Popup::BoostUnit(_) => draw_boost_unit_popup(&mut display, show_boost_psi),
                    }
                } else if egt_danger_active {
                    draw_danger_manifold_popup(&mut display, blink_on);
                }
            }

            Page::Debug => {
                // Collect memory stats
                let mem_stats = crate::memory::MemoryStats::collect();

                // Get SPI frequencies (requested from config, actual from hardware)
                let requested_spi_hz = display_spi_config().frequency;
                let actual_spi_hz = get_actual_spi_freq(cpu_freq_hz);

                draw_profiling_page(
                    &mut display,
                    &ProfilingData {
                        // Timing
                        current_fps,
                        frame_count,
                        render_time_us,
                        flush_time_us,
                        total_frame_time_us,
                        // Double buffer stats
                        buffer_swaps: BUFFER_SWAPS.load(Ordering::Relaxed),
                        buffer_waits: BUFFER_WAITS.load(Ordering::Relaxed),
                        render_buffer_idx: double_buffer.render_idx(),
                        flush_buffer_idx: FLUSH_BUFFER_IDX.load(Ordering::Relaxed),
                        // Memory
                        stack_used_kb: if mem_stats.stack_used > 0 && mem_stats.stack_used < 1024 {
                            1
                        } else {
                            mem_stats.stack_used / 1024
                        },
                        stack_total_kb: mem_stats.stack_total / 1024,
                        static_ram_kb: mem_stats.static_ram / 1024,
                        ram_total_kb: mem_stats.ram_total / 1024,
                        // CPU utilization
                        cpu_util_percent,
                        frame_cycles: frame_cycles_used,
                        // SPI clocks
                        requested_spi_mhz: requested_spi_hz / 1_000_000,
                        actual_spi_mhz: actual_spi_hz / 1_000_000,
                        // Voltage (read from hardware)
                        actual_voltage_mv: read_vreg_voltage_mv(),
                    },
                );
            }

            Page::Logs => {
                draw_logs_page(&mut display);
            }
        }

        // Profiling: end render timing
        render_time_us = render_start.elapsed().as_micros() as u32;

        // Wait for previous flush to complete before swapping (if one is in progress)
        if flush_in_progress {
            FLUSH_DONE.wait().await;
            BUFFER_WAITS.fetch_add(1, Ordering::Relaxed);
        }

        // Swap buffers and signal flush task
        let completed_idx = double_buffer.swap();
        BUFFER_SWAPS.fetch_add(1, Ordering::Relaxed);
        FLUSH_SIGNAL.signal(completed_idx);
        flush_in_progress = true;

        // Get flush time from previous frame (atomic read)
        flush_time_us = LAST_FLUSH_TIME_US.load(Ordering::Relaxed);
        total_frame_time_us = frame_start.elapsed().as_micros() as u32;

        // Calculate CPU utilization from cycle counts
        let frame_cycles_end = cpu_cycles::read();
        frame_cycles_used = cpu_cycles::elapsed(frame_cycles_start, frame_cycles_end);
        cpu_util_percent = cpu_cycles::calc_util_percent(frame_cycles_used, total_frame_time_us);

        // Log profiling data every 2 seconds
        if last_profile_log.elapsed() >= Duration::from_secs(2) {
            info!(
                "PROFILE: render={}us flush={}us total={}us ({} FPS) swaps={} waits={}",
                render_time_us,
                flush_time_us,
                total_frame_time_us,
                current_fps as u32,
                BUFFER_SWAPS.load(Ordering::Relaxed),
                BUFFER_WAITS.load(Ordering::Relaxed)
            );
            last_profile_log = Instant::now();
        }

        // Update danger popup state for next frame (outside page match)
        prev_egt_danger_active = egt_danger_active;

        render_state.end_frame();
        frame_count = frame_count.wrapping_add(1);

        // Toggle blue LED every second to show loop is running (time-based)
        if (elapsed_ms / 1000).is_multiple_of(2) {
            led_b.set_low(); // ON
        } else {
            led_b.set_high(); // OFF
        }

        // No artificial delay - run at maximum frame rate
        // Rendering continues immediately while flush runs in parallel
    }
}

/// Convert SensorState to SensorDisplayData for rendering.
fn to_display_data(state: &SensorState) -> SensorDisplayData<'_> {
    let (buffer, start_idx, count, min, max) = state.get_graph_data();
    SensorDisplayData {
        trend: state.get_trend(),
        is_new_peak: state.is_new_peak,
        graph_buffer: buffer,
        graph_buffer_size: crate::sensor_state::GRAPH_HISTORY_SIZE,
        graph_start_idx: start_idx,
        graph_count: count,
        graph_min: min,
        graph_max: max,
        average: state.get_average(),
    }
}
