//! Serialized Wi-Fi control over the permanent Runner's IOCTL/event channels.
//! Control cancellation clears borrowed response pointers and subscriptions, but does not undo firmware-side effects.
//! The selected dashboard/probe uses Bluetooth and leaves these Wi-Fi association/AP operations unused.
use core::cmp::{max, min};
use core::iter::zip;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::Relaxed;

use embassy_net_driver_channel as ch;
use embassy_net_driver_channel::driver::HardwareAddress;
use embassy_time::{Duration, Timer};

use crate::consts::*;
use crate::events::{Event, EventSubscriber, Events};
use crate::fmt::Bytes;
use crate::ioctl::{IoctlState, IoctlType};
use crate::structs::*;
use crate::{PowerManagementMode, countries, events};

/// Join errors.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum JoinError {
    /// Network not found.
    NetworkNotFound,
    /// SET_SSID failure, using the recognized status code or Unknown=255 for unrecognized/wide values.
    JoinFailure(
        #[doc = "Decoded firmware status; 255 preserves failure without truncating wide status to success."] u8,
    ),
    /// Authentication failure for a secure network.
    AuthenticationFailure,
}

/// Multicast errors.
#[derive(Debug)]
pub enum AddMulticastAddressError {
    /// Not a multicast address.
    NotMulticast,
    /// No free address slots.
    NoFreeSlots,
}

/// Exclusive Wi-Fi control handle; the paired Runner must remain polled to complete IOCTLs.
/// Firmware errors can panic in Runner, waits have no local timeout, and canceled operations may leave partial configuration.
pub struct Control<'a> {
    /// Publishes hardware address into the network channel after initialization.
    state_ch: ch::StateRunner<'a>,
    /// Shared host subscriptions and bounded event stream.
    events: &'a Events,
    /// Single outstanding command/response state, serialized by mutable Control access.
    ioctl_state: &'a IoctlState,
    /// Link policy consumed by Runner when join/authentication events arrive.
    secure_network: &'a AtomicBool,
}

/// WiFi scan type.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ScanType {
    /// Active scan: the station actively transmits probes that make APs respond.
    /// Firmware actively uses the configured probe count and active dwell.
    Active,
    /// Passive scan: the station doesn't transmit any probes, just listens for beacons.
    /// Uses the configured passive dwell; elapsed time depends on available channels and firmware scheduling.
    Passive,
}

/// Scan options.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct ScanOptions {
    /// UTF-8 SSID filter, at most 32 bytes; None requests any SSID.
    pub ssid: Option<heapless::String<32>>,
    /// If set to `None`, all APs will be returned. If set to `Some`, only APs
    /// with the specified BSSID will be returned.
    pub bssid: Option<[u8; 6]>,
    /// Active probes per channel; None requests firmware default, zero is passed through.
    pub nprobes: Option<u16>,
    /// Home-channel dwell, truncated to integer milliseconds then u32; None encodes u32::MAX default.
    pub home_time: Option<Duration>,
    /// Scan type: active or passive.
    pub scan_type: ScanType,
    /// Active or passive per-channel dwell according to scan_type; None requests default.
    /// Integer milliseconds truncate to u32; u32::MAX is reduced by one to avoid the default sentinel.
    pub dwell_time: Option<Duration>,
}

impl Default for ScanOptions {
    /// Returns passive all-SSID/all-BSSID scanning with firmware-default probes and dwell periods.
    fn default() -> Self {
        Self {
            ssid: None,
            bssid: None,
            nprobes: None,
            home_time: None,
            scan_type: ScanType::Passive,
            dwell_time: None,
        }
    }
}

/// Authentication type, used in [`JoinOptions::auth`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum JoinAuth {
    /// Open network
    Open,
    /// WPA only
    Wpa,
    /// WPA2 only
    Wpa2,
    /// WPA3 only
    Wpa3,
    /// WPA2 + WPA3
    Wpa2Wpa3,
}

/// Options for [`Control::join`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct JoinOptions<'a> {
    /// Authentication type. Default `Wpa2Wpa3`.
    pub auth: JoinAuth,
    /// Enable TKIP encryption. Default false.
    pub cipher_tkip: bool,
    /// Enable AES encryption. Default true.
    pub cipher_aes: bool,
    /// Credential bytes, default empty; join accepts at most 64 for WPA/WPA2 and 128 for WPA3 paths.
    /// Borrowed bytes are not erased, and derived Debug/defmt formatting of this options value includes them.
    pub passphrase: &'a [u8],
    /// If false, `passphrase` is the human-readable passphrase string.
    /// If true, `passphrase` is the result of applying the PBKDF2 hash to the
    /// passphrase string. This makes it possible to avoid storing unhashed passwords.
    ///
    /// This is not compatible with WPA3; the caller must choose a compatible auth mode (not validated here).
    /// Default false.
    pub passphrase_is_prehashed: bool,
}

impl<'a> JoinOptions<'a> {
    /// Returns open-network options with no ciphers or credential bytes.
    pub fn new_open() -> Self {
        Self {
            auth: JoinAuth::Open,
            cipher_tkip: false,
            cipher_aes: false,
            passphrase: &[],
            passphrase_is_prehashed: false,
        }
    }

    /// Returns WPA2/WPA3 options borrowing `passphrase`, with AES enabled and TKIP disabled.
    /// Does not validate credential length or prehash/auth compatibility.
    pub fn new(passphrase: &'a [u8]) -> Self {
        let mut this = Self::default();
        this.passphrase = passphrase;
        this
    }
}

impl<'a> Default for JoinOptions<'a> {
    /// Returns WPA2/WPA3 AES options with an empty passphrase; this is not a validated join configuration.
    fn default() -> Self {
        Self {
            auth: JoinAuth::Wpa2Wpa3,
            cipher_tkip: false,
            cipher_aes: true,
            passphrase: &[],
            passphrase_is_prehashed: false,
        }
    }
}

impl<'a> Control<'a> {
    /// Returns a software handle over `state_ch`, `event_sub`, `ioctl_state` and `secure_network`; performs no IOCTL.
    pub(crate) fn new(
        state_ch: ch::StateRunner<'a>,
        event_sub: &'a Events,
        ioctl_state: &'a IoctlState,
        secure_network: &'a AtomicBool,
    ) -> Self {
        Self {
            state_ch,
            events: event_sub,
            ioctl_state,
            secure_network,
        }
    }

    /// Uploads `clm` in chunks of at most 1024 bytes with begin/end flags, then requires zero firmware load status.
    /// Empty input sends no chunk but still checks status. Cancellation retains any already-uploaded prefix.
    async fn load_clm(&mut self, clm: &[u8]) {
        /// Maximum regulatory blob bytes per IOCTL chunk, excluding name/header.
        const CHUNK_SIZE: usize = 1024;

        debug!("Downloading CLM...");

        let mut offs = 0;
        for chunk in clm.chunks(CHUNK_SIZE) {
            let mut flag = DOWNLOAD_FLAG_HANDLER_VER;
            if offs == 0 {
                flag |= DOWNLOAD_FLAG_BEGIN;
            }
            offs += chunk.len();
            if offs == clm.len() {
                flag |= DOWNLOAD_FLAG_END;
            }

            let header = DownloadHeader {
                flag,
                dload_type: DOWNLOAD_TYPE_CLM,
                len: chunk.len() as _,
                crc: 0,
            };
            let mut buf = [0; 8 + 12 + CHUNK_SIZE];
            buf[0..8].copy_from_slice(b"clmload\x00");
            buf[8..20].copy_from_slice(&header.to_bytes());
            buf[20..][..chunk.len()].copy_from_slice(&chunk);
            self.ioctl(IoctlType::Set, Ioctl::SetVar, 0, &mut buf[..8 + 12 + chunk.len()])
                .await;
        }

        // check clmload ok
        assert_eq!(self.get_iovar_u32("clmload_status").await, 0);
    }

    /// Loads regulatory `clm`, selects world-wide XX country, configures firmware events/Wi-Fi and publishes the MAC address.
    /// Requires a running paired Runner. Firmware rejection or unexpected reply lengths panic; cancellation leaves partial setup.
    pub async fn init(&mut self, clm: &[u8]) {
        self.load_clm(&clm).await;

        debug!("Configuring misc stuff...");

        // Disable tx gloming which transfers multiple packets in one request.
        // 'glom' is short for "conglomerate" which means "gather together into
        // a compact mass".
        self.set_iovar_u32("bus:txglom", 0).await;
        self.set_iovar_u32("apsta", 1).await;

        // read MAC addr.
        let mac_addr = self.address().await;
        debug!("mac addr: {:02x}", Bytes(&mac_addr));

        let country = countries::WORLD_WIDE_XX;
        let country_info = CountryInfo {
            country_abbrev: [country.code[0], country.code[1], 0, 0],
            country_code: [country.code[0], country.code[1], 0, 0],
            rev: if country.rev == 0 { -1 } else { country.rev as _ },
        };
        self.set_iovar("country", &country_info.to_bytes()).await;

        // set country takes some time, next ioctls fail if we don't wait.
        Timer::after_millis(100).await;

        // Set antenna to chip antenna
        self.ioctl_set_u32(Ioctl::SetAntdiv, 0, 0).await;

        self.set_iovar_u32("bus:txglom", 0).await;
        Timer::after_millis(100).await;
        //self.set_iovar_u32("apsta", 1).await; // this crashes, also we already did it before...??
        //Timer::after_millis(100).await;
        self.set_iovar_u32("ampdu_ba_wsize", 8).await;
        Timer::after_millis(100).await;
        self.set_iovar_u32("ampdu_mpdu", 4).await;
        Timer::after_millis(100).await;
        //self.set_iovar_u32("ampdu_rx_factor", 0).await; // this crashes

        //Timer::after_millis(100).await;

        // evts
        let mut evts = EventMask {
            iface: 0,
            events: [0xFF; 24],
        };

        // Disable spammy uninteresting events.
        evts.unset(Event::RADIO);
        evts.unset(Event::IF);
        evts.unset(Event::PROBREQ_MSG);
        evts.unset(Event::PROBREQ_MSG_RX);
        evts.unset(Event::PROBRESP_MSG);
        evts.unset(Event::PROBRESP_MSG);
        evts.unset(Event::ROAM);

        self.set_iovar("bsscfg:event_msgs", &evts.to_bytes()).await;

        Timer::after_millis(100).await;

        // set wifi up
        self.up().await;

        Timer::after_millis(100).await;

        self.ioctl_set_u32(Ioctl::SetGmode, 0, 1).await; // SET_GMODE = auto
        self.ioctl_set_u32(Ioctl::SetBand, 0, 0).await; // SET_BAND = any

        Timer::after_millis(100).await;

        self.state_ch.set_hardware_address(HardwareAddress::Ethernet(mac_addr));

        debug!("cyw43 control init done");
    }

    /// Set the WiFi interface up.
    async fn up(&mut self) {
        self.ioctl(IoctlType::Set, Ioctl::Up, 0, &mut []).await;
    }

    /// Set the interface down.
    async fn down(&mut self) {
        self.ioctl(IoctlType::Set, Ioctl::Down, 0, &mut []).await;
    }

    /// Applies firmware power-management `mode`; mode 2 updates four timing variables before the final mode command.
    /// Other modes retain those variables. Cancellation can leave only a prefix of settings applied.
    pub async fn set_power_management(&mut self, mode: PowerManagementMode) {
        // power save mode
        let mode_num = mode.mode();
        if mode_num == 2 {
            self.set_iovar_u32("pm2_sleep_ret", mode.sleep_ret_ms() as u32).await;
            self.set_iovar_u32("bcn_li_bcn", mode.beacon_period() as u32).await;
            self.set_iovar_u32("bcn_li_dtim", mode.dtim_period() as u32).await;
            self.set_iovar_u32("assoc_listen", mode.assoc() as u32).await;
        }
        self.ioctl_set_u32(Ioctl::SetPm, 0, mode_num).await;
    }

    /// Configures and joins UTF-8 `ssid` with `options`; returns event-qualified success or network/authentication failure.
    /// SSID above 32 bytes or credentials above selected buffer capacity panic, possibly after partial firmware changes.
    /// No local deadline; event queue loss can prolong the wait. Cancellation removes subscriptions without disassociating.
    pub async fn join(&mut self, ssid: &str, options: JoinOptions<'_>) -> Result<(), JoinError> {
        self.set_iovar_u32("ampdu_ba_wsize", 8).await;

        if options.auth == JoinAuth::Open {
            self.ioctl_set_u32(Ioctl::SetWsec, 0, 0).await;
            self.set_iovar_u32x2("bsscfg:sup_wpa", 0, 0).await;
            self.ioctl_set_u32(Ioctl::SetInfra, 0, 1).await;
            self.ioctl_set_u32(Ioctl::SetAuth, 0, 0).await;
            self.ioctl_set_u32(Ioctl::SetWpaAuth, 0, WPA_AUTH_DISABLED).await;
        } else {
            let mut wsec = 0;
            if options.cipher_aes {
                wsec |= WSEC_AES;
            }
            if options.cipher_tkip {
                wsec |= WSEC_TKIP;
            }
            self.ioctl_set_u32(Ioctl::SetWsec, 0, wsec).await;

            self.set_iovar_u32x2("bsscfg:sup_wpa", 0, 1).await;
            self.set_iovar_u32x2("bsscfg:sup_wpa2_eapver", 0, 0xFFFF_FFFF).await;
            self.set_iovar_u32x2("bsscfg:sup_wpa_tmo", 0, 2500).await;

            Timer::after_millis(100).await;

            let (wpa12, wpa3, auth, mfp, wpa_auth) = match options.auth {
                JoinAuth::Open => unreachable!(),
                JoinAuth::Wpa => (true, false, AUTH_OPEN, MFP_NONE, WPA_AUTH_WPA_PSK),
                JoinAuth::Wpa2 => (true, false, AUTH_OPEN, MFP_CAPABLE, WPA_AUTH_WPA2_PSK),
                JoinAuth::Wpa3 => (false, true, AUTH_SAE, MFP_REQUIRED, WPA_AUTH_WPA3_SAE_PSK),
                JoinAuth::Wpa2Wpa3 => (true, true, AUTH_SAE, MFP_CAPABLE, WPA_AUTH_WPA3_SAE_PSK),
            };

            if wpa12 {
                let mut flags = 0;
                if !options.passphrase_is_prehashed {
                    flags |= 1;
                }
                let mut pfi = PassphraseInfo {
                    len: options.passphrase.len() as _,
                    flags,
                    passphrase: [0; 64],
                };
                pfi.passphrase[..options.passphrase.len()].copy_from_slice(options.passphrase);
                Timer::after_millis(3).await;
                self.ioctl(IoctlType::Set, Ioctl::SetWsecPmk, 0, &mut pfi.to_bytes())
                    .await;
            }

            if wpa3 {
                let mut pfi = SaePassphraseInfo {
                    len: options.passphrase.len() as _,
                    passphrase: [0; 128],
                };
                pfi.passphrase[..options.passphrase.len()].copy_from_slice(options.passphrase);
                Timer::after_millis(3).await;
                self.set_iovar("sae_password", &pfi.to_bytes()).await;
            }

            self.ioctl_set_u32(Ioctl::SetInfra, 0, 1).await;
            self.ioctl_set_u32(Ioctl::SetAuth, 0, auth).await;
            self.set_iovar_u32("mfp", mfp).await;
            self.ioctl_set_u32(Ioctl::SetWpaAuth, 0, wpa_auth).await;
        }

        let mut i = SsidInfo {
            len: ssid.len() as _,
            ssid: [0; 32],
        };
        i.ssid[..ssid.len()].copy_from_slice(ssid.as_bytes());

        let secure_network = options.auth != JoinAuth::Open;
        self.secure_network.store(secure_network, Relaxed);
        self.wait_for_join(i, secure_network).await
    }

    /// Sends SSID payload `i`, then returns the terminal event result; `secure_network` selects supplicant versus SET_SSID success.
    /// Wide statuses become Unknown, never success. No timeout; subscriber exhaustion panics. All exits clear the host event mask.
    async fn wait_for_join(&mut self, i: SsidInfo, secure_network: bool) -> Result<(), JoinError> {
        let _uod = UnsubscribeOnDrop(&self.events);

        self.events.mask.enable(&[Event::SET_SSID, Event::AUTH, Event::PSK_SUP]);
        let mut subscriber = self.events.queue.subscriber().unwrap();
        // the actual join operation starts here
        // we make sure to enable events before so we don't miss any

        self.ioctl(IoctlType::Set, Ioctl::SetSsid, 0, &mut i.to_bytes()).await;

        // To complete the join on an open network, we wait for a SET_SSID event with status SUCCESS
        // For secured networks, we wait for a PSK_SUP event with status 6 "UNSOLICITED"
        let result = loop {
            let msg = subscriber.next_message_pure().await;

            let status = u8::try_from(msg.header.status)
                .map(EStatus::from)
                .unwrap_or(EStatus::Unknown);
            match (msg.header.event_type, status, secure_network) {
                // Join operation ends with SET_SSID event for open networks
                (Event::SET_SSID, EStatus::SUCCESS, false) => break Ok(()),
                (Event::SET_SSID, EStatus::NO_NETWORKS, _) => break Err(JoinError::NetworkNotFound),
                (Event::SET_SSID, status, _) if status != EStatus::SUCCESS => {
                    break Err(JoinError::JoinFailure(status as u8));
                }
                // Ignore PSK_SUP "ABORT" which is sometimes sent before successful join
                (Event::PSK_SUP, EStatus::ABORT, true) => {}
                // Event PSK_SUP with status 6 "UNSOLICITED" indicates success for secure networks
                (Event::PSK_SUP, EStatus::UNSOLICITED, true) => break Ok(()),
                // Events indicating authentication failure, possibly due to incorrect password
                (Event::PSK_SUP, _, true) | (Event::AUTH, EStatus::FAIL, true) => {
                    break Err(JoinError::AuthenticationFailure);
                }
                _ => {}
            };
        };

        match result {
            Ok(()) => debug!("JOINED"),
            Err(JoinError::JoinFailure(status)) => debug!("JOIN failed: status={}", status),
            Err(JoinError::NetworkNotFound) => debug!("JOIN failed: network not found"),
            Err(JoinError::AuthenticationFailure) => debug!("JOIN failed: authentication failure"),
        };

        result
    }

    /// Sets radio GPIO `gpio_n` to high when `gpio_en`, otherwise low; panics unless index is 0..3.
    /// On Pico 2 W these are radio-side GPIOs, not RP2350 GP numbers.
    pub async fn gpio_set(&mut self, gpio_n: u8, gpio_en: bool) {
        assert!(gpio_n < 3);
        self.set_iovar_u32x2("gpioout", 1 << gpio_n, if gpio_en { 1 << gpio_n } else { 0 })
            .await
    }

    /// Starts an open AP named `ssid` on firmware `channel`; SSID must fit 32 bytes or copying panics.
    /// Channel/regulatory admission is delegated to firmware; cancellation leaves partial configuration.
    pub async fn start_ap_open(&mut self, ssid: &str, channel: u8) {
        self.start_ap(ssid, "", Security::OPEN, channel).await;
    }

    /// Starts a protected AP using `ssid`, `passphrase` and firmware `channel`.
    /// Passphrase must contain 8..=64 bytes and SSID at most 32, otherwise panics. Firmware auth mask permits WPA/WPA2 PSK.
    pub async fn start_ap_wpa2(&mut self, ssid: &str, passphrase: &str, channel: u8) {
        self.start_ap(ssid, passphrase, Security::WPA2_AES_PSK, channel).await;
    }

    /// Reconfigures Wi-Fi as an AP using `ssid`, `passphrase`, `security` and `channel`; no rollback on cancellation.
    /// Protected passphrases outside 8..=64 bytes panic before writes; oversized SSIDs panic after interface changes.
    async fn start_ap(&mut self, ssid: &str, passphrase: &str, security: Security, channel: u8) {
        if security != Security::OPEN
            && (passphrase.as_bytes().len() < MIN_PSK_LEN || passphrase.as_bytes().len() > MAX_PSK_LEN)
        {
            panic!("Passphrase is too short or too long");
        }

        // Temporarily set wifi down
        self.down().await;

        // Turn off APSTA mode
        self.set_iovar_u32("apsta", 0).await;

        // Set wifi up again
        self.up().await;

        // Disable authentication
        self.ioctl_set_u32(Ioctl::SetAuth, 0, AUTH_OPEN).await;

        // Turn on AP mode
        self.ioctl_set_u32(Ioctl::SetAp, 0, 1).await;

        // Set SSID
        let mut i = SsidInfoWithIndex {
            index: 0,
            ssid_info: SsidInfo {
                len: ssid.as_bytes().len() as _,
                ssid: [0; 32],
            },
        };
        i.ssid_info.ssid[..ssid.as_bytes().len()].copy_from_slice(ssid.as_bytes());
        self.set_iovar("bsscfg:ssid", &i.to_bytes()).await;

        // Set channel number
        self.ioctl_set_u32(Ioctl::SetChannel, 0, channel as u32).await;

        // Set security
        self.set_iovar_u32x2("bsscfg:wsec", 0, (security as u32) & 0xFF).await;

        if security != Security::OPEN {
            self.set_iovar_u32x2("bsscfg:wpa_auth", 0, 0x0084).await; // wpa_auth = WPA2_AUTH_PSK | WPA_AUTH_PSK

            Timer::after_millis(100).await;

            // Set passphrase
            let mut pfi = PassphraseInfo {
                len: passphrase.as_bytes().len() as _,
                flags: 1, // WSEC_PASSPHRASE
                passphrase: [0; 64],
            };
            pfi.passphrase[..passphrase.as_bytes().len()].copy_from_slice(passphrase.as_bytes());
            self.ioctl(IoctlType::Set, Ioctl::SetWsecPmk, 0, &mut pfi.to_bytes())
                .await;
        }

        // Change mutlicast rate from 1 Mbps to 11 Mbps
        self.set_iovar_u32("2g_mrate", 11000000 / 500000).await;

        // Start AP
        self.set_iovar_u32x2("bss", 0, 1).await; // bss = BSS_UP
    }

    /// Stops BSS/AP operation and re-enables APSTA mode through down/up commands; cancellation may stop between these states.
    pub async fn close_ap(&mut self) {
        // Stop AP
        self.set_iovar_u32x2("bss", 0, 0).await; // bss = BSS_DOWN

        // Turn off AP mode
        self.ioctl_set_u32(Ioctl::SetAp, 0, 0).await;

        // Temporarily set wifi down
        self.down().await;

        // Turn on APSTA mode
        self.set_iovar_u32("apsta", 1).await;

        // Set wifi up again
        self.up().await;
    }

    /// Adds multicast `address` (I/G bit set), returning the occupied slot count; duplicates return the existing count.
    /// Returns NotMulticast or NoFreeSlots at ten entries. Trusts firmware count/response layout; malformed counts can panic.
    pub async fn add_multicast_address(&mut self, address: [u8; 6]) -> Result<usize, AddMulticastAddressError> {
        // The firmware seems to ignore non-multicast addresses, so let's
        // prevent the user from adding them and wasting space.
        if address[0] & 0x01 != 1 {
            return Err(AddMulticastAddressError::NotMulticast);
        }

        let mut buf = [0; 64];
        self.get_iovar("mcast_list", &mut buf).await;

        let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
        let (used, free) = buf[4..].split_at_mut(n * 6);

        if used.chunks(6).any(|a| a == address) {
            return Ok(n);
        }

        if free.len() < 6 {
            return Err(AddMulticastAddressError::NoFreeSlots);
        }

        free[..6].copy_from_slice(&address);
        let n = n + 1;
        buf[..4].copy_from_slice(&(n as u32).to_le_bytes());

        self.set_iovar_v::<80>("mcast_list", &buf).await;
        Ok(n)
    }

    /// Copies configured addresses into the prefix of `result`, returning the reported count and leaving its tail unchanged.
    /// Trusts firmware count at most ten; malformed/truncated replies are not explicitly validated and can panic.
    pub async fn list_multicast_addresses(&mut self, result: &mut [[u8; 6]; 10]) -> usize {
        let mut buf = [0; 64];
        self.get_iovar("mcast_list", &mut buf).await;

        let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
        let used = &buf[4..][..n * 6];

        for (addr, output) in zip(used.chunks(6), result.iter_mut()) {
            output.copy_from_slice(addr)
        }

        n
    }

    /// Returns firmware RSSI in dBm as a signed native-endian i32; panics unless the reply is exactly four bytes.
    /// No freshness or disconnected-value validation is applied.
    pub async fn get_rssi(&mut self) -> i32 {
        let mut rssi_buf = [0u8; 4];
        let n = self.ioctl(IoctlType::Get, Ioctl::GetRssi, 0, &mut rssi_buf).await;
        assert_eq!(n, 4);
        i32::from_ne_bytes(rssi_buf)
    }

    /// Sends `val1` then `val2` in little-endian order under NUL-terminated iovar `name`; shared 196-byte capacity applies.
    async fn set_iovar_u32x2(&mut self, name: &str, val1: u32, val2: u32) {
        let mut buf = [0; 8];
        buf[0..4].copy_from_slice(&val1.to_le_bytes());
        buf[4..8].copy_from_slice(&val2.to_le_bytes());
        self.set_iovar(name, &buf).await
    }

    /// Sends little-endian `val` under NUL-terminated iovar `name`; errors follow the shared IOCTL path.
    async fn set_iovar_u32(&mut self, name: &str, val: u32) {
        self.set_iovar(name, &val.to_le_bytes()).await
    }

    /// Returns a little-endian word for iovar `name`; panics unless exactly four bytes are returned.
    async fn get_iovar_u32(&mut self, name: &str) -> u32 {
        let mut buf = [0; 4];
        let len = self.get_iovar(name, &mut buf).await;
        assert_eq!(len, 4);
        u32::from_le_bytes(buf)
    }

    /// Sends raw `val` under NUL-terminated `name`; combined name+terminator+value must fit 196 bytes or panics.
    async fn set_iovar(&mut self, name: &str, val: &[u8]) {
        self.set_iovar_v::<196>(name, val).await
    }

    /// Sends raw `val` under NUL-terminated `name` using BUFSIZE stack/future bytes; insufficient capacity panics.
    /// Logs name and length only, since values can contain credentials. Cancellation clears host ownership, not device effects.
    async fn set_iovar_v<const BUFSIZE: usize>(&mut self, name: &str, val: &[u8]) {
        debug!("iovar set {} ({} bytes)", name, val.len());

        let mut buf = [0; BUFSIZE];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        buf[name.len()] = 0;
        buf[name.len() + 1..][..val.len()].copy_from_slice(val);

        let total_len = name.len() + 1 + val.len();
        self.ioctl_inner(IoctlType::Set, Ioctl::SetVar, 0, &mut buf[..total_len])
            .await;
    }

    /// Fetches iovar `name` into the prefix of `res`, returning copied count capped to res capacity; tail remains unchanged.
    /// NUL-terminated name and requested response must each fit 64 bytes or slicing panics. Firmware rejection is not returned as Result.
    async fn get_iovar(&mut self, name: &str, res: &mut [u8]) -> usize {
        debug!("iovar get {}", name);

        let mut buf = [0; 64];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        buf[name.len()] = 0;

        let total_len = max(name.len() + 1, res.len());
        let res_len = self
            .ioctl_inner(IoctlType::Get, Ioctl::GetVar, 0, &mut buf[..total_len])
            .await;

        let out_len = min(res.len(), res_len);
        res[..out_len].copy_from_slice(&buf[..out_len]);
        out_len
    }

    /// Sends little-endian `val` with Set `cmd` on `iface`, discarding the response count.
    async fn ioctl_set_u32(&mut self, cmd: Ioctl, iface: u32, val: u32) {
        let mut buf = val.to_le_bytes();
        self.ioctl(IoctlType::Set, cmd, iface, &mut buf).await;
    }

    /// Sends `kind`/`cmd`/`iface` using in-place request/response `buf`; returns received byte count.
    /// Logs command metadata only. Requires the paired runner and inherits its panic/partial-effect/no-timeout behavior.
    async fn ioctl(&mut self, kind: IoctlType, cmd: Ioctl, iface: u32, buf: &mut [u8]) -> usize {
        if kind == IoctlType::Set {
            debug!("ioctl set {:?} iface {} ({} bytes)", cmd, iface, buf.len());
        }
        let n = self.ioctl_inner(kind, cmd, iface, buf).await;
        n
    }

    /// Publishes `kind`/`cmd`/`iface` with borrowed `buf`, returning response count after runner completion.
    /// Cancellation clears the stored raw pointer before buf expires, but sends no controller cancellation and wakes no waiter.
    async fn ioctl_inner(&mut self, kind: IoctlType, cmd: Ioctl, iface: u32, buf: &mut [u8]) -> usize {
        /// Owns cleanup of the currently published borrowed IOCTL buffer.
        struct CancelOnDrop<'a>(
            #[doc = "Shared request slot to clear if the caller future is dropped."] &'a IoctlState,
        );

        impl CancelOnDrop<'_> {
            /// Consumes the guard after normal completion without clearing the completed response count.
            fn defuse(self) {
                core::mem::forget(self);
            }
        }

        impl Drop for CancelOnDrop<'_> {
            /// Clears host raw-buffer state; firmware commands already sent remain in effect.
            fn drop(&mut self) {
                self.0.cancel_ioctl();
            }
        }

        let ioctl = CancelOnDrop(self.ioctl_state);
        let resp_len = ioctl.0.do_ioctl(kind, cmd, iface, buf).await;
        ioctl.defuse();

        resp_len
    }

    /// Starts scanning with `scan_opts`, returning a Scanner over the bounded host event queue.
    /// Poll promptly: overwritten events are lost. No scan deadline/abort is supplied; dropping Scanner only disables host subscriptions.
    /// Subscriber exhaustion panics. Cancellation clears the host mask and subscriber but may leave firmware scanning.
    pub async fn scan(&mut self, scan_opts: ScanOptions) -> Scanner<'_> {
        /// Firmware scan encoding that transmits probe requests.
        const SCANTYPE_ACTIVE: u8 = 0;
        /// Firmware scan encoding that only listens for beacons.
        const SCANTYPE_PASSIVE: u8 = 1;

        let dwell_time = match scan_opts.dwell_time {
            None => !0,
            Some(t) => {
                let mut t = t.as_millis() as u32;
                if t == !0 {
                    t = !0 - 1;
                }
                t
            }
        };

        let mut active_time = !0;
        let mut passive_time = !0;
        let scan_type = match scan_opts.scan_type {
            ScanType::Active => {
                active_time = dwell_time;
                SCANTYPE_ACTIVE
            }
            ScanType::Passive => {
                passive_time = dwell_time;
                SCANTYPE_PASSIVE
            }
        };

        let scan_params = ScanParams {
            version: 1,
            action: 1,
            sync_id: 1,
            ssid_len: scan_opts.ssid.as_ref().map(|e| e.as_bytes().len() as u32).unwrap_or(0),
            ssid: scan_opts
                .ssid
                .map(|e| {
                    let mut ssid = [0; 32];
                    ssid[..e.as_bytes().len()].copy_from_slice(e.as_bytes());
                    ssid
                })
                .unwrap_or([0; 32]),
            bssid: scan_opts.bssid.unwrap_or([0xff; 6]),
            bss_type: 2,
            scan_type,
            nprobes: scan_opts.nprobes.unwrap_or(!0).into(),
            active_time,
            passive_time,
            home_time: scan_opts.home_time.map(|e| e.as_millis() as u32).unwrap_or(!0),
            channel_num: 0,
            channel_list: [0; 1],
            reserved: [0; 2],
        };

        let subscription = UnsubscribeOnDrop(&self.events);
        self.events.mask.enable(&[Event::ESCAN_RESULT]);
        let subscriber = self.events.queue.subscriber().unwrap();
        self.set_iovar_v::<256>("escan", &scan_params.to_bytes()).await;
        subscription.defuse();

        Scanner {
            subscriber,
            events: &self.events,
        }
    }
    /// Sends Disassoc for interface zero; completion is the command response, not a separately observed link-down event.
    pub async fn leave(&mut self) {
        self.ioctl(IoctlType::Set, Ioctl::Disassoc, 0, &mut []).await;
        info!("Disassociated")
    }

    /// Returns six MAC-address bytes from firmware; panics if the response length differs from six.
    pub async fn address(&mut self) -> [u8; 6] {
        let mut mac_addr = [0; 6];
        assert_eq!(self.get_iovar("cur_etheraddr", &mut mac_addr).await, 6);
        mac_addr
    }
}

/// Clears subscriptions on completion/cancel without issuing a radio disconnect.
struct UnsubscribeOnDrop<'a>(#[doc = "Shared event mask owned by the current control operation."] &'a Events);

impl UnsubscribeOnDrop<'_> {
    /// Transfers mask cleanup to the returned Scanner after successful startup.
    fn defuse(self) {
        core::mem::forget(self);
    }
}

impl Drop for UnsubscribeOnDrop<'_> {
    /// Clears every host subscription, not just the join-specific events.
    fn drop(&mut self) {
        self.0.mask.disable_all();
    }
}

/// WiFi network scanner.
pub struct Scanner<'a> {
    /// Sole bounded event subscriber, released on drop.
    subscriber: EventSubscriber<'a>,
    /// Shared host mask cleared at terminal event or drop; does not own radio scan cancellation.
    events: &'a Events,
}

impl Scanner<'_> {
    /// Returns the next fixed BSS record for a PARTIAL event; None for terminal status or missing typed payload.
    /// Terminal status clears all host subscriptions. Cancellation while waiting consumes no event; no local deadline exists.
    pub async fn next(&mut self) -> Option<BssInfo> {
        let event = self.subscriber.next_message_pure().await;
        if event.header.status != EStatus::PARTIAL {
            self.events.mask.disable_all();
            return None;
        }

        if let events::Payload::BssInfo(bss) = event.payload {
            Some(bss)
        } else {
            None
        }
    }
}

impl Drop for Scanner<'_> {
    /// Clears all host event subscriptions; does not issue a device scan-abort command.
    fn drop(&mut self) {
        self.events.mask.disable_all();
    }
}

#[cfg(test)]
/// Control-state regressions driven by the real IOCTL/event handoff without a radio.
mod tests {
    extern crate std;
    use super::*;
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };
    use embassy_futures::block_on;

    #[cfg(feature = "log")]
    /// Captures actual logging calls to verify credential-bearing commands retain metadata only.
    struct CaptureLog(
        #[doc = "Mutex-protected formatted records captured by this test process."]
        std::sync::Mutex<std::vec::Vec<std::string::String>>,
    );

    #[cfg(feature = "log")]
    impl log::Log for CaptureLog {
        /// Returns true for every `metadata` level so payload leaks cannot hide behind filtering.
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            let _ = metadata;
            true
        }
        /// Stores formatted `record` text for the regression; poisoned capture state panics.
        fn log(&self, record: &log::Record<'_>) {
            self.0.lock().unwrap().push(std::format!("{}", record.args()));
        }
        /// No buffered external sink exists.
        fn flush(&self) {}
    }

    #[cfg(feature = "log")]
    /// Process-wide test logger; only this regression installs it.
    static CAPTURE: CaptureLog = CaptureLog(std::sync::Mutex::new(std::vec::Vec::new()));

    #[cfg(feature = "log")]
    /// Real iovar and IOCTL logging never formats the supplied secret bytes.
    #[test]
    fn credential_commands_log_metadata_without_payload() {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        let mut state = crate::State::new();
        let (runner, _) = ch::new(&mut state.net.ch, HardwareAddress::Ethernet([0; 6]));
        let mut control = Control::new(
            runner.state_runner(),
            &state.net.events,
            &state.ioctl_state,
            &state.net.secure_network,
        );
        let mut cx = Context::from_waker(Waker::noop());
        let mut secret = [0xda, 0x7a, 0x91, 0xee];
        {
            let mut request = pin!(control.set_iovar("sae_password", &secret));
            assert!(request.as_mut().poll(&mut cx).is_pending());
        }
        {
            let mut request = pin!(control.ioctl(IoctlType::Set, Ioctl::SetWsecPmk, 0, &mut secret));
            assert!(request.as_mut().poll(&mut cx).is_pending());
        }
        let records = CAPTURE.0.lock().unwrap();
        assert!(records.iter().any(|record| record.contains("sae_password")));
        assert!(records.iter().any(|record| record.contains("SetWsecPmk")));
        assert!(
            records
                .iter()
                .all(|record| !record.contains("0xda") && !record.contains("0xee")),
            "credential bytes appeared in logs"
        );
    }

    /// Canceling scan startup releases its IOCTL buffer, subscriber and enabled host mask.
    #[test]
    fn scan_start_cancellation_releases_host_state() {
        let mut state = crate::State::new();
        let (runner, _) = ch::new(&mut state.net.ch, HardwareAddress::Ethernet([0; 6]));
        let mut control = Control::new(
            runner.state_runner(),
            &state.net.events,
            &state.ioctl_state,
            &state.net.secure_network,
        );
        let mut cx = Context::from_waker(Waker::noop());
        {
            let mut start = pin!(control.scan(ScanOptions::default()));
            assert!(start.as_mut().poll(&mut cx).is_pending());
            assert!(state.net.events.mask.is_enabled(Event::ESCAN_RESULT));
            assert!(state.net.events.queue.subscriber().is_err());
        }
        assert!(!state.net.events.mask.is_enabled(Event::ESCAN_RESULT));
        assert!(state.net.events.queue.subscriber().is_ok());
        assert!(
            pin!(state.ioctl_state.wait_pending())
                .as_mut()
                .poll(&mut cx)
                .is_pending()
        );

        let mut start = pin!(control.scan(ScanOptions::default()));
        assert!(start.as_mut().poll(&mut cx).is_pending());
        let _request = block_on(state.ioctl_state.wait_pending());
        state.ioctl_state.ioctl_done(&[]);
        let Poll::Ready(scanner) = start.as_mut().poll(&mut cx) else {
            panic!("scan did not start")
        };
        assert!(state.net.events.mask.is_enabled(Event::ESCAN_RESULT));
        drop(scanner);
        assert!(!state.net.events.mask.is_enabled(Event::ESCAN_RESULT));
        assert!(state.net.events.queue.subscriber().is_ok());
    }

    /// Wide statuses cannot alias open-network success or secure supplicant success through byte truncation.
    #[test]
    fn join_rejects_wide_status_aliases() {
        for (event_type, status, secure, succeeds) in [
            (Event::SET_SSID, 0, false, true),
            (Event::SET_SSID, 256, false, false),
            (Event::SET_SSID, u32::MAX, false, false),
            (Event::PSK_SUP, 6, true, true),
            (Event::PSK_SUP, 262, true, false),
        ] {
            let mut state = crate::State::new();
            let (runner, _) = ch::new(&mut state.net.ch, HardwareAddress::Ethernet([0; 6]));
            let mut control = Control::new(
                runner.state_runner(),
                &state.net.events,
                &state.ioctl_state,
                &state.net.secure_network,
            );
            let mut join = pin!(control.wait_for_join(SsidInfo { len: 0, ssid: [0; 32] }, secure));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(join.as_mut().poll(&mut cx).is_pending());
            let _request = block_on(state.ioctl_state.wait_pending());
            state.ioctl_state.ioctl_done(&[]);
            state
                .net
                .events
                .queue
                .immediate_publisher()
                .publish_immediate(events::Message::new(
                    events::Status { event_type, status },
                    events::Payload::None,
                ));
            let Poll::Ready(result) = join.as_mut().poll(&mut cx) else {
                panic!("terminal join event was not consumed")
            };
            assert_eq!(result.is_ok(), succeeds, "event={event_type:?}, status={status}");
        }
    }
}
