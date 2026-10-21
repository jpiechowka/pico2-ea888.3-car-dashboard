//! Firmware event IDs and a single-executor, bounded notification queue.
//! IDs follow [Infineon WHD events](https://infineon.github.io/wifi-host-driver/html/whd__events_8h.html).
#![allow(dead_code)]
#![allow(non_camel_case_types)]

use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Subscriber};

use crate::structs::BssInfo;

crate::util::enum_from_u8! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    /// Firmware event discriminator; unknown bytes map to `Unknown`, not a valid subscription.
    enum Event {
        #[default]
        /// Unrecognized event ID; never enabled in the host event mask.
        Unknown = 0xFF,
        /// indicates status of set SSID
        SET_SSID = 0,
        /// differentiates join IBSS from found (START) IBSS
        JOIN = 1,
        /// STA founded an IBSS or AP started a BSS
        START = 2,
        /// 802.11 AUTH request
        AUTH = 3,
        /// 802.11 AUTH indication
        AUTH_IND = 4,
        /// 802.11 DEAUTH request
        DEAUTH = 5,
        /// 802.11 DEAUTH indication
        DEAUTH_IND = 6,
        /// 802.11 ASSOC request
        ASSOC = 7,
        /// 802.11 ASSOC indication
        ASSOC_IND = 8,
        /// 802.11 REASSOC request
        REASSOC = 9,
        /// 802.11 REASSOC indication
        REASSOC_IND = 10,
        /// 802.11 DISASSOC request
        DISASSOC = 11,
        /// 802.11 DISASSOC indication
        DISASSOC_IND = 12,
        /// 802.11h Quiet period started
        QUIET_START = 13,
        /// 802.11h Quiet period ended
        QUIET_END = 14,
        /// BEACONS received/lost indication
        BEACON_RX = 15,
        /// generic link indication
        LINK = 16,
        /// TKIP MIC error occurred
        MIC_ERROR = 17,
        /// NDIS style link indication
        NDIS_LINK = 18,
        /// roam attempt occurred: indicate status & reason
        ROAM = 19,
        /// change in dot11FailedCount (txfail)
        TXFAIL = 20,
        /// WPA2 pmkid cache indication
        PMKID_CACHE = 21,
        /// current AP's TSF value went backward
        RETROGRADE_TSF = 22,
        /// AP was pruned from join list for reason
        PRUNE = 23,
        /// report AutoAuth table entry match for join attempt
        AUTOAUTH = 24,
        /// Event encapsulating an EAPOL message
        EAPOL_MSG = 25,
        /// Scan results are ready or scan was aborted
        SCAN_COMPLETE = 26,
        /// indicate to host addts fail/success
        ADDTS_IND = 27,
        /// indicate to host delts fail/success
        DELTS_IND = 28,
        /// indicate to host of beacon transmit
        BCNSENT_IND = 29,
        /// Send the received beacon up to the host
        BCNRX_MSG = 30,
        /// indicate to host loss of beacon
        BCNLOST_MSG = 31,
        /// before attempting to roam
        ROAM_PREP = 32,
        /// PFN network found event
        PFN_NET_FOUND = 33,
        /// PFN network lost event
        PFN_NET_LOST = 34,
        /// Firmware reset completion notification.
        RESET_COMPLETE = 35,
        /// Beginning of a join attempt; does not establish link success.
        JOIN_START = 36,
        /// Beginning of a roam attempt; status is reported separately.
        ROAM_START = 37,
        /// Beginning of association; does not establish authorization.
        ASSOC_START = 38,
        /// Association in an independent BSS.
        IBSS_ASSOC = 39,
        /// Radio state notification.
        RADIO = 40,
        /// PSM microcode watchdog fired
        PSM_WATCHDOG = 41,
        /// CCX association start
        CCX_ASSOC_START = 42,
        /// CCX association abort
        CCX_ASSOC_ABORT = 43,
        /// probe request received
        PROBREQ_MSG = 44,
        /// Firmware scan confirmation indication.
        SCAN_CONFIRM_IND = 45,
        /// WPA Handshake
        PSK_SUP = 46,
        /// Regulatory country selection changed.
        COUNTRY_CODE_CHANGED = 47,
        /// WMMAC excedded medium time
        EXCEEDED_MEDIUM_TIME = 48,
        /// WEP ICV error occurred
        ICV_ERROR = 49,
        /// Unsupported unicast encrypted frame
        UNICAST_DECODE_ERROR = 50,
        /// Unsupported multicast encrypted frame
        MULTICAST_DECODE_ERROR = 51,
        /// Firmware trace notification; no typed payload is decoded here.
        TRACE = 52,
        /// BT-AMP HCI event
        BTA_HCI_EVENT = 53,
        /// I/F change (for wlan host notification)
        IF = 54,
        /// P2P Discovery listen state expires
        P2P_DISC_LISTEN_COMPLETE = 55,
        /// indicate RSSI change based on configured levels
        RSSI = 56,
        /// PFN best network batching event
        PFN_BEST_BATCHING = 57,
        /// Extended firmware log notification; no typed payload is decoded here.
        EXTLOG_MSG = 58,
        /// Action frame reception
        ACTION_FRAME = 59,
        /// Action frame Tx complete
        ACTION_FRAME_COMPLETE = 60,
        /// assoc request received
        PRE_ASSOC_IND = 61,
        /// re-assoc request received
        PRE_REASSOC_IND = 62,
        /// channel adopted (xxx: obsoleted)
        CHANNEL_ADOPTED = 63,
        /// AP started
        AP_STARTED = 64,
        /// AP stopped due to DFS
        DFS_AP_STOP = 65,
        /// AP resumed due to DFS
        DFS_AP_RESUME = 66,
        /// WAI stations event
        WAI_STA_EVENT = 67,
        /// event encapsulating an WAI message
        WAI_MSG = 68,
        /// escan result event
        ESCAN_RESULT = 69,
        /// action frame off channel complete
        ACTION_FRAME_OFF_CHAN_COMPLETE = 70,
        /// probe response received
        PROBRESP_MSG = 71,
        /// P2P Probe request received
        P2P_PROBREQ_MSG = 72,
        /// Dynamic channel selection request.
        DCS_REQUEST = 73,
        /// credits for D11 FIFOs. `AC0, AC1, AC2, AC3, BC_MC, ATIM`
        FIFO_CREDIT_MAP = 74,
        /// Received action frame event WITH wl_event_rx_frame_data_t header
        ACTION_FRAME_RX = 75,
        /// Wake Event timer fired, used for wake WLAN test mode
        WAKE_EVENT = 76,
        /// Radio measurement complete
        RM_COMPLETE = 77,
        /// Synchronize TSF with the host
        HTSFSYNC = 78,
        /// request an overlay IOCTL/iovar from the host
        OVERLAY_REQ = 79,
        /// Channel switch announcement completion indication.
        CSA_COMPLETE_IND = 80,
        /// excess PM Wake Event to inform host
        EXCESS_PM_WAKE_EVENT = 81,
        /// no PFN networks around
        PFN_SCAN_NONE = 82,
        /// last found PFN network gets lost
        PFN_SCAN_ALLGONE = 83,
        /// Group temporal key installed.
        GTK_PLUMBED = 84,
        /// 802.11 ASSOC indication for NDIS only
        ASSOC_IND_NDIS = 85,
        /// 802.11 REASSOC indication for NDIS only
        REASSOC_IND_NDIS = 86,
        /// Association request information elements.
        ASSOC_REQ_IE = 87,
        /// Association response information elements.
        ASSOC_RESP_IE = 88,
        /// association recreated on resume
        ASSOC_RECREATED = 89,
        /// rx action frame event for NDIS only
        ACTION_FRAME_RX_NDIS = 90,
        /// authentication request received
        AUTH_REQ = 91,
        /// fast assoc recreation failed
        SPEEDY_RECREATE_FAIL = 93,
        /// port-specific event and payload (e.g. NDIS)
        NATIVE = 94,
        /// event for tx pkt delay suddently jump
        PKTDELAY_IND = 95,
        /// AWDL AW period starts
        AWDL_AW = 96,
        /// AWDL Master/Slave/NE master role event
        AWDL_ROLE = 97,
        /// Generic AWDL event
        AWDL_EVENT = 98,
        /// NIC AF txstatus
        NIC_AF_TXS = 99,
        /// NAN event
        NAN = 100,
        /// Received beacon frame notification.
        BEACON_FRAME_RX = 101,
        /// desired service found
        SERVICE_FOUND = 102,
        /// GAS fragment received
        GAS_FRAGMENT_RX = 103,
        /// GAS sessions all complete
        GAS_COMPLETE = 104,
        /// New device found by p2p offload
        P2PO_ADD_DEVICE = 105,
        /// device has been removed by p2p offload
        P2PO_DEL_DEVICE = 106,
        /// WNM event to notify STA enter sleep mode
        WNM_STA_SLEEP = 107,
        /// Indication of MAC tx failures (exhaustion of 802.11 retries) exceeding threshold(s)
        TXFAIL_THRESH = 108,
        /// Proximity Detection event
        PROXD = 109,
        /// AWDL RX Probe response
        AWDL_RX_PRB_RESP = 111,
        /// AWDL RX Action Frames
        AWDL_RX_ACT_FRAME = 112,
        /// AWDL Wowl nulls
        AWDL_WOWL_NULLPKT = 113,
        /// AWDL Phycal status
        AWDL_PHYCAL_STATUS = 114,
        /// AWDL OOB AF status
        AWDL_OOB_AF_STATUS = 115,
        /// Interleaved Scan status
        AWDL_SCAN_STATUS = 116,
        /// AWDL AW Start
        AWDL_AW_START = 117,
        /// AWDL AW End
        AWDL_AW_END = 118,
        /// AWDL AW Extensions
        AWDL_AW_EXT = 119,
        /// AWDL peer-cache control notification; no typed payload is decoded here.
        AWDL_PEER_CACHE_CONTROL = 120,
        /// Channel switch announcement procedure started.
        CSA_START_IND = 121,
        /// Channel switch announcement procedure finished.
        CSA_DONE_IND = 122,
        /// Channel switch announcement procedure failed.
        CSA_FAILURE_IND = 123,
        /// CCA based channel quality report
        CCA_CHAN_QUAL = 124,
        /// to report change in BSSID while roaming
        BSSID = 125,
        /// tx error indication
        TX_STAT_ERROR = 126,
        /// credit check for BCMC supported
        BCMC_CREDIT_SUPPORT = 127,
        /// psta primary interface indication
        PSTA_PRIMARY_INTF_IND = 128,
        /// Handover Request Initiated
        BT_WIFI_HANDOVER_REQ = 130,
        /// Southpaw TxInhibit notification
        SPW_TXINHIBIT = 131,
        /// FBT Authentication Request Indication
        FBT_AUTH_REQ_IND = 132,
        /// Enhancement addition for RSSI
        RSSI_LQM = 133,
        /// Full probe/beacon (IEs etc) results
        PFN_GSCAN_FULL_RESULT = 134,
        /// Significant change in rssi of bssids being tracked
        PFN_SWC = 135,
        /// a STA been authroized for traffic
        AUTHORIZED = 136,
        /// probe req with wl_event_rx_frame_data_t header
        PROBREQ_MSG_RX = 137,
        /// PFN completed scan of network list
        PFN_SCAN_COMPLETE = 138,
        /// RMC Event
        RMC_EVENT = 139,
        /// DPSTA interface indication
        DPSTA_INTF_IND = 140,
        /// RRM Event
        RRM = 141,
        /// ULP entry event
        ULP = 146,
        /// TCP Keep Alive Offload Event
        TKO = 151,
        /// authentication request received
        EXT_AUTH_REQ = 187,
        /// authentication request received
        EXT_AUTH_FRAME_RX = 188,
        /// mgmt frame Tx complete
        MGMT_FRAME_TXSTATUS = 189,
        /// highest val + 1 for range checking
        LAST = 190,
    }
}

// TODO this PubSub can probably be replaced with shared memory to make it a bit more efficient.
/// Queue of two copied messages with one publisher/subscriber; immediate publication can lose old events.
pub type EventQueue = PubSubChannel<NoopRawMutex, Message, 2, 1, 1>;
/// Exclusive subscriber borrowing the queue for `'a`; lag reflects overwritten event messages.
pub type EventSubscriber<'a> = Subscriber<'a, NoopRawMutex, Message, 2, 1, 1>;

/// Runner-to-control event channel and host-side subscription mask; confined to one executor.
pub struct Events {
    /// Bounded event delivery; subscribing twice fails.
    pub queue: EventQueue,
    /// Filters host publication, not firmware event generation.
    pub mask: SharedEventMask,
}

impl Events {
    /// Returns an empty channel with every event disabled.
    pub const fn new() -> Self {
        Self {
            queue: EventQueue::new(),
            mask: SharedEventMask::new(),
        }
    }
}

/// Decoded event identity and full-width firmware status.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Status {
    /// Recognized firmware discriminator.
    pub event_type: Event,
    /// Firmware status preserved without truncation; zero success is event-specific.
    pub status: u32,
}

/// Payloads decoded by this driver; other firmware payloads are not exposed.
#[derive(Copy, Clone)]
pub enum Payload {
    /// Event has no decoded payload (also scan completion).
    None,
    /// One copied partial-scan BSS record.
    BssInfo(#[doc = "Validated fixed-size BSS information, excluding trailing variable IEs."] BssInfo),
}

/// Copied event delivered through the bounded queue.
#[derive(Copy, Clone)]
pub struct Message {
    /// Event identity and full-width status.
    pub header: Status,
    /// Optional decoded scan record; no raw payload ownership is transferred.
    pub payload: Payload,
}

impl Message {
    /// Returns a message containing `status` and `payload` without validating their combination.
    pub fn new(status: Status, payload: Payload) -> Self {
        Self {
            header: status,
            payload,
        }
    }
}

/// Dense bitset for recognized non-sentinel event discriminators.
struct EventMask {
    /// Bit n admits event n; unused high bits stay clear.
    mask: [u32; Self::WORD_COUNT],
}

impl EventMask {
    /// Number of words needed for IDs below the exclusive LAST sentinel.
    const WORD_COUNT: usize = ((Event::LAST as u32 + (u32::BITS - 1)) / u32::BITS) as usize;

    /// Returns an all-disabled mask.
    const fn new() -> Self {
        Self {
            mask: [0; Self::WORD_COUNT],
        }
    }

    /// Returns the word index and one-hot bit for `event`, or None for either sentinel.
    fn bit(event: Event) -> Option<(usize, u32)> {
        let n = event as u32;
        // Unknown and LAST are sentinels, not subscribable firmware events.
        (n < Event::LAST as u32).then_some(((n / u32::BITS) as usize, 1 << (n % u32::BITS)))
    }

    /// Enables `event`; sentinel inputs have no effect.
    fn enable(&mut self, event: Event) {
        if let Some((word, bit)) = Self::bit(event) {
            self.mask[word] |= bit;
        }
    }

    /// Disables `event`; sentinel inputs have no effect.
    fn disable(&mut self, event: Event) {
        if let Some((word, bit)) = Self::bit(event) {
            self.mask[word] &= !bit;
        }
    }

    /// Returns whether `event` is enabled; sentinels always return false.
    fn is_enabled(&self, event: Event) -> bool {
        Self::bit(event).is_some_and(|(word, bit)| self.mask[word] & bit != 0)
    }
}

/// Interior-mutable host filter; RefCell forbids cross-thread sharing and overlapping mutation.
pub struct SharedEventMask {
    /// Synchronous borrows never cross an await in this implementation.
    mask: RefCell<EventMask>,
}

impl SharedEventMask {
    /// Returns a host mask with every event disabled.
    pub const fn new() -> Self {
        Self {
            mask: RefCell::new(EventMask::new()),
        }
    }

    /// Enables every recognized entry in `events`; duplicates and sentinels are harmless.
    pub fn enable(&self, events: &[Event]) {
        let mut mask = self.mask.borrow_mut();
        for event in events {
            mask.enable(*event);
        }
    }

    #[allow(dead_code)]
    /// Disables every entry in `events`, preserving other subscriptions.
    pub fn disable(&self, events: &[Event]) {
        let mut mask = self.mask.borrow_mut();
        for event in events {
            mask.disable(*event);
        }
    }

    /// Clears all host subscriptions, including those established by another control operation.
    pub fn disable_all(&self) {
        let mut mask = self.mask.borrow_mut();
        mask.mask = Default::default();
    }

    /// Returns whether recognized `event` is subscribed; sentinels return false.
    pub fn is_enabled(&self, event: Event) -> bool {
        let mask = self.mask.borrow();
        mask.is_enabled(event)
    }
}

impl Default for SharedEventMask {
    /// Returns an all-disabled host mask.
    fn default() -> Self {
        Self::new()
    }
}
