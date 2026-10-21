//! CYW43439 bus registers, shared-memory encodings and retained firmware command identifiers.
//! Active protocol use is defined by the transport/runner/control modules; retained identifiers alone do not qualify a feature.
//! References: [Infineon SDIO registers](https://raw.githubusercontent.com/Infineon/wifi-host-driver/master/WHD/COMPONENT_WIFI6/src/bus_protocols/whd_sdio.h)
//! and [firmware IOCTL catalogue](https://raw.githubusercontent.com/Infineon/wifi-host-driver/master/WHD/COMPONENT_WIFI6/src/include/whd_wlioctl.h).
#![allow(unused)]

/// Bus-control function number used by SPI commands and SDIO CCCR access.
pub(crate) const FUNC_BUS: u32 = 0;
/// Function number for windowed backplane register/RAM access.
pub(crate) const FUNC_BACKPLANE: u32 = 1;
/// Function number carrying SDPCM Wi-Fi control, event and Ethernet frames.
pub(crate) const FUNC_WLAN: u32 = 2;
/// Retained function-three Bluetooth encoding; active Bluetooth uses shared-memory rings instead.
pub(crate) const FUNC_BT: u32 = 3;

// Register addresses
/// Function-zero SPI control register byte offset (word length, phase, polarity and interrupt mode).
pub(crate) const REG_BUS_CTRL: u32 = 0x0;
/// Function-zero response-delay byte register offset.
pub(crate) const REG_BUS_RESPONSE_DELAY: u32 = 0x1;
/// Function-zero status/trailing-status configuration byte register offset.
pub(crate) const REG_BUS_STATUS_ENABLE: u32 = 0x2;
/// Function-zero 16-bit interrupt-status register byte offset; selected bits are write-one-to-clear.
pub(crate) const REG_BUS_INTERRUPT: u32 = 0x04; // 16 bits - Interrupt status
/// Function-zero 16-bit interrupt-enable mask register byte offset.
pub(crate) const REG_BUS_INTERRUPT_ENABLE: u32 = 0x06; // 16 bits - Interrupt mask
/// Function-zero 32-bit status register byte offset, including F2/F3 packet lengths.
pub(crate) const REG_BUS_STATUS: u32 = 0x8;
/// Function-zero read-only test register byte offset; expected FEEDBEAD after bus setup.
pub(crate) const REG_BUS_TEST_RO: u32 = 0x14;
/// Function-zero writable 32-bit test register byte offset used to verify bus framing.
pub(crate) const REG_BUS_TEST_RW: u32 = 0x18;
/// Function-zero response-delay register base byte offset; F1 has its separate byte at 0x1d.
pub(crate) const REG_BUS_RESP_DELAY: u32 = 0x1c;

// SPI_BUS_CONTROL Bits
/// SPI control bit selecting 32-bit word transfers instead of 16-bit mode.
pub(crate) const WORD_LENGTH_32: u32 = 0x1;
/// SPI control bit selecting big-endian wire word arrangement; current setup leaves it clear.
pub(crate) const ENDIAN_BIG: u32 = 0x2;
/// SPI control bit selecting the alternate sampling phase.
pub(crate) const CLOCK_PHASE: u32 = 0x4;
/// SPI control bit selecting the alternate idle clock level.
pub(crate) const CLOCK_POLARITY: u32 = 0x8;
/// SPI control bit enabling high-speed operation.
pub(crate) const HIGH_SPEED: u32 = 0x10;
/// SPI control bit selecting active-high interrupt polarity on the shared data/IRQ pin.
pub(crate) const INTERRUPT_POLARITY_HIGH: u32 = 0x20;
/// Retained SPI control wake request bit.
pub(crate) const WAKE_UP: u32 = 0x80;

// SPI_STATUS_ENABLE bits
/// Status-enable register bit requesting a trailing status word.
pub(crate) const STATUS_ENABLE: u32 = 0x01;
/// Status-enable register bit permitting interrupt signaling with trailing status.
pub(crate) const INTR_WITH_STATUS: u32 = 0x02;
/// Status-enable register bit applying programmed response delay to all functions.
pub(crate) const RESP_DELAY_ALL: u32 = 0x04;
/// Status-enable register bit selecting word-unit packet-length reporting; current setup leaves it clear.
pub(crate) const DWORD_PKT_LEN_EN: u32 = 0x08;
/// Status-enable register bit enabling command-error checking.
pub(crate) const CMD_ERR_CHK_EN: u32 = 0x20;
/// Status-enable register bit enabling data-error checking.
pub(crate) const DATA_ERR_CHK_EN: u32 = 0x40;

// SPI_STATUS_REGISTER bits
/// Function-zero byte offset used by runner status polling; aliases REG_BUS_STATUS.
pub(crate) const SPI_STATUS_REGISTER: u32 = 0x00000008;
/// Initial SDIO packet prefix length in bytes, containing length and its complement.
pub(crate) const INITIAL_READ: u32 = 0x04;

/// Trailing SPI status bit indicating requested data was unavailable.
pub(crate) const STATUS_DATA_NOT_AVAILABLE: u32 = 0x00000001;
/// Trailing SPI status bit reporting FIFO underflow.
pub(crate) const STATUS_UNDERFLOW: u32 = 0x00000002;
/// Trailing SPI status bit reporting FIFO overflow.
pub(crate) const STATUS_OVERFLOW: u32 = 0x00000004;
/// Trailing SPI status bit reflecting a WLAN/function-two interrupt.
pub(crate) const STATUS_F2_INTR: u32 = 0x00000008;
/// Trailing SPI status bit reflecting a function-three interrupt.
pub(crate) const STATUS_F3_INTR: u32 = 0x00000010;
/// Trailing SPI status bit indicating function two can accept host transmit data.
pub(crate) const STATUS_F2_RX_READY: u32 = 0x00000020;
/// Trailing SPI status bit indicating function three can accept host transmit data.
pub(crate) const STATUS_F3_RX_READY: u32 = 0x00000040;
/// Trailing SPI status bit indicating host command/data error.
pub(crate) const STATUS_HOST_CMD_DATA_ERR: u32 = 0x00000080;
/// Trailing SPI status bit indicating a function-two packet is ready to read.
pub(crate) const STATUS_F2_PKT_AVAILABLE: u32 = 0x00000100;
/// Mask of function-two logical byte length in trailing SPI status, bits 9..19.
pub(crate) const STATUS_F2_PKT_LEN_MASK: u32 = 0x000FFE00;
/// Right-shift count converting masked F2 status length to bytes.
pub(crate) const STATUS_F2_PKT_LEN_SHIFT: u32 = 9;
/// Trailing SPI status bit indicating a function-three packet is ready to read.
pub(crate) const STATUS_F3_PKT_AVAILABLE: u32 = 0x00100000;
/// Mask of function-three logical byte length in trailing SPI status, bits 21..31.
pub(crate) const STATUS_F3_PKT_LEN_MASK: u32 = 0xFFE00000;
/// Right-shift count converting masked F3 status length to bytes.
pub(crate) const STATUS_F3_PKT_LEN_SHIFT: u32 = 21;

/// Function-one byte address selecting radio GPIO routing.
pub(crate) const REG_BACKPLANE_GPIO_SELECT: u32 = 0x10005;
/// Function-one byte address controlling radio GPIO output values.
pub(crate) const REG_BACKPLANE_GPIO_OUTPUT: u32 = 0x10006;
/// Function-one byte address controlling radio GPIO output enables.
pub(crate) const REG_BACKPLANE_GPIO_ENABLE: u32 = 0x10007;
/// Function-one byte address of the WLAN/function-two FIFO watermark.
pub(crate) const REG_BACKPLANE_FUNCTION2_WATERMARK: u32 = 0x10008;
/// Function-one device-control byte address, including backplane address reset selection.
pub(crate) const REG_BACKPLANE_DEVICE_CONTROL: u32 = 0x10009;
/// Function-one byte register holding backplane window address bits 8..15.
pub(crate) const REG_BACKPLANE_BACKPLANE_ADDRESS_LOW: u32 = 0x1000A;
/// Function-one byte register holding backplane window address bits 16..23.
pub(crate) const REG_BACKPLANE_BACKPLANE_ADDRESS_MID: u32 = 0x1000B;
/// Function-one byte register holding backplane window address bits 24..31.
pub(crate) const REG_BACKPLANE_BACKPLANE_ADDRESS_HIGH: u32 = 0x1000C;
/// Function-one frame-control byte register address.
pub(crate) const REG_BACKPLANE_FRAME_CONTROL: u32 = 0x1000D;
/// Function-one ALP/HT request and clock-availability byte register address.
pub(crate) const REG_BACKPLANE_CHIP_CLOCK_CSR: u32 = 0x1000E;
/// Function-one pad pull-up control byte register address; initialization clears pulls.
pub(crate) const REG_BACKPLANE_PULL_UP: u32 = 0x1000F;
/// Function-one low byte of the pending read-frame byte count.
pub(crate) const REG_BACKPLANE_READ_FRAME_BC_LOW: u32 = 0x1001B;
/// Function-one high byte of the pending read-frame byte count.
pub(crate) const REG_BACKPLANE_READ_FRAME_BC_HIGH: u32 = 0x1001C;
/// Function-one wake-policy byte register address.
pub(crate) const REG_BACKPLANE_WAKEUP_CTRL: u32 = 0x1001E;
/// Function-one sleep/status byte register address.
pub(crate) const REG_BACKPLANE_SLEEP_CSR: u32 = 0x1001F;

/// Mask of SDIO host-mailbox software interrupt bits 4..7.
pub(crate) const I_HMB_SW_MASK: u32 = 0x000000f0;
/// SDIO host-mailbox interrupt bit used by Bluetooth ring notification in this driver.
pub(crate) const I_HMB_FC_CHANGE: u32 = 1 << 5;
/// SDIO core-relative byte offset of interrupt status/clear register.
pub(crate) const SDIO_INT_STATUS: u32 = 0x20;
/// SDIO core-relative byte offset selecting interrupts presented to the host.
pub(crate) const SDIO_INT_HOST_MASK: u32 = 0x24;
/// SDIO core-relative byte offset enabling per-function interrupts.
pub(crate) const SDIO_FUNCTION_INT_MASK: u32 = 0x34;
/// SDIO core-relative byte offset of host-to-device mailbox control.
pub(crate) const SDIO_TO_SB_MAILBOX: u32 = 0x40;
/// SDIO core-relative byte offset of host-to-device mailbox data.
pub(crate) const SDIO_TO_SB_MAILBOX_DATA: u32 = 0x48;
/// SDIO core-relative byte offset of device-to-host mailbox data.
pub(crate) const SDIO_TO_HOST_MAILBOX_DATA: u32 = 0x4C;
/// Function-one sleep register address; distinct from core-relative mailbox offsets.
pub(crate) const SDIO_SLEEP_CSR: u32 = 0x1001F;
/// Sleep-control bit keeping WLAN KSO asserted during SDIO operation.
pub(crate) const SBSDIO_SLPCSR_KEEP_WL_KS: u32 = 1 << 0;

/// Host-to-device mailbox bit requesting device attention when transmit credits stall.
pub(crate) const SMB_DEV_INT: u32 = 1 << 3;
/// Host-to-device mailbox bit acknowledging a consumed host mailbox notification.
pub(crate) const SMB_INT_ACK: u32 = 1 << 1;
/// Device interrupt-status bit identifying a host mailbox notification.
pub(crate) const I_HMB_HOST_INT: u32 = 1 << 7;
/// Device-to-host mailbox data bit reporting firmware halt.
pub(crate) const I_HMB_DATA_FWHALT: u32 = 0x0010;

/// Host mailbox interrupt bits cleared by the SDIO interrupt service path.
pub(crate) const HOSTINTMASK: u32 = 0x000000F0;
/// CCCR bus-interface-control mask selecting SDIO data-bus width.
pub(crate) const BUS_SD_DATA_WIDTH_MASK: u32 = 0x03;
/// CCCR encoding selecting four SDIO data lines.
pub(crate) const BUS_SD_DATA_WIDTH_4BIT: u32 = 0x02;
/// CCCR speed-control bit enabling high-speed bus operation after support is observed.
pub(crate) const SDIO_SPEED_EHS: u32 = 0x02;
/// Vendor card-capability bit selecting the bootloader/chip-ID handshake path.
pub(crate) const SDIOD_CCCR_BRCM_CARDCAP_SECURE_MODE: u32 = 0x80;
/// Function-one device-control address used during secure-mode backplane address discovery.
pub(crate) const SBSDIO_DEVICE_CTL: u32 = 0x10009;
/// Device-control bit selecting backplane address reset/discovery state.
pub(crate) const SBSDIO_DEVCTL_ADDR_RST: u32 = 0x40;
/// Byte displacement from the discovered SDIO core address to its chip-ID register.
pub(crate) const SDIO_CORE_CHIPID_REG: u32 = 0x330;

/// Function-one window address low-byte register used during secure-mode discovery.
pub(crate) const SBSDIO_FUNC1_SBADDRLOW: u32 = 0x1000A;
/// Function-one window address middle-byte register used during secure-mode discovery.
pub(crate) const SBSDIO_FUNC1_SBADDRMID: u32 = 0x1000B;
/// Function-one window address high-byte register used during secure-mode discovery.
pub(crate) const SBSDIO_FUNC1_SBADDRHIGH: u32 = 0x1000C;

/// WLAN FIFO watermark register value selected for SPI; firmware-specific encoding, not a transfer length.
pub(crate) const SPI_F2_WATERMARK: u8 = 0x20;
/// WLAN FIFO watermark register value selected for SDIO; firmware-specific encoding, not a transfer length.
pub(crate) const SDIO_F2_WATERMARK: u8 = 0x08;

/// Window capacity in bytes; a bulk transfer is split before crossing this 32768-byte boundary.
pub(crate) const BACKPLANE_WINDOW_SIZE: usize = 0x8000;
/// Mask selecting a byte offset within the current 32768-byte backplane window.
pub(crate) const BACKPLANE_ADDRESS_MASK: u32 = 0x7FFF;
/// Bus-address flag selecting a 32-bit backplane register access; not part of the window offset.
pub(crate) const BACKPLANE_ADDRESS_32BIT_FLAG: u32 = 0x08000;
/// SPI backplane scratch chunk length and SDIO block length in bytes.
pub(crate) const BACKPLANE_MAX_TRANSFER_SIZE: usize = 64;
/// Maximum logical SDIO backplane scratch chunk length in bytes.
pub(crate) const BLOCK_BUFFER_SIZE: usize = 1024;
// Active Low Power (ALP) clock constants
/// Clock-control bit requesting Active Low Power oscillator availability.
pub(crate) const BACKPLANE_ALP_AVAIL_REQ: u8 = 0x08;
/// Clock-status bit acknowledging Active Low Power clock availability.
pub(crate) const BACKPLANE_ALP_AVAIL: u8 = 0x40;
/// Clock-control bit disabling automatic hardware clock requests during setup.
pub(crate) const BACKPLANE_FORCE_HW_CLKREQ_OFF: u8 = 0x20;
/// Clock-control bit forcing the ALP clock selection.
pub(crate) const BACKPLANE_FORCE_ALP: u8 = 0x01;
/// Clock-control bit forcing the high-throughput clock selection.
pub(crate) const BACKPLANE_FORCE_HT: u32 = 0x02;

// Broadcom AMBA (Advanced Microcontroller Bus Architecture) Interconnect
// (AI) pub (crate) constants
/// Core-wrapper-relative byte offset of Broadcom interconnect clock/control register.
pub(crate) const AI_IOCTRL_OFFSET: u32 = 0x408;
/// Wrapper control bit forcing gated clocks during reset sequencing.
pub(crate) const AI_IOCTRL_BIT_FGC: u8 = 0x0002;
/// Wrapper control bit enabling the selected core clock.
pub(crate) const AI_IOCTRL_BIT_CLOCK_EN: u8 = 0x0001;
/// Retained wrapper control bit holding the processor halted.
pub(crate) const AI_IOCTRL_BIT_CPUHALT: u8 = 0x0020;

/// Core-wrapper-relative byte offset of reset control.
pub(crate) const AI_RESETCTRL_OFFSET: u32 = 0x800;
/// Reset-control bit asserting core reset.
pub(crate) const AI_RESETCTRL_BIT_RESET: u8 = 1;

/// Core-wrapper-relative byte offset of reset status; retained for diagnostics.
pub(crate) const AI_RESETSTATUS_OFFSET: u32 = 0x804;

/// Writable bus-test word; mixed byte values reveal transfer byte/word ordering errors.
pub(crate) const TEST_PATTERN: u32 = 0x12345678;
/// Expected read-only test word after bus initialization.
pub(crate) const FEEDBEAD: u32 = 0xFEEDBEAD;

// SPI_INTERRUPT_REGISTER and SPI_INTERRUPT_ENABLE_REGISTER Bits
/// SPI interrupt bit for unavailable requested data; cleared by writing one.
pub(crate) const IRQ_DATA_UNAVAILABLE: u16 = 0x0001; // Requested data not available; Clear by writing a "1"
/// SPI interrupt bit reporting read underflow in function-two/three FIFOs.
pub(crate) const IRQ_F2_F3_FIFO_RD_UNDERFLOW: u16 = 0x0002;
/// SPI interrupt bit reporting write overflow in function-two/three FIFOs.
pub(crate) const IRQ_F2_F3_FIFO_WR_OVERFLOW: u16 = 0x0004;
/// SPI command-error interrupt bit; write one to clear.
pub(crate) const IRQ_COMMAND_ERROR: u16 = 0x0008; // Cleared by writing 1
/// SPI data-error interrupt bit; write one to clear.
pub(crate) const IRQ_DATA_ERROR: u16 = 0x0010; // Cleared by writing 1
/// SPI interrupt bit indicating a WLAN packet can be read.
pub(crate) const IRQ_F2_PACKET_AVAILABLE: u16 = 0x0020;
/// SPI interrupt bit indicating a function-three packet can be read.
pub(crate) const IRQ_F3_PACKET_AVAILABLE: u16 = 0x0040;
/// SPI backplane-overflow interrupt bit: the last write encountered pending backplane requests.
pub(crate) const IRQ_F1_OVERFLOW: u16 = 0x0080; // Due to last write. Bkplane has pending write requests
/// SPI miscellaneous interrupt source zero bit; formatter preserves the source number.
pub(crate) const IRQ_MISC_INTR0: u16 = 0x0100;
/// SPI miscellaneous interrupt source one bit.
pub(crate) const IRQ_MISC_INTR1: u16 = 0x0200;
/// SPI miscellaneous interrupt source two bit.
pub(crate) const IRQ_MISC_INTR2: u16 = 0x0400;
/// SPI miscellaneous interrupt source three bit.
pub(crate) const IRQ_MISC_INTR3: u16 = 0x0800;
/// SPI miscellaneous interrupt source four bit.
pub(crate) const IRQ_MISC_INTR4: u16 = 0x1000;
/// SPI function-one/backplane interrupt bit.
pub(crate) const IRQ_F1_INTR: u16 = 0x2000;
/// SPI function-two/WLAN interrupt bit.
pub(crate) const IRQ_F2_INTR: u16 = 0x4000;
/// SPI function-three interrupt bit.
pub(crate) const IRQ_F3_INTR: u16 = 0x8000;

/// Low-nibble SDPCM channel value for IOCTL request/response frames.
pub(crate) const CHANNEL_TYPE_CONTROL: u8 = 0;
/// Low-nibble SDPCM channel value for firmware event envelopes.
pub(crate) const CHANNEL_TYPE_EVENT: u8 = 1;
/// Low-nibble SDPCM channel value for Ethernet data frames.
pub(crate) const CHANNEL_TYPE_DATA: u8 = 2;

// CYW_SPID command structure constants.
/// SPI command direction selecting host-to-chip transfer.
pub(crate) const WRITE: bool = true;
/// SPI command direction selecting chip-to-host transfer.
pub(crate) const READ: bool = false;
/// SPI command address mode incrementing the register address across the transfer.
pub(crate) const INC_ADDR: bool = true;
/// SPI command address mode retaining one address, used for FIFO semantics where selected.
pub(crate) const FIXED_ADDR: bool = false;

/// Low security bit selecting AES encryption in combined AP security encoding.
pub(crate) const AES_ENABLED: u32 = 0x0004;
/// High security bit identifying WPA2 in the combined AP security encoding.
pub(crate) const WPA2_SECURITY: u32 = 0x00400000;

/// Minimum protected-AP text passphrase length in bytes enforced by Control.
pub(crate) const MIN_PSK_LEN: usize = 8;
/// Maximum protected-AP passphrase storage length in bytes enforced by Control.
pub(crate) const MAX_PSK_LEN: usize = 64;

// Bluetooth firmware extraction constants.
/// Bluetooth patch record containing bytes to write at the current address base plus offset.
pub(crate) const BTFW_HEX_LINE_TYPE_DATA: u8 = 0;
/// Bluetooth patch record terminating the image; parser requires a valid terminal record.
pub(crate) const BTFW_HEX_LINE_TYPE_END_OF_DATA: u8 = 1;
/// Bluetooth patch record selecting a segment base shifted left by four bits.
pub(crate) const BTFW_HEX_LINE_TYPE_EXTENDED_SEGMENT_ADDRESS: u8 = 2;
/// Bluetooth patch record selecting an upper address component shifted left by sixteen bits.
pub(crate) const BTFW_HEX_LINE_TYPE_EXTENDED_ADDRESS: u8 = 4;
/// Bluetooth patch record setting a persistent big-endian 32-bit base, not an execution entry point.
pub(crate) const BTFW_HEX_LINE_TYPE_ABSOLUTE_32BIT_ADDRESS: u8 = 5;

// Bluetooth constants.
/// Function-zero byte register for backplane response delay.
pub(crate) const SPI_RESP_DELAY_F1: u32 = 0x001d;
/// Leading response-delay bytes clocked and discarded for a SPI backplane read.
pub(crate) const WHD_BUS_SPI_BACKPLANE_READ_PADD_SIZE: u8 = 4;

/// Value written to the Bluetooth power-up handshake register before patch upload.
pub(crate) const BT2WLAN_PWRUP_WAKE: u32 = 3;
/// Byte offset from the Bluetooth processor base to the power-up handshake register.
pub(crate) const BT2WLAN_PWRUP_ADDR: u32 = 0x640894;

/// Absolute backplane register reporting Bluetooth firmware-ready and awake state.
pub(crate) const BT_CTRL_REG_ADDR: u32 = 0x18000c7c;
/// Absolute backplane host-control register for wake, software-ready and data-valid notification bits.
pub(crate) const HOST_CTRL_REG_ADDR: u32 = 0x18000d6c;
/// Absolute backplane register publishing the base of the shared Bluetooth rings and cursors.
pub(crate) const WLAN_RAM_BASE_REG_ADDR: u32 = 0x18000d68;

/// Host-control bit toggled to notify Bluetooth of host ring progress.
pub(crate) const BTSDIO_REG_DATA_VALID_BITMASK: u32 = 1 << 1;
/// Bluetooth-control bit acknowledging that firmware is awake.
pub(crate) const BTSDIO_REG_BT_AWAKE_BITMASK: u32 = 1 << 8;
/// Host-control bit requesting Bluetooth wake.
pub(crate) const BTSDIO_REG_WAKE_BT_BITMASK: u32 = 1 << 17;
/// Host-control bit announcing host software/ring readiness.
pub(crate) const BTSDIO_REG_SW_RDY_BITMASK: u32 = 1 << 24;
/// Bluetooth-control bit announcing firmware readiness.
pub(crate) const BTSDIO_REG_FW_RDY_BITMASK: u32 = 1 << 24;

/// Capacity of each Bluetooth shared-memory ring in bytes; cursor offsets wrap modulo 4096.
pub(crate) const BTSDIO_FWBUF_SIZE: u32 = 0x1000;
/// Shared-base byte offset of host-to-controller ring storage.
pub(crate) const BTSDIO_OFFSET_HOST_WRITE_BUF: u32 = 0;
/// Shared-base byte offset of controller-to-host ring storage.
pub(crate) const BTSDIO_OFFSET_HOST_READ_BUF: u32 = BTSDIO_FWBUF_SIZE;

/// Shared-base byte offset of the host-owned TX producer cursor register.
pub(crate) const BTSDIO_OFFSET_HOST2BT_IN: u32 = 0x00002000;
/// Shared-base byte offset of the controller-owned TX consumer cursor register.
pub(crate) const BTSDIO_OFFSET_HOST2BT_OUT: u32 = 0x00002004;
/// Shared-base byte offset of the controller-owned RX producer cursor register.
pub(crate) const BTSDIO_OFFSET_BT2HOST_IN: u32 = 0x00002008;
/// Shared-base byte offset of the host-owned RX consumer cursor register.
pub(crate) const BTSDIO_OFFSET_BT2HOST_OUT: u32 = 0x0000200C;

/// Function-zero CCCR byte offset enabling SDIO functions.
pub(crate) const SDIOD_CCCR_IOEN: u32 = 0x02;
/// Function-zero CCCR byte offset reporting ready SDIO functions.
pub(crate) const SDIOD_CCCR_IORDY: u32 = 0x03;
/// Function-zero CCCR byte offset enabling master/per-function interrupts.
pub(crate) const SDIOD_CCCR_INTEN: u32 = 0x04;
/// Function-zero CCCR byte offset selecting data width and bus-interface controls.
pub(crate) const SDIOD_CCCR_BICTRL: u32 = 0x07;
/// Function-zero block-size low-byte register offset.
pub(crate) const SDIOD_CCCR_BLKSIZE_0: u32 = 0x10;
/// Function-zero speed capability/enable register offset.
pub(crate) const SDIOD_CCCR_SPEED_CONTROL: u32 = 0x13;
/// Function-zero vendor card-capability register offset.
pub(crate) const SDIOD_CCCR_BRCM_CARDCAP: u32 = 0xf0;
/// Function-zero vendor out-of-band interrupt control register offset.
pub(crate) const SDIOD_SEP_INT_CTL: u32 = 0xf2;
/// Function-zero address of function-one block-size low byte.
pub(crate) const SDIOD_CCCR_F1BLKSIZE_0: u32 = 0x110;
/// Function-zero address of function-two block-size low byte.
pub(crate) const SDIOD_CCCR_F2BLKSIZE_0: u32 = 0x210;
/// Function-zero address of function-two block-size high byte.
pub(crate) const SDIOD_CCCR_F2BLKSIZE_1: u32 = 0x211;
/// CCCR interrupt-enable bit admitting all enabled per-function sources.
pub(crate) const INTR_CTL_MASTER_EN: u32 = 0x01;
/// CCCR interrupt-enable bit for function one/backplane.
pub(crate) const INTR_CTL_FUNC1_EN: u32 = 0x02;
/// CCCR interrupt-enable bit for function two/WLAN.
pub(crate) const INTR_CTL_FUNC2_EN: u32 = 0x04;
/// CCCR I/O-enable bit enabling function one/backplane.
pub(crate) const SDIO_FUNC_ENABLE_1: u32 = 0x02;
/// CCCR I/O-enable bit enabling function two/WLAN.
pub(crate) const SDIO_FUNC_ENABLE_2: u32 = 0x04;
/// CCCR I/O-ready bit acknowledging function one/backplane readiness.
pub(crate) const SDIO_FUNC_READY_1: u32 = 0x02;
/// CCCR I/O-ready bit acknowledging function two/WLAN readiness.
pub(crate) const SDIO_FUNC_READY_2: u32 = 0x04;
/// Configured SDIO block length in bytes, matching the bus trait's array element size.
pub(crate) const SDIO_64B_BLOCK: u32 = 64;
/// Function-one clock-control/status address alias retained for SDIO setup.
pub(crate) const SDIO_CHIP_CLOCK_CSR: u32 = 0x1000e;
/// Function-one pull-up control address alias retained for SDIO setup.
pub(crate) const SDIO_PULL_UP: u32 = 0x1000f;

// SDIOD_SEP_INT_CTL bits
/// Vendor out-of-band interrupt mask bit.
pub(crate) const SEP_INTR_CTL_MASK: u32 = 0x01; // out-of-band interrupt mask
/// Vendor out-of-band interrupt output-enable bit.
pub(crate) const SEP_INTR_CTL_EN: u32 = 0x02; // out-of-band interrupt output enable
/// Vendor out-of-band interrupt polarity-selection bit.
pub(crate) const SEP_INTR_CTL_POL: u32 = 0x04; // out-of-band interrupt polarity

/// Absolute backplane base of chip-common registers; SDIO helpers restore this window after accesses.
pub(crate) const CHIPCOMMON_BASE_ADDRESS: u32 = 0x18000000;
/// Absolute backplane base of the CYW43439 SDIO controller registers.
pub(crate) const SDIO_BASE_ADDRESS: u32 = 0x18002000;

// Security type (authentication and encryption types are combined using bit mask)
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq)]
#[repr(u32)]
/// Combined firmware authentication/cipher selection used by AP setup.
pub(crate) enum Security {
    /// No authentication or link encryption requested.
    OPEN = 0,
    /// WPA2 family marker plus AES cipher bit; AP setup derives its own WPA authentication mask.
    WPA2_AES_PSK = WPA2_SECURITY | AES_ENABLED,
}

crate::util::enum_from_u8! {
    #[allow(non_camel_case_types)]
    #[derive(Copy, Clone, PartialEq)]
    /// Legacy event result byte; the event envelope actually carries u32 status.
    /// Interpret in event context: PSK_SUP value 6 means keyed, unlike UNSOLICITED for association.
    enum EStatus {
        #[default]
        /// Unrecognized byte, also used by join failure reporting for values wider than u8.
        Unknown = 0xFF,
        /// operation was successful
        SUCCESS = 0,
        /// operation failed
        FAIL = 1,
        /// operation timed out
        TIMEOUT = 2,
        /// failed due to no matching network found
        NO_NETWORKS = 3,
        /// operation was aborted
        ABORT = 4,
        /// protocol failure: packet not ack'd
        NO_ACK = 5,
        /// AUTH or ASSOC packet was unsolicited
        UNSOLICITED = 6,
        /// attempt to assoc to an auto auth configuration
        ATTEMPT = 7,
        /// scan results are incomplete
        PARTIAL = 8,
        /// scan aborted by another scan
        NEWSCAN = 9,
        /// scan aborted due to assoc in progress
        NEWASSOC = 10,
        /// 802.11h quiet period started
        _11HQUIET = 11,
        /// user disabled scanning (WLC_SET_SCANSUPPRESS)
        SUPPRESS = 12,
        /// no allowable channels to scan
        NOCHANS = 13,
        /// scan aborted due to CCX fast roam
        CCXFASTRM = 14,
        /// abort channel select
        CS_ABORT = 15,
    }
}

impl PartialEq<EStatus> for u32 {
    /// Returns whether the full u32 status equals `other`; high bits are never discarded.
    fn eq(&self, other: &EStatus) -> bool {
        *self == *other as Self
    }
}

#[allow(dead_code)]
/// Selected SPI status flag names; packet lengths and unknown bits are omitted.
pub(crate) struct FormatStatus(
    /// Raw 32-bit trailing SPI status word.
    pub u32,
);

#[cfg(feature = "defmt")]
impl defmt::Format for FormatStatus {
    /// Writes recognized set-bit names to `fmt`, each prefixed by a separator.
    fn format(&self, fmt: defmt::Formatter) {
        /// Expands named bit masks into conditional output, stripping their common register prefix.
        macro_rules! implm {
            ($($name:ident),*) => {
                $(
                    if self.0 & $name > 0 {
                        defmt::write!(fmt, " | {}", &stringify!($name)[7..]);
                    }
                )*
            };
        }

        implm!(
            STATUS_DATA_NOT_AVAILABLE,
            STATUS_UNDERFLOW,
            STATUS_OVERFLOW,
            STATUS_F2_INTR,
            STATUS_F3_INTR,
            STATUS_F2_RX_READY,
            STATUS_F3_RX_READY,
            STATUS_HOST_CMD_DATA_ERR,
            STATUS_F2_PKT_AVAILABLE,
            STATUS_F3_PKT_AVAILABLE
        );
    }
}

#[cfg(feature = "log")]
impl core::fmt::Debug for FormatStatus {
    /// Writes recognized set-bit names to `fmt`; returns the first formatting error or success.
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        /// Expands named bit masks into conditional output, stripping their common register prefix.
        macro_rules! implm {
            ($($name:ident),*) => {
                $(
                    if self.0 & $name > 0 {
                        core::write!(fmt, " | {}", &stringify!($name)[7..])?;
                    }
                )*
            };
        }

        implm!(
            STATUS_DATA_NOT_AVAILABLE,
            STATUS_UNDERFLOW,
            STATUS_OVERFLOW,
            STATUS_F2_INTR,
            STATUS_F3_INTR,
            STATUS_F2_RX_READY,
            STATUS_F3_RX_READY,
            STATUS_HOST_CMD_DATA_ERR,
            STATUS_F2_PKT_AVAILABLE,
            STATUS_F3_PKT_AVAILABLE
        );
        Ok(())
    }
}

#[cfg(feature = "log")]
impl core::fmt::Display for FormatStatus {
    /// Delegates flag rendering to Debug through `f`; returns its formatting result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

#[allow(dead_code)]
/// SPI IRQ flag names in ascending bit order; zero formats as an empty string.
pub(crate) struct FormatInterrupt(
    /// Raw 16-bit function-zero interrupt register.
    pub u16,
);

#[cfg(feature = "defmt")]
impl defmt::Format for FormatInterrupt {
    /// Writes recognized set-bit names to `fmt`, each prefixed by a separator.
    fn format(&self, fmt: defmt::Formatter) {
        /// Expands named bit masks into conditional output, stripping their common register prefix.
        macro_rules! implm {
            ($($name:ident),*) => {
                $(
                    if self.0 & $name > 0 {
                        defmt::write!(fmt, " | {}", &stringify!($name)[4..]);
                    }
                )*
            };
        }

        implm!(
            IRQ_DATA_UNAVAILABLE,
            IRQ_F2_F3_FIFO_RD_UNDERFLOW,
            IRQ_F2_F3_FIFO_WR_OVERFLOW,
            IRQ_COMMAND_ERROR,
            IRQ_DATA_ERROR,
            IRQ_F2_PACKET_AVAILABLE,
            IRQ_F3_PACKET_AVAILABLE,
            IRQ_F1_OVERFLOW,
            IRQ_MISC_INTR0,
            IRQ_MISC_INTR1,
            IRQ_MISC_INTR2,
            IRQ_MISC_INTR3,
            IRQ_MISC_INTR4,
            IRQ_F1_INTR,
            IRQ_F2_INTR,
            IRQ_F3_INTR
        );
    }
}

#[cfg(feature = "log")]
impl core::fmt::Debug for FormatInterrupt {
    /// Writes recognized set-bit names to `fmt`; returns the first formatting error or success.
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        /// Expands named bit masks into conditional output, stripping their common register prefix.
        macro_rules! implm {
            ($($name:ident),*) => {
                $(
                    if self.0 & $name > 0 {
                        core::write!(fmt, " | {}", &stringify!($name)[4..])?;
                    }
                )*
            };
        }

        implm!(
            IRQ_DATA_UNAVAILABLE,
            IRQ_F2_F3_FIFO_RD_UNDERFLOW,
            IRQ_F2_F3_FIFO_WR_OVERFLOW,
            IRQ_COMMAND_ERROR,
            IRQ_DATA_ERROR,
            IRQ_F2_PACKET_AVAILABLE,
            IRQ_F3_PACKET_AVAILABLE,
            IRQ_F1_OVERFLOW,
            IRQ_MISC_INTR0,
            IRQ_MISC_INTR1,
            IRQ_MISC_INTR2,
            IRQ_MISC_INTR3,
            IRQ_MISC_INTR4,
            IRQ_F1_INTR,
            IRQ_F2_INTR,
            IRQ_F3_INTR
        );
        Ok(())
    }
}

#[cfg(feature = "log")]
impl core::fmt::Display for FormatInterrupt {
    /// Delegates flag rendering to Debug through `f`; returns its formatting result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u32)]
/// Firmware WLC command numbers transported in the CDC header, independent of get/set flags.
/// Most entries retain the upstream catalogue only: this driver supplies no payload codec or
/// capability admission for them. Firmware support and response layout must be checked before use.
/// Active control methods encode little-endian scalar or explicit wire-structure payloads.
pub(crate) enum Ioctl {
    /// Queries driver identification magic.
    GetMagic = 0,
    /// Queries firmware IOCTL interface version.
    GetVersion = 1,
    /// Starts the firmware Wi-Fi interface; this driver sends an empty payload.
    Up = 2,
    /// Stops the firmware Wi-Fi interface; this driver sends an empty payload.
    Down = 3,
    /// Queries loopback test mode.
    GetLoop = 4,
    /// Configures loopback test mode.
    SetLoop = 5,
    /// Requests a firmware diagnostic dump; no local response decoder.
    Dump = 6,
    /// Queries firmware diagnostic message mask.
    GetMsglevel = 7,
    /// Configures firmware diagnostic message mask.
    SetMsglevel = 8,
    /// Queries promiscuous receive mode.
    GetPromisc = 9,
    /// Configures promiscuous receive mode.
    SetPromisc = 10,
    /// Queries current transmit rate.
    GetRate = 12,
    /// Queries driver instance index.
    GetInstance = 14,
    /// Queries infrastructure versus independent-BSS mode.
    GetInfra = 19,
    /// Selects independent-BSS (0) or infrastructure (1); join uses 1.
    SetInfra = 20,
    /// Queries 802.11 authentication algorithm.
    GetAuth = 21,
    /// Sets the authentication algorithm as little-endian u32; join selects open-system or SAE.
    SetAuth = 22,
    /// Queries associated BSS MAC address.
    GetBssid = 23,
    /// Configures associated BSS MAC address.
    SetBssid = 24,
    /// Queries network name and length.
    GetSsid = 25,
    /// Starts association using SsidInfo length/name bytes; completion arrives as separate events.
    SetSsid = 26,
    /// Requests the legacy driver restart operation.
    Restart = 27,
    /// Queries radio channel selection.
    GetChannel = 29,
    /// Sets AP channel from the u8 public argument widened to little-endian u32.
    SetChannel = 30,
    /// Queries short-frame retry limit.
    GetSrl = 31,
    /// Configures short-frame retry limit.
    SetSrl = 32,
    /// Queries long-frame retry limit.
    GetLrl = 33,
    /// Configures long-frame retry limit.
    SetLrl = 34,
    /// Queries physical-layer convergence preamble selection.
    GetPlcphdr = 35,
    /// Configures physical-layer convergence preamble selection.
    SetPlcphdr = 36,
    /// Queries radio disable/enable control bits.
    GetRadio = 37,
    /// Configures radio disable/enable control bits.
    SetRadio = 38,
    /// Queries physical-layer implementation identifier.
    GetPhytype = 39,
    /// Requests transmit-rate diagnostic state.
    DumpRate = 40,
    /// Configures legacy transmit-rate selection parameters.
    SetRateParams = 41,
    /// Queries indexed encryption-key material.
    GetKey = 44,
    /// Configures indexed encryption-key material.
    SetKey = 45,
    /// Queries regulatory enforcement mode.
    GetRegulatory = 46,
    /// Configures regulatory enforcement mode.
    SetRegulatory = 47,
    /// Queries passive versus active scan policy.
    GetPassiveScan = 48,
    /// Configures passive versus active scan policy.
    SetPassiveScan = 49,
    /// Starts the legacy scan command; this driver uses the escan iovar instead.
    Scan = 50,
    /// Requests results of the legacy scan command.
    ScanResults = 51,
    /// Disassociates the current network; leave() sends an empty payload.
    Disassoc = 52,
    /// Requests reassociation with a supplied BSS.
    Reassoc = 53,
    /// Queries RSSI threshold initiating roaming.
    GetRoamTrigger = 54,
    /// Configures RSSI threshold initiating roaming.
    SetRoamTrigger = 55,
    /// Queries minimum RSSI improvement for roaming.
    GetRoamDelta = 56,
    /// Configures minimum RSSI improvement for roaming.
    SetRoamDelta = 57,
    /// Queries interval between roaming scans.
    GetRoamScanPeriod = 58,
    /// Configures interval between roaming scans.
    SetRoamScanPeriod = 59,
    /// Selects the radio error-vector-magnitude test operation.
    Evm = 60,
    /// Queries transmit antenna selection.
    GetTxant = 61,
    /// Configures transmit antenna selection.
    SetTxant = 62,
    /// Queries receive antenna diversity selection.
    GetAntdiv = 63,
    /// Sets receive antenna selection as little-endian u32; initialization selects antenna 0.
    SetAntdiv = 64,
    /// Queries hidden-SSID access-point policy.
    GetClosed = 67,
    /// Configures hidden-SSID access-point policy.
    SetClosed = 68,
    /// Queries MAC-address filter list.
    GetMaclist = 69,
    /// Configures MAC-address filter list.
    SetMaclist = 70,
    /// Queries supported rates and basic-rate flags.
    GetRateset = 71,
    /// Configures supported rates and basic-rate flags.
    SetRateset = 72,
    /// Selects the radio long-training test operation.
    Longtrain = 74,
    /// Queries beacon interval.
    GetBcnprd = 75,
    /// Configures beacon interval.
    SetBcnprd = 76,
    /// Queries delivery-traffic-indication beacon period.
    GetDtimprd = 77,
    /// Configures delivery-traffic-indication beacon period.
    SetDtimprd = 78,
    /// Queries serial-ROM configuration contents.
    GetSrom = 79,
    /// Configures serial-ROM configuration contents.
    SetSrom = 80,
    /// Queries WEP-only receive restriction.
    GetWepRestrict = 81,
    /// Configures WEP-only receive restriction.
    SetWepRestrict = 82,
    /// Queries regulatory country selection.
    GetCountry = 83,
    /// Configures regulatory country selection.
    SetCountry = 84,
    /// Queries firmware power-management mode.
    GetPm = 85,
    /// Sets the firmware power mode as little-endian u32; timing is configured with separate iovars.
    SetPm = 86,
    /// Queries wake control policy.
    GetWake = 87,
    /// Configures wake control policy.
    SetWake = 88,
    /// Queries forced-link test state.
    GetForcelink = 90,
    /// Configures forced-link test state.
    SetForcelink = 91,
    /// Selects the radio frequency-accuracy test operation.
    FreqAccuracy = 92,
    /// Selects the radio carrier-suppression test operation.
    CarrierSuppress = 93,
    /// Queries indexed PHY register contents.
    GetPhyreg = 94,
    /// Configures indexed PHY register contents.
    SetPhyreg = 95,
    /// Queries indexed radio register contents.
    GetRadioreg = 96,
    /// Configures indexed radio register contents.
    SetRadioreg = 97,
    /// Queries chip, board and firmware revision information.
    GetRevinfo = 98,
    /// Queries microcode antenna-diversity control.
    GetUcantdiv = 99,
    /// Configures microcode antenna-diversity control.
    SetUcantdiv = 100,
    /// Reads a directly addressed chip register through firmware.
    RReg = 101,
    /// Writes a directly addressed chip register through firmware.
    WReg = 102,
    /// Queries MAC-address filter policy.
    GetMacmode = 105,
    /// Configures MAC-address filter policy.
    SetMacmode = 106,
    /// Queries monitor receive mode.
    GetMonitor = 107,
    /// Configures monitor receive mode.
    SetMonitor = 108,
    /// Queries 802.11g coexistence/operation mode.
    GetGmode = 109,
    /// Sets 802.11g operation as little-endian u32; initialization selects auto (1).
    SetGmode = 110,
    /// Queries legacy extended-rate PHY policy.
    GetLegacyErp = 111,
    /// Configures legacy extended-rate PHY policy.
    SetLegacyErp = 112,
    /// Queries current receive antenna.
    GetRxAnt = 113,
    /// Queries currently negotiated rates.
    GetCurrRateset = 114,
    /// Queries scan suppression flag.
    GetScansuppress = 115,
    /// Configures scan suppression flag.
    SetScansuppress = 116,
    /// Queries access-point role enable.
    GetAp = 117,
    /// Sets AP role as little-endian u32: 1 during AP setup, 0 when stopping.
    SetAp = 118,
    /// Queries 802.1X authorization restriction.
    GetEapRestrict = 119,
    /// Configures 802.1X authorization restriction.
    SetEapRestrict = 120,
    /// Authorizes a station control block for data traffic.
    ScbAuthorize = 121,
    /// Revokes station data-traffic authorization.
    ScbDeauthorize = 122,
    /// Queries wireless-distribution peer list.
    GetWdslist = 123,
    /// Configures wireless-distribution peer list.
    SetWdslist = 124,
    /// Queries announcement-traffic-indication window.
    GetAtim = 125,
    /// Configures announcement-traffic-indication window.
    SetAtim = 126,
    /// Requests four RSSI bytes; Control::get_rssi decodes native-endian i32 on the supported little-endian MCU.
    GetRssi = 127,
    /// Queries PHY antenna-diversity control.
    GetPhyantdiv = 128,
    /// Configures PHY antenna-diversity control.
    SetPhyantdiv = 129,
    /// Selects the legacy receive-only access-point mode.
    ApRxOnly = 130,
    /// Queries transmit-path power control.
    GetTxPathPwr = 131,
    /// Configures transmit-path power control.
    SetTxPathPwr = 132,
    /// Queries link-encryption cipher mask.
    GetWsec = 133,
    /// Sets allowed encryption cipher bits as little-endian u32; zero selects no link encryption.
    SetWsec = 134,
    /// Queries PHY noise estimate.
    GetPhyNoise = 135,
    /// Queries current BSS descriptor.
    GetBssInfo = 136,
    /// Queries packet counters.
    GetPktcnts = 137,
    /// Queries automatic WDS peer handling.
    GetLazywds = 138,
    /// Configures automatic WDS peer handling.
    SetLazywds = 139,
    /// Queries supported radio bands.
    GetBandlist = 140,
    /// Queries radio band selection.
    GetBand = 141,
    /// Sets band selection as little-endian u32; initialization selects any (0).
    SetBand = 142,
    /// Deauthenticates a station identified by its control-block address.
    ScbDeauthenticate = 143,
    /// Queries current short-slot timing state.
    GetShortslot = 144,
    /// Queries short-slot timing override.
    GetShortslotOverride = 145,
    /// Configures short-slot timing override.
    SetShortslotOverride = 146,
    /// Queries short-slot association restriction.
    GetShortslotRestrict = 147,
    /// Configures short-slot association restriction.
    SetShortslotRestrict = 148,
    /// Queries current 802.11g protection state.
    GetGmodeProtection = 149,
    /// Queries 802.11g protection override.
    GetGmodeProtectionOverride = 150,
    /// Configures 802.11g protection override.
    SetGmodeProtectionOverride = 151,
    /// Transfers a firmware upgrade request; unused by this loader.
    Upgrade = 152,
    /// Queries beacon-ignore policy.
    GetIgnoreBcns = 155,
    /// Configures beacon-ignore policy.
    SetIgnoreBcns = 156,
    /// Queries station-control-block inactivity timeout.
    GetScbTimeout = 157,
    /// Configures station-control-block inactivity timeout.
    SetScbTimeout = 158,
    /// Queries associated station MAC-address list.
    GetAssoclist = 159,
    /// Queries core clock enable state.
    GetClk = 160,
    /// Configures core clock enable state.
    SetClk = 161,
    /// Queries driver operational state.
    GetUp = 162,
    /// Retained WLC_OUT selector; this driver implements no associated operation.
    Out = 163,
    /// Queries permitted WPA authentication mask.
    GetWpaAuth = 164,
    /// Sets allowed WPA authentication bits as little-endian u32; zero disables WPA authentication.
    SetWpaAuth = 165,
    /// Queries microcode flags.
    GetUcflags = 166,
    /// Configures microcode flags.
    SetUcflags = 167,
    /// Queries transmit power index.
    GetPwridx = 168,
    /// Configures transmit power index.
    SetPwridx = 169,
    /// Queries transmit signal-strength indication.
    GetTssi = 170,
    /// Queries supported-rate-set override.
    GetSupRatesetOverride = 171,
    /// Configures supported-rate-set override.
    SetSupRatesetOverride = 172,
    /// Queries protection-mode policy.
    GetProtectionControl = 178,
    /// Configures protection-mode policy.
    SetProtectionControl = 179,
    /// Queries available PHY implementations.
    GetPhylist = 180,
    /// Requests the legacy encryption-strength operation.
    EncryptStrength = 181,
    /// Requests legacy decryption status.
    DecryptStatus = 182,
    /// Queries encryption-key sequence counter.
    GetKeySeq = 183,
    /// Queries associated active-scan dwell time.
    GetScanChannelTime = 184,
    /// Configures associated active-scan dwell time.
    SetScanChannelTime = 185,
    /// Queries unassociated active-scan dwell time.
    GetScanUnassocTime = 186,
    /// Configures unassociated active-scan dwell time.
    SetScanUnassocTime = 187,
    /// Queries scan home-channel dwell time.
    GetScanHomeTime = 188,
    /// Configures scan home-channel dwell time.
    SetScanHomeTime = 189,
    /// Queries probe count per scanned channel.
    GetScanNprobes = 190,
    /// Configures probe count per scanned channel.
    SetScanNprobes = 191,
    /// Queries probe-response timeout.
    GetPrbRespTimeout = 192,
    /// Configures probe-response timeout.
    SetPrbRespTimeout = 193,
    /// Queries radio attenuation settings.
    GetAtten = 194,
    /// Configures radio attenuation settings.
    SetAtten = 195,
    /// Queries indexed firmware shared-memory contents.
    GetShmem = 196,
    /// Configures indexed firmware shared-memory contents.
    SetShmem = 197,
    /// Configures firmware wireless-security test behavior.
    SetWsecTest = 200,
    /// Deauthenticates a station with an explicit reason code.
    ScbDeauthenticateForReason = 201,
    /// Controls TKIP countermeasures after integrity failures.
    TkipCountermeasures = 202,
    /// Queries programmed-I/O transfer selection.
    GetPiomode = 203,
    /// Configures programmed-I/O transfer selection.
    SetPiomode = 204,
    /// Configures association preference.
    SetAssocPrefer = 205,
    /// Queries association preference.
    GetAssocPrefer = 206,
    /// Configures roaming preference.
    SetRoamPrefer = 207,
    /// Queries roaming preference.
    GetRoamPrefer = 208,
    /// Configures firmware-controlled LED configuration.
    SetLed = 209,
    /// Queries firmware-controlled LED configuration.
    GetLed = 210,
    /// Queries interference-mitigation mode.
    GetInterferenceMode = 211,
    /// Configures interference-mitigation mode.
    SetInterferenceMode = 212,
    /// Queries channel quality assessment result.
    GetChannelQa = 213,
    /// Starts a channel quality assessment.
    StartChannelQa = 214,
    /// Queries channel-selection result.
    GetChannelSel = 215,
    /// Starts firmware channel selection.
    StartChannelSel = 216,
    /// Queries regulatorily permitted channel list.
    GetValidChannels = 217,
    /// Queries frame-burst policy.
    GetFakefrag = 218,
    /// Configures frame-burst policy.
    SetFakefrag = 219,
    /// Queries transmit power as a percentage of maximum.
    GetPwroutPercentage = 220,
    /// Configures transmit power as a percentage of maximum.
    SetPwroutPercentage = 221,
    /// Configures bad-frame receive preemption policy.
    SetBadFramePreempt = 222,
    /// Queries bad-frame receive preemption policy.
    GetBadFramePreempt = 223,
    /// Configures LEAP authentication network list.
    SetLeapList = 224,
    /// Queries LEAP authentication network list.
    GetLeapList = 225,
    /// Queries minimum contention window.
    GetCwmin = 226,
    /// Configures minimum contention window.
    SetCwmin = 227,
    /// Queries maximum contention window.
    GetCwmax = 228,
    /// Configures maximum contention window.
    SetCwmax = 229,
    /// Queries wireless Ethernet bridge mode.
    GetWet = 230,
    /// Configures wireless Ethernet bridge mode.
    SetWet = 231,
    /// Queries legacy public driver-state block.
    GetPub = 232,
    /// Queries primary transmit encryption-key index.
    GetKeyPrimary = 235,
    /// Configures primary transmit encryption-key index.
    SetKeyPrimary = 236,
    /// Queries adjacent-channel interference parameters.
    GetAciArgs = 238,
    /// Configures adjacent-channel interference parameters.
    SetAciArgs = 239,
    /// Removes a legacy firmware callback registration.
    UnsetCallback = 240,
    /// Installs a legacy firmware callback registration.
    SetCallback = 241,
    /// Queries radar-detection mode.
    GetRadar = 242,
    /// Configures radar-detection mode.
    SetRadar = 243,
    /// Configures 802.11h spectrum-management mode.
    SetSpectManagment = 244,
    /// Queries 802.11h spectrum-management mode.
    GetSpectManagment = 245,
    /// Requests the remote hardware address of a WDS link.
    WdsGetRemoteHwaddr = 246,
    /// Requests the WDS WPA supplicant role.
    WdsGetWpaSup = 247,
    /// Configures channel-selection scan timer.
    SetCsScanTimer = 248,
    /// Queries channel-selection scan timer.
    GetCsScanTimer = 249,
    /// Submits a spectrum-management measurement request.
    MeasureRequest = 250,
    /// Requests legacy firmware driver initialization.
    Init = 251,
    /// Sends an 802.11h quiet-period announcement.
    SendQuiet = 252,
    /// Configures the legacy firmware keepalive operation.
    Keepalive = 253,
    /// Sends an 802.11h power-constraint announcement.
    SendPwrConstraint = 254,
    /// Requests status of the legacy firmware upgrade operation.
    UpgradeStatus = 255,
    /// Requests current radio transmit-power information.
    CurrentPwr = 256,
    /// Queries passive-scan dwell time.
    GetScanPassiveTime = 257,
    /// Configures passive-scan dwell time.
    SetScanPassiveTime = 258,
    /// Selects legacy link-management behavior.
    LegacyLinkBehavior = 259,
    /// Queries channel list for a supplied country.
    GetChannelsInCountry = 260,
    /// Queries available regulatory country list.
    GetCountryList = 261,
    /// Queries a NUL-terminated named iovar followed by request bytes; response overwrites that buffer.
    GetVar = 262,
    /// Writes a NUL-terminated named iovar followed by its encoded value.
    SetVar = 263,
    /// Reads a named firmware NVRAM variable.
    NvramGet = 264,
    /// Writes a named firmware NVRAM variable.
    NvramSet = 265,
    /// Requests the firmware NVRAM contents.
    NvramDump = 266,
    /// Requests a firmware reboot; unused by the host MCU reboot path.
    Reboot = 267,
    /// Supplies PassphraseInfo bytes for firmware key derivation or a precomputed PMK.
    SetWsecPmk = 268,
    /// Queries legacy authentication-mode selector.
    GetAuthMode = 269,
    /// Configures legacy authentication-mode selector.
    SetAuthMode = 270,
    /// Queries indexed wake pattern entry.
    GetWakeentry = 271,
    /// Configures indexed wake pattern entry.
    SetWakeentry = 272,
    /// Retained WLC_NDCONFIG_ITEM selector; no local payload codec.
    NdconfigItem = 273,
    /// Requests NVRAM-formatted OTP programming; no local caller.
    Nvotpw = 274,
    /// Requests raw OTP programming; no local caller.
    Otpw = 275,
    /// Requests legacy iovar block metadata.
    IovBlockGet = 276,
    /// Requests legacy iovar module metadata.
    IovModulesGet = 277,
    /// Requests a legacy soft reset of firmware state.
    SoftReset = 278,
    /// Queries legacy allow-mode selector.
    GetAllowMode = 279,
    /// Configures legacy allow-mode selector.
    SetAllowMode = 280,
    /// Queries requested BSS MAC address.
    GetDesiredBssid = 281,
    /// Configures requested BSS MAC address.
    SetDesiredBssid = 282,
    /// Requests disassociation from the station interface's access point.
    DisassocMyap = 283,
    /// Queries number of supported bands.
    GetNbands = 284,
    /// Queries legacy per-band state block.
    GetBandstates = 285,
    /// Queries legacy internal BSS information.
    GetWlcBssInfo = 286,
    /// Queries association request/response information.
    GetAssocInfo = 287,
    /// Queries legacy OID physical-layer selector.
    GetOidPhy = 288,
    /// Configures legacy OID physical-layer selector.
    SetOidPhy = 289,
    /// Configures legacy association-time control.
    SetAssocTime = 290,
    /// Queries requested network name.
    GetDesiredSsid = 291,
    /// Queries encoded channel/bandwidth specification.
    GetChanspec = 292,
    /// Queries association state.
    GetAssocState = 293,
    /// Configures physical-layer operational state.
    SetPhyState = 294,
    /// Queries firmware scan-in-progress state.
    GetScanPending = 295,
    /// Queries queued scan-request state.
    GetScanreqPending = 296,
    /// Queries previous roaming reason code.
    GetPrevRoamReason = 297,
    /// Configures previous roaming reason code.
    SetPrevRoamReason = 298,
    /// Queries legacy per-band PHY state block.
    GetBandstatesPi = 299,
    /// Queries physical-layer operational state.
    GetPhyState = 300,
    /// Queries BSS WPA robust-security-network information.
    GetBssWpaRsn = 301,
    /// Queries BSS WPA2 robust-security-network information.
    GetBssWpa2Rsn = 302,
    /// Queries BSS beacon timestamp.
    GetBssBcnTs = 303,
    /// Queries internal disassociation state.
    GetIntDisassoc = 304,
    /// Configures legacy peer-count control.
    SetNumPeers = 305,
    /// Queries number of BSS records.
    GetNumBss = 306,
    /// Queries pairwise master-key material.
    GetWsecPmk = 318,
    /// Queries firmware-generated random bytes.
    GetRandomBytes = 319,
}

/// Firmware Wsec cipher-mask bit enabling TKIP.
pub(crate) const WSEC_TKIP: u32 = 0x02;
/// Firmware Wsec cipher-mask bit enabling AES.
pub(crate) const WSEC_AES: u32 = 0x04;

/// Firmware authentication algorithm code for open-system authentication.
pub(crate) const AUTH_OPEN: u32 = 0x00;
/// Firmware authentication algorithm code for WPA3 SAE.
pub(crate) const AUTH_SAE: u32 = 0x03;

/// Firmware management-frame-protection policy disabling MFP.
pub(crate) const MFP_NONE: u32 = 0;
/// Firmware management-frame-protection policy permitting negotiation without requiring it.
pub(crate) const MFP_CAPABLE: u32 = 1;
/// Firmware management-frame-protection policy requiring negotiated MFP.
pub(crate) const MFP_REQUIRED: u32 = 2;

/// Firmware WPA authentication mask with all modes disabled (open network path).
pub(crate) const WPA_AUTH_DISABLED: u32 = 0x0000;
/// Firmware authentication-mask bit admitting WPA pre-shared-key mode.
pub(crate) const WPA_AUTH_WPA_PSK: u32 = 0x0004;
/// Firmware authentication-mask bit admitting WPA2 pre-shared-key mode.
pub(crate) const WPA_AUTH_WPA2_PSK: u32 = 0x0080;
/// Firmware authentication-mask bit admitting WPA3 SAE pre-shared-key mode.
pub(crate) const WPA_AUTH_WPA3_SAE_PSK: u32 = 0x40000;

#[cfg(all(test, feature = "log"))]
/// Human-readable interrupt diagnostics retain the function and source identity.
mod tests {
    extern crate std;
    use super::*;

    /// IRQ_ has four prefix bytes; STATUS_'s seven-byte prefix must not be reused.
    #[test]
    fn interrupt_formatter_keeps_complete_source_names() {
        assert_eq!(
            std::format!("{}", FormatInterrupt(IRQ_F2_PACKET_AVAILABLE)),
            " | F2_PACKET_AVAILABLE"
        );
        assert_eq!(
            std::format!("{}", FormatInterrupt(IRQ_F1_INTR | IRQ_MISC_INTR0)),
            " | MISC_INTR0 | F1_INTR"
        );
        assert_eq!(std::format!("{}", FormatInterrupt(0)), "");
    }
}
