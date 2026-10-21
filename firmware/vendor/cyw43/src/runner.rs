//! Exclusive radio bus owner: firmware boot, SDPCM credits, IOCTLs, Ethernet and optional Bluetooth service.
//! Maintenance hooks run between complete transactions; canceling an in-flight runner is not a quiescence protocol.
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::Relaxed;

use aligned::{A4, Aligned};
use embassy_futures::select::{Either4, select4};
use embassy_net_driver_channel as ch;
use embassy_net_driver_channel::driver::LinkState;
use embassy_time::{Duration, Timer, block_for};

use crate::consts::*;
use crate::events::{Event, Events, Status};
use crate::fmt::Bytes;
use crate::ioctl::{IoctlState, IoctlType, PendingIoctl};
use crate::structs::*;
use crate::util::{aligned_mut, aligned_ref, slice8_mut, slice16_mut, try_until};
use crate::{CHIP, Core, MTU, events};

/// Checked chip-RAM placement for Wi-Fi at the RAM base and NVRAM below its final length word.
struct FirmwareLayout {
    /// Absolute address of the NVRAM payload, including reserved round-up space above it.
    nvram_address: u32,
    /// Absolute address of the final four-byte NVRAM length/complement word.
    magic_address: u32,
    /// Low 16 bits are NVRAM words; high 16 bits are their complement.
    magic: u32,
}

/// Returns nonoverlapping placement of `wifi_len` and `nvram_len` bytes within `ram_size` at `base`.
/// NVRAM reserves whole words; lengths, addresses and the 16-bit word count must fit, otherwise Err.
/// Empty inputs remain permitted; this checks placement, not firmware validity or NVRAM termination.
fn firmware_layout(wifi_len: usize, nvram_len: usize, base: u32, ram_size: u32) -> Result<FirmwareLayout, ()> {
    let rounded = nvram_len.checked_add(3).ok_or(())? & !3;
    let words = u16::try_from(rounded / 4).map_err(|_| ())?;
    let tail_offset = ram_size.checked_sub(4).ok_or(())?;
    let nvram_offset = tail_offset
        .checked_sub(u32::try_from(rounded).map_err(|_| ())?)
        .ok_or(())?;
    if wifi_len > nvram_offset as usize {
        return Err(());
    }
    let magic_address = base.checked_add(tail_offset).ok_or(())?;
    magic_address.checked_add(3).ok_or(())?;
    Ok(FirmwareLayout {
        nvram_address: base.checked_add(nvram_offset).ok_or(())?,
        magic_address,
        magic: (u32::from(!words) << 16) | u32::from(words),
    })
}

#[cfg(feature = "firmware-logs")]
/// Host cursor and bounded line assembly for the firmware's assumed 1024-byte console ring.
struct LogState {
    /// Backplane address of the firmware log descriptor, discovered at initialization.
    addr: u32,
    /// Next unread byte offset in 0..1024; wraps independently of the device producer.
    last_idx: usize,
    /// Current log line; bytes beyond 256 before newline are dropped.
    buf: [u8; 256],
    /// Initialized line prefix length, at most 256.
    buf_count: usize,
}

#[cfg(all(test, feature = "bluetooth"))]
/// Credit-window wrap regressions using the production runner and inert transport.
mod tests {
    use super::*;
    use crate::regression_tests::MockBus;

    /// RAM edges, NVRAM word-count limits, rounded tails and address/usize overflow fail before writes.
    #[test]
    fn firmware_layout_bounds_and_length_magic() {
        for (nvram, expected_reserved) in [(0, 0), (1, 4), (3, 4), (4, 4), (5, 8)] {
            let layout = firmware_layout(32 - 4 - expected_reserved, nvram, 0x1000, 32).unwrap();
            assert_eq!(layout.nvram_address, 0x1000 + 28 - expected_reserved as u32);
            assert_eq!(layout.magic_address, 0x101c);
            let words = expected_reserved as u32 / 4;
            assert_eq!(layout.magic & 0xffff, words);
            assert_eq!(layout.magic >> 16, (!words) & 0xffff);
            assert!(firmware_layout(29 - expected_reserved, nvram, 0x1000, 32).is_err());
        }
        assert!(firmware_layout(0, 65535 * 4, 0, 512 * 1024).is_ok());
        assert!(firmware_layout(0, 65535 * 4 + 1, 0, 512 * 1024).is_err());
        for (wifi, nvram, base, ram) in [
            (0, 0, 0, 0),
            (0, 0, 0, 3),
            (1, 0, 0, 4),
            (0, 1, 0, 4),
            (usize::MAX, 0, 0, 512 * 1024),
            (0, usize::MAX, 0, 512 * 1024),
            (0, 0, u32::MAX, 8),
            (0, 0, u32::MAX, 4),
        ] {
            assert!(firmware_layout(wifi, nvram, base, ram).is_err());
        }
        let wifi = include_bytes!("../../cyw43-firmware/43439A0.bin");
        let nvram = include_bytes!("../../cyw43-firmware/nvram_rp2040.bin");
        let selected =
            firmware_layout(wifi.len(), nvram.len(), CHIP.atcm_ram_base_address, CHIP.chip_ram_size).unwrap();
        let rounded = (nvram.len() + 3) / 4 * 4;
        assert_eq!(
            selected.nvram_address,
            CHIP.atcm_ram_base_address + CHIP.chip_ram_size - 4 - rounded as u32
        );
        assert_eq!(selected.magic, (!(rounded as u32 / 4) << 16) | (rounded as u32 / 4));
        let empty = firmware_layout(0, 0, 0, 4).unwrap();
        assert_eq!(
            (empty.nvram_address, empty.magic_address, empty.magic),
            (0, 0, 0xffff0000)
        );
    }

    /// Recovery from invalid peer windows wraps at 255 while valid credit and unknown channels retain their rules.
    #[test]
    fn credit_recovery_wraps_at_sequence_boundary() {
        let mut state = crate::State::new();
        let (ch, _) = ch::new(&mut state.net.ch, ch::driver::HardwareAddress::Ethernet([0; 6]));
        let mut runner = Runner::new(
            ch,
            MockBus::default(),
            &state.ioctl_state,
            &state.net.events,
            &state.net.secure_network,
            None,
        );
        for (seq, peer, expected) in [(253, 0, 0), (254, 128, 0), (255, 128, 1), (255, 0, 0), (255, 255, 255)] {
            runner.sdpcm_seq = seq;
            let mut header = SdpcmHeader {
                len: 12,
                len_inv: !12,
                sequence: 0,
                channel_and_flags: CHANNEL_TYPE_CONTROL,
                next_length: 0,
                header_length: 12,
                wireless_flow_control: 0,
                bus_data_credit: peer,
                reserved: [0; 2],
            };
            runner.update_credit(&header);
            assert_eq!(runner.sdpcm_seq_max, expected);
            assert_eq!(runner.has_credit(), expected != seq);
            header.channel_and_flags = 3;
            header.bus_data_credit = 20;
            runner.update_credit(&header);
            assert_eq!(runner.sdpcm_seq_max, expected);
        }
    }
}

#[cfg(feature = "firmware-logs")]
impl Default for LogState {
    /// Returns empty host cursors; log_init must discover the descriptor before reads.
    fn default() -> Self {
        Self {
            addr: Default::default(),
            last_idx: Default::default(),
            buf: [0; 256],
            buf_count: Default::default(),
        }
    }
}

/// Transport-specific initialization, interrupt and packet-framing branch.
pub(crate) enum BusType {
    /// Half-duplex GSPI with status words and a shared interrupt/data pin.
    Spi,
    /// SDIO function/mailbox protocol; current adapter polls rather than awaiting IRQ.
    Sdio,
}

/// Private common bus operations; implementations define transport failures and cancellation effects.
/// Byte buffers must retain backing storage until awaited transfers complete. No method reports a transport Result.
pub(crate) trait SealedBus {
    /// Selects initialization and receive framing rules.
    const TYPE: BusType;

    /// Initializes physical transport, using `bluetooth_enabled` for shared-pin interrupt configuration where relevant.
    async fn init(&mut self, bluetooth_enabled: bool);
    /// Fills logical WLAN `buf` bytes; backing storage must include any SPI word rounding.
    async fn wlan_read(&mut self, buf: &mut Aligned<A4, [u8]>);
    /// Sends logical WLAN `buf` bytes; SPI requires initialized backing through the last rounded word.
    async fn wlan_write(&mut self, buf: &Aligned<A4, [u8]>);
    #[allow(unused)]
    /// Reads `data` from aligned backplane `addr`; partial writes and window state can remain on failure/cancel.
    async fn bp_read(&mut self, addr: u32, data: &mut [u8]);
    /// Writes `data` at aligned backplane `addr`; completion is not readback verification.
    async fn bp_write(&mut self, addr: u32, data: &[u8]);
    /// Returns the byte at backplane `addr`, with implementation-specific transport failure substitution/panic.
    async fn bp_read8(&mut self, addr: u32) -> u8;
    /// Writes byte `val` at backplane `addr`.
    async fn bp_write8(&mut self, addr: u32, val: u8);
    /// Returns the little-endian halfword at backplane `addr`.
    async fn bp_read16(&mut self, addr: u32) -> u16;
    #[allow(unused)]
    /// Writes little-endian halfword `val` at backplane `addr`.
    async fn bp_write16(&mut self, addr: u32, val: u16);
    #[allow(unused)]
    /// Returns the little-endian word at backplane `addr`.
    async fn bp_read32(&mut self, addr: u32) -> u32;
    /// Writes little-endian word `val` at backplane `addr`.
    async fn bp_write32(&mut self, addr: u32, val: u32);
    /// Returns a byte from bus function `func` and relative register `addr`.
    async fn read8(&mut self, func: u32, addr: u32) -> u8;
    /// Writes byte `val` to bus function `func` and relative register `addr`.
    async fn write8(&mut self, func: u32, addr: u32, val: u8);
    /// Returns a little-endian halfword from bus function `func` and relative register `addr`.
    async fn read16(&mut self, func: u32, addr: u32) -> u16;
    /// Writes little-endian halfword `val` to bus function `func` and relative register `addr`.
    async fn write16(&mut self, func: u32, addr: u32, val: u16);
    /// Returns a little-endian word from bus function `func` and relative register `addr`.
    async fn read32(&mut self, func: u32, addr: u32) -> u32;
    #[allow(unused)]
    /// Writes little-endian word `val` to bus function `func` and relative register `addr`.
    async fn write32(&mut self, func: u32, addr: u32, val: u32);
    #[allow(dead_code)] // Bluetooth uses a bounded polling fallback.
    /// Waits for transport wake/poll delay; this does not establish that a complete packet is available.
    async fn wait_for_event(&mut self);
}

#[allow(private_bounds)]
/// Sealed bus bound accepted by Runner; only this crate's transport adapters can implement it.
pub trait Bus: SealedBus {}
impl<T: SealedBus> Bus for T {}

/// Permanent owner of radio bus transactions and firmware protocol state.
/// Must be polled alongside Control/HCI consumers; cancellation may leave hardware or protocol work in flight.
pub struct Runner<'a, BUS> {
    /// Network RX/TX channel owner; bounded queues can drop incoming Ethernet frames when full.
    ch: ch::Runner<'a, MTU>,
    /// Sole physical bus/window-cache owner.
    pub(crate) bus: BUS,

    /// Shared single-request IOCTL state; Control's drop guard clears the raw pointer before its buffer expires.
    ioctl_state: &'a IoctlState,
    /// Wrapping u16 identity of the most recently transmitted IOCTL.
    ioctl_id: u16,
    /// Next outgoing SDPCM sequence, wrapping modulo 256.
    sdpcm_seq: u8,
    /// Exclusive upper bound of the firmware-granted wrapping sequence window.
    sdpcm_seq_max: u8,

    /// Host subscriptions and bounded event queue; immediate publish can overwrite queued events.
    events: &'a Events,

    /// Control-published requirement for key exchange before marking the network up.
    secure_network: &'a AtomicBool,
    /// Last accepted JOIN success, cleared on qualified link/authentication failures.
    join_ok: bool,
    /// Last qualified supplicant/key-exchange success.
    key_exchange_ok: bool,
    /// Last non-unsolicited authentication success; gates supplicant success.
    auth_ok: bool,

    #[cfg(feature = "firmware-logs")]
    /// Optional bounded firmware console reader state.
    log: LogState,

    #[cfg(feature = "bluetooth")]
    /// Bluetooth ring owner, absent for Wi-Fi-only initialization even when compiled in.
    pub(crate) bt: Option<crate::bluetooth::BtRunner<'a>>,
}

impl<'a, BUS> Runner<'a, BUS>
where
    BUS: Bus,
{
    /// Returns an uninitialized owner of `ch`/`bus`, borrowing `ioctl_state`, `events` and `secure_network`.
    /// Optional `bt` owns HCI queues/rings; starts with one SDPCM transmit credit and no successful connection state.
    pub(crate) fn new(
        ch: ch::Runner<'a, MTU>,
        bus: BUS,
        ioctl_state: &'a IoctlState,
        events: &'a Events,
        secure_network: &'a AtomicBool,
        #[cfg(feature = "bluetooth")] bt: Option<crate::bluetooth::BtRunner<'a>>,
    ) -> Self {
        Self {
            ch,
            bus,
            ioctl_state,
            ioctl_id: 0,
            sdpcm_seq: 0,
            sdpcm_seq_max: 1,
            events,
            secure_network,
            join_ok: false,
            key_exchange_ok: false,
            auth_ok: false,
            #[cfg(feature = "firmware-logs")]
            log: LogState::default(),
            #[cfg(feature = "bluetooth")]
            bt,
        }
    }

    /// Loads `wifi_fw` and `nvram`, optionally validates/loads `bt_fw`, then starts firmware cores and transport queues.
    /// Returns Err for invalid image placement, Bluetooth records or exhausted ALP/F2 polls; hardware/assertion failures can panic.
    /// HT-clock waits have no deadline, and each bus operation can stall. Canceling leaves partially initialized hardware.
    /// Rejects image overlap, out-of-RAM placement and unrepresentable NVRAM word count before bus initialization.
    pub(crate) async fn init(
        &mut self,
        wifi_fw: &Aligned<A4, [u8]>,
        nvram: &Aligned<A4, [u8]>,
        bt_fw: Option<&[u8]>,
    ) -> Result<(), ()> {
        let layout = firmware_layout(
            wifi_fw.len(),
            nvram.len(),
            CHIP.atcm_ram_base_address,
            CHIP.chip_ram_size,
        )?;
        #[cfg(feature = "bluetooth")]
        if let Some(firmware) = bt_fw {
            crate::bluetooth_firmware::Firmware::new(firmware, CHIP.bluetooth_base_address)?;
        }
        self.bus.init(bt_fw.is_some()).await;

        // Init ALP (Active Low Power) clock
        debug!("init alp");
        match BUS::TYPE {
            BusType::Spi => {
                self.bus
                    .write8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR, BACKPLANE_ALP_AVAIL_REQ)
                    .await;

                // Not present in whd driver
                debug!("set f2 watermark");
                self.bus
                    .write8(FUNC_BACKPLANE, REG_BACKPLANE_FUNCTION2_WATERMARK, 0x10)
                    .await;
                let watermark = self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_FUNCTION2_WATERMARK).await;
                debug!("watermark = {:02x}", watermark);
                assert!(watermark == 0x10);
            }
            BusType::Sdio => {
                self.bus
                    .write8(
                        FUNC_BACKPLANE,
                        REG_BACKPLANE_CHIP_CLOCK_CSR,
                        BACKPLANE_FORCE_HW_CLKREQ_OFF | BACKPLANE_ALP_AVAIL_REQ | BACKPLANE_FORCE_ALP,
                    )
                    .await;
            }
        }

        debug!("waiting for clock...");
        if !try_until(
            async || self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR).await & BACKPLANE_ALP_AVAIL != 0,
            Duration::from_millis(100),
        )
        .await
        {
            debug!("timeout while waiting for alp clock!");
            return Err(());
        }
        debug!("clock ok");

        // clear request for ALP
        debug!("clear request for ALP");
        self.bus.write8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR, 0).await;

        let chip_id = match BUS::TYPE {
            BusType::Spi => self.bus.bp_read16(CHIPCOMMON_BASE_ADDRESS).await,
            BusType::Sdio => {
                // Disable the extra sdio pull-ups
                // self.bus.write8(FUNC_BACKPLANE, SDIO_PULL_UP, 0).await;

                // Enable f1 and f2
                self.bus
                    .write8(
                        FUNC_BUS,
                        SDIOD_CCCR_IOEN,
                        (SDIO_FUNC_ENABLE_1 | SDIO_FUNC_ENABLE_2) as u8,
                    )
                    .await;

                // Enable out-of-band interrupt signal
                // whd_bus_sdio_init_oob_intr

                // Note: only GPIO0 using rising edge is currently supported
                self.bus
                    .write8(
                        FUNC_BUS,
                        SDIOD_SEP_INT_CTL,
                        (SEP_INTR_CTL_MASK | SEP_INTR_CTL_EN | SEP_INTR_CTL_POL) as u8,
                    )
                    .await;

                // Enable f2 interrupt only
                self.bus
                    .write8(
                        FUNC_BUS,
                        SDIOD_CCCR_INTEN,
                        (INTR_CTL_MASTER_EN | INTR_CTL_FUNC2_EN) as u8,
                    )
                    .await;

                self.bus.read8(FUNC_BUS, SDIOD_CCCR_IORDY).await;

                let reg = self.bus.read8(FUNC_BUS, SDIOD_CCCR_BRCM_CARDCAP).await;
                if reg & SDIOD_CCCR_BRCM_CARDCAP_SECURE_MODE as u8 != 0 {
                    debug!("chip supports bootloader handshake");

                    let devctrl = self.bus.read8(FUNC_BACKPLANE, SBSDIO_DEVICE_CTL).await;

                    self.bus
                        .write8(
                            FUNC_BACKPLANE,
                            SBSDIO_DEVICE_CTL,
                            devctrl | SBSDIO_DEVCTL_ADDR_RST as u8,
                        )
                        .await;

                    let addr_low = self.bus.read8(FUNC_BACKPLANE, SBSDIO_FUNC1_SBADDRLOW).await as u32;
                    let addr_mid = self.bus.read8(FUNC_BACKPLANE, SBSDIO_FUNC1_SBADDRMID).await as u32;
                    let addr_high = self.bus.read8(FUNC_BACKPLANE, SBSDIO_FUNC1_SBADDRHIGH).await as u32;

                    let reg_addr = ((addr_low << 8) | (addr_mid << 16) | (addr_high << 24)) + SDIO_CORE_CHIPID_REG;

                    self.bus.write8(FUNC_BACKPLANE, SBSDIO_DEVICE_CTL, devctrl).await;

                    self.bus.bp_read16(reg_addr).await
                } else {
                    self.bus.bp_read16(CHIPCOMMON_BASE_ADDRESS).await
                }
            }
        };

        debug!("chip ID: {}", chip_id);

        // Upload firmware.
        self.core_disable(Core::WLAN).await;
        self.core_disable(Core::SOCSRAM).await; // TODO: is this needed if we reset right after?
        self.core_reset(Core::SOCSRAM).await;

        // this is 4343x specific stuff: Disable remap for SRAM_3
        self.bus.bp_write32(CHIP.socsram_base_address + 0x10, 3).await;
        self.bus.bp_write32(CHIP.socsram_base_address + 0x44, 0).await;

        let ram_addr = CHIP.atcm_ram_base_address;

        debug!("loading fw");
        self.bus.bp_write(ram_addr, wifi_fw).await;

        debug!("loading nvram");
        self.bus.bp_write(layout.nvram_address, nvram).await;
        self.bus.bp_write32(layout.magic_address, layout.magic).await;

        // Start core!
        debug!("starting up core...");
        self.core_reset(Core::WLAN).await;
        assert!(self.core_is_up(Core::WLAN).await);

        // wait until HT clock is available; takes about 29ms
        debug!("waiting for HT clock...");
        while self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR).await & 0x80 == 0 {}

        // "Set up the interrupt mask and enable interrupts"
        debug!("setup interrupt mask");
        self.bus
            .bp_write32(CHIP.sdiod_core_base_address + SDIO_INT_HOST_MASK, I_HMB_SW_MASK)
            .await;

        match BUS::TYPE {
            BusType::Sdio => {
                self.bus
                    .bp_write8(CHIP.sdiod_core_base_address + SDIO_FUNCTION_INT_MASK, 2 | 1)
                    .await;

                // "Lower F2 Watermark to avoid DMA Hang in F2 when SD Clock is stopped."
                // Sounds scary...
                self.bus
                    .write8(FUNC_BACKPLANE, REG_BACKPLANE_FUNCTION2_WATERMARK, SDIO_F2_WATERMARK)
                    .await;
            }

            BusType::Spi => {
                // Set up the interrupt mask and enable interrupts
                if bt_fw.is_some() {
                    debug!("bluetooth setup interrupt mask");
                    self.bus
                        .bp_write32(CHIP.sdiod_core_base_address + SDIO_INT_HOST_MASK, I_HMB_FC_CHANGE)
                        .await;
                }

                self.bus
                    .write16(FUNC_BUS, REG_BUS_INTERRUPT_ENABLE, IRQ_F2_PACKET_AVAILABLE)
                    .await;

                // "Lower F2 Watermark to avoid DMA Hang in F2 when SD Clock is stopped."
                // Sounds scary...
                self.bus
                    .write8(FUNC_BACKPLANE, REG_BACKPLANE_FUNCTION2_WATERMARK, SPI_F2_WATERMARK)
                    .await;
            }
        }

        // wait for F2 to be ready
        debug!("waiting for F2 to be ready...");
        if !try_until(
            async || match BUS::TYPE {
                BusType::Sdio => self.bus.read8(FUNC_BUS, SDIOD_CCCR_IORDY).await as u32 & SDIO_FUNC_READY_2 != 0,
                BusType::Spi => self.bus.read32(FUNC_BUS, REG_BUS_STATUS).await & STATUS_F2_RX_READY != 0,
            },
            Duration::from_millis(1000),
        )
        .await
        {
            debug!("timeout while waiting for function 2 to be ready");
            return Err(());
        }

        match BUS::TYPE {
            BusType::Sdio => {
                self.bus
                    .write8(FUNC_BACKPLANE, SDIO_SLEEP_CSR, SBSDIO_SLPCSR_KEEP_WL_KS as u8)
                    .await;

                self.bus
                    .write8(FUNC_BACKPLANE, SDIO_SLEEP_CSR, SBSDIO_SLPCSR_KEEP_WL_KS as u8)
                    .await;

                assert!(self.bus.read8(FUNC_BACKPLANE, SDIO_SLEEP_CSR).await & SBSDIO_SLPCSR_KEEP_WL_KS as u8 != 0);
            }
            BusType::Spi => {}
        }

        // Some random configs related to sleep.
        // These aren't needed if we don't want to sleep the bus.
        // TODO do we need to sleep the bus to read the irq line, due to
        // being on the same pin as MOSI/MISO?

        // let mut val = self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_WAKEUP_CTRL).await;
        // val |= 0x02; // WAKE_TILL_HT_AVAIL
        // self.bus.write8(FUNC_BACKPLANE, REG_BACKPLANE_WAKEUP_CTRL, val).await;
        // self.bus.write8(FUNC_BUS, 0xF0, 0x08).await; // SDIOD_CCCR_BRCM_CARDCAP.CMD_NODEC = 1
        // self.bus.write8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR, 0x02).await; // SBSDIO_FORCE_HT
        //
        // let mut val = self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_SLEEP_CSR).await;
        // val |= 0x01; // SBSDIO_SLPCSR_KEEP_SDIO_ON
        // self.bus.write8(FUNC_BACKPLANE, REG_BACKPLANE_SLEEP_CSR, val).await;

        // clear pulls
        debug!("clear pad pulls");
        self.bus.write8(FUNC_BACKPLANE, REG_BACKPLANE_PULL_UP, 0).await;
        let _ = self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_PULL_UP).await;

        // start HT clock
        self.bus
            .write8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR, 0x10)
            .await; // SBSDIO_HT_AVAIL_REQ
        debug!("waiting for HT clock...");
        while self.bus.read8(FUNC_BACKPLANE, REG_BACKPLANE_CHIP_CLOCK_CSR).await & 0x80 == 0 {}
        debug!("clock ok");

        #[cfg(feature = "firmware-logs")]
        self.log_init().await;

        #[cfg(feature = "bluetooth")]
        if let Some(bt_fw) = bt_fw {
            self.bt.as_mut().unwrap().init_bluetooth(&mut self.bus, bt_fw).await?;
        }

        debug!("cyw43 runner init done");

        Ok(())
    }

    #[cfg(feature = "firmware-logs")]
    /// Discovers the firmware console descriptor through the RAM-end shared pointer; trusts firmware addresses.
    async fn log_init(&mut self) {
        // Initialize shared memory for logging.

        let addr = CHIP.atcm_ram_base_address + CHIP.chip_ram_size - 4 - CHIP.socram_srmem_size;
        let shared_addr = self.bus.bp_read32(addr).await;
        debug!("shared_addr {:08x}", shared_addr);

        let mut shared: Aligned<A4, [u8; _]> = Aligned([0; SharedMemData::SIZE]);
        self.bus.bp_read(shared_addr, &mut shared[..]).await;
        let shared = SharedMemData::from_bytes(&shared);

        self.log.addr = shared.console_addr + 8;
    }

    #[cfg(feature = "firmware-logs")]
    /// Reads one 1024-byte firmware ring snapshot and emits completed nonempty lines, truncating lines to 256 bytes.
    /// Invalid producer offsets are ignored; overwritten ring data and missed complete laps cannot be recovered.
    async fn log_read(&mut self) {
        // Read log struct
        let mut log: Aligned<A4, [u8; _]> = Aligned([0; SharedMemLog::SIZE]);
        self.bus.bp_read(self.log.addr, &mut log[..]).await;
        let log = SharedMemLog::from_bytes(&log);

        let idx = log.idx as usize;

        // If pointer hasn't moved, no need to do anything.
        if idx >= 0x400 || idx == self.log.last_idx {
            return;
        }

        // Read entire buf for now. We could read only what we need, but then we
        // run into annoying alignment issues in `bp_read`.
        let mut buf: Aligned<A4, [u8; _]> = Aligned([0; 0x400]);
        self.bus.bp_read(log.buf, &mut buf[..]).await;

        while self.log.last_idx != idx as usize {
            let b = buf[self.log.last_idx];
            if b == b'\r' || b == b'\n' {
                if self.log.buf_count != 0 {
                    let s = core::str::from_utf8(&self.log.buf[..self.log.buf_count]).unwrap_or("<invalid UTF-8>");
                    debug!("LOGS: {}", s);
                    self.log.buf_count = 0;
                }
            } else if self.log.buf_count < self.log.buf.len() {
                self.log.buf[self.log.buf_count] = b;
                self.log.buf_count += 1;
            }

            self.log.last_idx += 1;
            if self.log.last_idx == 0x400 {
                self.log.last_idx = 0;
            }
        }
    }

    /// Never returns: runs with an empty maintenance hook; panics propagate and canceling may leave bus/DMA work active.
    pub async fn run(self) -> ! {
        self.run_with_boundary(async || {}).await
    }

    /// Never returns: calls `boundary` before each service turn after prior bus operations complete.
    /// The hook may park this sole owner. The loop retains a 2048-byte scratch buffer and bounds each F2 drain to four frames.
    /// Bluetooth uses a 1 ms polling fallback and receives after every selected branch; bus waits are not time-bounded.
    /// Cancellation is not safe maintenance: it can leave DMA, IOCTL state or radio ring transactions unfinished.
    pub async fn run_with_boundary<F: AsyncFnMut()>(mut self, mut boundary: F) -> ! {
        let mut buf = [0; 512];
        loop {
            boundary().await;
            embassy_futures::yield_now().await;
            #[cfg(feature = "firmware-logs")]
            self.log_read().await;

            if self.has_credit() {
                let ioctl = self.ioctl_state.wait_pending();
                let wifi_tx = self.ch.tx_buf();
                #[cfg(feature = "bluetooth")]
                let bt_tx = async {
                    match &mut self.bt {
                        Some(bt) => bt.tx_chan.receive().await,
                        None => core::future::pending().await,
                    }
                };
                #[cfg(not(feature = "bluetooth"))]
                let bt_tx = core::future::pending::<()>();

                // Released Bluetooth IRQ support is incomplete. A 1 ms fallback
                // bounds idle bus work and maintenance wakeup, without losing TX wakeups.
                #[cfg(feature = "bluetooth")]
                let ev = Timer::after_millis(1);
                #[cfg(not(feature = "bluetooth"))]
                let ev = self.bus.wait_for_event();

                match select4(ioctl, wifi_tx, bt_tx, ev).await {
                    Either4::First(PendingIoctl {
                        buf: iobuf,
                        kind,
                        cmd,
                        iface,
                    }) => {
                        self.send_ioctl(kind, cmd, iface, unsafe { &*iobuf }, &mut buf).await;
                        self.check_status(&mut buf).await;
                    }
                    Either4::Second(packet) => {
                        trace!("Ethernet TX: {} bytes", packet.len());

                        let buf8 = slice8_mut(&mut buf);

                        // There MUST be 2 bytes of padding between the SDPCM and BDC headers.
                        // And ONLY for data packets!
                        // No idea why, but the firmware will append two zero bytes to the tx'd packets
                        // otherwise. If the packet is exactly 1514 bytes (the max MTU), this makes it
                        // be oversized and get dropped.
                        // WHD adds it here https://github.com/Infineon/wifi-host-driver/blob/c04fcbb6b0d049304f376cf483fd7b1b570c8cd5/WiFi_Host_Driver/src/include/whd_sdpcm.h#L90
                        // and adds it to the header size her https://github.com/Infineon/wifi-host-driver/blob/c04fcbb6b0d049304f376cf483fd7b1b570c8cd5/WiFi_Host_Driver/src/whd_sdpcm.c#L597
                        // ¯\_(ツ)_/¯
                        /// Firmware-required bytes between SDPCM and BDC for Ethernet TX only.
                        const PADDING_SIZE: usize = 2;
                        let total_len = SdpcmHeader::SIZE + PADDING_SIZE + BdcHeader::SIZE + packet.len();

                        let seq = self.sdpcm_seq;
                        self.sdpcm_seq = self.sdpcm_seq.wrapping_add(1);

                        let sdpcm_header = SdpcmHeader {
                            len: total_len as u16, // TODO does this len need to be rounded up to u32?
                            len_inv: !total_len as u16,
                            sequence: seq,
                            channel_and_flags: CHANNEL_TYPE_DATA,
                            next_length: 0,
                            header_length: (SdpcmHeader::SIZE + PADDING_SIZE) as _,
                            wireless_flow_control: 0,
                            bus_data_credit: 0,
                            reserved: [0, 0],
                        };

                        let bdc_header = BdcHeader {
                            flags: BDC_VERSION << BDC_VERSION_SHIFT,
                            priority: 0,
                            flags2: 0,
                            data_offset: 0,
                        };
                        trace!("tx {:?}", sdpcm_header);
                        trace!("    {:?}", bdc_header);

                        buf8[0..SdpcmHeader::SIZE].copy_from_slice(&sdpcm_header.to_bytes());
                        buf8[SdpcmHeader::SIZE + PADDING_SIZE..][..BdcHeader::SIZE]
                            .copy_from_slice(&bdc_header.to_bytes());
                        buf8[SdpcmHeader::SIZE + PADDING_SIZE + BdcHeader::SIZE..][..packet.len()]
                            .copy_from_slice(packet);

                        let total_len = (total_len + 3) & !3; // round up to 4byte

                        trace!("SDPCM TX: {} transport bytes", total_len);

                        self.bus.wlan_write(&aligned_ref(&buf)[..total_len]).await;
                        self.ch.tx_done();
                        self.check_status(&mut buf).await;
                    }
                    Either4::Third(_) => {
                        #[cfg(feature = "bluetooth")]
                        self.bt.as_mut().unwrap().hci_write(&mut self.bus).await;
                    }
                    Either4::Fourth(()) => {
                        self.handle_irq(&mut buf).await;

                        // If we do busy-polling, make sure to yield.
                        // `handle_irq` will only do a 32bit read if there's no work to do, which is really fast.
                        // Depending on optimization level, it is possible that the 32-bit read finishes on
                        // first poll, so it never yields and we starve all other tasks.
                        #[cfg(feature = "bluetooth")]
                        embassy_futures::yield_now().await;
                    }
                }
            } else {
                warn!("TX stalled");
                match BUS::TYPE {
                    BusType::Sdio => {
                        // whd_bus_sdio_poke_wlan
                        self.bus
                            .bp_write32(CHIP.sdiod_core_base_address + SDIO_TO_SB_MAILBOX, SMB_DEV_INT)
                            .await;
                    }
                    BusType::Spi => {}
                }
                #[cfg(feature = "bluetooth")]
                Timer::after_millis(1).await;
                #[cfg(not(feature = "bluetooth"))]
                self.bus.wait_for_event().await;
                self.handle_irq(&mut buf).await;
            }
            // Service RX after every selected branch, including continuously
            // ready TX/ioctl. No branch can keep resetting the idle timer.
            #[cfg(feature = "bluetooth")]
            if let Some(bt) = &mut self.bt {
                bt.handle_irq(&mut self.bus).await;
            }
        }
    }

    /// Services current transport interrupt/mailbox state using `buf`; does not wait for the IRQ itself.
    /// SDIO acknowledges mailbox after drain; SPI clears unavailable-data IRQ. At most one bounded drain is performed.
    async fn handle_irq(&mut self, buf: &mut [u32; 512]) {
        match BUS::TYPE {
            BusType::Sdio => {
                // TODO: get irqs working
                self.check_status(buf).await;

                // whd_bus_sdio_packet_available_to_read
                let irq = self.bus.bp_read32(CHIP.sdiod_core_base_address + SDIO_INT_STATUS).await;
                if irq & I_HMB_HOST_INT == 0 {
                    // return;
                }

                let hmb_data = self
                    .bus
                    .bp_read32(CHIP.sdiod_core_base_address + SDIO_TO_HOST_MAILBOX_DATA)
                    .await;
                self.bus
                    .bp_write32(CHIP.sdiod_core_base_address + SDIO_TO_SB_MAILBOX, SMB_INT_ACK)
                    .await;

                trace!("hmb ack");
                if hmb_data & I_HMB_DATA_FWHALT != 0 {
                    debug!("hmb data fault");
                }

                // Clear irq must be done here to avoid a race
                if irq & HOSTINTMASK != 0 {
                    trace!("clear irq");
                    self.bus
                        .bp_write32(CHIP.sdiod_core_base_address + SDIO_INT_STATUS, irq & HOSTINTMASK)
                        .await;
                }
            }
            BusType::Spi => {
                // Receive stuff
                let irq = self.bus.read16(FUNC_BUS, REG_BUS_INTERRUPT).await;
                if irq != 0 {
                    trace!("irq{}", FormatInterrupt(irq));
                }

                if irq & IRQ_F2_PACKET_AVAILABLE != 0 {
                    self.check_status(buf).await;
                }

                if irq & IRQ_DATA_UNAVAILABLE != 0 {
                    // this seems to be ignorable with no ill effects.
                    trace!("IRQ DATA_UNAVAILABLE, clearing...");
                    self.bus.write16(FUNC_BUS, REG_BUS_INTERRUPT, 1).await;
                }
            }
        }
    }

    /// Drains up to four F2 packets into `buf`, admitting at most 2048 bytes each before parsing/updating credits.
    /// Malformed/oversized framing stops or skips the current drain without allocating. Transport calls can stall.
    pub(crate) async fn check_status(&mut self, buf: &mut [u32; 512]) {
        for _ in 0..4 {
            match BUS::TYPE {
                BusType::Spi => {
                    let status = self.bus.read32(FUNC_BUS, SPI_STATUS_REGISTER).await;
                    trace!("check status{}", FormatStatus(status));

                    if status & STATUS_F2_PKT_AVAILABLE != 0 {
                        let len = (status & STATUS_F2_PKT_LEN_MASK) >> STATUS_F2_PKT_LEN_SHIFT;
                        if len as usize > core::mem::size_of_val(buf) {
                            break;
                        }
                        self.bus.wlan_read(&mut aligned_mut(buf)[..len as usize]).await;
                        trace!("SDPCM RX: {} bytes", len);
                        self.rx(&mut slice8_mut(buf)[..len as usize]);
                    } else {
                        break;
                    }
                }
                BusType::Sdio => {
                    self.bus.wlan_read(&mut aligned_mut(&mut buf[..1])).await;
                    let (len, len_inv) = {
                        let hwtag = slice16_mut(&mut buf[..1]);

                        (hwtag[0], hwtag[1])
                    };

                    if (len | len_inv) == 0 || (len ^ len_inv) != 0xFFFF {
                        trace!("hwtag mismatch (hwtag[0] = {}, hwtag[1] = {})", len, len_inv);
                        break;
                    }

                    trace!("pkt ready...");
                    let len = len as usize;
                    if len > core::mem::size_of_val(buf) {
                        break;
                    }
                    if len > INITIAL_READ as usize {
                        self.bus
                            .wlan_read(&mut aligned_mut(&mut buf[1..])[..len - INITIAL_READ as usize])
                            .await;
                    } else {
                        // TODO: investigate this condition
                        trace!("no extra space required");
                        continue;
                    }

                    if len == SdpcmHeader::SIZE {
                        let Some((sdpcm_header, _)) = SdpcmHeader::parse(slice8_mut(&mut buf[..3])) else {
                            debug!("failed to parse sdpcm header");
                            break;
                        };

                        self.update_credit(&sdpcm_header);
                    } else if len > SdpcmHeader::SIZE {
                        trace!("SDPCM RX: {} bytes", len);
                        self.rx(&mut slice8_mut(buf)[..len]);
                    }
                }
            }
        }
    }

    /// Processes aligned SDPCM `packet`: updates credit, completes matching IOCTLs, publishes events or queues Ethernet.
    /// Bad bounds/identifiers are ignored; matched nonzero IOCTL status panics. Two-byte packet alignment is required.
    /// Event parsing mutates byte order in place; full queues drop/overwrite data according to channel type.
    pub(crate) fn rx(&mut self, packet: &mut [u8]) {
        let Some((sdpcm_header, payload)) = SdpcmHeader::parse(packet) else {
            return;
        };

        self.update_credit(&sdpcm_header);

        let channel = sdpcm_header.channel_and_flags & 0x0F;

        match channel {
            CHANNEL_TYPE_CONTROL => {
                let Some((cdc_header, response)) = CdcHeader::parse(payload) else {
                    return;
                };
                trace!("    {:?}", cdc_header);

                if cdc_header.id == self.ioctl_id {
                    if cdc_header.status != 0 {
                        // TODO: propagate error instead
                        panic!("IOCTL error {}", cdc_header.status as i32);
                    }

                    self.ioctl_state.ioctl_done(response);
                }
            }
            CHANNEL_TYPE_EVENT => {
                let Some((_, bdc_packet)) = BdcHeader::parse(payload) else {
                    warn!("BDC event, incomplete header");
                    return;
                };

                let Some((event_packet, evt_data)) = EventPacket::parse(bdc_packet) else {
                    warn!("BDC event, incomplete data");
                    return;
                };

                /// Ethernet type assigned to Broadcom link-control event envelopes.
                const ETH_P_LINK_CTL: u16 = 0x886C;
                if event_packet.eth.ether_type != ETH_P_LINK_CTL {
                    warn!(
                        "unexpected ethernet type 0x{:04x}, expected Broadcom ether type 0x{:04x}",
                        event_packet.eth.ether_type, ETH_P_LINK_CTL
                    );
                    return;
                }
                /// Required vendor identifier for accepted event envelopes.
                const BROADCOM_OUI: &[u8] = &[0x00, 0x10, 0x18];
                if event_packet.hdr.oui != BROADCOM_OUI {
                    warn!(
                        "unexpected ethernet OUI {:02x}, expected Broadcom OUI {:02x}",
                        Bytes(&event_packet.hdr.oui),
                        Bytes(BROADCOM_OUI)
                    );
                    return;
                }
                /// Long vendor envelope subtype required before interpreting event metadata.
                const BCMILCP_SUBTYPE_VENDOR_LONG: u16 = 32769;
                if event_packet.hdr.subtype != BCMILCP_SUBTYPE_VENDOR_LONG {
                    warn!("unexpected subtype {}", event_packet.hdr.subtype);
                    return;
                }

                /// Vendor payload subtype identifying an event message.
                const BCMILCP_BCM_SUBTYPE_EVENT: u16 = 1;
                if event_packet.hdr.user_subtype != BCMILCP_BCM_SUBTYPE_EVENT {
                    warn!("unexpected user_subtype {}", event_packet.hdr.subtype);
                    return;
                }

                // Wire fields are u32: truncation could turn an unknown event
                // into JOIN or an unknown failure status into SUCCESS.
                let event_type = u8::try_from(event_packet.msg.event_type)
                    .map(Event::from)
                    .unwrap_or(Event::Unknown);
                if matches!(event_type, Event::Unknown | Event::LAST) {
                    return;
                }
                let status = u8::try_from(event_packet.msg.status)
                    .map(EStatus::from)
                    .unwrap_or(EStatus::Unknown);
                debug!(
                    "=== EVENT {:?}: {:?}, {} payload bytes",
                    event_type,
                    event_packet.msg,
                    evt_data.len()
                );

                let update_link_status = match (
                    event_type,
                    status,
                    event_packet.msg.flags,
                    event_packet.msg.reason,
                    event_packet.msg.auth_type,
                ) {
                    // Events indicating that the link is down
                    // Event LINK with flag 0 indicates link down. reason = 1: loss of signal (e.g. out of range),
                    // reason = 2: controlled network shutdown Event AUTH with status FAIL, reason
                    // 16, and auth_type 3 is specific for WPA3 networks
                    (Event::LINK, EStatus::SUCCESS, 0, ..)
                    | (Event::DEAUTH, EStatus::SUCCESS, ..)
                    | (Event::AUTH, EStatus::FAIL, _, 16, 3) => {
                        self.auth_ok = false;
                        self.join_ok = false;
                        self.key_exchange_ok = false;
                        true
                    }
                    // Update auth flag. Ignore unsolicited events.
                    // When changing passwords on a WPA3 AP which we are already connected to, or we roam to, PSK_SUP
                    // events indicating success are still sent. Only the AUTH events indicate
                    // failure and this flag helps cover that scenario
                    (Event::AUTH, status, ..) if status != EStatus::UNSOLICITED => {
                        self.auth_ok = status == EStatus::SUCCESS;
                        debug!("auth_ok flag: {}", self.auth_ok as u8);
                        false
                    }
                    // Successfully joined the network. Open or WPA3 networks are now fully connected - WPA1/2 networks
                    // additionally require a successful key exchange.
                    (Event::JOIN, EStatus::SUCCESS, ..) => {
                        self.join_ok = true;
                        true
                    }

                    // Key exchange events (PSK_SUP) for secure networks
                    // The status codes for PSK_SUP events seem to have different meanings from other event types

                    // Successful key exchange, indicated by a PSK_SUP event with status 6 "UNSOLICITED"
                    // Disregard if auth_ok is false, which can happen in WPA3 networks
                    (Event::PSK_SUP, EStatus::UNSOLICITED, 0, 0, _) => {
                        if self.auth_ok {
                            self.key_exchange_ok = true;
                            true
                        } else {
                            false
                        }
                    }
                    // Ignore PSK_SUP events with reason 14 as they are often sent when the device roams from one AP to
                    // another
                    (Event::PSK_SUP, _, _, 14, _) => false,
                    // Other PSK_SUP events indicate key exchange errors
                    (Event::PSK_SUP, ..) => {
                        self.key_exchange_ok = false;
                        true
                    }
                    _ => false,
                };

                if update_link_status {
                    let secure_network = self.secure_network.load(Relaxed);
                    let link_state = if self.join_ok && (!secure_network || self.key_exchange_ok) {
                        LinkState::Up
                    } else {
                        LinkState::Down
                    };

                    self.ch.set_link_state(link_state);

                    debug!(
                        "link_ok: {}, secure_network: {}, auth_ok: {}, password_ok: {}, link_state {}",
                        self.join_ok as u8,
                        secure_network as u8,
                        self.auth_ok as u8,
                        self.key_exchange_ok as u8,
                        link_state as u8
                    );
                }

                if self.events.mask.is_enabled(event_type) {
                    let status = event_packet.msg.status;
                    let event_payload = match event_type {
                        Event::ESCAN_RESULT if status == EStatus::PARTIAL => {
                            let Some((_, bss_info)) = ScanResults::parse(evt_data) else {
                                return;
                            };
                            let Some(bss_info) = BssInfo::parse(bss_info) else {
                                return;
                            };
                            events::Payload::BssInfo(*bss_info)
                        }
                        Event::ESCAN_RESULT => events::Payload::None,
                        _ => events::Payload::None,
                    };

                    // this intentionally uses the non-blocking publish immediate
                    // publish() is a deadlock risk in the current design as awaiting here prevents ioctls
                    // The `Runner` always yields when accessing the device, so consumers always have a chance to
                    // receive the event (if they are actively awaiting the queue)
                    self.events
                        .queue
                        .immediate_publisher()
                        .publish_immediate(events::Message::new(Status { event_type, status }, event_payload));
                }
            }
            CHANNEL_TYPE_DATA => {
                let Some((_, packet)) = BdcHeader::parse(payload) else {
                    return;
                };
                trace!("Ethernet RX: {} bytes", packet.len());

                if packet.len() > MTU {
                    warn!("oversized WLAN packet");
                    return;
                }
                match self.ch.try_rx_buf() {
                    Some(buf) => {
                        buf[..packet.len()].copy_from_slice(packet);
                        self.ch.rx_done(packet.len())
                    }
                    None => warn!("failed to push rxd packet to the channel."),
                }
            }
            _ => {}
        }
    }

    /// Adopts control/event/data credit from `sdpcm_header`; invalid forward windows above 64 clamp to two credits.
    /// Sequence arithmetic wraps modulo 256; unknown channels leave the current window unchanged.
    fn update_credit(&mut self, sdpcm_header: &SdpcmHeader) {
        if sdpcm_header.channel_and_flags & 0xF < 3 {
            let mut sdpcm_seq_max = sdpcm_header.bus_data_credit;
            if sdpcm_seq_max.wrapping_sub(self.sdpcm_seq) > 0x40 {
                sdpcm_seq_max = self.sdpcm_seq.wrapping_add(2);
            }
            self.sdpcm_seq_max = sdpcm_seq_max;
        }
    }

    /// Returns whether the exclusive credit limit is ahead by 1..=127 in wrapping sequence space.
    fn has_credit(&self) -> bool {
        self.sdpcm_seq != self.sdpcm_seq_max && self.sdpcm_seq_max.wrapping_sub(self.sdpcm_seq) & 0x80 == 0
    }

    /// Frames `kind`/`cmd` for `iface` with `data` in initialized `buf`, increments wrapping identities, and transmits.
    /// Data must fit 2020 bytes after headers; larger inputs panic while copying. Interface is truncated to u16 before shifting.
    /// Cancel/failure can consume identities and partially deliver; this method does not await a firmware response.
    async fn send_ioctl(&mut self, kind: IoctlType, cmd: Ioctl, iface: u32, data: &[u8], buf: &mut [u32; 512]) {
        let buf8 = slice8_mut(buf);

        let total_len = SdpcmHeader::SIZE + CdcHeader::SIZE + data.len();

        let sdpcm_seq = self.sdpcm_seq;
        self.sdpcm_seq = self.sdpcm_seq.wrapping_add(1);
        self.ioctl_id = self.ioctl_id.wrapping_add(1);

        let sdpcm_header = SdpcmHeader {
            len: total_len as u16, // TODO does this len need to be rounded up to u32?
            len_inv: !total_len as u16,
            sequence: sdpcm_seq,
            channel_and_flags: CHANNEL_TYPE_CONTROL,
            next_length: 0,
            header_length: SdpcmHeader::SIZE as _,
            wireless_flow_control: 0,
            bus_data_credit: 0,
            reserved: [0, 0],
        };

        let cdc_header = CdcHeader {
            cmd: cmd as u32,
            len: data.len() as _,
            flags: kind as u16 | (iface as u16) << 12,
            id: self.ioctl_id,
            status: 0,
        };
        trace!("tx {:?}", sdpcm_header);
        trace!("    {:?}", cdc_header);

        buf8[0..SdpcmHeader::SIZE].copy_from_slice(&sdpcm_header.to_bytes());
        buf8[SdpcmHeader::SIZE..][..CdcHeader::SIZE].copy_from_slice(&cdc_header.to_bytes());
        buf8[SdpcmHeader::SIZE + CdcHeader::SIZE..][..data.len()].copy_from_slice(data);

        let total_len = (total_len + 3) & !3; // round up to 4byte,
        trace!("SDPCM control TX: {} transport bytes", total_len);

        self.bus.wlan_write(&aligned_ref(buf)[..total_len]).await;
    }

    /// Asserts reset for firmware `core`, skipping already-reset cores; blocks locally for 1 ms before asserting reset.
    /// No verification result or rollback is supplied; bus errors follow adapter policy.
    async fn core_disable(&mut self, core: Core) {
        let base = core.base_addr();

        // Dummy read?
        let _ = self.bus.bp_read8(base + AI_RESETCTRL_OFFSET).await;

        // Check it isn't already reset
        let r = self.bus.bp_read8(base + AI_RESETCTRL_OFFSET).await;
        if r & AI_RESETCTRL_BIT_RESET != 0 {
            return;
        }

        self.bus.bp_write8(base + AI_IOCTRL_OFFSET, 0).await;
        let _ = self.bus.bp_read8(base + AI_IOCTRL_OFFSET).await;

        block_for(Duration::from_millis(1));

        self.bus
            .bp_write8(base + AI_RESETCTRL_OFFSET, AI_RESETCTRL_BIT_RESET)
            .await;
        let _ = self.bus.bp_read8(base + AI_RESETCTRL_OFFSET).await;
    }

    /// Disables then restarts firmware `core`, with two asynchronous 1 ms settling waits.
    /// Cancellation can leave reset/forced-clock state between steps; core_is_up performs the separate status check.
    async fn core_reset(&mut self, core: Core) {
        self.core_disable(core).await;

        let base = core.base_addr();
        self.bus
            .bp_write8(base + AI_IOCTRL_OFFSET, AI_IOCTRL_BIT_FGC | AI_IOCTRL_BIT_CLOCK_EN)
            .await;
        let _ = self.bus.bp_read8(base + AI_IOCTRL_OFFSET).await;

        self.bus.bp_write8(base + AI_RESETCTRL_OFFSET, 0).await;

        Timer::after_millis(1).await;

        self.bus
            .bp_write8(base + AI_IOCTRL_OFFSET, AI_IOCTRL_BIT_CLOCK_EN)
            .await;
        let _ = self.bus.bp_read8(base + AI_IOCTRL_OFFSET).await;

        Timer::after_millis(1).await;
    }

    /// Returns whether `core` has clock enabled without forced gate and reset deasserted, from two register reads.
    /// This is a transient register observation, not proof the firmware is responsive.
    async fn core_is_up(&mut self, core: Core) -> bool {
        let base = core.base_addr();

        let io = self.bus.bp_read8(base + AI_IOCTRL_OFFSET).await;
        if io & (AI_IOCTRL_BIT_FGC | AI_IOCTRL_BIT_CLOCK_EN) != AI_IOCTRL_BIT_CLOCK_EN {
            debug!("core_is_up: returning false due to bad ioctrl {:02x}", io);
            return false;
        }

        let r = self.bus.bp_read8(base + AI_RESETCTRL_OFFSET).await;
        if r & (AI_RESETCTRL_BIT_RESET) != 0 {
            debug!("core_is_up: returning false due to bad resetctrl {:02x}", r);
            return false;
        }

        true
    }
}
