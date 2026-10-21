//! Advertisement config.
use bt_hci::param::{AddrKind, AdvEventProps, BdAddr};
pub use bt_hci::param::{AdvChannelMap, AdvFilterPolicy, AdvHandle, AdvSet, PhyKind};
use embassy_time::Duration;

use crate::cursor::{ReadCursor, WriteCursor};
use crate::types::uuid::Uuid;
use crate::{bt_hci_duration, codec, Address};

/// Transmit power levels.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
#[repr(i8)]
pub enum TxPower {
    /// Requested -40 dBm; controller support and actual selected power are separate.
    Minus40dBm = -40,
    /// Requested -20 dBm; controller support and actual selected power are separate.
    Minus20dBm = -20,
    /// Requested -16 dBm; controller support and actual selected power are separate.
    Minus16dBm = -16,
    /// Requested -12 dBm; controller support and actual selected power are separate.
    Minus12dBm = -12,
    /// Requested -8 dBm; controller support and actual selected power are separate.
    Minus8dBm = -8,
    /// Requested -4 dBm; controller support and actual selected power are separate.
    Minus4dBm = -4,
    /// Requested 0 dBm; controller may select a supported value.
    ZerodBm = 0,
    /// Requested +2 dBm; controller support and actual selected power are separate.
    Plus2dBm = 2,
    /// Requested +3 dBm; controller support and actual selected power are separate.
    Plus3dBm = 3,
    /// Requested +4 dBm; controller support and actual selected power are separate.
    Plus4dBm = 4,
    /// Requested +5 dBm; controller support and actual selected power are separate.
    Plus5dBm = 5,
    /// Requested +6 dBm; controller support and actual selected power are separate.
    Plus6dBm = 6,
    /// Requested +7 dBm; controller support and actual selected power are separate.
    Plus7dBm = 7,
    /// Requested +8 dBm; controller support and actual selected power are separate.
    Plus8dBm = 8,
    /// Requested +10 dBm; controller support and actual selected power are separate.
    Plus10dBm = 10,
    /// Requested +12 dBm; controller support and actual selected power are separate.
    Plus12dBm = 12,
    /// Requested +14 dBm; controller support and actual selected power are separate.
    Plus14dBm = 14,
    /// Requested +16 dBm; controller support and actual selected power are separate.
    Plus16dBm = 16,
    /// Requested +18 dBm; controller support and actual selected power are separate.
    Plus18dBm = 18,
    /// Requested +20 dBm; controller support and actual selected power are separate.
    Plus20dBm = 20,
}

/// Configuration and borrowed payloads for one extended advertising set.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AdvertisementSet<'d> {
    /// Parameters for the advertisement.
    pub params: AdvertisementParameters,
    /// Advertisement data.
    pub data: Advertisement<'d>,
    /// Override the random address for this advertising set.
    /// `None` = use host default address.
    pub address: Option<BdAddr>,
}

impl<'d> AdvertisementSet<'d> {
    /// Returns N sequential handle descriptors from `sets`, with timeouts/events copied from each set.
    /// The advertiser admits at most63 sets; this helper alone casts indices to u8 without checking N.
    /// Durations floor to10 ms ticks and panic above the u16 tick range; None/zero means no duration limit.
    pub fn handles<const N: usize>(sets: &[AdvertisementSet<'d>; N]) -> [AdvSet; N] {
        /// Zero-duration, unlimited-event descriptor overwritten for each input set.
        const NEW_SET: AdvSet = AdvSet {
            adv_handle: AdvHandle::new(0),
            duration: bt_hci::param::Duration::from_u16(0),
            max_ext_adv_events: 0,
        };

        let mut ret = [NEW_SET; N];
        for (i, set) in sets.iter().enumerate() {
            ret[i].adv_handle = AdvHandle::new(i as u8);
            ret[i].duration = bt_hci_duration(set.params.timeout.unwrap_or(embassy_time::Duration::from_micros(0)));
            ret[i].max_ext_adv_events = set.params.max_events.unwrap_or(0);
        }
        ret
    }
}

/// Parameters for an advertisement.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Copy, Clone, Debug)]
pub struct AdvertisementParameters {
    /// Primary advertising PHY for extended advertising; legacy API ignores it.
    pub primary_phy: PhyKind,

    /// Secondary advertising PHY for extended advertising.
    pub secondary_phy: PhyKind,

    /// Requested extended-advertising TX power in dBm; actual controller choice can differ.
    pub tx_power: TxPower,

    /// Extended-set duration, floored to10 ms ticks; None/zero means unlimited. Legacy API does not enforce it.
    pub timeout: Option<Duration>,

    /// Extended-set event count limit; None/zero means unlimited. Legacy API ignores it.
    pub max_events: Option<u8>,

    /// Lower interval bound, floored to625 us ticks; legacy uses16 bits, extended24 bits.
    pub interval_min: Duration,

    /// Upper interval bound in625 us ticks after conversion; hardware validates ordering/range.
    pub interval_max: Duration,

    /// Enabled primary channels; None selects all three.
    pub channel_map: Option<AdvChannelMap>,

    /// Controller filter policy for scan/connection requests; list contents are configured separately.
    pub filter_policy: AdvFilterPolicy,

    /// Extended controller fragmentation preference; does not enable multi-command HCI uploads.
    pub fragment: bool,

    /// Override the own address type for this advertising set.
    /// `None` = use host default (derived from host address + privacy state).
    pub own_addr_kind: Option<AddrKind>,
}

impl Default for AdvertisementParameters {
    /// Returns160 ms intervals, LE1M PHYs,0 dBm, all channels and no duration/event/address override.
    fn default() -> Self {
        Self {
            primary_phy: PhyKind::Le1M,
            secondary_phy: PhyKind::Le1M,
            tx_power: TxPower::ZerodBm,
            timeout: None,
            max_events: None,
            interval_min: Duration::from_millis(160),
            interval_max: Duration::from_millis(160),
            filter_policy: AdvFilterPolicy::default(),
            channel_map: None,
            fragment: false,
            own_addr_kind: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Borrowed normalized HCI properties and payloads; conversion alone does not validate lengths.
pub(crate) struct RawAdvertisement<'d> {
    /// PDU property bits selecting legacy/extended, direction, scan and connection behavior.
    pub(crate) props: AdvEventProps,
    /// Complete advertising-data payload, without HCI framing.
    pub(crate) adv_data: &'d [u8],
    /// Complete scan-response payload, empty when the PDU cannot carry it.
    pub(crate) scan_data: &'d [u8],
    /// Target address for directed advertising, None otherwise.
    pub(crate) peer: Option<Address>,
}

impl Default for RawAdvertisement<'_> {
    /// Returns empty legacy connectable/scannable undirected fields; flags still require admission validation.
    fn default() -> Self {
        Self {
            props: AdvEventProps::new()
                .set_connectable_adv(true)
                .set_scannable_adv(true)
                .set_legacy_adv(true),
            adv_data: &[],
            scan_data: &[],
            peer: None,
        }
    }
}

/// Advertisement payload depending on which advertisement kind requested.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Advertisement<'d> {
    /// Connectable and scannable undirected advertisement.
    ConnectableScannableUndirected {
        /// Advertisement data.
        adv_data: &'d [u8],
        /// Scan data.
        scan_data: &'d [u8],
    },
    /// Connectable and non-scannable directed advertisement.
    ConnectableNonscannableDirected {
        /// Address of the peer to direct the advertisement to.
        peer: Address,
    },
    /// Connectable and non-scannable directed advertisement with high duty cycle.
    ConnectableNonscannableDirectedHighDuty {
        /// Address of the peer to direct the advertisement to.
        peer: Address,
    },
    /// Nonconnectable and scannable undirected advertisement.
    NonconnectableScannableUndirected {
        /// Advertisement data.
        adv_data: &'d [u8],
        /// Scan data.
        scan_data: &'d [u8],
    },
    /// Nonconnectable and nonscannable undirected advertisement.
    NonconnectableNonscannableUndirected {
        /// Advertisement data.
        adv_data: &'d [u8],
    },
    /// Extended connectable and non-scannable undirected advertisement.
    ExtConnectableNonscannableUndirected {
        /// Advertisement data.
        adv_data: &'d [u8],
    },
    /// Extended connectable and non-scannable directed advertisement.
    ExtConnectableNonscannableDirected {
        /// Address of the peer to direct the advertisement to.
        peer: Address,
        /// Advertisement data.
        adv_data: &'d [u8],
    },
    /// Extended nonconnectable and scannable undirected advertisement.
    ExtNonconnectableScannableUndirected {
        /// Scan data.
        scan_data: &'d [u8],
    },
    /// Extended nonconnectable and scannable directed advertisement.
    ExtNonconnectableScannableDirected {
        /// Address of the peer to direct the advertisement to.
        peer: Address,
        /// Scan data.
        scan_data: &'d [u8],
    },
    /// Extended nonconnectable and nonscannable undirected advertisement.
    ExtNonconnectableNonscannableUndirected {
        /// Whether the advertisement is anonymous.
        anonymous: bool,
        /// Advertisement data.
        adv_data: &'d [u8],
    },
    /// Extended nonconnectable and nonscannable directed advertisement.
    ExtNonconnectableNonscannableDirected {
        /// Whether the advertisement is anonymous.
        anonymous: bool,
        /// Address of the peer to direct the advertisement to.
        peer: Address,
        /// Advertisement data.
        adv_data: &'d [u8],
    },
}

impl<'d> Advertisement<'d> {
    /// Returns whether applicable connectable payloads start with Flags containing BR_EDR_NOT_SUPPORTED.
    /// Other variants pass here; this is not full AD validation or a length/controller-capability check.
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Advertisement::ConnectableScannableUndirected { adv_data, .. }
            | Advertisement::ExtConnectableNonscannableUndirected { adv_data }
            | Advertisement::ExtConnectableNonscannableDirected { adv_data, .. } => {
                // Connectable advertisements must have flags as the first advertisement data with the LE Only flag set
                adv_data.len() >= 3 && adv_data[0] == 2 && adv_data[1] == 1 && (adv_data[2] & BR_EDR_NOT_SUPPORTED) != 0
            }
            _ => true,
        }
    }
}
impl<'d> From<Advertisement<'d>> for RawAdvertisement<'d> {
    /// Returns HCI properties and borrowed fields for `val`; performs no payload/length validation.
    fn from(val: Advertisement<'d>) -> RawAdvertisement<'d> {
        match val {
            Advertisement::ConnectableScannableUndirected { adv_data, scan_data } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(true)
                    .set_scannable_adv(true)
                    .set_anonymous_adv(false)
                    .set_legacy_adv(true),
                adv_data,
                scan_data,
                peer: None,
            },
            Advertisement::ConnectableNonscannableDirected { peer } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(true)
                    .set_scannable_adv(false)
                    .set_directed_adv(true)
                    .set_anonymous_adv(false)
                    .set_legacy_adv(true),
                adv_data: &[],
                scan_data: &[],
                peer: Some(peer),
            },
            Advertisement::ConnectableNonscannableDirectedHighDuty { peer } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(true)
                    .set_scannable_adv(false)
                    .set_directed_adv(true)
                    .set_high_duty_cycle_directed_connectable_adv(true)
                    .set_anonymous_adv(false)
                    .set_legacy_adv(true),
                adv_data: &[],
                scan_data: &[],
                peer: Some(peer),
            },
            Advertisement::NonconnectableScannableUndirected { adv_data, scan_data } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(false)
                    .set_scannable_adv(true)
                    .set_anonymous_adv(false)
                    .set_legacy_adv(true),
                adv_data,
                scan_data,
                peer: None,
            },
            Advertisement::NonconnectableNonscannableUndirected { adv_data } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(false)
                    .set_scannable_adv(false)
                    .set_anonymous_adv(false)
                    .set_legacy_adv(true),
                adv_data,
                scan_data: &[],
                peer: None,
            },
            Advertisement::ExtConnectableNonscannableUndirected { adv_data } => RawAdvertisement {
                props: AdvEventProps::new().set_connectable_adv(true).set_scannable_adv(false),
                adv_data,
                scan_data: &[],
                peer: None,
            },
            Advertisement::ExtConnectableNonscannableDirected { adv_data, peer } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(true)
                    .set_scannable_adv(false)
                    .set_directed_adv(true),
                adv_data,
                scan_data: &[],
                peer: Some(peer),
            },

            Advertisement::ExtNonconnectableScannableUndirected { scan_data } => RawAdvertisement {
                props: AdvEventProps::new().set_connectable_adv(false).set_scannable_adv(true),
                adv_data: &[],
                scan_data,
                peer: None,
            },
            Advertisement::ExtNonconnectableScannableDirected { scan_data, peer } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(false)
                    .set_scannable_adv(true)
                    .set_directed_adv(true),
                adv_data: &[],
                scan_data,
                peer: Some(peer),
            },
            Advertisement::ExtNonconnectableNonscannableUndirected { adv_data, anonymous } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(false)
                    .set_scannable_adv(false)
                    .set_anonymous_adv(anonymous)
                    .set_directed_adv(false),
                adv_data,
                scan_data: &[],
                peer: None,
            },
            Advertisement::ExtNonconnectableNonscannableDirected {
                adv_data,
                peer,
                anonymous,
            } => RawAdvertisement {
                props: AdvEventProps::new()
                    .set_connectable_adv(false)
                    .set_scannable_adv(false)
                    .set_anonymous_adv(anonymous)
                    .set_directed_adv(true),
                adv_data,
                scan_data: &[],
                peer: Some(peer),
            },
        }
    }
}

/// Flags AD bit for limited LE discoverability.
pub const AD_FLAG_LE_LIMITED_DISCOVERABLE: u8 = 0b00000001;

/// Flags AD bit for general LE discoverability.
pub const LE_GENERAL_DISCOVERABLE: u8 = 0b00000010;

/// BR/EDR not supported.
pub const BR_EDR_NOT_SUPPORTED: u8 = 0b00000100;

/// Simultaneous LE and BR/EDR to same device capable (controller).
pub const SIMUL_LE_BR_CONTROLLER: u8 = 0b00001000;

/// Simultaneous LE and BR/EDR to same device capable (Host).
pub const SIMUL_LE_BR_HOST: u8 = 0b00010000;

/// Error encoding advertisement data.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdvertisementDataError {
    /// Advertisement data too long for buffer.
    TooLong,
}

/// Advertisement data structure.
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdStructure<'a> {
    /// Device flags and baseband capabilities.
    ///
    /// This should be sent if any flags apply to the device. If not (ie. the value sent would be
    /// 0), this may be omitted.
    ///
    /// Must not be used in scan response data.
    Flags(#[doc = "Flags bitset; no reserved-bit validation is performed."] u8),

    /// Incomplete List of 16-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    IncompleteServiceUuids16(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 2]]),

    /// Complete List of 16-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    CompleteServiceUuids16(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 2]]),

    /// Incomplete List of 32-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    IncompleteServiceUuids32(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 4]]),

    /// Complete List of 32-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    CompleteServiceUuids32(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 4]]),

    /// Incomplete List of 128-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    IncompleteServiceUuids128(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 16]]),

    /// Complete List of 128-bit service UUIDs.
    /// The UUID data matches the ble network's endian order (should be little endian).
    CompleteServiceUuids128(#[doc = "Borrowed little-endian UUID arrays, without a count prefix."] &'a [[u8; 16]]),

    /// Sets the Tx Power Level in dBm.
    TxPowerLevel(#[doc = "Signed transmit power in dBm."] i8),

    /// Service data with 16-bit service UUID.
    /// The UUID data matches the ble network's endian order (should be little endian).
    ServiceData16 {
        /// The 16-bit service UUID.
        uuid: [u8; 2],
        /// The associated service data. May be empty.
        data: &'a [u8],
    },

    /// Sets the full (unabbreviated) device name.
    ///
    /// This will be shown to the user when this device is found.
    CompleteLocalName(#[doc = "Borrowed name bytes; this codec does not validate UTF-8 or append NUL."] &'a [u8]),

    /// Sets the shortened device name.
    ShortenedLocalName(#[doc = "Borrowed name bytes; this codec does not validate UTF-8 or append NUL."] &'a [u8]),

    /// Set manufacturer specific data
    ManufacturerSpecificData {
        /// Company identifier.
        company_identifier: u16,
        /// Payload data.
        payload: &'a [u8],
    },

    /// An unknown or unimplemented AD structure stored as raw bytes.
    Unknown {
        /// Type byte.
        ty: u8,
        /// Raw data transmitted after the type.
        data: &'a [u8],
    },
}

impl AdStructure<'_> {
    /// Encodes `data` in order into `dest`; returns bytes used or capacity/one-byte-length error.
    /// Failure may leave earlier structures and part of the failing structure written; no rollback.
    pub fn encode_slice(data: &[AdStructure<'_>], dest: &mut [u8]) -> Result<usize, codec::Error> {
        let mut w = WriteCursor::new(dest);
        for item in data.iter() {
            item.encode(&mut w)?;
        }
        Ok(w.len())
    }

    /// Appends this AD structure to `w`; returns Ok or checked-length/cursor-capacity error.
    /// The type plus payload must fit255 bytes even in extended advertising; partial writes are retained.
    pub(crate) fn encode(&self, w: &mut WriteCursor<'_>) -> Result<(), codec::Error> {
        // The AD length remains one octet even inside extended advertising.
        let length = |count: usize, width: usize, overhead: usize| {
            count
                .checked_mul(width)
                .and_then(|n| n.checked_add(overhead))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(codec::Error::InvalidValue)
        };
        match self {
            AdStructure::Flags(flags) => {
                w.append(&[0x02, 0x01, *flags])?;
            }
            AdStructure::IncompleteServiceUuids16(uuids) => {
                w.append(&[length(uuids.len(), 2, 1)?, 0x02])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid16(*uuid))?;
                }
            }
            AdStructure::CompleteServiceUuids16(uuids) => {
                w.append(&[length(uuids.len(), 2, 1)?, 0x03])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid16(*uuid))?;
                }
            }
            AdStructure::IncompleteServiceUuids32(uuids) => {
                w.append(&[length(uuids.len(), 4, 1)?, 0x04])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid32(*uuid))?;
                }
            }
            AdStructure::CompleteServiceUuids32(uuids) => {
                w.append(&[length(uuids.len(), 4, 1)?, 0x05])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid32(*uuid))?;
                }
            }
            AdStructure::IncompleteServiceUuids128(uuids) => {
                w.append(&[length(uuids.len(), 16, 1)?, 0x06])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid128(*uuid))?;
                }
            }
            AdStructure::CompleteServiceUuids128(uuids) => {
                w.append(&[length(uuids.len(), 16, 1)?, 0x07])?;
                for uuid in uuids.iter() {
                    w.write_ref(&Uuid::Uuid128(*uuid))?;
                }
            }
            AdStructure::ShortenedLocalName(name) => {
                w.append(&[length(name.len(), 1, 1)?, 0x08])?;
                w.append(name)?;
            }
            AdStructure::CompleteLocalName(name) => {
                w.append(&[length(name.len(), 1, 1)?, 0x09])?;
                w.append(name)?;
            }
            AdStructure::TxPowerLevel(power) => {
                w.append(&[0x02, 0x0a, *power as u8])?;
            }
            AdStructure::ServiceData16 { uuid, data } => {
                w.append(&[length(data.len(), 1, 3)?, 0x16])?;
                w.write(Uuid::Uuid16(*uuid))?;
                w.append(data)?;
            }
            AdStructure::ManufacturerSpecificData {
                company_identifier,
                payload,
            } => {
                w.append(&[length(payload.len(), 1, 3)?, 0xff])?;
                w.write(*company_identifier)?;
                w.append(payload)?;
            }
            AdStructure::Unknown { ty, data } => {
                w.append(&[length(data.len(), 1, 1)?, *ty])?;
                w.append(data)?;
            }
        }
        Ok(())
    }

    /// Returns a borrowing iterator over `data`; errors are yielded per attempted structure.
    /// Zero terminators and type-only fields are errors here; flags/power use the first payload byte.
    /// Malformed/truncated input may consume a prefix, so callers should stop on the first error.
    pub fn decode(data: &[u8]) -> impl Iterator<Item = Result<AdStructure<'_>, codec::Error>> {
        AdStructureIter {
            // Remaining input and byte position.
            cursor: ReadCursor::new(data),
        }
    }
}

/// Iterator over advertisement structures.
pub struct AdStructureIter<'d> {
    /// Remaining input and byte position; failed reads can advance the position.
    cursor: ReadCursor<'d>,
}

impl<'d> AdStructureIter<'d> {
    /// Returns the next borrowed structure or an error after any already-consumed prefix.
    /// UUID-list widths must divide the payload; short manufacturer data is retained as Unknown.
    fn read(&mut self) -> Result<AdStructure<'d>, codec::Error> {
        let len: u8 = self.cursor.read()?;
        if len < 2 {
            return Err(codec::Error::InvalidValue);
        }
        let code: u8 = self.cursor.read()?;
        let data = self.cursor.slice(len as usize - 1)?;
        // These codes correspond to the table in 2.3 Common Data Types table `assigned_numbers/core/ad_types.yaml` from <https://www.bluetooth.com/specifications/assigned-numbers/>
        // Look there for more information on each.
        match code {
            // Flags
            0x01 => Ok(AdStructure::Flags(data[0])),
            // Incomplete List of 16-bit Service or Service Class UUIDs
            0x02 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::IncompleteServiceUuids16(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Complete List of 16-bit Service or Service Class UUIDs
            0x03 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::CompleteServiceUuids16(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Incomplete List of 32-bit Service or Service Class UUIDs
            0x04 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::IncompleteServiceUuids32(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Complete List of 32-bit Service or Service Class UUIDs
            0x05 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::CompleteServiceUuids32(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Incomplete List of 128-bit Service or Service Class UUIDs
            0x06 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::IncompleteServiceUuids128(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Complete List of 128-bit Service or Service Class UUIDs
            0x07 => match zerocopy::FromBytes::ref_from_bytes(data) {
                Ok(x) => Ok(AdStructure::CompleteServiceUuids128(x)),
                Err(e) => {
                    let _ = zerocopy::SizeError::from(e);
                    Err(codec::Error::InvalidValue)
                }
            },
            // Shortened Local Name
            0x08 => Ok(AdStructure::ShortenedLocalName(data)),
            // Complete Local Name
            0x09 => Ok(AdStructure::CompleteLocalName(data)),
            // Tx Power Level
            0x0a => Ok(AdStructure::TxPowerLevel(data[0] as i8)),
            /*
            0x0D Class of Device
            0x0E Simple Pairing Hash C-192
            0x0F Simple Pairing Randomizer R-192
            0x10 Device ID Device: ID Profile (when used in EIR data)
            0x10 Security Manager TK Value when used in OOB data blocks
            0x11 Security Manager Out of Band Flags
            0x12 Peripheral Connection Interval Range
            0x14 List of 16-bit Service Solicitation UUIDs
            0x15 List of 128-bit Service Solicitation UUIDs
            */
            // Service Data - 16-bit UUID
            0x16 => {
                if data.len() < 2 {
                    return Err(codec::Error::InvalidValue);
                }
                let uuid = data[0..2].try_into().unwrap();
                Ok(AdStructure::ServiceData16 { uuid, data: &data[2..] })
            }
            /*
            0x17 Public Target Address
            0x18 Random Target Address
            0x19 Appearance
            0x1A Advertising Interval
            0x1B LE Bluetooth Device Address
            0x1C LE Role
            0x1D Simple Pairing Hash C-256
            0x1E Simple Pairing Randomizer R-256
            0x1F List of 32-bit Service Solicitation UUIDs
            0x20 Service Data - 32-bit UUID
            0x21 Service Data - 128-bit UUID
            0x22 LE Secure Connections Confirmation Value
            0x23 LE Secure Connections Random Value
            0x24 URI
            0x25 Indoor Positioning
            0x26 Transport Discovery Data
            0x27 LE Supported Features
            0x28 Channel Map Update Indication
            0x29 PB-ADV
            0x2A Mesh Message
            0x2B Mesh Beacon
            0x2C BIGInfo
            0x2D Broadcast_Code
            0x2E Resolvable Set Identifier
            0x2F Advertising Interval - long
            0x30 Broadcast_Name
            0x31 Encrypted Advertising Data
            0x32 Periodic Advertising Response Timing
            0x34 Electronic Shelf Label
            0x3D 3D Information Data
            */
            // Manufacturer Specific Data
            0xff if data.len() >= 2 => Ok(AdStructure::ManufacturerSpecificData {
                company_identifier: u16::from_le_bytes([data[0], data[1]]),
                payload: &data[2..],
            }),
            ty => Ok(AdStructure::Unknown { ty, data }),
        }
    }
}

impl<'d> Iterator for AdStructureIter<'d> {
    /// Borrowed decoded structure or malformed/truncated-field error.
    type Item = Result<AdStructure<'d>, codec::Error>;
    /// Returns None only at the byte-stream end, otherwise the next decode result; errors do not fuse the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.available() == 0 {
            return None;
        }
        Some(self.read())
    }
}

#[cfg(test)]
/// Regressions for AD byte limits, directed properties and destination exhaustion.
mod tests {
    use super::*;

    /// AD length overflow is rejected independently of available destination space.
    #[test]
    fn oversized_fields_are_rejected_even_when_the_destination_is_large() {
        let data = [0x55; 255];
        let uuids16 = [[0; 2]; 128];
        let uuids32 = [[0; 4]; 64];
        let uuids128 = [[0; 16]; 16];
        for item in [
            AdStructure::CompleteLocalName(&data),
            AdStructure::ShortenedLocalName(&data),
            AdStructure::Unknown { ty: 0x40, data: &data },
            AdStructure::ManufacturerSpecificData {
                company_identifier: 1,
                payload: &data[..253],
            },
            AdStructure::ServiceData16 {
                uuid: [1, 2],
                data: &data[..253],
            },
            AdStructure::CompleteServiceUuids16(&uuids16),
            AdStructure::IncompleteServiceUuids16(&uuids16),
            AdStructure::CompleteServiceUuids32(&uuids32),
            AdStructure::IncompleteServiceUuids32(&uuids32),
            AdStructure::CompleteServiceUuids128(&uuids128),
            AdStructure::IncompleteServiceUuids128(&uuids128),
        ] {
            assert!(AdStructure::encode_slice(&[item], &mut [0; 512]).is_err(), "{item:?}");
        }
    }

    /// All directed variants retain their target address and directed HCI property.
    #[test]
    fn every_directed_variant_sets_directed_property_and_retains_peer() {
        let peer = Address::random([1, 2, 3, 4, 5, 0xc6]);
        for item in [
            Advertisement::ConnectableNonscannableDirected { peer },
            Advertisement::ConnectableNonscannableDirectedHighDuty { peer },
            Advertisement::ExtConnectableNonscannableDirected {
                peer,
                adv_data: &[2, 1, 4],
            },
            Advertisement::ExtNonconnectableScannableDirected { peer, scan_data: &[] },
            Advertisement::ExtNonconnectableNonscannableDirected {
                peer,
                adv_data: &[],
                anonymous: false,
            },
        ] {
            let raw: RawAdvertisement = item.into();
            assert!(raw.props.directed_adv(), "{item:?}");
            assert_eq!(raw.peer, Some(peer));
        }
    }

    /// The maximum one-byte AD length round-trips and undersized output returns an error.
    #[test]
    fn maximum_length_field_round_trips_and_tiny_buffer_returns_error() {
        let name = [b'x'; 254];
        let mut buffer = [0; 256];
        assert_eq!(
            AdStructure::encode_slice(&[AdStructure::CompleteLocalName(&name)], &mut buffer).unwrap(),
            256
        );
        assert!(
            matches!(AdStructure::decode(&buffer).next(), Some(Ok(AdStructure::CompleteLocalName(decoded))) if decoded == name)
        );
        assert!(AdStructure::encode_slice(&[AdStructure::Flags(4)], &mut [0; 2]).is_err());
    }

    /// A name that exceeds the remaining31-byte advertising capacity is rejected rather than truncated.
    #[test]
    fn adv_name_truncate() {
        let mut adv_data = [0; 31];
        assert!(AdStructure::encode_slice(
            &[
                AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                AdStructure::IncompleteServiceUuids16(&[[0x0f, 0x18]]),
                AdStructure::CompleteLocalName(b"12345678901234567890123"),
            ],
            &mut adv_data[..],
        )
        .is_err());
    }
}
