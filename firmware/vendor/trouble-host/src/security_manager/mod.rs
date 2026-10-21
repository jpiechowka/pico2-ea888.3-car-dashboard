#![warn(missing_docs)]
//! Bluetooth Security Manager: one active pairing state machine, bounded deferred HCI work and bonds.
//!
//! Synchronous methods use RefCell borrows and a NoopRawMutex event queue; callers must serialize
//! access on the host executor and avoid reentrant borrows. A queued encryption request is not
//! encryption completion. HCI completion updates each connection's admitted security level.
//! See [Bluetooth SMP](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html).
// ([Vol 3] Part H, Section 3.5.5)

mod constants;
pub(crate) mod crypto;
mod pairing;
mod types;
use core::cell::{Ref, RefCell};
use core::future::{poll_fn, Future};
use core::task::Poll;

use bt_hci::event::le::{LeEventKind, LeEventPacket, LeLongTermKeyRequest};
use bt_hci::event::{EncryptionChangeV1, EncryptionChangeV2, EncryptionKeyRefreshComplete, EventKind, EventPacket};
#[cfg(feature = "central")]
use bt_hci::param::BdAddr;
use bt_hci::param::{ConnHandle, EncryptionEnabledLevel, LeConnRole};
use bt_hci::FromHciBytes;
pub use crypto::{IdentityResolvingKey, LongTermKey};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::waitqueue::WakerRegistration;
use embassy_time::{Instant, TimeoutError, WithTimeout};
use heapless::VecView;
pub use pairing::OobData;
use rand_chacha::ChaCha12Rng;
use rand_core::SeedableRng;
use types::Command;
pub use types::{PassKey, Reason};

use crate::connection::SecurityLevel;
use crate::connection_manager::{ConnectionManager, ConnectionStorage};
use crate::pdu::Pdu;
use crate::prelude::ConnectionEvent;
use crate::security_manager::pairing::{Pairing, PairingOps};
#[cfg(feature = "legacy-pairing")]
use crate::security_manager::types::AuthReq;
use crate::security_manager::types::BondingFlag;
use crate::types::l2cap::L2CAP_CID_LE_U_SECURITY_MANAGER;
use crate::{Address, Error, Identity, IoCapabilities, PacketPool};

/// Events of interest to the security manager
pub(crate) enum SecurityEventData {
    /// A long term key request has been issued (handle, ediv, rand)
    SendLongTermKey(
        /// Controller connection requesting the key.
        ConnHandle,
        /// Legacy encrypted diversifier; zero for Secure Connections.
        u16,
        /// Eight wire-order legacy random bytes; zero for Secure Connections.
        [u8; 8],
    ),
    /// Enable encryption on channel
    EnableEncryption(
        /// Controller connection to encrypt.
        ConnHandle,
        /// Key, identity and intended security properties to submit to HCI.
        BondInformation,
    ),
    /// Pairing timeout
    Timeout,
    /// Pairing timer changed
    TimerChange,
    /// A new bond was stored during pairing
    BondAdded(
        /// Connection whose bond gained identity resolution data.
        ConnHandle,
        /// Identity/IRK to synchronize with controller privacy state.
        crate::Identity,
    ),
}

/// Bond Information
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct BondInformation {
    /// Long Term Key (LTK)
    pub ltk: LongTermKey,
    /// Peer identity
    pub identity: Identity,
    /// True if this bond information is from a bonded pairing
    pub is_bonded: bool,
    /// Security level of this long term key.
    pub security_level: SecurityLevel,
    #[cfg(feature = "legacy-pairing")]
    /// Encrypted diversifier (zero for LESC; legacy generation can also produce zero).
    pub ediv: u16,
    #[cfg(feature = "legacy-pairing")]
    /// Eight wire-order random bytes (zero for LESC; legacy random generation does not exclude zero).
    pub rand: [u8; 8],
    #[cfg(feature = "legacy-pairing")]
    /// Negotiated encryption key length in bytes (16 for LESC)
    pub encryption_key_len: u8,
}

impl BondInformation {
    /// Returns a record binding `identity` to `ltk`, claimed `security_level` and retention flag
    /// `is_bonded`; inputs are not validated. Legacy fields default to EDIV/Rand zero and 16-byte key.
    pub fn new(identity: Identity, ltk: LongTermKey, security_level: SecurityLevel, is_bonded: bool) -> Self {
        Self {
            ltk,
            identity,
            is_bonded,
            security_level,
            #[cfg(feature = "legacy-pairing")]
            ediv: 0,
            #[cfg(feature = "legacy-pairing")]
            rand: [0; 8],
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 16,
        }
    }
}

impl core::fmt::Display for BondInformation {
    /// Writes identity and unredacted LTK to `f`; Returns its write result. Not safe for secret-free logs.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Identity {:?} LTK {}", self.identity, self.ltk)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for BondInformation {
    /// Writes identity and unredacted LTK to `fmt`; do not use in secret-free logs.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Identity {:?} LTK {}", self.identity, self.ltk);
    }
}

/// Security manager data
struct SecurityManagerData {
    /// Local device address
    local_address: Option<Address>,
    /// Local Identity Resolving Key (set when privacy is enabled)
    local_irk: Option<IdentityResolvingKey>,
}

impl SecurityManagerData {
    /// Returns unconfigured identity/privacy state with no local address or IRK.
    fn new() -> Self {
        Self {
            local_address: None,
            local_irk: None,
        }
    }
}

/// Inserts `bond_information` into `bond`, replacing the first matching identity when present.
/// Returns OutOfMemory without insertion when a new identity exceeds capacity; replacement can succeed
/// in a full store. This in-memory update does not persist anything outside the host resources.
fn add_bond(bond: &mut VecView<BondInformation>, bond_information: BondInformation) -> Result<(), Error> {
    trace!("[security manager] Add bond for {:?}", bond_information.identity.addr);
    if let Some(idx) = bond
        .iter()
        .position(|b| bond_information.identity.match_identity(&b.identity))
    {
        bond[idx] = bond_information;
    } else {
        bond.push(bond_information).map_err(|_| Error::OutOfMemory)?;
    }
    Ok(())
}

/// Packet structure for sending security manager protocol (SMP) commands
struct TxPacket<P: PacketPool> {
    /// Underlying packet
    packet: P::Packet,
    /// Command to send
    command: Command,
}

impl<P: PacketPool> TxPacket<P> {
    /// Size of L2CAP header and command
    const HEADER_SIZE: usize = 5;

    /// Writes L2CAP length/CID and `command` into owned `packet`; Returns the packet wrapper.
    /// Payload bytes remain untouched. Panics if storage is shorter than five bytes;
    /// payload access additionally requires capacity for the command's complete body.
    pub fn new(mut packet: P::Packet, command: Command) -> Result<Self, Error> {
        let packet_data = packet.as_mut();
        let smp_size = command.payload_size() + 1;
        packet_data[..2].copy_from_slice(&(smp_size).to_le_bytes());
        packet_data[2..4].copy_from_slice(&L2CAP_CID_LE_U_SECURITY_MANAGER.to_le_bytes());
        packet_data[4] = command.into();
        Ok(Self { packet, command })
    }
    /// Returns the opcode selected when this packet was constructed.
    pub fn command(&self) -> Command {
        self.command
    }

    /// Returns the exact command-body slice; panics if the pool packet is undersized.
    pub fn payload(&self) -> &[u8] {
        &self.packet.as_ref()[Self::HEADER_SIZE..Self::HEADER_SIZE + usize::from(self.command.payload_size())]
    }
    /// Returns the exact mutable command-body slice, excluding headers; panics on undersized storage.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.packet.as_mut()[Self::HEADER_SIZE..Self::HEADER_SIZE + usize::from(self.command.payload_size())]
    }
    /// Returns total used bytes: four-byte L2CAP header, opcode and fixed command payload.
    pub fn total_size(&self) -> usize {
        usize::from(self.command.payload_size()) + Self::HEADER_SIZE
    }
    /// Consumes this wrapper; Returns the owned packet PDU with its used length, without transmission.
    pub fn into_pdu(self) -> Pdu<P::Packet> {
        let len = self.total_size();
        Pdu::new(self.packet, len)
    }
}

/// Inner mutable state of the security manager
struct Inner {
    /// Random generator
    rng: ChaCha12Rng,
    /// Persistent LESC keypair (generated once when RNG is seeded)
    secret_key: crypto::SecretKey,
    /// Corresponding public key
    public_key: crypto::PublicKey,
    /// Security manager data
    state: SecurityManagerData,
    /// State of an ongoing pairing
    pairing_sm: Option<Pairing>,
    /// Waker for pairing finished
    finished_waker: WakerRegistration,
    /// Io capabilities
    io_capabilities: IoCapabilities,
    /// Optional fixed passkey used by supported display-side paths; no range validation here.
    /// Secure Connections central display currently generates its own random value.
    passkey: Option<u32>,
    /// When true, reject legacy pairing even if the feature is compiled in
    #[cfg(feature = "legacy-pairing")]
    secure_connections_only: bool,
}

/// Parsed SMP command data extracted from an L2CAP PDU
struct SmpCommand<'a> {
    /// Validated opcode whose exact payload size has been checked.
    command: Command,
    /// Borrowed command-body bytes without opcode/L2CAP header.
    payload: &'a [u8],
    /// Connection handle associated with this received PDU.
    handle: ConnHandle,
    /// Connection's resolved or observed peer identity for bond lookup.
    peer_identity: Identity,
}

impl Inner {
    /// Returns true when no pairing machine exists or its result is terminal, including failure.
    fn is_idle(&self) -> bool {
        self.pairing_sm.as_ref().map(|sm| sm.result().is_some()).unwrap_or(true)
    }

    /// Returns `storage`'s local RPA when recorded, otherwise the configured identity address.
    /// Missing both returns InvalidValue; pairing must bind the actual link address.
    fn connection_local_address<P>(&self, storage: &ConnectionStorage<P>) -> Result<Address, Error> {
        if let Some(rpa) = storage.resolvable_addrs.local {
            return Ok(Address {
                kind: bt_hci::param::AddrKind::RANDOM,
                addr: rpa,
            });
        }
        self.state.local_address.ok_or(Error::InvalidValue)
    }

    /// Returns `storage`'s recorded peer RPA, otherwise its identity address, for pairing calculations.
    fn connection_peer_address<P>(storage: &ConnectionStorage<P>) -> Address {
        if let Some(rpa) = storage.resolvable_addrs.peer {
            Address {
                kind: bt_hci::param::AddrKind::RANDOM,
                addr: rpa,
            }
        } else {
            storage.peer_identity.addr
        }
    }

    /// Returns `sm` only if its bound peer address matches the actual link peer in `storage`.
    /// None leaves an unrelated pairing machine untouched; matching uses normalized Address equality.
    fn pairing_sm_for<'a, P>(sm: Option<&'a mut Pairing>, storage: &ConnectionStorage<P>) -> Option<&'a mut Pairing> {
        let expected = Self::connection_peer_address(storage);
        sm.filter(|sm| sm.peer_address() == expected)
    }

    /// Advances responder pairing for `cmd` on `storage`, using `bonds`, deferred `events` and
    /// `connections` for side effects. Returns state, validation, crypto or queue errors; earlier
    /// effects remain. A fresh request may install/switch a machine; SC-only mode rejects legacy.
    fn handle_peripheral<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        cmd: &SmpCommand<'_>,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let SmpCommand {
            command,
            payload,
            handle,
            peer_identity,
        } = *cmd;
        if self.is_idle() && command == Command::PairingRequest {
            let local_address = self.connection_local_address(storage)?;
            let peer_address = Self::connection_peer_address(storage);
            let local_io = self.io_capabilities;

            #[cfg(feature = "legacy-pairing")]
            {
                // Check if peer supports SC by peeking at PairingRequest AuthReq byte,
                // or if peer requests a key size smaller than LESC requires
                let use_legacy = command == Command::PairingRequest
                    && payload.len() >= 4
                    && (!AuthReq::from(payload[2]).secure_connection()
                        || payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS);

                if use_legacy && self.secure_connections_only {
                    if payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS {
                        return Err(Error::Security(Reason::EncryptionKeySize));
                    }
                    return Err(Error::Security(Reason::AuthenticationRequirements));
                }

                if use_legacy {
                    self.pairing_sm = Some(Pairing::new_legacy_peripheral(local_address, peer_address, local_io));
                } else {
                    self.pairing_sm = Some(Pairing::new_peripheral(local_address, peer_address, local_io));
                }
            }
            #[cfg(not(feature = "legacy-pairing"))]
            {
                self.pairing_sm = Some(Pairing::new_peripheral(local_address, peer_address, local_io));
            }
        }

        // Check if we need to switch from LESC to legacy peripheral
        // when receiving a PairingRequest without SC flag or with insufficient key size
        #[cfg(feature = "legacy-pairing")]
        if command == Command::PairingRequest
            && payload.len() >= 4
            && (!AuthReq::from(payload[2]).secure_connection()
                || payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS)
            && self.pairing_sm.as_ref().is_some_and(|sm| {
                sm.peer_address() == Self::connection_peer_address(storage) && sm.is_lesc_peripheral()
            })
        {
            if self.secure_connections_only {
                if payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS {
                    return Err(Error::Security(Reason::EncryptionKeySize));
                }
                return Err(Error::Security(Reason::AuthenticationRequirements));
            }
            if let Some(old) = self.pairing_sm.take() {
                self.pairing_sm = Some(old.switch_to_legacy_peripheral()?);
            }
        }

        let Some(pairing_sm) = Self::pairing_sm_for(self.pairing_sm.as_mut(), storage) else {
            return Err(Error::InvalidState);
        };

        if pairing_sm.is_central() {
            return Err(Error::InvalidState);
        }

        let mut ops = PairingOpsImpl {
            bonds,
            events,
            secret_key: &self.secret_key,
            public_key: &self.public_key,
            passkey: self.passkey,
            conn_handle: handle,
            connections,
            storage,
            peer_identity,
            state: &self.state,
        };
        pairing_sm.handle_l2cap_command(command, payload, &mut ops, &mut self.rng)
    }

    /// Advances initiator pairing for `cmd` on `storage`, using `bonds`, deferred `events` and
    /// `connections`. Returns state/admission/crypto/queue errors without undoing earlier effects.
    /// Security Request can install an idle machine; legacy downgrade obeys SC-only policy.
    fn handle_central<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        cmd: &SmpCommand<'_>,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let SmpCommand {
            command,
            payload,
            handle,
            peer_identity,
        } = *cmd;
        if self.is_idle() && command == Command::SecurityRequest {
            self.pairing_sm = Some(Pairing::new_central(
                self.connection_local_address(storage)?,
                Self::connection_peer_address(storage),
                self.io_capabilities,
            ));
        }

        // Check if we need to switch from LESC to legacy central
        // when receiving a PairingResponse without SC flag or with insufficient key size
        #[cfg(feature = "legacy-pairing")]
        if command == Command::PairingResponse
            && payload.len() >= 4
            && (!AuthReq::from(payload[2]).secure_connection()
                || payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS)
            && self
                .pairing_sm
                .as_ref()
                .is_some_and(|sm| sm.peer_address() == Self::connection_peer_address(storage) && sm.is_lesc_central())
        {
            if self.secure_connections_only {
                if payload[3] < crate::security_manager::constants::ENCRYPTION_KEY_SIZE_128_BITS {
                    return Err(Error::Security(Reason::EncryptionKeySize));
                }
                return Err(Error::Security(Reason::AuthenticationRequirements));
            }
            if let Some(old) = self.pairing_sm.take() {
                self.pairing_sm = Some(old.switch_to_legacy_central()?);
            }
        }

        let Some(pairing_sm) = Self::pairing_sm_for(self.pairing_sm.as_mut(), storage) else {
            return Err(Error::InvalidState);
        };

        if !pairing_sm.is_central() {
            return Err(Error::InvalidState);
        }

        let mut ops = PairingOpsImpl {
            bonds,
            events,
            secret_key: &self.secret_key,
            public_key: &self.public_key,
            passkey: self.passkey,
            conn_handle: handle,
            connections,
            storage,
            peer_identity,
            state: &self.state,
        };
        pairing_sm.handle_l2cap_command(command, payload, &mut ops, &mut self.rng)
    }

    /// Applies `pairing_event` only to the peer bound to `storage`, through `connections`, `bonds`
    /// and `events`. Returns InvalidState for another peer or the transition error. Successful
    /// handling restarts the timeout; the TimerChange notification is best-effort.
    fn handle_pairing_event<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        pairing_event: pairing::Event,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        if let Some(sm) = Self::pairing_sm_for(self.pairing_sm.as_mut(), storage) {
            let mut ops = PairingOpsImpl {
                bonds,
                events,
                secret_key: &self.secret_key,
                public_key: &self.public_key,
                passkey: self.passkey,
                peer_identity: storage.peer_identity,
                conn_handle: storage.handle,
                connections,
                storage,
                state: &self.state,
            };
            let res = sm.handle_event(pairing_event, &mut ops, &mut self.rng);
            if res.is_ok() {
                sm.reset_timeout();
                let _ = events.try_send(SecurityEventData::TimerChange);
            }
            res
        } else {
            Err(Error::InvalidState)
        }
    }

    /// Applies a successful HCI status with actual `encrypted` state to mutable `storage`.
    /// Uses matching pairing or `bonds`, with `connections`/`events` for follow-up work; Returns
    /// the pairing transition error if any. Updates admission before best-effort Encrypted delivery;
    /// an unrelated peer's pairing is preserved. The outer HCI handler revokes/disconnects on Off.
    fn handle_encryption_success<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        encrypted: bool,
        connections: &ConnectionManager<'_, P>,
        storage: &mut ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let mut bond = None;
        let res: Result<(), Error> = if let Some(sm) = Self::pairing_sm_for(self.pairing_sm.as_mut(), storage) {
            let mut ops = PairingOpsImpl {
                bonds,
                events,
                secret_key: &self.secret_key,
                public_key: &self.public_key,
                passkey: self.passkey,
                peer_identity: storage.peer_identity,
                connections,
                storage,
                conn_handle: storage.handle,
                state: &self.state,
            };
            let res = sm.handle_event(pairing::Event::LinkEncryptedResult(encrypted), &mut ops, &mut self.rng);
            if res.is_ok() {
                storage.security_level = sm.security_level();
                storage.bond_rejected = false;
                #[cfg(feature = "legacy-pairing")]
                if let Some(bond) = sm.bond_information() {
                    storage.encryption_key_len = bond.encryption_key_len;
                }
            }
            if sm.result().is_some() {
                self.finished_waker.wake();
                let _ = events.try_send(SecurityEventData::TimerChange);
            }
            res
        } else {
            match bonds
                .iter()
                .find(|bond| bond.identity.match_identity(&storage.peer_identity))
                .cloned()
            {
                Some(b) if encrypted => {
                    info!("[smp] Encrypted using bond {:?}", b.identity.addr);
                    storage.security_level = b.security_level;
                    #[cfg(feature = "legacy-pairing")]
                    {
                        storage.encryption_key_len = b.encryption_key_len;
                    }
                    bond = Some(b);
                }
                _ => {
                    warn!(
                        "[smp] Either encryption failed to enable or bond not found for {:?}",
                        storage.peer_identity.addr
                    );
                    storage.security_level = SecurityLevel::NoEncryption;
                }
            }
            Ok(())
        };

        // Emit an Encrypted event for both new-pairing and resumed-bond paths.
        if encrypted && storage.security_level != SecurityLevel::NoEncryption {
            let _ = storage.events.try_send(ConnectionEvent::Encrypted {
                security_level: storage.security_level,
                bond,
            });
        }

        res
    }

    /// Revokes `storage`'s encryption and marks its existing `bonds` entry rejected for this connection.
    /// Advances only the matching machine through `connections`/`events`, wakes terminal waiters,
    /// and preserves stored keys; no SMP PairingFailed is sent for an HCI failure.
    fn handle_encryption_failure<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        connections: &ConnectionManager<'_, P>,
        storage: &mut ConnectionStorage<P::Packet>,
    ) {
        storage.security_level = SecurityLevel::NoEncryption;
        let has_bond = bonds.iter().any(|b| b.identity.match_identity(&storage.peer_identity));
        if has_bond {
            storage.bond_rejected = true;
        }
        if let Some(sm) = Self::pairing_sm_for(self.pairing_sm.as_mut(), storage) {
            // If we were waiting for bonded encryption, mark the bond as
            // rejected on this connection so the next pairing attempt will
            // skip bonded encryption and initiate fresh pairing instead.
            if sm.is_waiting_bonded_encryption() {
                storage.bond_rejected = true;
            }
            let _res = sm.handle_event(
                pairing::Event::LinkEncryptedResult(false),
                &mut PairingOpsImpl {
                    bonds,
                    events,
                    secret_key: &self.secret_key,
                    public_key: &self.public_key,
                    passkey: self.passkey,
                    peer_identity: storage.peer_identity,
                    connections,
                    storage,
                    conn_handle: storage.handle,
                    state: &self.state,
                },
                &mut self.rng,
            );
            // Don't call handle_security_error here: sending SMP PairingFailed
            // for an HCI-level encryption failure would cause the peer to
            // delete its bond information, preventing future re-encryption.
            if sm.result().is_some() {
                self.finished_waker.wake();
                let _ = events.try_send(SecurityEventData::TimerChange);
            }
        }
    }

    /// Starts role-appropriate pairing for `storage`, using `bonds`, `events` and `connections`;
    /// `user_initiated` controls user-visible terminal reporting. Returns Ok for an existing active
    /// machine for this peer, InvalidState if another peer owns it, or startup/queue errors.
    /// Address validation precedes creation; attempted sends are not rolled back after failure.
    fn initiate<P: PacketPool>(
        &mut self,
        bonds: &mut VecView<BondInformation>,
        events: &Channel<NoopRawMutex, SecurityEventData, 3>,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<<P as PacketPool>::Packet>,
        user_initiated: bool,
    ) -> Result<(), Error> {
        let role = storage.role;

        if !self.is_idle() {
            // If pairing is already in progress for this peer, consider the request fulfilled.
            if Self::pairing_sm_for(self.pairing_sm.as_mut(), storage).is_some() {
                return Ok(());
            } else {
                return Err(Error::InvalidState);
            }
        }

        let handle = storage.handle;
        let peer_identity = storage.peer_identity;
        let local_address = self.connection_local_address(storage)?;
        let peer_address = Self::connection_peer_address(storage);
        let local_io = self.io_capabilities;
        let mut ops = PairingOpsImpl {
            bonds,
            events,
            secret_key: &self.secret_key,
            public_key: &self.public_key,
            passkey: self.passkey,
            conn_handle: handle,
            connections,
            storage,
            peer_identity,
            state: &self.state,
        };
        if role == LeConnRole::Peripheral {
            self.pairing_sm = Some(Pairing::initiate_peripheral(
                local_address,
                peer_address,
                &mut ops,
                local_io,
                user_initiated,
            )?);
        } else {
            self.pairing_sm = Some(Pairing::initiate_central(
                local_address,
                peer_address,
                &mut ops,
                local_io,
                user_initiated,
            )?);
        }
        Ok(())
    }
}

/// Executor-local security coordinator with one pairing machine and externally owned bond storage.
pub struct SecurityManager<'d> {
    /// Inner mutable state
    inner: RefCell<Inner>,
    /// Bond storage (externally owned by HostResources)
    bonds: &'d RefCell<VecView<BondInformation>>,
    /// Received events
    events: Channel<NoopRawMutex, SecurityEventData, 3>,
}

impl<'d> SecurityManager<'d> {
    /// Returns a coordinator borrowing `bonds` for its lifetime; no entries are cleared.
    /// The deterministic zero-seeded RNG/keypair is a placeholder: host initialization must call
    /// set_random_generator_seed with controller entropy before any security use.
    pub(crate) fn new(bonds: &'d RefCell<VecView<BondInformation>>) -> Self {
        let mut rng = ChaCha12Rng::from_seed([0u8; 32]);
        let secret_key = crypto::SecretKey::new(&mut rng);
        let public_key = secret_key.public_key();
        Self {
            inner: RefCell::new(Inner {
                rng,
                secret_key,
                public_key,
                state: SecurityManagerData::new(),
                pairing_sm: None,
                finished_waker: WakerRegistration::new(),
                io_capabilities: IoCapabilities::NoInputNoOutput,
                passkey: None,
                #[cfg(feature = "legacy-pairing")]
                secure_connections_only: false,
            }),
            bonds,
            events: Channel::new(),
        }
    }

    /// Sets advertised `io_capabilities` for subsequent pairing creation; does not rewrite active features.
    pub(crate) fn set_io_capabilities(&self, io_capabilities: IoCapabilities) {
        self.inner.borrow_mut().io_capabilities = io_capabilities;
    }

    /// Stores optional display-side `passkey` (callers must constrain Some to 0 through 999999).
    /// None selects random generation. Secure Connections central display does not use this override.
    pub(crate) fn set_passkey(&self, passkey: Option<u32>) {
        self.inner.borrow_mut().passkey = passkey;
    }

    /// Sets `enabled` to reject later legacy selection/downgrade even when compiled in.
    /// Does not cancel a legacy procedure already in progress.
    #[cfg(feature = "legacy-pairing")]
    pub(crate) fn set_secure_connections_only(&self, enabled: bool) {
        self.inner.borrow_mut().secure_connections_only = enabled;
    }

    /// Replaces the CSPRNG from full-entropy `random_seed` and regenerates the persistent LESC keypair.
    /// Call before security operations; existing OOB commitments become stale and no active pairing
    /// is cancelled. Key generation can panic only through its RNG/backend invariants.
    pub(crate) fn set_random_generator_seed(&self, random_seed: [u8; 32]) {
        let mut inner = self.inner.borrow_mut();
        inner.rng = ChaCha12Rng::from_seed(random_seed);
        inner.secret_key = crypto::SecretKey::new(&mut inner.rng);
        inner.public_key = inner.secret_key.public_key();
    }

    /// Stores `address` as local identity/fallback pairing address without validating its kind.
    pub(crate) fn set_local_address(&self, address: Address) {
        self.inner.borrow_mut().state.local_address = Some(address);
    }

    /// Returns true if no pairing is currently in progress.
    fn is_idle(&self) -> bool {
        self.inner.borrow().is_idle()
    }

    /// Returns whether an unfinished machine is bound to link peer `address`.
    pub(crate) fn is_pairing_in_progress(&self, address: Address) -> bool {
        let inner = self.inner.borrow();
        match &inner.pairing_sm {
            Some(sm) => sm.peer_address() == address && sm.result().is_none(),
            None => false,
        }
    }

    /// Returns the active link peer, or None when no machine exists or its result is terminal.
    pub(crate) fn peer_address(&self) -> Option<Address> {
        self.inner
            .borrow()
            .pairing_sm
            .as_ref()
            .and_then(|sm| sm.result().is_none().then_some(sm.peer_address()))
    }

    /// Waits for the machine bound to link peer `address`; Returns its terminal result, Busy if a
    /// different peer owns the machine, or Disconnected if none exists. Dropping this future does not
    /// cancel pairing. One registered waker is retained, so concurrent waiters replace each other.
    pub(crate) async fn wait_finished(&self, address: Address) -> Result<(), Error> {
        poll_fn(|cx| {
            let mut inner = self.inner.borrow_mut();
            inner.finished_waker.register(cx.waker());
            match &inner.pairing_sm {
                Some(sm) => {
                    if sm.peer_address() != address {
                        return Poll::Ready(Err(Error::Busy));
                    }
                    match sm.result() {
                        Some(result) => Poll::Ready(result),
                        None => Poll::Pending,
                    }
                }
                None => Poll::Ready(Err(Error::Disconnected)),
            }
        })
        .await
    }

    /// Returns a clone of the first bond matching `identity`, including IRK resolution, or None.
    pub(crate) fn get_peer_bond_information(&self, identity: &Identity) -> Option<BondInformation> {
        trace!("[security manager] Find long term key for {:?}", identity.addr);
        self.bonds.borrow().iter().find_map(|bond| {
            if bond.identity.match_identity(identity) {
                Some(bond.clone())
            } else {
                None
            }
        })
    }

    /// Returns fresh little-endian random/confirm bytes for local LESC OOB exchange.
    ///
    /// The returned data should be exchanged with the peer via an out-of-band channel.
    /// The persistent keypair is used so the confirm value matches the public key
    /// that will be sent during pairing.
    pub(crate) fn get_local_oob_data(&self) -> pairing::OobData {
        let mut inner = self.inner.borrow_mut();
        let r = crypto::Nonce::new(&mut inner.rng);
        let c = r.f4(inner.public_key.x(), inner.public_key.x(), 0);
        pairing::OobData {
            random: r.0.to_le_bytes(),
            confirm: c.0.to_le_bytes(),
        }
    }

    /// Returns the configured local identity/fallback address, or None before configuration.
    pub(crate) fn get_local_address(&self) -> Option<Address> {
        self.inner.borrow().state.local_address
    }

    /// Replaces the local privacy key with `irk`; controller resolving-list updates occur elsewhere.
    pub(crate) fn set_local_irk(&self, irk: IdentityResolvingKey) {
        self.inner.borrow_mut().state.local_irk = Some(irk);
    }

    /// Get the local Identity Resolving Key (IRK).
    /// Returns `Some` when privacy is enabled, `None` otherwise.
    pub(crate) fn get_local_irk(&self) -> Option<IdentityResolvingKey> {
        self.inner.borrow().state.local_irk
    }

    /// Returns a fresh local RPA, None without an IRK, or InvalidValue if bounded random generation fails.
    /// Does not install the resulting address in the controller.
    #[cfg(feature = "central")]
    pub(crate) fn generate_local_rpa(&self) -> Result<Option<BdAddr>, Error> {
        let mut inner = self.inner.borrow_mut();
        let Some(irk) = inner.state.local_irk else {
            return Ok(None);
        };
        Ok(Some(BdAddr::new(irk.generate_resolvable_address(&mut inner.rng)?)))
    }

    /// Inserts/replaces `bond_information` in memory; Returns OutOfMemory for a new identity in a full
    /// store. The record is accepted without validation or persistence/controller synchronization.
    pub(crate) fn add_bond_information(&self, bond_information: BondInformation) -> Result<(), Error> {
        add_bond(&mut self.bonds.borrow_mut(), bond_information)
    }

    /// Removes the first entry matching `identity`; Returns NotFound without mutation if absent.
    /// Does not disconnect, erase external persistence or change a controller resolving list.
    pub(crate) fn remove_bond_information(&self, identity: Identity) -> Result<(), Error> {
        trace!("[security manager] Remove bond for {:?}", identity.addr);
        let mut bonds = self.bonds.borrow_mut();
        let index = bonds.iter().position(|bond| bond.identity.match_identity(&identity));
        match index {
            Some(index) => {
                bonds.remove(index);
                Ok(())
            }
            None => Err(Error::NotFound),
        }
    }

    /// Returns a shared borrow of all retained records; holding it prevents mutable bond operations.
    pub(crate) fn get_bond_information(&self) -> Ref<'_, VecView<BondInformation>> {
        self.bonds.borrow()
    }

    /// Returns the number of in-memory records, including transient nonbonded encryption records.
    pub(crate) fn bond_count(&self) -> usize {
        self.bonds.borrow().len()
    }

    /// Returns a cloned record at zero-based `index`, or None outside the current store length.
    pub(crate) fn get_bond(&self, index: usize) -> Option<BondInformation> {
        self.bonds.borrow().get(index).cloned()
    }

    /// Returns the identity at zero-based `index`, or None outside the current store length.
    pub(crate) fn get_bond_identity(&self, index: usize) -> Option<Identity> {
        self.bonds.borrow().get(index).map(|b| b.identity)
    }

    /// Copies/relinquishes opcode-bearing `pdu`; Returns (command, padded 72-byte buffer, copied length).
    /// Requires an exact command body size; short/mismatched packets return InvalidParameters and
    /// unknown opcodes return CommandNotSupported. Oversized PDUs are capped at 72 and rejected by size.
    fn parse_smp_command<P: PacketPool>(pdu: Pdu<P::Packet>) -> Result<(Command, [u8; 72], usize), Error> {
        let mut buffer = [0u8; 72];
        let size = {
            let size = pdu.len().min(buffer.len());
            buffer[..size].copy_from_slice(&pdu.as_ref()[..size]);
            size
        };
        if size < 2 {
            error!("[security manager] Payload size too small {}", size);
            return Err(Error::Security(Reason::InvalidParameters));
        }
        let payload_len = size - 1;
        let command = match Command::try_from(buffer[0]) {
            Ok(command) => {
                if usize::from(command.payload_size()) != payload_len {
                    error!("[security manager] Payload size mismatch for command {}", command);
                    return Err(Error::Security(Reason::InvalidParameters));
                }
                command
            }
            Err(_) => return Err(Error::Security(Reason::CommandNotSupported)),
        };
        Ok((command, buffer, size))
    }

    /// Parses consumed `pdu` and advances responder state for `storage` via `connections`.
    /// Returns malformed-PDU or transition errors; the packet is released before state processing.
    fn handle_peripheral<P: PacketPool>(
        &self,
        pdu: Pdu<P::Packet>,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let (command, buffer, size) = Self::parse_smp_command::<P>(pdu)?;
        let cmd = SmpCommand {
            command,
            payload: &buffer[1..size],
            handle: storage.handle,
            peer_identity: storage.peer_identity,
        };
        self.inner.borrow_mut().handle_peripheral(
            &mut self.bonds.borrow_mut(),
            &self.events,
            &cmd,
            connections,
            storage,
        )
    }

    /// Parses consumed `pdu` and advances initiator state for `storage` via `connections`.
    /// Returns malformed-PDU or transition errors; the packet is released before state processing.
    fn handle_central<P: PacketPool>(
        &self,
        pdu: Pdu<P::Packet>,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let (command, buffer, size) = Self::parse_smp_command::<P>(pdu)?;
        let cmd = SmpCommand {
            command,
            payload: &buffer[1..size],
            handle: storage.handle,
            peer_identity: storage.peer_identity,
        };
        self.inner
            .borrow_mut()
            .handle_central(&mut self.bonds.borrow_mut(), &self.events, &cmd, connections, storage)
    }

    /// Consumes SMP `pdu` for role/peer in `storage`, using `connections` for packet/event effects.
    /// Returns the original parse/transition error after best-effort PairingFailed reporting.
    /// Success resets the active timeout; terminal states wake completion waiters. Effects may be partial.
    pub(crate) fn handle_l2cap_command<P: PacketPool>(
        &self,
        pdu: Pdu<P::Packet>,
        connections: &ConnectionManager<P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let role = storage.role;

        let result = if role == LeConnRole::Peripheral {
            self.handle_peripheral(pdu, connections, storage)
        } else {
            self.handle_central(pdu, connections, storage)
        };

        {
            let mut inner = self.inner.borrow_mut();
            if inner.is_idle() {
                inner.finished_waker.wake();
            }
            if result.is_ok() {
                if let Some(sm) = inner.pairing_sm.as_mut() {
                    sm.reset_timeout();
                    let _ = self.events.try_send(SecurityEventData::TimerChange);
                }
            }
        }

        if result.is_err() {
            if let Err(e) = self.handle_security_error(connections, storage, &result) {
                error!("[security manager] Failed sending pairing failed message! {:?}", e);
            }
        }
        result
    }

    /// Reports an error in `result` as SMP PairingFailed for `storage` through `connections`.
    /// Ok/Timeout produce no packet; nonsecurity failures map to UnspecifiedReason.
    /// Returns packet allocation/queue errors only, without changing the original pairing result.
    fn handle_security_error<P: PacketPool>(
        &self,
        connections: &ConnectionManager<P>,
        storage: &ConnectionStorage<<P as PacketPool>::Packet>,
        result: &Result<(), Error>,
    ) -> Result<(), Error> {
        if let Err(error) = result {
            let reason = if let Error::Security(secuity_error) = error {
                *secuity_error
            } else {
                Reason::UnspecifiedReason
            };

            error!("Handling of command failed {:?}", error);

            // Cease sending security manager messages on timeout
            if *error != Error::Timeout {
                let handle = storage.handle;
                let mut packet = self.prepare_packet(Command::PairingFailed, connections)?;
                let payload = packet.payload_mut();
                payload[0] = u8::from(reason);

                match self.try_send_packet(packet, connections, handle) {
                    Ok(()) => (),
                    Err(error) => {
                        error!("[security manager] Failed to send pairing failed {:?}", error);
                        return Err(error);
                    }
                }
            }
        }

        Ok(())
    }

    /// Requests pairing on `storage` through `connections`; `user_initiated` controls user-visible
    /// completion reporting. Returns a security error if already encrypted, otherwise startup/state/
    /// queue errors. Success means initiation or an existing same-peer procedure, not completed pairing.
    pub fn initiate<P: PacketPool>(
        &self,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<<P as PacketPool>::Packet>,
        user_initiated: bool,
    ) -> Result<(), Error> {
        if storage.security_level != SecurityLevel::NoEncryption {
            return Err(Error::Security(Reason::UnspecifiedReason));
        }
        self.inner.borrow_mut().initiate(
            &mut self.bonds.borrow_mut(),
            &self.events,
            connections,
            storage,
            user_initiated,
        )
    }

    /// Marks the current active state timed out and wakes its completion waiter; sends no SMP packet.
    pub(crate) fn cancel_timeout(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(pairing) = inner.pairing_sm.as_mut() {
            pairing.mark_timeout();
            inner.finished_waker.wake();
        }
    }

    /// Drops the machine for `storage`'s link peer and wakes its waiter; preserves other peers.
    /// Removes nonbonded records whose full identity equals `storage`'s peer identity.
    pub(crate) fn disconnect<P>(&self, storage: &ConnectionStorage<P>) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner
                .pairing_sm
                .take_if(|p| p.peer_address() == Inner::connection_peer_address(storage))
                .is_some()
            {
                inner.finished_waker.wake();
            }
        }
        self.bonds
            .borrow_mut()
            .retain(|x| x.is_bonded || x.identity != storage.peer_identity);
    }

    /// Decodes an LE LTK-request `event` and queues key lookup; other LE events are ignored.
    /// `connections` supplies the packet-pool type but is otherwise unused. Returns HCI decode or
    /// OutOfMemory queue errors; no controller command is issued synchronously.
    pub(crate) fn handle_hci_le_event<P: PacketPool>(
        &self,
        event: LeEventPacket,
        connections: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        #[allow(clippy::single_match)]
        match event.kind {
            LeEventKind::LeLongTermKeyRequest => {
                let event_data = LeLongTermKeyRequest::from_hci_bytes_complete(event.data)?;
                self.try_send_event(SecurityEventData::SendLongTermKey(
                    event_data.handle,
                    event_data.encrypted_diversifier,
                    event_data.random_number,
                ))?;
            }
            _ => (),
        }
        Ok(())
    }

    /// Applies encryption change/refresh `event` to its live connection in `connections`.
    /// Returns decode/lookup/transition errors. Failure or encryption-Off revokes admission and
    /// requests AuthenticationFailure disconnect after releasing storage; unrelated events are ignored.
    pub(crate) fn handle_hci_event<P: PacketPool>(
        &self,
        event: EventPacket,
        connections: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        // Extract (handle, status, encrypted) from each enabled event version.
        let encryption_event = match event.kind {
            EventKind::EncryptionChangeV1 => {
                let e = EncryptionChangeV1::from_hci_bytes_complete(event.data)?;
                Some((e.handle, e.status, e.enabled != EncryptionEnabledLevel::Off))
            }
            EventKind::EncryptionChangeV2 => {
                let e = EncryptionChangeV2::from_hci_bytes_complete(event.data)?;
                // Core Vol 4 Part E 7.7.8: Encryption_Key_Size is ignored on LE.
                Some((e.handle, e.status, e.encryption_enabled != EncryptionEnabledLevel::Off))
            }
            EventKind::EncryptionKeyRefreshComplete => {
                let e = EncryptionKeyRefreshComplete::from_hci_bytes_complete(event.data)?;
                // Key refresh implies encryption is still on
                Some((e.handle, e.status, true))
            }
            _ => None,
        };

        if let Some((handle, status, encrypted)) = encryption_event {
            let result = match status.to_result() {
                Ok(()) => {
                    trace!("[smp] Encryption event (encrypted={})", encrypted);
                    connections.with_connected_handle(handle, |storage| {
                        let res = self.inner.borrow_mut().handle_encryption_success(
                            &mut self.bonds.borrow_mut(),
                            &self.events,
                            encrypted,
                            connections,
                            storage,
                        );
                        if !encrypted {
                            storage.security_level = SecurityLevel::NoEncryption;
                        }
                        let _ = self.handle_security_error(connections, storage, &res);
                        res
                    })
                }
                Err(error) => {
                    error!("[security manager] Encryption event error {:?}", error);
                    connections.with_connected_handle(handle, |storage| {
                        self.inner.borrow_mut().handle_encryption_failure(
                            &mut self.bonds.borrow_mut(),
                            &self.events,
                            connections,
                            storage,
                        );
                        Ok(())
                    })
                }
            };
            // Release the storage borrow before requesting disconnect. Neither
            // failed refresh nor encryption-off may retain application access.
            if status.to_result().is_err() || !encrypted {
                connections.request_handle_disconnect(handle, bt_hci::param::DisconnectReason::AuthenticationFailure);
            }
            result?;
        }
        Ok(())
    }

    /// Dispatches application `pairing_event` for `storage` through `connections`.
    /// Returns the original transition error after best-effort SMP failure notification;
    /// successfully queued side effects are not rolled back.
    fn handle_event<P: PacketPool>(
        &self,
        pairing_event: pairing::Event,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        let res = self.inner.borrow_mut().handle_pairing_event(
            &mut self.bonds.borrow_mut(),
            &self.events,
            pairing_event,
            connections,
            storage,
        );
        if res.is_err() {
            if let Err(e) = self.handle_security_error(connections, storage, &res) {
                error!("[security manager] Failed sending pairing failed message! {:?}", e);
            }
        }
        res
    }

    /// Submits decimal `input` for `storage` through `connections`; Returns state/crypto/queue errors.
    /// Caller must constrain the value to 0 through 999999; this path does not enforce the range.
    pub(crate) fn handle_pass_key_input<P: PacketPool>(
        &self,
        input: u32,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        self.handle_event(pairing::Event::PassKeyInput(input), connections, storage)
    }

    /// Submits `local_oob`/`peer_oob` to the active phase for `storage` through `connections`.
    /// Returns state/confirm/queue errors, including OobNotAvailable for absent peer SC material when
    /// local availability was advertised. Consumed fields depend on legacy versus Secure Connections.
    /// This wrapper neither authenticates the external channel nor persists the material.
    pub(crate) fn handle_oob_data_received<P: PacketPool>(
        &self,
        local_oob: pairing::OobData,
        peer_oob: pairing::OobData,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<P::Packet>,
    ) -> Result<(), Error> {
        self.handle_event(
            pairing::Event::OobDataReceived {
                local: local_oob,
                peer: peer_oob,
            },
            connections,
            storage,
        )
    }

    /// Submits acceptance when `confirmed`, rejection otherwise, for `storage` through `connections`.
    /// Returns state/security/queue errors; a false comparison can terminate the procedure.
    pub(crate) fn handle_pass_key_confirm<P: PacketPool>(
        &self,
        confirmed: bool,
        connections: &ConnectionManager<'_, P>,
        storage: &ConnectionStorage<<P as PacketPool>::Packet>,
    ) -> Result<(), Error> {
        let pairing_event = match confirmed {
            true => pairing::Event::PassKeyConfirm,
            false => pairing::Event::PassKeyCancel,
        };
        self.handle_event(pairing_event, connections, storage)
    }

    /// Returns a pool packet with header for `command`, or OutOfMemory on exhaustion.
    /// `connections` supplies the pool type only. Payload remains unset; undersized packets panic.
    fn prepare_packet<P: PacketPool>(
        &self,
        command: Command,
        connections: &ConnectionManager<P>,
    ) -> Result<TxPacket<P>, Error> {
        let packet = P::allocate().ok_or(Error::OutOfMemory)?;
        TxPacket::new(packet, command)
    }

    /// Consumes `packet` into `connections`' outbound queue for `handle`; Returns its admission error.
    /// Success is queue acceptance, not controller/wire completion.
    fn try_send_packet<P: PacketPool>(
        &self,
        packet: TxPacket<P>,
        connections: &ConnectionManager<P>,
        handle: ConnHandle,
    ) -> Result<(), Error> {
        let len = packet.total_size();
        trace!("[security manager] Send {} {}", packet.command, len);
        connections.try_outbound(handle, packet.into_pdu())
    }

    /// Queues deferred `event`; Returns OutOfMemory if the three-entry queue is full.
    fn try_send_event(&self, event: SecurityEventData) -> Result<(), Error> {
        self.events.try_send(event).map_err(|_| Error::OutOfMemory)
    }

    /// Returns a future receiving one deferred event, or TimeoutError at the captured pairing deadline.
    /// Deadline is sampled at creation; TimerChange tells the caller to create a fresh future.
    /// Dropping a pending future does not cancel pairing or consume an event.
    pub(crate) fn poll_events(&self) -> impl Future<Output = Result<SecurityEventData, TimeoutError>> + use<'_, 'd> {
        let deadline = self
            .inner
            .borrow()
            .pairing_sm
            .as_ref()
            .map(|x| x.timeout_at())
            .unwrap_or(Instant::now() + constants::TIMEOUT_DISABLE);
        // try to pop an event from the channel
        poll_fn(|cx| self.events.poll_receive(cx)).with_deadline(deadline)
    }
}

/// Borrowed bridge from one synchronous pairing transition to connection/bond queues.
struct PairingOpsImpl<'sm, 'cm, 'cm2, 'cs, P: PacketPool> {
    /// Mutable in-memory records; insertion/replacement can survive a later queue error.
    bonds: &'sm mut VecView<BondInformation>,
    /// Three-entry deferred security/HCI work queue.
    events: &'sm Channel<NoopRawMutex, SecurityEventData, 3>,
    /// Persistent private scalar used for this host's Secure Connections exchanges.
    secret_key: &'sm crypto::SecretKey,
    /// Public point corresponding to secret_key.
    public_key: &'sm crypto::PublicKey,
    /// Optional configured display passkey; caller owns its six-digit validity.
    passkey: Option<u32>,
    /// Connection owner providing bounded outbound admission.
    connections: &'cm ConnectionManager<'cm2, P>,
    /// Connection flags and user-event queue borrowed for the transition.
    storage: &'cs ConnectionStorage<P::Packet>,
    /// Controller handle associated with all emitted work.
    conn_handle: ConnHandle,
    /// Resolved/observed identity used to select retained keys.
    peer_identity: Identity,
    /// Configured local identity address and optional privacy IRK.
    state: &'sm SecurityManagerData,
}

impl<'sm, 'cm, 'cm2, 'cs, P: PacketPool> PairingOps<P> for PairingOpsImpl<'sm, 'cm, 'cm2, 'cs, P> {
    /// Queues owned `packet` using this already-borrowed storage's checked slot identity, without
    /// reborrowing the connection table. Returns outbound errors, otherwise sends a best-effort
    /// TimerChange. Queue acceptance does not establish wire completion.
    fn try_send_packet(&mut self, packet: TxPacket<P>) -> Result<(), Error> {
        let len = packet.total_size();
        trace!("[security manager] Send {} {}", packet.command, len);
        self.connections.try_outbound_storage(self.storage, packet.into_pdu())?;
        let _ = self.events.try_send(SecurityEventData::TimerChange);
        Ok(())
    }

    /// Stores cloned `bond`; Returns capacity errors. A bond with IRK also emits best-effort BondAdded;
    /// successful storage is retained even if notification is dropped. No external persistence occurs.
    fn try_update_bond_information(&mut self, bond: &BondInformation) -> Result<(), Error> {
        add_bond(self.bonds, bond.clone())?;
        if bond.identity.irk.is_some() {
            let _ = self
                .events
                .try_send(SecurityEventData::BondAdded(self.conn_handle, bond.identity));
        }
        Ok(())
    }

    /// Returns a clone of the first identity-matching record, or None; does not check bond_rejected.
    fn find_bond(&self) -> Option<BondInformation> {
        self.bonds
            .iter()
            .find(|x| x.identity.match_identity(&self.peer_identity))
            .cloned()
    }

    /// Stores `ltk`, claimed `security_level` and retention flag `is_bonded`, with legacy `ediv`,
    /// wire-order `rand` and byte length `encryption_key_len` when enabled. Returns the record after
    /// queuing EnableEncryption, or capacity/queue error. A queue error can leave the new bond stored;
    /// no arguments are validated and successful return does not imply HCI completion.
    fn try_enable_encryption(
        &mut self,
        ltk: &LongTermKey,
        security_level: SecurityLevel,
        is_bonded: bool,
        #[cfg(feature = "legacy-pairing")] ediv: u16,
        #[cfg(feature = "legacy-pairing")] rand: [u8; 8],
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<BondInformation, Error> {
        info!("Enabling encryption for {:?}", self.peer_identity.addr);
        let bond_info = BondInformation {
            ltk: *ltk,
            identity: self.peer_identity,
            is_bonded,
            security_level,
            #[cfg(feature = "legacy-pairing")]
            ediv,
            #[cfg(feature = "legacy-pairing")]
            rand,
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len,
        };
        self.try_update_bond_information(&bond_info)?;
        self.events
            .try_send(SecurityEventData::EnableEncryption(self.conn_handle, bond_info.clone()))
            .map_err(|_| Error::OutOfMemory)?;
        Ok(bond_info)
    }

    /// Returns None for a rejected/missing bond, Some cloned bond after queueing encryption, or
    /// OutOfMemory when the work queue is full. The stored record is not changed.
    fn try_enable_bonded_encryption(&mut self) -> Result<Option<BondInformation>, Error> {
        if self.storage.bond_rejected {
            return Ok(None);
        }
        if let Some(bond) = self.find_bond() {
            self.events
                .try_send(SecurityEventData::EnableEncryption(self.conn_handle, bond.clone()))
                .map_err(|_| Error::OutOfMemory)?;
            Ok(Some(bond))
        } else {
            Ok(None)
        }
    }

    /// Returns the configured display passkey, or None for random generation; no validation occurs.
    fn passkey(&self) -> Option<u32> {
        self.passkey
    }

    /// Returns Bonding exactly when this connection permits retaining a new bond.
    fn bonding_flag(&self) -> BondingFlag {
        if self.storage.bondable {
            BondingFlag::Bonding
        } else {
            BondingFlag::NoBonding
        }
    }

    /// Returns the controller handle bound to this transition without changing it.
    fn connection_handle(&mut self) -> ConnHandle {
        self.conn_handle
    }

    /// Queues user `event`; Returns OutOfMemory if full. Accepted completion/failure events also
    /// attempt TimerChange delivery, whose failure is ignored.
    fn try_send_connection_event(&mut self, event: ConnectionEvent) -> Result<(), Error> {
        let timer_changed = matches!(
            event,
            ConnectionEvent::PairingComplete { .. } | ConnectionEvent::PairingFailed(_)
        );
        self.storage.events.try_send(event).map_err(|_| Error::OutOfMemory)?;
        if timer_changed {
            let _ = self.events.try_send(SecurityEventData::TimerChange);
        }
        Ok(())
    }

    /// Returns this connection's advertised possession of peer OOB data.
    fn oob_available(&self) -> bool {
        self.storage.oob_available
    }

    /// Returns the borrowed persistent local private scalar.
    fn secret_key(&self) -> &crypto::SecretKey {
        self.secret_key
    }

    /// Returns the borrowed public point corresponding to the persistent local private scalar.
    fn public_key(&self) -> &crypto::PublicKey {
        self.public_key
    }

    /// Returns local IRK wire bytes, or sixteen zeros when privacy is not configured.
    fn local_irk(&self) -> [u8; 16] {
        self.state.local_irk.map(|k| k.to_le_bytes()).unwrap_or_default()
    }

    /// Returns the configured identity address, or InvalidValue before configuration; excludes per-link RPA.
    fn local_identity_address(&self) -> Result<Address, Error> {
        self.state.local_address.ok_or(Error::InvalidValue)
    }
}

#[cfg(test)]
/// Peer-specific pairing/HCI routing and immediate admission revocation on encryption loss.
mod tests {
    extern crate std;

    use core::cell::RefCell;
    use std::boxed::Box;

    use bt_hci::param::{ConnHandle, LeConnRole};
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use rand_chacha::ChaCha12Rng;
    use rand_core::SeedableRng;

    use super::*;
    use crate::connection::ConnParams;
    use crate::connection_manager::{ConnectionManager, ConnectionStorage};
    use crate::prelude::DefaultPacketPool;
    use crate::IoCapabilities;

    /// Returns a deliberately leaked two-connection/four-bond manager for independent native tests.
    fn setup_manager() -> &'static ConnectionManager<'static, DefaultPacketPool> {
        let storage: &RefCell<[_]> = Box::leak(Box::new(RefCell::new([const { ConnectionStorage::new() }; 2])));
        let bond_storage: &RefCell<heapless::VecView<_>> =
            Box::leak(Box::new(RefCell::new(heapless::Vec::<BondInformation, 4>::new())));
        Box::leak(Box::new(ConnectionManager::new(storage, bond_storage)))
    }

    /// Returns deterministic no-I/O responder state bound to `peer_address`, awaiting its request.
    fn test_inner(peer_address: Address) -> Inner {
        let mut rng = ChaCha12Rng::from_seed([7; 32]);
        let secret_key = crypto::SecretKey::new(&mut rng);
        let public_key = secret_key.public_key();
        Inner {
            rng,
            secret_key,
            public_key,
            passkey: None,
            state: SecurityManagerData {
                local_address: Some(Address::random([1, 2, 3, 4, 5, 6])),
                local_irk: None,
            },
            pairing_sm: Some(Pairing::new_peripheral(
                Address::random([1, 2, 3, 4, 5, 6]),
                peer_address,
                IoCapabilities::NoInputNoOutput,
            )),
            finished_waker: WakerRegistration::new(),
            io_capabilities: IoCapabilities::NoInputNoOutput,
            #[cfg(feature = "legacy-pairing")]
            secure_connections_only: false,
        }
    }

    #[test]
    /// Bond resumption for one peer publishes its level without advancing another peer's pairing.
    fn encryption_success_uses_bond_when_pairing_sm_is_for_other_peer() {
        let mgr = setup_manager();
        let handle = ConnHandle::new(2);
        let current = Identity::from(Address::random([0x10, 2, 3, 4, 5, 6]));
        let other = Address::random([0x40, 2, 3, 4, 5, 6]);
        unwrap!(mgr.connect(handle, current.addr, LeConnRole::Peripheral, ConnParams::new()));

        let mut inner = test_inner(other);
        let mut bonds: heapless::Vec<BondInformation, 4> = heapless::Vec::new();
        unwrap!(bonds.push(BondInformation::new(
            current,
            LongTermKey::new(1),
            SecurityLevel::EncryptedAuthenticated,
            true,
        )));
        let events = Channel::<NoopRawMutex, SecurityEventData, 3>::new();

        unwrap!(mgr.with_connected_handle(handle, |storage| {
            unwrap!(inner.handle_encryption_success(&mut bonds, &events, true, mgr, storage));
            assert_eq!(storage.security_level, SecurityLevel::EncryptedAuthenticated);
            assert!(inner.pairing_sm.is_some());
            assert_eq!(inner.pairing_sm.as_ref().unwrap().peer_address(), other);
            Ok(())
        }));
    }

    #[test]
    /// HCI failure marks only the current peer's retained bond rejected and preserves another machine.
    fn encryption_failure_rejects_current_bond_without_touching_other_peer_sm() {
        let mgr = setup_manager();
        let handle = ConnHandle::new(3);
        let current = Identity::from(Address::random([0x11, 2, 3, 4, 5, 6]));
        let other = Address::random([0x41, 2, 3, 4, 5, 6]);
        unwrap!(mgr.connect(handle, current.addr, LeConnRole::Peripheral, ConnParams::new()));

        let mut inner = test_inner(other);
        let mut bonds: heapless::Vec<BondInformation, 4> = heapless::Vec::new();
        unwrap!(bonds.push(BondInformation::new(
            current,
            LongTermKey::new(2),
            SecurityLevel::Encrypted,
            true,
        )));
        let events = Channel::<NoopRawMutex, SecurityEventData, 3>::new();

        unwrap!(mgr.with_connected_handle(handle, |storage| {
            inner.handle_encryption_failure(&mut bonds, &events, mgr, storage);
            assert!(storage.bond_rejected);
            assert!(inner.pairing_sm.is_some());
            assert_eq!(inner.pairing_sm.as_ref().unwrap().peer_address(), other);
            Ok(())
        }));
    }

    #[test]
    /// Failed refresh/change and encryption-Off revoke admission before physical disconnect completion.
    fn lost_encryption_revokes_live_connection_before_disconnect_complete() {
        // Decoded HCI failures on refresh/change, then encryption switched off.
        for (kind, data) in [
            (EventKind::EncryptionKeyRefreshComplete, &[0x05, 0x05, 0x00][..]),
            (EventKind::EncryptionChangeV1, &[0x05, 0x05, 0x00, 0x00][..]),
            (EventKind::EncryptionChangeV1, &[0x00, 0x05, 0x00, 0x00][..]),
            (EventKind::EncryptionChangeV2, &[0x05, 0x05, 0x00, 0x00, 0][..]),
            (EventKind::EncryptionChangeV2, &[0x00, 0x05, 0x00, 0x00, 0][..]),
        ] {
            let mgr = setup_manager();
            let handle = ConnHandle::new(5);
            let peer = Address::random([0x13, 2, 3, 4, 5, 6]);
            unwrap!(mgr.connect(handle, peer, LeConnRole::Central, ConnParams::new()));
            let conn = embassy_futures::block_on(mgr.accept(LeConnRole::Central, &[peer]));
            unwrap!(mgr.with_connected_handle(handle, |storage| {
                storage.security_level = SecurityLevel::Encrypted;
                Ok(())
            }));
            assert!(conn.security_level().unwrap().encrypted());
            unwrap!(mgr.security_manager.handle_hci_event(EventPacket { kind, data }, mgr));
            // Inspect source state, not just the application's admission flag.
            assert_eq!(mgr.connection(0).security_level, SecurityLevel::NoEncryption);
            assert!(!conn.is_connected());
            assert!(!conn.is_disconnected());
            assert!(!mgr.all_disconnected());
            unwrap!(mgr.disconnected(handle, bt_hci::param::Status::SUCCESS));
            assert!(conn.is_disconnected());
        }
    }

    #[test]
    /// A user event for the wrong peer returns InvalidState without replacing the active machine.
    fn pairing_event_for_other_peer_fails_without_advancing_sm() {
        let mgr = setup_manager();
        let handle = ConnHandle::new(4);
        let current = Identity::from(Address::random([0x12, 2, 3, 4, 5, 6]));
        let other = Address::random([0x42, 2, 3, 4, 5, 6]);
        unwrap!(mgr.connect(handle, current.addr, LeConnRole::Peripheral, ConnParams::new()));

        let mut inner = test_inner(other);
        let mut bonds: heapless::Vec<BondInformation, 4> = heapless::Vec::new();
        let events = Channel::<NoopRawMutex, SecurityEventData, 3>::new();

        unwrap!(mgr.with_connected_handle(handle, |storage| {
            assert_eq!(
                inner.handle_pairing_event(&mut bonds, &events, pairing::Event::PassKeyConfirm, mgr, storage),
                Err(Error::InvalidState)
            );
            assert!(inner.pairing_sm.is_some());
            assert_eq!(inner.pairing_sm.as_ref().unwrap().peer_address(), other);
            Ok(())
        }));
    }

    /// A retained but connection-rejected bond falls back to fresh pairing on Security Request.
    #[test]
    fn security_request_after_rejected_bond_starts_fresh_pairing() {
        let mgr = setup_manager();
        let handle = ConnHandle::new(6);
        let peer = Address::random([0x14, 2, 3, 4, 5, 6]);
        unwrap!(mgr.connect(handle, peer, LeConnRole::Central, ConnParams::new()));
        let mut inner = test_inner(peer);
        inner.pairing_sm = Some(Pairing::new_central(
            Address::random([1, 2, 3, 4, 5, 6]),
            peer,
            IoCapabilities::NoInputNoOutput,
        ));
        let mut bonds: heapless::Vec<BondInformation, 4> = heapless::Vec::new();
        unwrap!(bonds.push(BondInformation::new(
            peer.into(),
            LongTermKey::new(3),
            SecurityLevel::EncryptedAuthenticated,
            true,
        )));
        let events = Channel::<NoopRawMutex, SecurityEventData, 3>::new();
        unwrap!(mgr.with_connected_handle(handle, |storage| {
            storage.bond_rejected = true;
            inner.handle_central(
                &mut bonds,
                &events,
                &SmpCommand {
                    command: Command::SecurityRequest,
                    payload: &[0x0d],
                    handle,
                    peer_identity: peer.into(),
                },
                mgr,
                storage,
            )?;
            let sm = inner.pairing_sm.as_ref().unwrap();
            assert!(!sm.is_waiting_bonded_encryption());
            assert_eq!(sm.result(), None);
            assert_eq!(storage.security_level, SecurityLevel::NoEncryption);
            assert!(storage.bond_rejected);
            Ok(())
        }));
    }
    /// Captures real bond lookup/update/encryption and both-role IRK distribution traces without exposing keys.
    #[cfg(feature = "log")]
    #[test]
    fn automatic_security_diagnostics_omit_keys() {
        /// Environment marker isolating the process-global logger from unrelated tests.
        const CHILD: &str = "TROUBLE_SECURITY_TRACE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "security_manager::tests::automatic_security_diagnostics_omit_keys",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        /// Synchronous test-only log sink for security-manager module descendants.
        struct Capture;
        /// Actual formatted trace lines, populated only by this isolated test process.
        static LINES: std::sync::Mutex<std::vec::Vec<std::string::String>> =
            std::sync::Mutex::new(std::vec::Vec::new());
        /// Process-lifetime logger required by log's static registration contract.
        static LOGGER: Capture = Capture;
        impl log::Log for Capture {
            /// Returns whether `metadata` belongs to the security manager or its pairing descendants.
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.target().starts_with("trouble_host::security_manager")
            }
            /// Formats actual `record` arguments and retains matching security traces for assertions.
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    LINES.lock().unwrap().push(std::format!("{}", record.args()));
                }
            }
            /// In-memory recording is synchronous and requires no flush.
            fn flush(&self) {}
        }
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        let mut identity = Identity::from(Address::random([0x10, 2, 3, 4, 5, 6]));
        identity.irk = crypto::IdentityResolvingKey::new(0x112233445566778899aabbccddeeff00);
        let bond = BondInformation::new(identity, LongTermKey::new(0x5a5a), SecurityLevel::Encrypted, true);
        let stored = RefCell::new(heapless::Vec::<BondInformation, 4>::new());
        let security = SecurityManager::new(&stored);
        security.add_bond_information(bond.clone()).unwrap();
        assert!(security.get_peer_bond_information(&identity).is_some());
        security.remove_bond_information(identity).unwrap();

        let mgr = setup_manager();
        let handle = ConnHandle::new(2);
        mgr.connect(handle, identity.addr, LeConnRole::Peripheral, ConnParams::new())
            .unwrap();
        let mut inner = test_inner(Address::random([0x40, 2, 3, 4, 5, 6]));
        let mut bonds = heapless::Vec::<BondInformation, 4>::new();
        bonds.push(bond).unwrap();
        let events = Channel::<NoopRawMutex, SecurityEventData, 3>::new();
        mgr.with_connected_handle(handle, |storage| {
            storage.peer_identity = identity;
            inner.handle_encryption_success(&mut bonds, &events, true, mgr, storage)?;
            inner.handle_encryption_success(&mut bonds, &events, false, mgr, storage)?;
            let mut ops = PairingOpsImpl {
                bonds: &mut bonds,
                events: &events,
                secret_key: &inner.secret_key,
                public_key: &inner.public_key,
                passkey: None,
                peer_identity: identity,
                connections: mgr,
                storage,
                conn_handle: handle,
                state: &inner.state,
            };
            ops.try_enable_encryption(
                &LongTermKey::new(0x5a5a),
                SecurityLevel::Encrypted,
                true,
                #[cfg(feature = "legacy-pairing")]
                0,
                #[cfg(feature = "legacy-pairing")]
                [0; 8],
                #[cfg(feature = "legacy-pairing")]
                16,
            )?;
            Ok(())
        })
        .unwrap();
        // Reuse the complete bilateral SMP exchange, including both IRK receivers.
        pairing::tests::bondable_just_works_with_irk([0x77; 16]);
        let lines = LINES.lock().unwrap();
        for marker in [
            "Add bond",
            "Find long term key",
            "Remove bond",
            "Encrypted using bond",
            "Either encryption failed",
            "Enabling encryption",
        ] {
            assert!(
                lines.iter().any(|line| line.contains(marker)),
                "missing exercised site: {marker}"
            );
        }
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("Identity information"))
                .count(),
            2
        );
        for line in lines.iter() {
            assert!(
                !line.contains("Identity {") && !line.contains("IdentityResolvingKey") && !line.contains("IRK:"),
                "automatic trace contains identity/key payload: {line}"
            );
        }
    }
}
