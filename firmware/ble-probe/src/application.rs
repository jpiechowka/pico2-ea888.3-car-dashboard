// Included once by each explicit binary. Keeping owners in the binary optimization
// unit preserves the reviewed binary-only LLVM policy without applying it to HAL
// dependencies. Core0 owns hardware; core1 owns catalogue/rendering. No ECU polling.

/// Bounded cross-core messages and atomic progress/fault observations.
mod bridge;
/// Affine storage reply-envelope exchange with core1.
mod capture_reply;
/// Lossless stage-aware mapping of host/controller failures.
mod errors;
/// Bounded adapter-only identity orchestration behind explicit image selection.
#[cfg(feature = "ngm-identity")]
mod identity_flow;
/// Hardware CoC adapter for the manually selected identity utility.
#[cfg(feature = "ngm-identity")]
pub mod identity_radio;
/// Bounded standard-service GATT enumeration and allowed reads.
mod inventory;
/// One flash operation per finite watchdog/owner barrier.
mod maintenance;
/// Coherent polling-work, stack sample and transfer-count observations.
mod metrics;
/// Retained CYW/PIO/DMA and host connection/scan lifecycle.
pub mod radio;
/// Bounded HCI advertisement decoding and raw-report preservation.
mod reports;
/// Cancellation ordering and terminal connection evidence publication.
mod session;
/// Atomic fault-priority RGB policy with protected recovery.
mod status;
/// Sole core0 journal owner and read-only USB service.
mod storage;
/// Core1 live/saved catalogue state and affine rendering.
mod ui;
/// Read-only CDC export with bounded packets and session checks.
mod usb;
/// Non-wrapping bus-generation fence excluding stale endpoint work.
mod usb_session;
include!(concat!(env!("OUT_DIR"), "/probe_identity.rs"));
/// Permanent measured owner-task wrappers and independent supervision.
mod tasks;

/// Immutable image-selected behavior and labels, fixed before owners start.
mod tool;
use dashboard_core::display::FrameBuffer;
use embassy_executor::{Executor, Spawner};
use embassy_rp::multicore::Stack;
use embassy_rp::{bind_interrupts, dma, i2c, peripherals, pio};
use static_cell::StaticCell;
pub use tool::{ToolKind, tool_kind};

bind_interrupts!(
/// Core0 display/radio DMA, I2C, PIO radio and USB controller interrupt bindings.
pub struct Irqs {
    DMA_IRQ_0 => dma::InterruptHandler<peripherals::DMA_CH0>, dma::InterruptHandler<peripherals::DMA_CH1>;
    I2C0_IRQ => i2c::InterruptHandler<peripherals::I2C0>;
    PIO0_IRQ_0 => pio::InterruptHandler<peripherals::PIO0>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<peripherals::USB>;
});

// Each object is taken once before core1 starts; channel ownership is affine.
#[unsafe(no_mangle)]
/// First fixed RGB565 frame; boot constructs its only mutable reference once.
static mut BLE_PROBE_FRAMEBUFFER_0: FrameBuffer = FrameBuffer::new();
#[unsafe(no_mangle)]
/// Second fixed RGB565 frame circulating exclusively through FREE/READY.
static mut BLE_PROBE_FRAMEBUFFER_1: FrameBuffer = FrameBuffer::new();
#[unsafe(no_mangle)]
/// Dedicated aligned 32 KiB core1 stack, borrowed once before core launch.
static mut BLE_PROBE_CORE1_STACK: Stack<32768> = Stack::new();
/// One-time core1 thread-mode executor residence.
static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();

/// Initializes board clocks/watchdog, qualifies capture flash, transfers both
/// framebuffers and boot evidence to core1, then uses `spawner` for each permanent
/// core0 owner. The binary's constant policy selects the tool; identity requires
/// its build feature, checked before hardware access. Returns after owners spawn.
/// Qualification failure leaves capture storage unavailable and latched red.
/// Panics on repeated initialization, an unavailable kind or broken ownership
/// invariants. Never pairs or issues an automatic inspection request.
pub async fn run(spawner: Spawner) {
    assert!(tool_kind() == ToolKind::StandardProbe || cfg!(feature = "ngm-identity"));
    let mut config = embassy_rp::config::Config::default();
    config.clocks = dashboard_pico2::clock_profile::clock_config();
    let p = embassy_rp::init(config);
    let watchdog = dashboard_pico2::recovery_watchdog::RecoveryWatchdog::start(p.WATCHDOG);
    let boot = watchdog.boot_evidence();
    if boot.watchdog_fault() {
        bridge::STATUS.record(status::Fault::Watchdog);
    }
    let mut flash =
        dashboard_pico2::flash_guard::CheckedFlash::new(p.FLASH, dashboard_pico2::flash_guard::Region::Captures);
    if let Err(error) = flash.qualify() {
        bridge::STATUS.record(status::Fault::Storage);
        defmt::warn!("probe.capture unavailable: {:?}", error);
    }
    defmt::info!(
        "Probe boot: CPU={} SPI request={} reset={}",
        embassy_rp::clocks::clk_sys_freq(),
        dashboard_pico2::clock_profile::REQUESTED_SPI_HZ,
        watchdog.reset_label()
    );
    // SAFETY: main executes once; static references never reconstructed. Core1
    // receives each buffer only through FREE and returns it only through READY.
    let (frame0, frame1, stack) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(BLE_PROBE_FRAMEBUFFER_0),
            &mut *core::ptr::addr_of_mut!(BLE_PROBE_FRAMEBUFFER_1),
            &mut *core::ptr::addr_of_mut!(BLE_PROBE_CORE1_STACK),
        )
    };
    metrics::init(stack.mem.as_ptr() as u32, stack.mem.len() as u32);
    assert!(bridge::FREE.try_send(frame0).is_ok());
    assert!(bridge::FREE.try_send(frame1).is_ok());
    assert!(bridge::BOOT.try_send(boot).is_ok());
    embassy_rp::multicore::spawn_core1(p.CORE1, stack, probe_core1);
    assert!(
        tasks::SUPERVISOR
            .try_send(tasks::SupervisorResources {
                watchdog,
                red: p.PIN_26,
                green: p.PIN_27,
                blue: p.PIN_28,
            })
            .is_ok()
    );
    spawner.spawn(tasks::probe_supervisor_task().unwrap());
    spawner.spawn(tasks::probe_inputs_task(p.PIN_12, p.PIN_13, p.PIN_14, p.PIN_15).unwrap());
    spawner.spawn(storage::probe_storage_task(flash).unwrap());
    usb::start(spawner, p.USB);
    let mut i2c_config = i2c::Config::default();
    i2c_config.frequency = 400_000;
    spawner.spawn(tasks::probe_encoder_task(i2c::I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, i2c_config)).unwrap());
    let mut spi_config = embassy_rp::spi::Config::default();
    spi_config.frequency = dashboard_pico2::clock_profile::REQUESTED_SPI_HZ;
    let spi = embassy_rp::spi::Spi::new_txonly(p.SPI0, p.PIN_18, p.PIN_19, p.DMA_CH0, Irqs, spi_config);
    spawner.spawn(tasks::probe_display_task(spi, p.PIN_16, p.PIN_17, p.PIN_20).unwrap());
    spawner.spawn(
        tasks::probe_radio_task(radio::Resources {
            pio: p.PIO0,
            dma: p.DMA_CH1,
            power: p.PIN_23,
            data: p.PIN_24,
            cs: p.PIN_25,
            clock: p.PIN_29,
        })
        .unwrap(),
    );
}

#[inline(never)]
/// Never returns: runs the sole UI task forever on core1; panics on repeated executor/task
/// initialization. Peripheral and flash owners remain on core0.
fn probe_core1() -> ! {
    CORE1_EXECUTOR.init(Executor::new()).run(|spawner| {
        spawner.spawn(tasks::probe_ui_task().unwrap());
    })
}
