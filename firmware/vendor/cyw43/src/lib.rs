#![no_std]
#![cfg_attr(not(test), no_main)]
#![allow(async_fn_in_trait)]
#![allow(unsafe_op_in_unsafe_fn)]
#![deny(unused_must_use)]
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

// This mod MUST go first, so that the others see its macros.
pub(crate) mod fmt;

#[cfg(feature = "bluetooth")]
/// Bluetooth module.
pub mod bluetooth;
#[cfg(feature = "bluetooth")]
mod bluetooth_firmware;
mod consts;
mod control;
mod countries;
mod events;
mod ioctl;
mod runner;
mod sdio;
mod spi;
mod structs;
mod util;

use core::sync::atomic::AtomicBool;

pub use aligned::{A4, Aligned};
use embassy_net_driver_channel as ch;
use embedded_hal_1::digital::OutputPin;
use events::Events;
use ioctl::IoctlState;

pub use crate::control::{
    AddMulticastAddressError, Control, JoinAuth, JoinError, JoinOptions, ScanOptions, ScanType, Scanner,
};
pub use crate::runner::Runner;
pub use crate::sdio::{SdioBus, SdioBusCyw43};
pub use crate::spi::{SpiBus, SpiBusCyw43};
pub use crate::structs::BssInfo;

/// Ethernet buffer capacity: 1514 bytes normally, 64 for the Bluetooth-only inert channel.
/// Oversized unsolicited frames are rejected; the dashboard does not initialize Wi-Fi.
const MTU: usize = if cfg!(feature = "bluetooth-only") { 64 } else { 1514 };
/// RX and TX packet slot count per Ethernet channel (one in Bluetooth-only builds, otherwise four).
const NET_QUEUE: usize = if cfg!(feature = "bluetooth-only") { 1 } else { 4 };

#[cfg(all(test, feature = "bluetooth"))]
mod regression_tests;

#[allow(unused)]
#[derive(Clone, Copy, PartialEq, Eq)]
/// Radio internal backplane core, separate from RP2350 CPU cores.
enum Core {
    /// WLAN firmware processor wrapper.
    WLAN = 0,
    /// Shared on-chip SRAM wrapper.
    SOCSRAM = 1,
    /// SDIO controller core.
    SDIOD = 2,
}

impl Core {
    /// Returns the selected core's backplane control-register base address.
    fn base_addr(&self) -> u32 {
        match self {
            Self::WLAN => CHIP.arm_core_base_address,
            Self::SOCSRAM => CHIP.socsram_wrapper_base_address,
            Self::SDIOD => CHIP.sdiod_core_base_address,
        }
    }
}

#[allow(unused)]
/// CYW43439 memory map and channel-specification bit encodings; unused encodings are retained for protocol completeness.
struct Chip {
    /// WLAN ARM processor wrapper base in backplane address space.
    arm_core_base_address: u32,
    /// Shared SRAM core register base in backplane address space.
    socsram_base_address: u32,
    /// Bluetooth processor patch address base.
    bluetooth_base_address: u32,
    /// Shared SRAM reset/clock wrapper base.
    socsram_wrapper_base_address: u32,
    /// SDIO controller mailbox/interrupt register base.
    sdiod_core_base_address: u32,
    /// Chip-common power-management register base.
    pmu_base_address: u32,
    /// Firmware RAM capacity in bytes, used to place NVRAM and its length marker.
    chip_ram_size: u32,
    /// Address of the start of firmware tightly coupled RAM.
    atcm_ram_base_address: u32,
    /// Firmware retention SRAM reservation in bytes, used to find the logging descriptor pointer.
    socram_srmem_size: u32,
    /// Mask selecting frequency-band bits from a firmware channel specification.
    chanspec_band_mask: u32,
    /// Channel-specification encoding for the 2.4 GHz band.
    chanspec_band_2g: u32,
    /// Retained encoding for 5 GHz; presence does not imply CYW43439 radio support.
    chanspec_band_5g: u32,
    /// Right-shift count for extracting frequency-band bits.
    chanspec_band_shift: u32,
    /// Retained channel-specification encoding for 10 MHz width.
    chanspec_bw_10: u32,
    /// Channel-specification encoding for 20 MHz width.
    chanspec_bw_20: u32,
    /// Retained channel-specification encoding for 40 MHz width.
    chanspec_bw_40: u32,
    /// Mask selecting channel bandwidth bits.
    chanspec_bw_mask: u32,
    /// Right-shift count for extracting channel bandwidth bits.
    chanspec_bw_shift: u32,
    /// Control sideband encoding for the lower subchannel.
    chanspec_ctl_sb_lower: u32,
    /// Control sideband encoding for the upper subchannel.
    chanspec_ctl_sb_upper: u32,
    /// Encoding when no control sideband is selected.
    chanspec_ctl_sb_none: u32,
    /// Mask selecting control sideband bits.
    chanspec_ctl_sb_mask: u32,
}

/// Byte displacement from a radio core's register base to its reset/clock wrapper.
const WRAPPER_REGISTER_OFFSET: u32 = 0x100000;

/// Fixed CYW43439 map; this driver does not select chip geometry dynamically from the read chip ID.
const CHIP: Chip = Chip {
    arm_core_base_address: 0x18003000 + WRAPPER_REGISTER_OFFSET,
    socsram_base_address: 0x18004000,
    bluetooth_base_address: 0x19000000,
    socsram_wrapper_base_address: 0x18004000 + WRAPPER_REGISTER_OFFSET,
    sdiod_core_base_address: 0x18002000,
    pmu_base_address: 0x18000000,
    chip_ram_size: 512 * 1024,
    atcm_ram_base_address: 0,
    socram_srmem_size: 64 * 1024,
    chanspec_band_mask: 0xC000,
    chanspec_band_2g: 0x0000,
    chanspec_band_5g: 0xC000,
    chanspec_band_shift: 14,
    chanspec_bw_10: 0x0800,
    chanspec_bw_20: 0x1000,
    chanspec_bw_40: 0x1800,
    chanspec_bw_mask: 0x3800,
    chanspec_bw_shift: 11,
    chanspec_ctl_sb_lower: 0x0000,
    chanspec_ctl_sb_upper: 0x0100,
    chanspec_ctl_sb_none: 0x0000,
    chanspec_ctl_sb_mask: 0x0700,
};

/// Static backing for the network, IOCTL and optional Bluetooth queues.
/// Initialization borrows this state for every returned handle, preventing movement while self-referential queues are live.
pub struct State {
    /// Single outstanding host-to-firmware control request and cancellation state.
    ioctl_state: IoctlState,
    /// Bounded Ethernet queues and host event/link policy state.
    net: NetState,
    #[cfg(feature = "bluetooth")]
    /// Four RX and four TX HCI packet slots plus their queue metadata.
    bt: bluetooth::BtState,
}

/// Network data-plane storage and shared control observations.
struct NetState {
    /// Fixed MTU queues, initialized when a constructor splits the state into handles.
    ch: ch::State<MTU, NET_QUEUE, NET_QUEUE>,
    /// Host subscription mask and bounded firmware-event queue.
    events: Events,
    /// Whether the selected network requires a successful key exchange before link-up.
    secure_network: AtomicBool,
}

impl State {
    /// Returns zeroed/empty software state without touching hardware or allocating dynamically.
    pub const fn new() -> Self {
        Self {
            ioctl_state: IoctlState::new(),
            net: NetState {
                ch: ch::State::new(),
                events: Events::new(),
                secure_network: AtomicBool::new(false),
            },
            #[cfg(feature = "bluetooth")]
            bt: bluetooth::BtState::new(),
        }
    }
}

/// Power management modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerManagementMode {
    /// Custom mode 2: 2000 ms sleep retention and beacon/DTIM/association listen intervals of 255.
    /// Firmware support and resulting power/latency depend on the radio image and access point.
    SuperSave,

    /// Mode 2 with 2000 ms sleep retention, beacon/DTIM intervals one and association interval ten.
    Aggressive,

    /// Default mode 2 with 200 ms sleep retention, beacon/DTIM intervals one and association interval ten.
    PowerSave,

    /// Mode 2 with 20 ms sleep retention and all listen intervals one.
    Performance,

    /// Firmware mode 1; mode-2 timing variables are not changed by this selection.
    ThroughputThrottling,

    /// Firmware mode zero disables power management; mode-2 timing variables are left unchanged.
    None,
}

impl Default for PowerManagementMode {
    /// Returns PowerSave; constructing this value does not configure the device.
    fn default() -> Self {
        Self::PowerSave
    }
}

impl PowerManagementMode {
    /// Returns mode-2 sleep-retention milliseconds; zero for modes where this setting is unused.
    fn sleep_ret_ms(&self) -> u16 {
        match self {
            PowerManagementMode::SuperSave => 2000,
            PowerManagementMode::Aggressive => 2000,
            PowerManagementMode::PowerSave => 200,
            PowerManagementMode::Performance => 20,
            PowerManagementMode::ThroughputThrottling => 0, // value doesn't matter
            PowerManagementMode::None => 0,                 // value doesn't matter
        }
    }

    /// Returns mode-2 listen interval in beacon periods; zero for modes where unused.
    fn beacon_period(&self) -> u8 {
        match self {
            PowerManagementMode::SuperSave => 255,
            PowerManagementMode::Aggressive => 1,
            PowerManagementMode::PowerSave => 1,
            PowerManagementMode::Performance => 1,
            PowerManagementMode::ThroughputThrottling => 0, // value doesn't matter
            PowerManagementMode::None => 0,                 // value doesn't matter
        }
    }

    /// Returns mode-2 listen interval in DTIM periods; zero for modes where unused.
    fn dtim_period(&self) -> u8 {
        match self {
            PowerManagementMode::SuperSave => 255,
            PowerManagementMode::Aggressive => 1,
            PowerManagementMode::PowerSave => 1,
            PowerManagementMode::Performance => 1,
            PowerManagementMode::ThroughputThrottling => 0, // value doesn't matter
            PowerManagementMode::None => 0,                 // value doesn't matter
        }
    }

    /// Returns association listen-interval value sent for mode 2; zero for modes where unused.
    fn assoc(&self) -> u8 {
        match self {
            PowerManagementMode::SuperSave => 255,
            PowerManagementMode::Aggressive => 10,
            PowerManagementMode::PowerSave => 10,
            PowerManagementMode::Performance => 1,
            PowerManagementMode::ThroughputThrottling => 0, // value doesn't matter
            PowerManagementMode::None => 0,                 // value doesn't matter
        }
    }

    /// Returns firmware power-management mode: zero disabled, one throttled, two configured retention/listen intervals.
    fn mode(&self) -> u32 {
        match self {
            PowerManagementMode::ThroughputThrottling => 1,
            PowerManagementMode::None => 0,
            _ => 2,
        }
    }
}

/// Borrowed Ethernet endpoint; its peer Runner must be polled to service frames and link changes.
pub type NetDriver<'a> = ch::Device<'a, MTU>;

/// Initializes SPI Wi-Fi using borrowed `state`, owned power pin `pwr`/bus `spi`, and aligned `firmware`/`nvram` images.
/// Returns network, control and permanent runner handles. Control::init separately loads CLM/configures Wi-Fi.
/// Invalid image placement panics before bus initialization; this low-level constructor does not validate image identity.
/// Initialization errors/assertions panic, some clock waits have no deadline, and cancellation leaves partial hardware state.
pub async fn new<'a, PWR, SPI>(
    state: &'a mut State,
    pwr: PWR,
    spi: SPI,
    firmware: &Aligned<A4, [u8]>,
    nvram: &Aligned<A4, [u8]>,
) -> (NetDriver<'a>, Control<'a>, Runner<'a, SpiBus<PWR, SPI>>)
where
    PWR: OutputPin,
    SPI: SpiBusCyw43,
{
    let (ch_runner, device) = ch::new(&mut state.net.ch, ch::driver::HardwareAddress::Ethernet([0; 6]));
    let state_ch = ch_runner.state_runner();

    let mut runner = Runner::new(
        ch_runner,
        SpiBus::new(pwr, spi),
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        #[cfg(feature = "bluetooth")]
        None,
    );

    runner.init(firmware, nvram, None).await.unwrap();
    let control = Control::new(
        state_ch,
        &state.net.events,
        &state.ioctl_state,
        &state.net.secure_network,
    );

    (device, control, runner)
}

/// Initializes Wi-Fi using borrowed `state`, owned `sdio` and aligned `firmware`/`nvram` images.
/// Returns network, control and permanent runner handles; Control::init performs separate CLM/Wi-Fi setup.
/// Image overlap, RAM overflow and excessive NVRAM length panic before bus initialization. SDIO transport errors may be swallowed; init failures/assertions panic.
/// Clock waits may stall indefinitely; cancellation leaves partial state. Backplane transfers retain 1024 scratch bytes.
pub async fn new_sdio<'a, SDIO>(
    state: &'a mut State,
    sdio: SDIO,
    firmware: &Aligned<A4, [u8]>,
    nvram: &Aligned<A4, [u8]>,
) -> (NetDriver<'a>, Control<'a>, Runner<'a, SdioBus<SDIO>>)
where
    SDIO: SdioBusCyw43<64>,
{
    let (ch_runner, device) = ch::new(&mut state.net.ch, ch::driver::HardwareAddress::Ethernet([0; 6]));
    let state_ch = ch_runner.state_runner();

    let mut runner = Runner::new(
        ch_runner,
        SdioBus::new(sdio),
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        #[cfg(feature = "bluetooth")]
        None,
    );

    runner.init(firmware, nvram, None).await.unwrap();
    let control = Control::new(
        state_ch,
        &state.net.events,
        &state.ioctl_state,
        &state.net.secure_network,
    );

    (device, control, runner)
}

/// Initializes shared radio state from `state`, power pin `pwr`, `spi`, aligned `wifi_firmware`/`nvram` and `bluetooth_firmware` patch records.
/// Returns network, HCI transport, control and permanent runner handles; selected dashboard/probe routes leave Wi-Fi control unused.
/// Wi-Fi/NVRAM layout is checked before bus initialization; invalid placement panics. Invalid Bluetooth records, exhausted polls and hardware assertions panic;
/// some clock waits are unbounded. Do not cancel initialization or the DMA-owning runner to perform flash maintenance.
#[cfg(feature = "bluetooth")]
pub async fn new_with_bluetooth<'a, PWR, SPI>(
    state: &'a mut State,
    pwr: PWR,
    spi: SPI,
    wifi_firmware: &Aligned<A4, [u8]>,
    bluetooth_firmware: &Aligned<A4, [u8]>,
    nvram: &Aligned<A4, [u8]>,
) -> (
    NetDriver<'a>,
    bluetooth::BtDriver<'a>,
    Control<'a>,
    Runner<'a, SpiBus<PWR, SPI>>,
)
where
    PWR: OutputPin,
    SPI: SpiBusCyw43,
{
    let (ch_runner, device) = ch::new(&mut state.net.ch, ch::driver::HardwareAddress::Ethernet([0; 6]));
    let state_ch = ch_runner.state_runner();

    let (bt_runner, bt_driver) = bluetooth::new(&mut state.bt);
    let mut runner = Runner::new(
        ch_runner,
        SpiBus::new(pwr, spi),
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        #[cfg(feature = "bluetooth")]
        Some(bt_runner),
    );

    runner
        .init(wifi_firmware, nvram, Some(bluetooth_firmware))
        .await
        .unwrap();
    let control = Control::new(
        state_ch,
        &state.net.events,
        &state.ioctl_state,
        &state.net.secure_network,
    );

    (device, bt_driver, control, runner)
}

/// Includes the file at `$path` as static four-byte-aligned storage and returns its logical byte slice.
/// The compiler resolves the path relative to the invoking source file; missing files fail compilation.
/// Alignment does not promise initialized padding beyond the included file's logical length.
#[macro_export]
macro_rules! aligned_bytes {
    ($path:expr) => {{
        {
            /// Immutable included file bytes; the borrowed aligned slice retains the file's exact length.
            static BYTES: &cyw43::Aligned<cyw43::A4, [u8]> = &cyw43::Aligned(*include_bytes!($path));

            BYTES
        }
    }};
}
