//! CYW43 firmware memory layouts and bounded packet views.
//! Integer fields are native endian (little endian on supported boards), except Ethernet/event
//! fields converted explicitly from network order. Parsing validates framing, not firmware authenticity.
//! By-value byte conversion requires every byte to belong to an initialized field:
//! see [Rust transmute validity](https://doc.rust-lang.org/core/mem/fn.transmute.html).
use crate::events::Event;

/// Adds size, byte-copy and aligned borrowed views for an integer-only layout with no implicit padding.
/// Every invocation must preserve those layout/validity properties; fields are not endian-converted.
macro_rules! impl_bytes {
    ($t:ident) => {
        impl $t {
            /// Complete fixed layout length in bytes, including explicit reserved fields.
            pub const SIZE: usize = core::mem::size_of::<Self>();

            /// Returns an owned copy of the complete native-endian layout, including reserved fields.
            #[allow(unused)]
            pub fn to_bytes(&self) -> [u8; Self::SIZE] {
                unsafe { core::mem::transmute(*self) }
            }

            /// Returns an immutable view into `bytes`; panics if the address lacks this type's alignment.
            /// Does not validate field ranges, length fields, versions or byte order.
            #[allow(unused)]
            pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> &Self {
                let alignment = core::mem::align_of::<Self>();
                assert_eq!(
                    bytes.as_ptr().align_offset(alignment),
                    0,
                    "{} is not aligned",
                    core::any::type_name::<Self>()
                );
                unsafe { core::mem::transmute(bytes) }
            }

            /// Returns an exclusive view into `bytes`; panics if the address lacks this type's alignment.
            /// Mutations change the original bytes; field ranges, versions and byte order are unchecked.
            #[allow(unused)]
            pub fn from_bytes_mut(bytes: &mut [u8; Self::SIZE]) -> &mut Self {
                let alignment = core::mem::align_of::<Self>();
                assert_eq!(
                    bytes.as_ptr().align_offset(alignment),
                    0,
                    "{} is not aligned",
                    core::any::type_name::<Self>()
                );

                unsafe { core::mem::transmute(bytes) }
            }
        }
    };
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Firmware-published shared-memory descriptor; addresses refer to chip backplane memory.
pub struct SharedMemData {
    /// Firmware descriptor flags/version bits, retained without validation.
    pub flags: u32,
    /// Backplane address of firmware trap information, unused by this driver.
    pub trap_addr: u32,
    /// Backplane address of the assertion expression string, unused here.
    pub assert_exp_addr: u32,
    /// Backplane address of the assertion source-file string, unused here.
    pub assert_file_addr: u32,
    /// Firmware assertion source line, unused here.
    pub assert_line: u32,
    /// Console descriptor address; runner reads its log structure at this address plus eight bytes.
    pub console_addr: u32,
    /// Message-trace descriptor address, unused here.
    pub msgtrace_addr: u32,
    /// Firmware build identifier, retained without interpretation.
    pub fwid: u32,
}
impl_bytes!(SharedMemData);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Firmware console ring descriptor; runner currently assumes a 1024-byte ring.
pub struct SharedMemLog {
    /// Backplane address of ring storage.
    pub buf: u32,
    /// Advertised byte capacity; runner does not currently use this field.
    pub buf_size: u32,
    /// Firmware producer byte offset; runner ignores values at or above 1024.
    pub idx: u32,
    /// Firmware output cursor, unused by the host's independent read cursor.
    pub out_idx: u32,
}
impl_bytes!(SharedMemLog);

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Twelve-byte SDPCM framing and transport-credit header.
pub struct SdpcmHeader {
    /// Logical frame length in bytes, including this header and excluding transport padding.
    pub len: u16,
    /// Bitwise complement of len; parser rejects mismatches.
    pub len_inv: u16,
    /// Rx/Tx sequence number
    pub sequence: u8,
    /// Low four bits select control/event/data channel; high four bits are flags.
    pub channel_and_flags: u8,
    /// Firmware look-ahead next-frame length encoding; transmitted as zero and unused on receive.
    pub next_length: u8,
    /// Payload byte offset from frame start; parser requires at least twelve and at most len.
    pub header_length: u8,
    /// Flow control bits, reserved for Tx
    pub wireless_flow_control: u8,
    /// Maximum Sequence number allowed by firmware for Tx
    pub bus_data_credit: u8,
    /// Reserved
    pub reserved: [u8; 2],
}
impl_bytes!(SdpcmHeader);

impl SdpcmHeader {
    /// Returns header and payload views of `packet`, or None for short, inconsistent-length or invalid-offset frames.
    /// Requires two-byte-aligned storage (misalignment panics); does not validate channel, sequence or credit.
    pub fn parse(packet: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
        let packet_len = packet.len();
        if packet_len < Self::SIZE {
            warn!("packet too short, len={}", packet.len());
            return None;
        }
        let (sdpcm_header, sdpcm_packet) = packet.split_at_mut(Self::SIZE);
        let sdpcm_header = Self::from_bytes_mut(sdpcm_header.try_into().unwrap());
        trace!("rx {:?}", sdpcm_header);

        if sdpcm_header.len != !sdpcm_header.len_inv {
            warn!("len inv mismatch");
            return None;
        }

        if sdpcm_header.len as usize != packet_len {
            warn!("len from header doesn't match len from spi");
            return None;
        }

        let offset = (sdpcm_header.header_length as usize).checked_sub(Self::SIZE)?;
        let sdpcm_packet = sdpcm_packet.get_mut(offset..)?;
        Some((sdpcm_header, sdpcm_packet))
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed(2))]
/// Sixteen-byte control request/response header, allowing two-byte-aligned SDPCM payloads.
pub struct CdcHeader {
    /// Firmware IOCTL command number.
    pub cmd: u32,
    /// Control payload byte length; parse accepts trailing packet bytes beyond this length.
    pub len: u32,
    /// Request kind in low bits and interface in bits 12..15.
    pub flags: u16,
    /// Wrapping request identity matched by the runner before completing an IOCTL.
    pub id: u16,
    /// Firmware status; zero succeeds, nonzero currently panics in the runner.
    pub status: u32,
}
impl_bytes!(CdcHeader);

#[cfg(feature = "defmt")]
impl defmt::Format for CdcHeader {
    /// Writes fields to `fmt`, copying packed integers before formatting to avoid unaligned references.
    fn format(&self, fmt: defmt::Formatter) {
        /// Returns `t` by value so formatting never borrows a packed field.
        fn copy<T: Copy>(t: T) -> T {
            t
        }

        defmt::write!(
            fmt,
            "CdcHeader{{cmd: {=u32:08x}, len: {=u32:08x}, flags: {=u16:04x}, id: {=u16:04x}, status: {=u32:08x}}}",
            copy(self.cmd),
            copy(self.len),
            copy(self.flags),
            copy(self.id),
            copy(self.status),
        )
    }
}

impl CdcHeader {
    /// Returns header and declared payload views of `packet`, or None for short/misaligned/truncated data.
    /// Does not validate command, flags, identity or status, and does not mutate bytes.
    pub fn parse(packet: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
        if packet.len() < Self::SIZE || packet.as_ptr().align_offset(core::mem::align_of::<Self>()) != 0 {
            warn!("payload too short, len={}", packet.len());
            return None;
        }

        let (cdc_header, payload) = packet.split_at_mut(Self::SIZE);
        let cdc_header = Self::from_bytes_mut(cdc_header.try_into().unwrap());

        let payload = payload.get_mut(..cdc_header.len as usize)?;
        Some((cdc_header, payload))
    }
}

/// BDC protocol revision emitted by network transmission.
pub const BDC_VERSION: u8 = 2;
/// Bit offset of the protocol revision in BDC flags.
pub const BDC_VERSION_SHIFT: u8 = 4;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Four-byte Broadcom data encapsulation header preceding Ethernet/event payloads.
pub struct BdcHeader {
    /// Protocol revision in the high nibble and firmware flags in low bits; parse does not validate them.
    pub flags: u8,
    /// 802.1d Priority (low 3 bits)
    pub priority: u8,
    /// Secondary firmware flags/interface bits; transmission currently uses zero.
    pub flags2: u8,
    /// Offset from end of BDC header to packet data, in 4-uint8_t words. Leaves room for optional headers.
    pub data_offset: u8,
}
impl_bytes!(BdcHeader);

impl BdcHeader {
    /// Returns header and payload views of `packet`, or None if header/offset exceeds available bytes.
    /// Skips data_offset four-byte words after the header; does not validate flags or revision.
    pub fn parse(packet: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
        if packet.len() < Self::SIZE || packet.as_ptr().align_offset(core::mem::align_of::<Self>()) != 0 {
            return None;
        }

        let (bdc_header, bdc_packet) = packet.split_at_mut(Self::SIZE);
        let bdc_header = Self::from_bytes_mut(bdc_header.try_into().unwrap());
        trace!("    {:?}", bdc_header);

        let packet_start = 4 * bdc_header.data_offset as usize;

        let bdc_packet = bdc_packet.get_mut(packet_start..)?;
        trace!("BDC payload: {} bytes", bdc_packet.len());

        Some((bdc_header, bdc_packet))
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Fourteen-byte Ethernet envelope; ether_type arrives in network byte order.
pub struct EthernetHeader {
    /// Six wire bytes identifying the recipient interface.
    pub destination_mac: [u8; 6],
    /// Six wire bytes identifying the sender interface.
    pub source_mac: [u8; 6],
    /// EtherType; 0x886c identifies firmware event traffic after byteswap.
    pub ether_type: u16,
}

impl EthernetHeader {
    /// Converts EtherType between network/native order; swaps on little-endian hosts and is a no-op on big endian.
    pub fn byteswap(&mut self) {
        self.ether_type = self.ether_type.to_be();
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Broadcom vendor event envelope following the Ethernet header.
pub struct EventHeader {
    /// Vendor subtype in network order before conversion; runner requires 32769.
    pub subtype: u16,
    /// Firmware-declared envelope length; retained but not used for bounds admission.
    pub length: u16,
    /// Vendor envelope revision, not validated by parse.
    pub version: u8,
    /// Three-byte organization identifier; runner requires 00:10:18.
    pub oui: [u8; 3],
    /// Network-order vendor payload subtype; runner requires event subtype one after conversion.
    pub user_subtype: u16,
}

impl EventHeader {
    /// Converts the three u16 fields between network/native order; byte fields remain unchanged.
    pub fn byteswap(&mut self) {
        self.subtype = self.subtype.to_be();
        self.length = self.length.to_be();
        self.user_subtype = self.user_subtype.to_be();
    }
}

#[derive(Debug, Clone, Copy)]
// #[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C, packed(2))]
/// Firmware event metadata; multibyte fields arrive in network order and all numeric discriminants remain raw.
pub struct EventMessage {
    /// Event-message layout revision; retained without validation.
    pub version: u16,
    /// Event-specific flags, interpreted with event/status by the runner.
    pub flags: u16,
    /// Full-width event discriminator; values beyond u8 range must not alias known events.
    pub event_type: u32,
    /// Full-width event-specific status; zero often indicates success, but meaning depends on event.
    pub status: u32,
    /// Reason code (if applicable)
    pub reason: u32,
    /// Authentication algorithm identifier for authentication events.
    pub auth_type: u32,
    /// Following event-data byte length, checked against available bytes by EventPacket::parse.
    pub datalen: u32,
    /// Station address (if applicable)
    pub addr: [u8; 6],
    /// name of the incoming packet interface
    pub ifname: [u8; 16],
    /// destination OS i/f index
    pub ifidx: u8,
    /// source bsscfg index
    pub bsscfgidx: u8,
}
impl_bytes!(EventMessage);

#[cfg(feature = "defmt")]
impl defmt::Format for EventMessage {
    /// Writes metadata to `fmt`, copying packed u32 values before formatting.
    fn format(&self, fmt: defmt::Formatter) {
        let event_type = self.event_type;
        let status = self.status;
        let reason = self.reason;
        let auth_type = self.auth_type;
        let datalen = self.datalen;

        defmt::write!(
            fmt,
            "EventMessage {{ \
            version: {=u16}, \
            flags: {=u16}, \
            event_type: {=u32}, \
            status: {=u32}, \
            reason: {=u32}, \
            auth_type: {=u32}, \
            datalen: {=u32}, \
            addr: {=[u8; 6]:x}, \
            ifname: {=[u8; 16]:x}, \
            ifidx: {=u8}, \
            bsscfgidx: {=u8}, \
        }} ",
            self.version,
            self.flags,
            event_type,
            status,
            reason,
            auth_type,
            datalen,
            self.addr,
            self.ifname,
            self.ifidx,
            self.bsscfgidx
        );
    }
}

impl EventMessage {
    /// Converts multibyte metadata fields between network/native order; address/interface bytes remain unchanged.
    pub fn byteswap(&mut self) {
        self.version = self.version.to_be();
        self.flags = self.flags.to_be();
        self.event_type = self.event_type.to_be();
        self.status = self.status.to_be();
        self.reason = self.reason.to_be();
        self.auth_type = self.auth_type.to_be();
        self.datalen = self.datalen.to_be();
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C, packed(2))]
/// Fixed Ethernet/vendor/event prefix; variable event data follows separately.
pub struct EventPacket {
    /// Ethernet routing and EtherType envelope.
    pub eth: EthernetHeader,
    /// Vendor organization/subtype envelope.
    pub hdr: EventHeader,
    /// Event discriminator, status and declared following payload length.
    pub msg: EventMessage,
}
impl_bytes!(EventPacket);

impl EventPacket {
    /// Returns converted header and declared data views of `packet`, or None for short/misaligned/truncated input.
    /// Converts fields in place before checking datalen, so a truncated-data failure still mutates the header.
    /// Calling again on already converted bytes swaps them again on little-endian hosts; outer identifiers are unchecked.
    pub fn parse(packet: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
        if packet.len() < Self::SIZE || packet.as_ptr().align_offset(core::mem::align_of::<Self>()) != 0 {
            return None;
        }

        let (event_header, event_packet) = packet.split_at_mut(Self::SIZE);
        let event_header = Self::from_bytes_mut(event_header.try_into().unwrap());
        // warn!("event_header {:x}", event_header as *const _);
        event_header.byteswap();

        let event_packet = event_packet.get_mut(..event_header.msg.datalen as usize)?;

        Some((event_header, event_packet))
    }

    /// Converts every multibyte envelope/message field between network/native order.
    pub fn byteswap(&mut self) {
        self.eth.byteswap();
        self.hdr.byteswap();
        self.msg.byteswap();
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
/// Twelve-byte firmware download chunk prefix used for CLM regulatory data.
pub struct DownloadHeader {
    /// Begin/end/version and optional no-CRC flag bits.
    pub flag: u16, //
    /// Download object discriminator; two selects CLM.
    pub dload_type: u16,
    /// Following chunk length in bytes, excluding this header.
    pub len: u32,
    /// Firmware CRC field; current CLM upload sends zero without computing a checksum.
    pub crc: u32,
}
impl_bytes!(DownloadHeader);

#[allow(unused)]
/// Download flag requesting CRC omission; retained but not set by current CLM uploader.
pub const DOWNLOAD_FLAG_NO_CRC: u16 = 0x0001;
/// Marks the first download chunk.
pub const DOWNLOAD_FLAG_BEGIN: u16 = 0x0002;
/// Marks the last download chunk (also set on the first for a single chunk).
pub const DOWNLOAD_FLAG_END: u16 = 0x0004;
/// Download handler protocol version one in the high version field.
pub const DOWNLOAD_FLAG_HANDLER_VER: u16 = 0x1000;

/// Firmware download object code for Country Locale Matrix regulatory data.
pub const DOWNLOAD_TYPE_CLM: u16 = 2;

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Firmware country selection; current init uses the world-wide XX entry.
pub struct CountryInfo {
    /// Two-byte country abbreviation followed by zero terminators in current callers.
    pub country_abbrev: [u8; 4],
    /// Regulatory revision, or -1 for firmware default.
    pub rev: i32,
    /// Regulatory locale code followed by zero terminators in current callers.
    pub country_code: [u8; 4],
}
impl_bytes!(CountryInfo);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Fixed-capacity SSID command payload; name bytes need not be UTF-8 at the firmware boundary.
pub struct SsidInfo {
    /// Meaningful SSID bytes, at most 32 for valid commands; not checked by byte views.
    pub len: u32,
    /// SSID prefix followed by zero padding in current callers.
    pub ssid: [u8; 32],
}
impl_bytes!(SsidInfo);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// WPA/WPA2 key payload; contains credentials and must not be logged indiscriminately.
pub struct PassphraseInfo {
    /// Meaningful key bytes, at most 64 for valid commands.
    pub len: u16,
    /// Bit zero denotes a text passphrase; zero denotes a prehashed key in current callers.
    pub flags: u16,
    /// Credential bytes followed by zero padding; no automatic erasure on drop.
    pub passphrase: [u8; 64],
}
impl_bytes!(PassphraseInfo);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// WPA3 SAE credential payload; no automatic erasure on drop.
pub struct SaePassphraseInfo {
    /// Meaningful credential bytes, at most 128 for valid commands.
    pub len: u16,
    /// Credential prefix followed by zero padding in current callers.
    pub passphrase: [u8; 128],
}
impl_bytes!(SaePassphraseInfo);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// SSID command qualified by a firmware BSS configuration index.
pub struct SsidInfoWithIndex {
    /// Firmware BSS index; current AP configuration uses zero.
    pub index: u32,
    /// Bounded SSID length and bytes.
    pub ssid_info: SsidInfo,
}
impl_bytes!(SsidInfoWithIndex);

#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
/// Firmware-side 192-event subscription mask, distinct from the host event queue filter.
pub struct EventMask {
    /// Interface index; current setup configures zero.
    pub iface: u32,
    /// Little bit numbering within each byte; one enables event number byte_index*8+bit_index.
    pub events: [u8; 24],
}
impl_bytes!(EventMask);

impl EventMask {
    /// Disables encoded `evt`; panics for event codes at or above 192 (including Unknown=255).
    pub fn unset(&mut self, evt: Event) {
        let evt = evt as u8 as usize;
        self.events[evt / 8] &= !(1 << (evt % 8));
    }
}

/// Fixed 76-byte extended Wi-Fi scan request; current caller requests all channels and sync identity one.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
pub struct ScanParams {
    /// Extended scan format revision; current caller sends one.
    pub version: u32,
    /// Scan action; one starts a scan.
    pub action: u16,
    /// Caller correlation identity echoed by firmware results; current caller uses one.
    pub sync_id: u16,
    /// SSID filter byte count, zero for any SSID and at most 32 for valid requests.
    pub ssid_len: u32,
    /// Optional SSID filter followed by zero bytes.
    pub ssid: [u8; 32],
    /// Optional BSSID filter; all 0xff accepts any BSSID.
    pub bssid: [u8; 6],
    /// Firmware BSS type; current caller uses two (any).
    pub bss_type: u8,
    /// Zero for active probes, one for passive listening.
    pub scan_type: u8,
    /// Probe count, or u32::MAX for firmware default.
    pub nprobes: u32,
    /// Active per-channel dwell in milliseconds, or u32::MAX for default.
    pub active_time: u32,
    /// Passive per-channel dwell in milliseconds, or u32::MAX for default.
    pub passive_time: u32,
    /// Time on the home channel between visits in milliseconds, or u32::MAX for default.
    pub home_time: u32,
    /// Channel count; current caller sends zero for all available channels.
    pub channel_num: u32,
    /// Fixed one-entry placeholder; ignored when channel_num is zero.
    pub channel_list: [u16; 1],
    /// Explicit zeroed tail bytes preserve the C layout without exposing uninitialized Rust padding.
    pub reserved: [u8; 2],
}
impl_bytes!(ScanParams);

/// Wifi Scan Results Header, followed by `bss_count` `BssInfo`
#[derive(Clone, Copy)]
// #[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C, packed(2))]
pub struct ScanResults {
    /// Firmware-declared result byte length; parser does not use this value for bounds.
    pub buflen: u32,
    /// Result format revision; not checked by parser.
    pub version: u32,
    /// Echoed request synchronization identity; current runner does not match it.
    pub sync_id: u16,
    /// Number of following BSS records; parser only requires one fixed prefix when nonzero.
    pub bss_count: u16,
}
impl_bytes!(ScanResults);

impl ScanResults {
    /// Returns result-header and remaining `packet` views, or None for short/misaligned input or a missing first BSS prefix.
    /// Does not validate version, buflen, identity, or that all bss_count records are present.
    pub fn parse(packet: &mut [u8]) -> Option<(&mut ScanResults, &mut [u8])> {
        if packet.len() < ScanResults::SIZE || packet.as_ptr().align_offset(core::mem::align_of::<Self>()) != 0 {
            return None;
        }

        let (scan_results, bssinfo) = packet.split_at_mut(ScanResults::SIZE);
        let scan_results = ScanResults::from_bytes_mut(scan_results.try_into().unwrap());

        if scan_results.bss_count > 0 && bssinfo.len() < BssInfo::SIZE {
            warn!("Scan result, incomplete BssInfo");
            return None;
        }

        Some((scan_results, bssinfo))
    }
}

/// Wifi Scan Result
#[derive(Clone, Copy)]
// #[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C, packed(2))]
#[non_exhaustive]
pub struct BssInfo {
    /// Firmware BSS layout revision; parsing does not verify compatibility.
    pub version: u32,
    /// Total firmware BSS record length in bytes; parsing only admits the fixed prefix.
    pub length: u32,
    /// BSSID.
    pub bssid: [u8; 6],
    /// Beacon period in IEEE 802.11 time units (1024 microseconds).
    pub beacon_period: u16,
    /// Capability.
    pub capability: u16,
    /// Declared SSID byte count; not validated against the 32-byte array by this parser.
    pub ssid_len: u8,
    /// SSID.
    pub ssid: [u8; 32],
    /// Explicit layout byte after SSID; no semantic interpretation.
    reserved1: [u8; 1],
    /// Number of rates in the rates field.
    pub rateset_count: u32,
    /// Encoded rates in 500 kbps units with firmware flag bits; count is not validated here.
    pub rates: [u8; 16],
    /// Channel specification.
    pub chanspec: u16,
    /// Announcement traffic indication message.
    pub atim_window: u16,
    /// Delivery traffic indication message.
    pub dtim_period: u8,
    /// Explicit alignment byte before RSSI.
    reserved2: [u8; 1],
    /// Receive signal strength (in dbM).
    pub rssi: i16,
    /// Received noise (in dbM).
    pub phy_noise: i8,
    /// 802.11n capability.
    pub n_cap: u8,
    /// Explicit alignment bytes before 802.11n capability flags.
    reserved3: [u8; 2],
    /// 802.11n BSS capabilities.
    pub nbss_cap: u32,
    /// 802.11n control channel number.
    pub ctl_ch: u8,
    /// Explicit alignment bytes after control channel.
    reserved4: [u8; 3],
    /// Reserved firmware word, retained without interpretation.
    reserved32: [u32; 1],
    /// Flags.
    pub flags: u8,
    /// VHT capability.
    pub vht_cap: u8,
    /// Explicit reserved bytes before the MCS bitmap.
    reserved5: [u8; 2],
    /// 802.11n BSS required MCS.
    pub basic_mcs: [u8; 16],
    /// Information Element byte offset from the BSS record start; not bounds-checked here.
    pub ie_offset: u16,
    /// Length of Information Elements (IE) in bytes.
    pub ie_length: u32,
    /// Average receive signal-to-noise ratio in dB.
    pub snr: i16,
    // there will be more stuff here
}
impl_bytes!(BssInfo);

impl BssInfo {
    /// Returns a view of the fixed BSS prefix in `packet`, or None if too short; misalignment panics.
    /// Two-byte alignment is required. Variable fields, counts, IE offsets and firmware revision remain unvalidated.
    pub(crate) fn parse(packet: &mut [u8]) -> Option<&mut Self> {
        if packet.len() < BssInfo::SIZE {
            return None;
        }

        Some(BssInfo::from_bytes_mut(
            packet[..BssInfo::SIZE].as_mut().try_into().unwrap(),
        ))
    }
}

#[cfg(test)]
/// Wire-layout regressions independent of radio hardware.
mod tests {
    use super::*;

    /// Const evaluation rejects implicit uninitialized padding in the same by-value conversion used by impl_bytes.
    #[test]
    fn scan_wire_layout_has_no_uninitialized_padding() {
        /// All-zero field values must produce initialized bytes across the complete retained 76-byte layout.
        const BYTES: [u8; ScanParams::SIZE] = unsafe {
            core::mem::transmute(ScanParams {
                version: 0,
                action: 0,
                sync_id: 0,
                ssid_len: 0,
                ssid: [0; 32],
                bssid: [0; 6],
                bss_type: 0,
                scan_type: 0,
                nprobes: 0,
                active_time: 0,
                passive_time: 0,
                home_time: 0,
                channel_num: 0,
                channel_list: [0],
                reserved: [0; 2],
            })
        };
        assert_eq!(ScanParams::SIZE, 76);
        assert_eq!(core::mem::align_of::<ScanParams>(), 4);
        assert_eq!(core::mem::offset_of!(ScanParams, channel_list), 72);
        assert_eq!(BYTES, [0; 76]);
        let backing = aligned::Aligned::<aligned::A4, _>(BYTES);
        let value = ScanParams::from_bytes(&backing);
        assert_eq!(value.to_bytes(), BYTES);
    }

    /// Every BDC offset respects available payload length; event data bounds use converted network-order length.
    #[test]
    fn bdc_and_event_payload_bounds_are_checked() {
        for offset in 0..=u8::MAX {
            let mut bytes = [0u8; 32];
            bytes[3] = offset;
            assert_eq!(BdcHeader::parse(&mut bytes).is_some(), offset <= 7);
        }
        for length in [0, 8, 9, u32::MAX] {
            let mut bytes = aligned::Aligned::<aligned::A4, _>([0u8; 80]);
            bytes[12..14].copy_from_slice(&0x886cu16.to_be_bytes());
            bytes[44..48].copy_from_slice(&length.to_be_bytes());
            assert_eq!(EventPacket::parse(&mut bytes[..]).is_some(), length <= 8);
            // Header conversion happens even when the declared payload is rejected.
            assert_eq!(u16::from_ne_bytes(bytes[12..14].try_into().unwrap()), 0x886c);
        }
    }

    /// Scan admission distinguishes zero records from a truncated first record without claiming all records exist.
    #[test]
    fn scan_result_prefix_bounds_and_alignment_are_checked() {
        let mut bytes = aligned::Aligned::<aligned::A4, _>([0u8; ScanResults::SIZE + BssInfo::SIZE + 2]);
        assert!(ScanResults::parse(&mut bytes[..ScanResults::SIZE]).is_some());
        bytes[10..12].copy_from_slice(&1u16.to_ne_bytes());
        assert!(ScanResults::parse(&mut bytes[..ScanResults::SIZE]).is_none());
        assert!(ScanResults::parse(&mut bytes[..ScanResults::SIZE + BssInfo::SIZE - 1]).is_none());
        assert!(ScanResults::parse(&mut bytes[..ScanResults::SIZE + BssInfo::SIZE]).is_some());
        assert!(ScanResults::parse(&mut bytes[1..]).is_none());
        assert!(BssInfo::parse(&mut bytes[..BssInfo::SIZE - 1]).is_none());
        assert!(BssInfo::parse(&mut bytes[..BssInfo::SIZE]).is_some());
    }
}
