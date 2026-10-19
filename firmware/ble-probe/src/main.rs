#![no_std]
#![no_main]

mod bridge;
mod errors;
mod inventory;
mod maintenance;
mod metrics;
mod radio;
mod reports;
mod session;
mod storage;
mod ui;
mod usb;
mod usb_session;
include!(concat!(env!("OUT_DIR"), "/probe_identity.rs"));
mod tasks;

use dashboard_core::display::FrameBuffer;
use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::multicore::Stack;
use embassy_rp::{bind_interrupts, dma, i2c, peripherals, pio};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(pub struct Irqs {
    DMA_IRQ_0 => dma::InterruptHandler<peripherals::DMA_CH0>, dma::InterruptHandler<peripherals::DMA_CH1>;
    I2C0_IRQ => i2c::InterruptHandler<peripherals::I2C0>;
    PIO0_IRQ_0 => pio::InterruptHandler<peripherals::PIO0>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<peripherals::USB>;
});

// Each object is taken once before core1 starts; channel ownership is affine.
#[unsafe(no_mangle)]
static mut BLE_PROBE_FRAMEBUFFER_0: FrameBuffer = FrameBuffer::new();
#[unsafe(no_mangle)]
static mut BLE_PROBE_FRAMEBUFFER_1: FrameBuffer = FrameBuffer::new();
#[unsafe(no_mangle)]
static mut BLE_PROBE_CORE1_STACK: Stack<32768> = Stack::new();
static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[unsafe(link_section = ".bi_entries")]
#[used]
static INFO: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2w-ble-probe"),
    embassy_rp::binary_info::rp_program_description!(c"BLE discovery and manual read-only GATT inspection"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_rp::config::Config::default();
    config.clocks = dashboard_pico2::clock_profile::clock_config();
    let p = embassy_rp::init(config);
    let watchdog = dashboard_pico2::recovery_watchdog::RecoveryWatchdog::start(p.WATCHDOG);
    let mut flash =
        dashboard_pico2::flash_guard::CheckedFlash::new(p.FLASH, dashboard_pico2::flash_guard::Region::Captures);
    if let Err(error) = flash.qualify() {
        defmt::warn!("probe.capture unavailable: {:?}", defmt::Debug2Format(&error));
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
    embassy_rp::multicore::spawn_core1(p.CORE1, stack, probe_core1);
    spawner.spawn(tasks::probe_supervisor_task(watchdog, p.PIN_26, p.PIN_27, p.PIN_28).unwrap());
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
fn probe_core1() -> ! {
    CORE1_EXECUTOR.init(Executor::new()).run(|spawner| {
        spawner.spawn(tasks::probe_ui_task().unwrap());
    })
}
