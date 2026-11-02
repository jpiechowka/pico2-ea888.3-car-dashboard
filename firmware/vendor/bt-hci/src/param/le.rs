//! Low Energy HCI wire parameters, bounded record decoders and report iterators.
//! Raw wrappers preserve reserved values unless a typed enum decoder rejects them.
use core::iter::FusedIterator;

use super::{param, param_slice, BdAddr, ConnHandle, Duration, RemainingBytes};
use crate::{ByteAlignedValue, FixedSizeValue, FromHciBytes, FromHciBytesError, WriteHci};

#[cfg(test)]
#[path = "../declaration_regression_tests.rs"]
mod declaration_regression_tests;

param!(struct AddrKind(u8));

#[allow(missing_docs)]
impl AddrKind {
    /// Public device address.
    pub const PUBLIC: AddrKind = AddrKind(0);
    /// Random device address.
    pub const RANDOM: AddrKind = AddrKind(1);
    /// Resolvable private address, falling back to public identity when no peer IRK is available.
    pub const RESOLVABLE_PRIVATE_OR_PUBLIC: AddrKind = AddrKind(2);
    /// Resolvable private address, falling back to random identity when no peer IRK is available.
    pub const RESOLVABLE_PRIVATE_OR_RANDOM: AddrKind = AddrKind(3);
    /// Anonymous advertising report without an advertiser address.
    pub const ANONYMOUS_ADV: AddrKind = AddrKind(0xff);

    /// Create a new instance.
    /// Returns a wrapper preserving raw `v`; no context-specific address-kind range is checked.
    pub const fn new(v: u8) -> Self {
        Self(v)
    }

    /// Get the inner representation.
    /// Returns the raw address-kind byte, including reserved values.
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

unsafe impl ByteAlignedValue for AddrKind {}

impl<'de> crate::FromHciBytes<'de> for &'de AddrKind {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <AddrKind as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    bitfield AdvChannelMap[1] {
        (0, is_channel_37_enabled, enable_channel_37);
        (1, is_channel_38_enabled, enable_channel_38);
        (2, is_channel_39_enabled, enable_channel_39);
    }
}

#[allow(missing_docs)]
impl AdvChannelMap {
    /// Enables all three primary advertising channels.
    pub const ALL: AdvChannelMap = AdvChannelMap(0x07);
    /// Enables only primary advertising channel 37.
    pub const CHANNEL_37: AdvChannelMap = AdvChannelMap(0x01);
    /// Enables only primary advertising channel 38.
    pub const CHANNEL_38: AdvChannelMap = AdvChannelMap(0x02);
    /// Enables only primary advertising channel 39.
    pub const CHANNEL_39: AdvChannelMap = AdvChannelMap(0x04);
}

param!(struct ChannelMap([u8; 5]));

impl ChannelMap {
    /// Create a new instance.
    /// Returns a map with data channels 0..=36 enabled and reserved bits clear.
    pub fn new() -> Self {
        Self([0xff, 0xff, 0xff, 0xff, 0x1f])
    }

    /// Check if channel is marked as bad.
    /// Returns whether `channel` has its map bit clear. Caller must use data channels 0..=36;
    /// 37..=39 access reserved bits and values above 39 panic.
    pub fn is_channel_bad(&self, channel: u8) -> bool {
        let byte = usize::from(channel / 8);
        let bit = channel % 8;
        (self.0[byte] & (1 << bit)) == 0
    }

    /// Set channel to be marked as bad.
    /// Sets `channel` to unusable when `bad` is true. Caller must use 0..=36;
    /// 37..=39 modify reserved bits and values above 39 panic.
    pub fn set_channel_bad(&mut self, channel: u8, bad: bool) {
        let byte = usize::from(channel / 8);
        let bit = channel % 8;
        self.0[byte] = (self.0[byte] & !(1 << bit)) | (u8::from(!bad) << bit);
    }
}

unsafe impl ByteAlignedValue for ChannelMap {}

impl<'de> crate::FromHciBytes<'de> for &'de ChannelMap {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <ChannelMap as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    #[derive(Default)]
    enum AdvKind {
        #[default]
        AdvInd = 0,
        AdvDirectIndHigh = 1,
        AdvScanInd = 2,
        AdvNonconnInd = 3,
        AdvDirectIndLow = 4,
    }
}

param! {
    #[derive(Default)]
    enum AdvFilterPolicy {
        #[default]
        Unfiltered = 0,
        FilterScan = 1,
        FilterConn = 2,
        FilterConnAndScan = 3,
    }
}

param! {
    #[derive(Default)]
    enum LeScanKind {
        #[default]
        Passive = 0,
        Active = 1,
    }
}

param! {
    #[derive(Default)]
    enum ScanningFilterPolicy {
        #[default]
        BasicUnfiltered = 0,
        BasicFiltered = 1,
        ExtUnfiltered = 2,
        ExtFiltered = 3,
    }
}

param! {
    #[derive(Default)]
    enum PhyKind {
        #[default]
        Le1M = 1,
        Le2M = 2,
        LeCoded = 3,
        LeCodedS2 = 4,
    }
}

param! {
    bitfield SpacingTypes[2] {
        (0, has_t_ifs_acl_cp, set_t_ifs_acl_cp);
        (1, has_t_ifs_acl_pc, set_t_ifs_acl_pc);
        (2, has_t_mces, set_t_mces);
        (3, has_t_ifs_cis, set_t_ifs_cis);
        (4, has_t_mss_cis, set_t_mss_cis);
    }
}

param! {
    enum FrameSpaceInitiator {
        LocalHostInitiated = 0x00,
        LocalControllerInitiated = 0x01,
        PeerInitiated = 0x02,
    }
}

param! {
    bitfield AllPhys[1] {
        (0, has_no_tx_phy_preference, set_has_no_tx_phy_preference);
        (1, has_no_rx_phy_preference, set_has_no_rx_phy_preference);
    }
}

param! {
    bitfield PhyMask[1] {
        (0, has_le_1m_phy, set_le_1m_phy);
        (1, has_le_2m_phy, set_le_2m_phy);
        (2, has_le_coded_phy, set_le_coded_phy);
    }
}

/// Preferences when one can choose the phy.
/// Its two-byte alignment permits owned decoding from wire bytes, not borrowed byte-aligned decoding.
#[derive(Default)]
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum PhyOptions {
    #[default]
    /// No preference between coded S=2 and S=8.
    NoPreferredCoding = 0,
    /// Prefer coded S=2.
    S2CodingPreferred = 1,
    /// Prefer coded S=8.
    S8CodingPreferred = 2,
}

unsafe impl FixedSizeValue for PhyOptions {
    /// Both little-endian discriminant bytes must encode one of the three variants.
    #[inline(always)]
    /// Returns whether `data` is exactly a valid two-byte little-endian discriminant.
    fn is_valid(data: &[u8]) -> bool { matches!(data, [0..=2, 0]) }
}

/// PHY preference or requirement during extended advertisement (BLE5.4)
/// Its two-byte alignment permits owned decoding from wire bytes, not borrowed byte-aligned decoding.
#[derive(Default)]
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum AdvPhyOptions {
    #[default]
    /// No preference between coded S=2 and S=8.
    NoPreferredCoding = 0,
    /// Prefer coded S=2.
    S2CodingPreferred = 1,
    /// Prefer coded S=8.
    S8CodingPreferred = 2,
    /// Require coded S=2.
    S2CodingRequired = 3,
    /// Require coded S=8.
    S8CodingRequired = 4,
}

unsafe impl FixedSizeValue for AdvPhyOptions {
    /// Both little-endian discriminant bytes must encode one of the five variants.
    #[inline(always)]
    /// Returns whether `data` is exactly one of the five two-byte discriminants.
    fn is_valid(data: &[u8]) -> bool { matches!(data, [0..=4, 0]) }
}

param! {
    struct ScanningPhy {
        active_scan: bool,
        scan_interval: Duration<625>,
        scan_window: Duration<625>,
    }
}

param! {
    struct InitiatingPhy {
        scan_interval: Duration<625>,
        scan_window: Duration<625>,
        conn_interval_min: Duration<1_250>,
        conn_interval_max: Duration<1_250>,
        max_latency: u16,
        supervision_timeout: Duration<10_000>,
        min_ce_len: Duration<625>,
        max_ce_len: Duration<625>,
    }
}

param! {
    struct ConnIntervalGroup {
        min: Duration<125>,
        max: Duration<125>,
        stride: Duration<125>,
    }
}

/// Parameters for different phy representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PhyParams<T> {
    /// 1M phy parameters.
    pub le_1m_phy: Option<T>,
    /// 2M phy parameters.
    pub le_2m_phy: Option<T>,
    /// Coded phy parameters.
    pub le_coded_phy: Option<T>,
}

impl<T> PhyParams<T> {
    /// Get the mask associated with the parameters.
    /// Returns the mask of PHY entries that are present, independently of their parameter validity.
    pub fn scanning_phys(&self) -> PhyMask {
        PhyMask::new()
            .set_le_1m_phy(self.le_1m_phy.is_some())
            .set_le_2m_phy(self.le_2m_phy.is_some())
            .set_le_coded_phy(self.le_coded_phy.is_some())
    }
}

impl<T: WriteHci> WriteHci for PhyParams<T> {
    #[inline(always)]
    /// Returns the mask byte plus the sizes of present PHY parameters; propagates nested size panics.
    fn size(&self) -> usize {
        1 + self.le_1m_phy.size() + self.le_2m_phy.size() + self.le_coded_phy.size()
    }

    #[inline(always)]
    /// Writes mask then present 1M/2M/coded parameters to `writer`; returns its error.
    /// Nested serialization failures may follow an already-written mask or parameter prefix.
    fn write_hci<W: embedded_io::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        self.scanning_phys().write_hci(&mut writer)?;
        self.le_1m_phy.write_hci(&mut writer)?;
        self.le_2m_phy.write_hci(&mut writer)?;
        self.le_coded_phy.write_hci(&mut writer)?;
        Ok(())
    }

    #[inline(always)]
    /// Writes mask then present PHY parameters to `writer`; returns its error.
    /// Cancellation or nested failure may retain a prefix; no rollback is attempted.
    async fn write_hci_async<W: ::embedded_io_async::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        self.scanning_phys().write_hci_async(&mut writer).await?;
        self.le_1m_phy.write_hci_async(&mut writer).await?;
        self.le_2m_phy.write_hci_async(&mut writer).await?;
        self.le_coded_phy.write_hci_async(&mut writer).await?;
        Ok(())
    }
}

param!(struct AdvHandle(u8));

impl AdvHandle {
    /// Create a new instance.
    /// Returns an advertising handle preserving `v`; controller-specific validity is not checked.
    pub const fn new(v: u8) -> Self {
        Self(v)
    }

    /// Get the inner representation.
    /// Returns the raw advertising handle byte.
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

unsafe impl ByteAlignedValue for AdvHandle {}

impl<'de> crate::FromHciBytes<'de> for &'de AdvHandle {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <AdvHandle as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    bitfield AdvEventProps[2] {
        (0, connectable_adv, set_connectable_adv);
        (1, scannable_adv, set_scannable_adv);
        (2, directed_adv, set_directed_adv);
        (3, high_duty_cycle_directed_connectable_adv, set_high_duty_cycle_directed_connectable_adv);
        (4, legacy_adv, set_legacy_adv);
        (5, anonymous_adv, set_anonymous_adv);
        (6, include_tx_power, set_include_tx_power);
    }
}

param! {
    #[derive(Default)]
    enum Operation {
        #[default]
        IntermediateFragment = 0,
        FirstFragment = 1,
        LastFragment = 2,
        Complete = 3,
        Unchanged = 4,
    }
}

param! {
    struct AdvSet {
        adv_handle: AdvHandle,
        duration: Duration<10_000>,
        max_ext_adv_events: u8,
    }
}

param_slice!(&'a [AdvSet]);

param! {
    bitfield PeriodicAdvProps[2] {
        (6, is_tx_power_included, include_tx_power);
    }
}

param! {
    #[derive(Default)]
    enum FilterDuplicates {
        #[default]
        Disabled = 0,
        Enabled = 1,
        EnabledPerScanPeriod = 2,
    }
}

param! {
    bitfield LePeriodicAdvCreateSyncOptions[1] {
        (0, is_using_periodic_adv_list, use_periodic_adv_list);
        (1, is_reporting_initially_disabled, disable_initial_reporting);
        (2, is_duplicate_filtering_enabled, enable_duplicate_filtering);
    }
}

param! {
    bitfield CteMask[1] {
        (0, is_aoa_cte, set_aoa_cte);
        (1, is_aod_1us_cte, set_aod_1us_cte);
        (2, is_aod_2us_cte, set_aod_2us_cte);
        (3, is_type_3_cte, set_type_3_cte);
        (4, is_non_cte, set_non_cte);
    }
}

param!(struct SyncHandle(u16));

param!(struct BigHandle(u8));

param! {
    #[derive(Default)]
    enum PrivacyMode {
        #[default]
        Network = 0,
        Device = 1,
    }
}

param! {
    #[derive(Default)]
    enum CteKind {
        #[default]
        AoA = 0,
        AoD1Us = 1,
        AoD2Us = 2,
        NoCte = 0xff,
    }
}

param! {
    bitfield SwitchingSamplingRates[1] {
        (0, is_1us_aod_tx, set_1us_aod_tx);
        (1, is_1us_aod_rx, set_1us_aod_rx);
        (2, is_1us_aoa_rx, set_1us_aoa_rx);
    }
}

param! {
    bitfield LePeriodicAdvReceiveEnable[1] {
        (0, is_reporting, set_reporting);
        (1, is_duplicate_filtering, set_duplicate_filtering);
    }
}

param! {
    #[derive(Default)]
    enum LePeriodicAdvSyncTransferMode {
        #[default]
        NoSync = 0,
        SyncRx = 1,
        SyncRxReport = 2,
        SyncRxReportFilterDuplicates = 3,
    }
}

param! {
    bitfield LeDataRelatedAddrChangeReasons[1] {
        (0, change_on_adv_data_change, set_change_addr_on_adv_data_changes);
        (1, change_on_scan_response_data_change, set_change_addr_on_scan_response_data_changes);
    }
}

param! {
    #[derive(Default)]
    enum LeConnRole {
        #[default]
        Central = 0,
        Peripheral = 1,
    }
}

param! {
    #[derive(Default)]
    enum ClockAccuracy {
        #[default]
        Ppm500 = 0,
        Ppm250 = 1,
        Ppm150 = 2,
        Ppm100 = 3,
        Ppm75 = 4,
        Ppm50 = 5,
        Ppm30 = 6,
        Ppm20 = 7,
    }
}

param! {
    struct LeAdvertisingReportParam<'a> {
        event_type: u8,
        addr_kind: AddrKind,
        addr: BdAddr,
        data: &'a [u8],
        rssi: i8,
    }
}

param_slice! {
    [LeDirectedAdvertisingReportParam; 16] {
        event_type[0]: u8,
        addr_kind[1]: AddrKind,
        addr[2]: BdAddr,
        direct_addr_kind[8]: AddrKind,
        direct_addr[9]: BdAddr,
        rssi[15]: i8,
    }
}

param_slice! {
    [LeIQSample; 2] {
        i_sample[0]: i8,
        q_sample[1]: i8,
    }
}

param_slice! {
    [BisConnHandle; 2] {
        handle[0]: ConnHandle,
    }
}

param! {
    #[derive(Default)]
    enum DataStatus {
        #[default]
        Complete = 0,
        Incomplete = 1,
        Failed = 0xff,
    }
}

param! {
    #[derive(Default)]
    enum PacketStatus {
        #[default]
        CrcCorrect = 0,
        CrcIncorrectUsedLength = 1,
        CrcIncorrectUsedOther = 2,
        InsufficientResources = 0xff,
    }
}

param! {
    #[derive(Default)]
    enum TxStatus {
        #[default]
        Transmitted = 0,
        NotTransmitted = 1,
    }
}

param! {
    #[derive(Default)]
    enum ZoneEntered {
        #[default]
        Low = 0,
        Middle = 1,
        High = 2,
    }
}

param! {
    #[derive(Default)]
    enum LeTxPowerReportingReason {
        #[default]
        LocalTxPowerChanged = 0,
        RemoteTxPowerChanged = 1,
        LeReadRemoteTxPowerLevelCompleted = 2,
    }
}

param! {
    #[derive(Default)]
    enum LeAdvEventKind {
        #[default]
        AdvInd = 0,
        AdvDirectInd = 1,
        AdvScanInd = 2,
        AdvNonconnInd = 3,
        ScanRsp = 4,
    }
}

param! {
    bitfield LeExtAdvEventKind[2] {
        (0, connectable, set_connectable);
        (1, scannable, set_scannable);
        (2, directed, set_directed);
        (3, scan_response, set_scan_response);
        (4, legacy, set_legacy);
    }
}

/// Advertising data status.
#[allow(missing_docs)]
pub enum LeExtAdvDataStatus {
    /// The report contains complete advertising data.
    Complete,
    /// Additional report data is expected.
    IncompleteMoreExpected,
    /// Data is incomplete and no further fragment is expected.
    IncompleteTruncated,
    /// Reserved status value retained from the wire.
    Reserved,
}

impl LeExtAdvEventKind {
    /// Get data status.
    /// Returns the two-bit data status without changing other event flags.
    pub fn data_status(&self) -> LeExtAdvDataStatus {
        let data_status = (self.0[0] >> 5) & 0x03;
        match data_status {
            0 => LeExtAdvDataStatus::Complete,
            1 => LeExtAdvDataStatus::IncompleteMoreExpected,
            2 => LeExtAdvDataStatus::IncompleteTruncated,
            _ => LeExtAdvDataStatus::Reserved,
        }
    }

    /// Set data status.
    /// Returns the modified flags with data `status`, preserving all other bits.
    pub fn set_data_status(mut self, status: LeExtAdvDataStatus) -> Self {
        let value = match status {
            LeExtAdvDataStatus::Complete => 0,
            LeExtAdvDataStatus::IncompleteMoreExpected => 1,
            LeExtAdvDataStatus::IncompleteTruncated => 2,
            LeExtAdvDataStatus::Reserved => 3,
        };
        self.0[0] &= !(0x03 << 5);
        self.0[0] |= value << 5;
        self
    }
}

param! {
    struct LeAdvReport<'a> {
        event_kind: LeAdvEventKind,
        addr_kind: AddrKind,
        addr: BdAddr,
        data: &'a [u8],
        rssi: i8,
    }
}

param! {
    struct LeAdvReports<'a> {
        num_reports: u8,
        bytes: RemainingBytes<'a>,
    }
}

impl LeAdvReports<'_> {
    /// Check if there are more reports available.
    /// Returns whether the declared report count is zero; does not validate the payload.
    pub fn is_empty(&self) -> bool {
        self.num_reports == 0
    }

    /// Number of advertising reports.
    /// Returns the declared count, which may exceed the number of decodable records.
    pub fn len(&self) -> usize {
        usize::from(self.num_reports)
    }

    /// Iterate over valid reports followed by at most one terminal decoding error.
    /// Pre-parses at most the declared u8 count so `ExactSizeIterator` stays exact on malformed input.
    /// Returns an exact-size iterator over the valid prefix and at most one terminal error.
    /// Bytes beyond the declared count are not yielded or rejected here.
    pub fn iter(&self) -> LeAdvReportsIter<'_> {
        let mut probe = LeAdvReportsIter { len: self.len(), bytes: &self.bytes };
        let mut len = 0;
        while probe.next().is_some() {
            len += 1;
        }
        LeAdvReportsIter { len, bytes: &self.bytes }
    }
}

/// An iterator for advertising reports.
pub struct LeAdvReportsIter<'a> {
    /// Remaining iterator yields, including one terminal error if the pre-parse found malformed input.
    len: usize,
    /// Unconsumed record bytes borrowed from the original event.
    bytes: &'a [u8],
}

impl<'a> Iterator for LeAdvReportsIter<'a> {
    /// One borrowed report or a terminal field/size decoding error.
    type Item = Result<LeAdvReport<'a>, FromHciBytesError>;

    /// Returns the next decoded report, one terminal error, or `None` once exhausted.
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            match LeAdvReport::from_hci_bytes(self.bytes) {
                Ok((report, rest)) => {
                    self.bytes = rest;
                    self.len -= 1;
                    Some(Ok(report))
                }
                Err(err) => {
                    self.len = 0;
                    Some(Err(err))
                }
            }
        }
    }

    /// Returns identical lower/upper bounds for remaining yields, including a possible terminal error.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl ExactSizeIterator for LeAdvReportsIter<'_> {
    /// Returns the exact remaining yield count, including a possible terminal error.
    fn len(&self) -> usize {
        self.len
    }
}

impl FusedIterator for LeAdvReportsIter<'_> {}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
/// One extended advertising report; raw signal values and reserved wrapper values are retained.
pub struct LeExtAdvReport<'a> {
    /// Event flags including connectability, legacy status and data completeness.
    pub event_kind: LeExtAdvEventKind,
    /// Advertiser address kind; anonymous reports may not have a usable address.
    pub addr_kind: AddrKind,
    /// Raw advertiser address; interpreted with `addr_kind`.
    pub addr: BdAddr,
    /// Primary advertising PHY selection.
    pub primary_adv_phy: PhyKind,
    /// Secondary PHY, or `None` when its wire byte is zero.
    pub secondary_adv_phy: Option<PhyKind>,
    /// Advertising SID; 0xFF denotes unavailable.
    pub adv_sid: u8,
    /// Transmit power in dBm; 127 denotes unavailable.
    pub tx_power: i8,
    /// Received power in dBm; 127 denotes unavailable.
    pub rssi: i8,
    /// Periodic advertising interval in 1.25 ms units; zero means no periodic advertising.
    pub adv_interval: Duration<1_250>,
    /// Directed target address kind, meaningful for directed reports.
    pub direct_addr_kind: AddrKind,
    /// Directed target address, interpreted with `direct_addr_kind`.
    pub direct_addr: BdAddr,
    /// Borrowed advertising bytes with an encoded one-byte length.
    pub data: &'a [u8],
}

impl WriteHci for LeExtAdvReport<'_> {
    #[inline(always)]
    /// Returns the fixed header plus counted payload size; panics if data exceeds 255 bytes.
    fn size(&self) -> usize {
        WriteHci::size(&self.event_kind)
            + WriteHci::size(&self.addr_kind)
            + WriteHci::size(&self.addr)
            + WriteHci::size(&self.primary_adv_phy)
            + 1 //secondary_adv_phy
            + WriteHci::size(&self.adv_sid)
            + WriteHci::size(&self.tx_power)
            + WriteHci::size(&self.rssi)
            + WriteHci::size(&self.adv_interval)
            + WriteHci::size(&self.direct_addr_kind)
            + WriteHci::size(&self.direct_addr)
            + WriteHci::size(&self.data)
    }
    #[inline(always)]
    /// Writes the report to `writer`; returns its error. Oversized data panics when reached,
    /// after fixed fields may have been written; `size()` can preflight this constraint.
    fn write_hci<W: ::embedded_io::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        self.event_kind.write_hci(&mut writer)?;
        self.addr_kind.write_hci(&mut writer)?;
        self.addr.write_hci(&mut writer)?;
        self.primary_adv_phy.write_hci(&mut writer)?;
        match self.secondary_adv_phy {
            None => 0u8.write_hci(&mut writer)?,
            Some(val) => val.write_hci(&mut writer)?,
        };
        self.adv_sid.write_hci(&mut writer)?;
        self.tx_power.write_hci(&mut writer)?;
        self.rssi.write_hci(&mut writer)?;
        self.adv_interval.write_hci(&mut writer)?;
        self.direct_addr_kind.write_hci(&mut writer)?;
        self.direct_addr.write_hci(&mut writer)?;
        self.data.write_hci(&mut writer)?;
        Ok(())
    }
    #[inline(always)]
    /// Writes the report to `writer`; returns its error. Cancellation may leave a prefix;
    /// oversized data panics when reached after fixed fields, unless preflighted with `size()`.
    async fn write_hci_async<W: ::embedded_io_async::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        self.event_kind.write_hci_async(&mut writer).await?;
        self.addr_kind.write_hci_async(&mut writer).await?;
        self.addr.write_hci_async(&mut writer).await?;
        self.primary_adv_phy.write_hci_async(&mut writer).await?;
        match self.secondary_adv_phy {
            None => 0u8.write_hci_async(&mut writer).await?,
            Some(val) => val.write_hci_async(&mut writer).await?,
        };
        self.adv_sid.write_hci_async(&mut writer).await?;
        self.tx_power.write_hci_async(&mut writer).await?;
        self.rssi.write_hci_async(&mut writer).await?;
        self.adv_interval.write_hci_async(&mut writer).await?;
        self.direct_addr_kind.write_hci_async(&mut writer).await?;
        self.direct_addr.write_hci_async(&mut writer).await?;
        self.data.write_hci_async(&mut writer).await?;
        Ok(())
    }
}

impl<'de> crate::FromHciBytes<'de> for LeExtAdvReport<'de> {
    #[allow(unused_variables)]
    /// Returns one report decoded from `data` and its unused tail. Truncation and invalid
    /// typed PHY values return errors; secondary PHY zero is decoded as unavailable.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        let (event_kind, data) = <LeExtAdvEventKind as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (addr_kind, data) = <AddrKind as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (addr, data) = <BdAddr as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (primary_adv_phy, data) = <PhyKind as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (secondary, data) = u8::from_hci_bytes(data)?;
        let secondary_adv_phy = if secondary == 0 {
            None
        } else {
            Some(PhyKind::from_hci_bytes_complete(&[secondary])?)
        };
        let (adv_sid, data) = <u8 as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (tx_power, data) = <i8 as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (rssi, data) = <i8 as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (adv_interval, data) = <Duration<1_250> as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (direct_addr_kind, data) = <AddrKind as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (direct_addr, data) = <BdAddr as crate::FromHciBytes>::from_hci_bytes(data)?;
        let (data, rest) = <&'de [u8] as crate::FromHciBytes>::from_hci_bytes(data)?;
        Ok((
            Self {
                event_kind,
                addr_kind,
                addr,
                primary_adv_phy,
                secondary_adv_phy,
                adv_sid,
                tx_power,
                rssi,
                adv_interval,
                direct_addr_kind,
                direct_addr,
                data,
            },
            rest,
        ))
    }
}

param! {
    struct LeExtAdvReports<'a> {
        num_reports: u8,
        bytes: RemainingBytes<'a>,
    }
}

impl LeExtAdvReports<'_> {
    /// Check if there are more reports available.
    /// Returns whether the declared extended-report count is zero, without validating records.
    pub fn is_empty(&self) -> bool {
        self.num_reports == 0
    }

    /// Number of advertising reports.
    /// Returns the declared extended-report count, not the successfully decoded count.
    pub fn len(&self) -> usize {
        usize::from(self.num_reports)
    }

    /// Iterate over valid reports followed by at most one terminal decoding error.
    /// Pre-parses at most the declared u8 count so `ExactSizeIterator` stays exact on malformed input.
    /// Returns an exact-size iterator over valid reports and at most one terminal error.
    /// Surplus bytes beyond the declared count are not yielded or rejected here.
    pub fn iter(&self) -> LeExtAdvReportsIter<'_> {
        let mut probe = LeExtAdvReportsIter { len: self.len(), bytes: &self.bytes };
        let mut len = 0;
        while probe.next().is_some() {
            len += 1;
        }
        LeExtAdvReportsIter { len, bytes: &self.bytes }
    }
}

/// An iterator for extended advertising reports.
pub struct LeExtAdvReportsIter<'a> {
    /// Remaining yields, including one terminal error if malformed input was found.
    len: usize,
    /// Unconsumed extended-report bytes borrowed from the event.
    bytes: &'a [u8],
}

impl<'a> Iterator for LeExtAdvReportsIter<'a> {
    /// One borrowed extended report or a terminal decoding error.
    type Item = Result<LeExtAdvReport<'a>, FromHciBytesError>;

    /// Returns the next report, one terminal error, or `None` after exhaustion.
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            match LeExtAdvReport::from_hci_bytes(self.bytes) {
                Ok((report, rest)) => {
                    self.bytes = rest;
                    self.len -= 1;
                    Some(Ok(report))
                }
                Err(err) => {
                    self.len = 0;
                    Some(Err(err))
                }
            }
        }
    }

    /// Returns equal lower/upper bounds on the remaining report/error yields.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl ExactSizeIterator for LeExtAdvReportsIter<'_> {
    /// Returns the exact remaining report/error yield count.
    fn len(&self) -> usize {
        self.len
    }
}

impl FusedIterator for LeExtAdvReportsIter<'_> {}

param! {
    struct LePeriodicAdvSubeventData<'a> {
        subevent: u8,
        response_slot_start: u8,
        response_slot_count: u8,
        subevent_data: &'a [u8],
    }
}

impl<'a, 'b: 'a> WriteHci for &'a [LePeriodicAdvSubeventData<'b>] {
    /// Preflights count and nested payload lengths, each at most 255, before returning wire bytes.
    /// A larger caller-provided value panics; both writers run this check before output.
    #[inline(always)]
    fn size(&self) -> usize {
        crate::checked_hci_u8_len(self.len());
        1 + self.iter().map(WriteHci::size).sum::<usize>()
    }
    #[inline(always)]
    /// Preflights count and all payload lengths, then writes counted records to `writer`.
    /// Returns writer errors; invalid local lengths panic before output and I/O errors may retain a prefix.
    fn write_hci<W: ::embedded_io::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        let count = crate::checked_hci_u8_len(self.len());
        self.size();
        writer.write_all(&[count])?;
        for x in self.iter() {
            <LePeriodicAdvSubeventData as WriteHci>::write_hci(x, &mut writer)?;
        }
        Ok(())
    }
    #[inline(always)]
    /// Preflights on first poll then writes counted records to `writer`; returns its error.
    /// Invalid lengths panic before output; cancellation or I/O failure may leave a prefix.
    async fn write_hci_async<W: ::embedded_io_async::Write>(&self, mut writer: W) -> Result<(), W::Error> {
        let count = crate::checked_hci_u8_len(self.len());
        self.size();
        writer.write_all(&[count]).await?;
        for x in self.iter() {
            <LePeriodicAdvSubeventData as WriteHci>::write_hci_async(x, &mut writer).await?;
        }
        Ok(())
    }
}

/// Defines a report with one explicit byte length and its borrowed payload.
/// Decoding validates every field and payload before publication; serialization panics
/// before writing if the public length field disagrees with the payload's byte count.
macro_rules! variable_report {
    ($name:ident { $($field:ident: $ty:ty,)* } $length:ident, $payload:ident) => {
        #[doc = concat!(stringify!($name), " with one byte-counted payload. Serialization requires the length field to match the payload and may retain a written prefix on writer error or cancellation.")]
        #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub struct $name<'a> {
            $(#[doc = concat!("Wire field ", stringify!($field), "; interpreted according to the report's HCI definition.")]
            pub $field: $ty,)*
            /// Number of bytes in the following payload, 0 through 255.
            pub $length: u8,
            /// Borrowed payload; its byte length must equal the explicit length field.
            pub $payload: &'a [u8],
        }

        impl<'a> FromHciBytes<'a> for $name<'a> {
            fn from_hci_bytes(data: &'a [u8]) -> Result<(Self, &'a [u8]), FromHciBytesError> {
                $(let ($field, data) = <$ty>::from_hci_bytes(data)?;)*
                let ($payload, rest) = <&[u8]>::from_hci_bytes(data)?;
                Ok((Self { $($field,)* $length: $payload.len() as u8, $payload }, rest))
            }
        }

        impl WriteHci for $name<'_> {
            fn size(&self) -> usize {
                assert_eq!(usize::from(self.$length), self.$payload.len(), "report payload length mismatch");
                1 + self.$payload.len() $(+ self.$field.size())*
            }
            fn write_hci<W: embedded_io::Write>(&self, mut writer: W) -> Result<(), W::Error> {
                assert_eq!(usize::from(self.$length), self.$payload.len(), "report payload length mismatch");
                $(self.$field.write_hci(&mut writer)?;)*
                self.$length.write_hci(&mut writer)?;
                writer.write_all(self.$payload)
            }
            async fn write_hci_async<W: embedded_io_async::Write>(&self, mut writer: W) -> Result<(), W::Error> {
                assert_eq!(usize::from(self.$length), self.$payload.len(), "report payload length mismatch");
                $(self.$field.write_hci_async(&mut writer).await?;)*
                self.$length.write_hci_async(&mut writer).await?;
                writer.write_all(self.$payload).await
            }
        }
    };
}

variable_report! {
    LePeriodicAdvertisingResponseReport {
        tx_power: i8,
        rssi: i8,
        cte_type: CteKind,
        response_slot: u8,
        data_status: DataStatus,
    } data_length, data
}

/// Container for periodic advertising response report data.
/// Validates interleaved records per Core 6.2 Vol 4 Part E sections 5.2 and 7.7.65.37.
/// Truncated fields, invalid enum bytes and short payloads fail before publication;
/// the decoder returns any unused tail separately. Indexed access scans preceding records.
#[derive(Debug, Clone, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LePeriodicAdvertisingResponseReports<'a> {
    /// Number of fully admitted response records in `bytes` (0..=255).
    num_responses: u8,
    /// Fully validated interleaved records, excluding subsequent packet bytes.
    bytes: &'a [u8],
}

impl<'a> LePeriodicAdvertisingResponseReports<'a> {
    /// Returns `true` if there are no responses.
    pub fn is_empty(&self) -> bool { self.num_responses == 0 }

    /// Returns the number of responses.
    pub fn len(&self) -> usize { usize::from(self.num_responses) }

    /// Returns the response entry at the given index, or `None` if out of bounds.
    /// `index` is zero-based; scans at most `index + 1` admitted records.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<LePeriodicAdvertisingResponseReport<'a>> {
        if index >= self.len() {
            return None;
        }
        let mut bytes = self.bytes;
        for current in 0..=index {
            let (entry, rest) = LePeriodicAdvertisingResponseReport::from_hci_bytes(bytes).ok()?;
            if current == index {
                return Some(entry);
            }
            bytes = rest;
        }
        None
    }

    /// Returns an iterator over all response entries.
    pub fn iter(&self) -> LePeriodicAdvertisingResponseReportsIter<'_> {
        LePeriodicAdvertisingResponseReportsIter {
            reports: self,
            index: 0,
        }
    }
}

impl<'de> FromHciBytes<'de> for LePeriodicAdvertisingResponseReports<'de> {
    /// Returns all count-declared records admitted from `data` plus unused tail.
    /// Invalid typed fields or truncated headers/payloads return an error before publication.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (num_responses, data) = u8::from_hci_bytes(data)?;
        let mut rest = data;
        for _ in 0..num_responses {
            rest = LePeriodicAdvertisingResponseReport::from_hci_bytes(rest)?.1;
        }

        Ok((
            Self {
                num_responses,
                bytes: &data[..data.len() - rest.len()],
            },
            rest,
        ))
    }
}

/// An iterator over the LePeriodicAdvertisingResponse reports.
pub struct LePeriodicAdvertisingResponseReportsIter<'a> {
    /// Borrowed container whose records were fully admitted by decoding.
    reports: &'a LePeriodicAdvertisingResponseReports<'a>,
    /// Zero-based next record index, never exceeding the admitted count.
    index: usize,
}

impl<'a> Iterator for LePeriodicAdvertisingResponseReportsIter<'a> {
    /// One borrowed, previously admitted response record.
    type Item = LePeriodicAdvertisingResponseReport<'a>;

    /// Returns the next admitted response or `None` after the count is exhausted.
    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.reports.get(self.index)?;
        self.index += 1;
        Some(entry)
    }

    /// Returns equal lower/upper bounds for remaining admitted responses.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.reports.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for LePeriodicAdvertisingResponseReportsIter<'_> {
    /// Returns the exact number of admitted responses left.
    fn len(&self) -> usize { self.reports.len() - self.index }
}

impl FusedIterator for LePeriodicAdvertisingResponseReportsIter<'_> {}

variable_report! {
    LeCsSubeventStepEntry {
        step_mode: u8,
        step_channel: u8,
    } step_data_length, step_data
}

/// Container for CS subevent step data.
///
/// Parses a count followed by interleaved mode/channel/length/data records,
/// as required by Core 6.2 Vol 4 Part E sections 5.2 and 7.7.65.44/45.
/// All records are validated before the container is returned; unused tail bytes
/// remain with the caller. No controller data is reinterpreted as enum references.
///
/// Entries are accessed via [`get`](LeCsSubeventStepData::get)
/// or [`iter`](LeCsSubeventStepData::iter); indexed access scans preceding records.
#[derive(Debug, Clone, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LeCsSubeventStepData<'a> {
    /// Number of fully admitted interleaved step records (0..=255).
    num_steps_reported: u8,
    /// Fully validated interleaved records, excluding subsequent packet bytes.
    bytes: &'a [u8],
}

impl<'a> LeCsSubeventStepData<'a> {
    /// Returns `true` if there are no steps.
    pub fn is_empty(&self) -> bool { self.num_steps_reported == 0 }

    /// Returns the number of steps.
    pub fn len(&self) -> usize { usize::from(self.num_steps_reported) }

    /// Returns the step entry at the given index, or `None` if out of bounds.
    /// `index` is zero-based; preceding records are scanned without copying payloads.
    pub fn get(
        &self,
        index: usize,
    ) -> Option<LeCsSubeventStepEntry<'a>> {
        if index >= self.len() {
            return None;
        }
        let mut bytes = self.bytes;
        for current in 0..=index {
            let (entry, rest) = LeCsSubeventStepEntry::from_hci_bytes(bytes).ok()?;
            if current == index {
                return Some(entry);
            }
            bytes = rest;
        }
        None
    }

    /// Returns an iterator over all step entries.
    pub fn iter(&self) -> LeCsSubeventStepDataIter<'_> { LeCsSubeventStepDataIter { data: self, index: 0 } }
}

impl<'de> FromHciBytes<'de> for LeCsSubeventStepData<'de> {
    /// Returns the count-declared step records decoded from `data` and the unused tail.
    /// A missing header, field or payload returns an error before exposing the container.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), FromHciBytesError> {
        let (num_steps_reported, data) = u8::from_hci_bytes(data)?;
        let mut rest = data;
        for _ in 0..num_steps_reported {
            rest = LeCsSubeventStepEntry::from_hci_bytes(rest)?.1;
        }

        Ok((
            Self {
                num_steps_reported,
                bytes: &data[..data.len() - rest.len()],
            },
            rest,
        ))
    }
}

/// An iterator over LeCsSubeventStepEntry values.
pub struct LeCsSubeventStepDataIter<'a> {
    /// Borrowed container of fully admitted interleaved steps.
    data: &'a LeCsSubeventStepData<'a>,
    /// Next zero-based step index, bounded by the admitted count.
    index: usize,
}

impl<'a> Iterator for LeCsSubeventStepDataIter<'a> {
    /// One previously admitted step with a borrowed payload.
    type Item = LeCsSubeventStepEntry<'a>;

    /// Returns the next admitted step, or `None` once exhausted.
    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.data.get(self.index)?;
        self.index += 1;
        Some(entry)
    }

    /// Returns equal lower/upper bounds for the remaining steps.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.data.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for LeCsSubeventStepDataIter<'_> {
    /// Returns the exact remaining step count.
    fn len(&self) -> usize { self.data.len() - self.index }
}

impl FusedIterator for LeCsSubeventStepDataIter<'_> {}

param! {
    #[derive(Default)]
    enum DoneStatus {
        #[default]
        Complete = 0,
        Partial = 1,
        Aborted = 0xf,
    }
}

/// Procedure abort reason.
#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProcedureAbortReason {
    #[default]
    /// Report with no abort.
    NoAbort = 0x0,
    /// Abort because of local Host or remote request.
    HostRequest = 0x1,
    /// Abort because filtered channel map has less than 15 channels.
    FilteredChannelMap = 0x2,
    /// Abort because the channel map update instant has passed.
    ChannelMapInstantPassed = 0x3,
    /// Abort because of unspecified reasons.
    Unspecified = 0xf,
}

impl From<u8> for ProcedureAbortReason {
    /// Returns the reason for raw `v`; unknown values map to `Unspecified`.
    fn from(v: u8) -> Self {
        match v {
            0x0 => Self::NoAbort,
            0x1 => Self::HostRequest,
            0x2 => Self::FilteredChannelMap,
            0x3 => Self::ChannelMapInstantPassed,
            _ => Self::Unspecified,
        }
    }
}

/// Subevent abort reason.
#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SubeventAbortReason {
    #[default]
    /// Report with no abort.
    NoAbort = 0x0,
    /// Abort because of local Host or remote request.
    HostRequest = 0x1,
    /// Abort because no CS_SYNC (mode-0) received.
    NoCsSync = 0x2,
    /// Abort because of scheduling conflicts or limited resources.
    SchedulingConflict = 0x3,
    /// Abort because of unspecified reasons.
    Unspecified = 0xf,
}

impl From<u8> for SubeventAbortReason {
    /// Returns the reason for raw `v`; unknown values map to `Unspecified`.
    fn from(v: u8) -> Self {
        match v {
            0x0 => Self::NoAbort,
            0x1 => Self::HostRequest,
            0x2 => Self::NoCsSync,
            0x3 => Self::SchedulingConflict,
            _ => Self::Unspecified,
        }
    }
}

/// Abort reason for CS subevent result, packed as two 4-bit nibbles.
///
/// Bits 0-3: procedure abort reason ([`ProcedureAbortReason`])
/// Bits 4-7: subevent abort reason ([`SubeventAbortReason`])
#[repr(transparent)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PackedAbortReasons(
    /// Raw procedure low nibble and subevent high nibble; unknown reasons remain stored.
    u8,
);

impl PackedAbortReasons {
    /// Returns the procedure-level abort reason.
    pub fn procedure_reason(&self) -> ProcedureAbortReason {
        ProcedureAbortReason::from(self.0 & 0x0f)
    }

    /// Returns the subevent-level abort reason.
    pub fn subevent_reason(&self) -> SubeventAbortReason {
        SubeventAbortReason::from((self.0 >> 4) & 0x0f)
    }
}

unsafe impl FixedSizeValue for PackedAbortReasons {
    /// Returns true for any `_data`; all packed reason bits are stored. The caller enforces one-byte width.
    fn is_valid(_data: &[u8]) -> bool {
        true
    }
}

/// Frequency compensation value in units of 0.01 ppm.
#[repr(transparent)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FrequencyCompensation(
    /// Raw signed 15-bit value or 0xC000 unavailable sentinel, retaining reserved bits.
    u16,
);

impl FrequencyCompensation {
    /// Returns the raw 16-bit value.
    pub fn as_raw(&self) -> u16 {
        self.0
    }

    /// Returns `true` if the value is available.
    pub fn is_available(&self) -> bool {
        self.0 != 0xC000
    }

    /// Returns the frequency compensation in 0.01 ppm units, or `None` if not available.
    pub fn as_ppm_x100(&self) -> Option<i16> {
        if !self.is_available() {
            return None;
        }
        let val = self.0 & 0x7FFF;
        Some(((val << 1) as i16) >> 1)
    }
}

unsafe impl FixedSizeValue for FrequencyCompensation {
    /// Returns true for any `_data`; reserved values remain raw storage. The caller enforces two-byte width.
    fn is_valid(_data: &[u8]) -> bool {
        true
    }
}

// ============================================================================
// LE Audio / Isochronous parameters (Bluetooth Core Specification v5.4+)
// ============================================================================

param!(
    /// CIG ID (0x00 – 0xEF)
    struct CigId(u8)
);

#[allow(missing_docs)]
impl CigId {
    /// Create a new instance.
    /// Returns a CIG identifier preserving `v`; the specified 0..=0xEF range is not enforced.
    pub const fn new(v: u8) -> Self {
        Self(v)
    }

    /// Get the inner representation.
    /// Returns the raw CIG identifier byte.
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

unsafe impl ByteAlignedValue for CigId {}

impl<'de> crate::FromHciBytes<'de> for &'de CigId {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <CigId as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param!(
    /// CIS ID (0x00 – 0xEF)
    struct CisId(u8)
);

#[allow(missing_docs)]
impl CisId {
    /// Create a new instance.
    /// Returns a CIS identifier preserving `v`; the specified 0..=0xEF range is not enforced.
    pub const fn new(v: u8) -> Self {
        Self(v)
    }

    /// Get the inner representation.
    /// Returns the raw CIS identifier byte.
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

unsafe impl ByteAlignedValue for CisId {}

impl<'de> crate::FromHciBytes<'de> for &'de CisId {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <CisId as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    /// Per-CIS configuration for `LE Set CIG Parameters`.
    struct CisConfig {
        cis_id: CisId,
        max_sdu_c_to_p: u16,
        max_sdu_p_to_c: u16,
        phy_c_to_p: PhyMask,
        phy_p_to_c: PhyMask,
        rtn_c_to_p: u8,
        rtn_p_to_c: u8,
    }
}

param_slice!(&'a [CisConfig]);

param! {
    /// Per-CIS configuration for `LE Set CIG Parameters Test`.
    struct CisConfigTest {
        cis_id: CisId,
        nse: u8,
        max_sdu_c_to_p: u16,
        max_sdu_p_to_c: u16,
        max_pdu_c_to_p: u16,
        max_pdu_p_to_c: u16,
        phy_c_to_p: PhyMask,
        phy_p_to_c: PhyMask,
        bn_c_to_p: u8,
        bn_p_to_c: u8,
    }
}

param_slice!(&'a [CisConfigTest]);

param! {
    /// CIS-to-ACL handle mapping for `LE Create CIS`.
    struct CisConnConfig {
        cis_handle: ConnHandle,
        acl_handle: ConnHandle,
    }
}

param_slice!(&'a [CisConnConfig]);

param! {
    /// Data path direction.
    #[derive(Default)]
    enum DataPathDirection {
        #[default]
        Input = 0,
        Output = 1,
    }
}

param!(
    /// Data path identifier.
    ///
    /// - `0x00` = HCI
    /// - `0x01`–`0xFE` = Logical channel number (vendor-specific)
    /// - `0xFF` = Audio test mode
    struct DataPathId(u8)
);

#[allow(missing_docs)]
impl DataPathId {
    /// HCI data path.
    pub const HCI: DataPathId = DataPathId(0x00);
    /// Audio test mode.
    pub const AUDIO_TEST_MODE: DataPathId = DataPathId(0xFF);

    /// Create a new instance.
    /// Returns a data-path identifier preserving `v`, without validating vendor support.
    pub const fn new(v: u8) -> Self {
        Self(v)
    }

    /// Get the inner representation.
    /// Returns the raw data-path identifier.
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

unsafe impl ByteAlignedValue for DataPathId {}

impl<'de> crate::FromHciBytes<'de> for &'de DataPathId {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <DataPathId as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    /// Codec ID (5 octets).
    struct CodecId {
        coding_format: u8,
        company_id: u16,
        vendor_specific_codec_id: u16,
    }
}

param!(
    /// Broadcast code (16 octets).
    struct BroadcastCode([u8; 16])
);

#[allow(missing_docs)]
impl BroadcastCode {
    /// Create a new instance.
    /// Returns a broadcast-code wrapper owning the 16 bytes in `v`; no key derivation occurs.
    pub const fn new(v: [u8; 16]) -> Self {
        Self(v)
    }

    /// Get the byte representation.
    /// Returns a borrowed view of all 16 broadcast-code bytes.
    pub fn raw(&self) -> &[u8] {
        &self.0[..]
    }
}

unsafe impl ByteAlignedValue for BroadcastCode {}

impl<'de> crate::FromHciBytes<'de> for &'de BroadcastCode {
    #[inline(always)]
    /// Returns a borrowed fixed-width value from `data` plus unused tail, or a size error
    /// on truncation. Raw wrapper values are retained without controller-policy validation.
    fn from_hci_bytes(data: &'de [u8]) -> Result<(Self, &'de [u8]), crate::FromHciBytesError> {
        <BroadcastCode as crate::ByteAlignedValue>::ref_from_hci_bytes(data)
    }
}

param! {
    /// BIG encryption mode.
    #[derive(Default)]
    enum EncryptionMode {
        #[default]
        Unencrypted = 0,
        Encrypted = 1,
    }
}

param! {
    /// CIS/BIG packing method.
    #[derive(Default)]
    enum Packing {
        #[default]
        Sequential = 0,
        Interleaved = 1,
    }
}

param! {
    /// CIS/BIG framing mode.
    #[derive(Default)]
    enum Framing {
        #[default]
        Unframed = 0,
        Framed = 1,
    }
}

param! {
    /// ISO test payload type.
    #[derive(Default)]
    enum PayloadType {
        #[default]
        ZeroLength = 0,
        VariableLength = 1,
        MaximumLength = 2,
    }
}

#[cfg(test)]
/// Wire-flag, channel-map and frequency-value regression fixtures; no hardware execution.
mod tests {
    use super::*;

    #[test]
    /// Checks data-status updates preserve unrelated event flags; panics on encoding drift.
    fn test_ext_adv_event_kind() {
        let k = LeExtAdvEventKind::new().set_connectable(true);
        assert_eq!(k.0[0], 0b0000001);
        let k = k.set_data_status(LeExtAdvDataStatus::Complete);
        assert_eq!(k.0[0], 0b0000001);
        let k = k.set_data_status(LeExtAdvDataStatus::IncompleteMoreExpected);
        assert_eq!(k.0[0], 0b0100001);
        let k = k.set_data_status(LeExtAdvDataStatus::IncompleteTruncated);
        assert_eq!(k.0[0], 0b1000001);
    }

    #[test]
    /// Checks all 37 data channels start usable and the three reserved map bits stay clear.
    fn test_channel_map_new() {
        let m = ChannelMap::new();
        for chan in 0..37 {
            assert!(!m.is_channel_bad(chan));
        }

        for chan in 37..40 {
            assert!(m.is_channel_bad(chan));
        }
    }

    #[test]
    /// Checks positive frequency compensation in hundredths of a ppm.
    fn test_frequency_compensation_positive() {
        let fc = FrequencyCompensation(0x2710);
        assert!(fc.is_available());
        assert_eq!(fc.as_ppm_x100(), Some(10000));
        assert_eq!(fc.as_raw(), 0x2710);
    }

    #[test]
    /// Checks signed negative frequency compensation in hundredths of a ppm.
    fn test_frequency_compensation_negative() {
        let fc = FrequencyCompensation(0x58F0);
        assert!(fc.is_available());
        assert_eq!(fc.as_ppm_x100(), Some(-10000));
        assert_eq!(fc.as_raw(), 0x58F0);
    }

    #[test]
    /// Checks the unavailable sentinel returns no numeric frequency value.
    fn test_frequency_compensation_not_available() {
        let fc = FrequencyCompensation(0xC000);
        assert!(!fc.is_available());
        assert_eq!(fc.as_ppm_x100(), None);
        assert_eq!(fc.as_raw(), 0xC000);
    }
}
