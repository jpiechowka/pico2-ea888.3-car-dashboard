//! Synchronous role-specific SMP machines, shared negotiated data and application/transport boundary.
//!
//! Input calls can enqueue packets/events and mutate bond storage before an error. They do not await
//! controller completion. The manager validates incoming command sizes and owns timeout/waker routing.

use bt_hci::param::ConnHandle;
use embassy_time::Instant;
use rand_core::{CryptoRng, RngCore};

use self::util::PairingMethod;
use crate::connection::{ConnectionEvent, SecurityLevel};
use crate::security_manager::types::{BondingFlag, Command, PairingFeatures};
use crate::security_manager::TxPacket;
use crate::{Address, BondInformation, Error, IoCapabilities, LongTermKey, PacketPool};

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Link addresses, feature negotiation and bond result shared across one role's pairing phases.
struct PairingData {
    /// Actual local link address used by cryptographic checks, possibly an RPA.
    local_address: Address,
    /// Peer address used for cryptographic checks/routing; current SC identity handling replaces it.
    peer_address: Address,
    /// Local feature advertisement and negotiated key-distribution choices.
    local_features: PairingFeatures,
    /// Last admitted peer Pairing Request/Response features.
    peer_features: PairingFeatures,
    /// Selected association model; initialized to Just Works before feature exchange.
    pairing_method: PairingMethod,
    /// Absolute Embassy monotonic pairing deadline; refreshed by the manager after progress.
    timeout_at: Instant,
    /// Pending/completed encryption record, including transient nonbonded keys.
    bond_information: Option<BondInformation>,
}

impl PairingData {
    /// Returns fresh negotiation state for link `local_address`/`peer_address`, advertised `local_io`
    /// and deadline now plus `timeout`; no packet is sent and addresses are not validated.
    fn new(
        local_address: Address,
        peer_address: Address,
        local_io: IoCapabilities,
        timeout: embassy_time::Duration,
    ) -> PairingData {
        PairingData {
            local_address,
            peer_address,
            local_features: PairingFeatures {
                io_capabilities: local_io,
                ..Default::default()
            },
            peer_features: PairingFeatures::default(),
            pairing_method: PairingMethod::JustWorks,
            timeout_at: Instant::now() + timeout,
            bond_information: None,
        }
    }

    /// Returns true only when both feature advertisements request bonding.
    fn want_bonding(&self) -> bool {
        matches!(self.local_features.security_properties.bond(), BondingFlag::Bonding)
            && matches!(self.peer_features.security_properties.bond(), BondingFlag::Bonding)
    }
}

pub mod central;
#[cfg(feature = "legacy-pairing")]
pub mod legacy_central;
#[cfg(feature = "legacy-pairing")]
pub mod legacy_peripheral;
pub mod peripheral;
mod util;

/// One already-framed peer command or local application/controller event.
pub(super) enum Input<'a> {
    /// SMP input whose payload length was validated by the manager.
    Command(
        /// Decoded SMP opcode.
        Command,
        /// Body bytes without opcode or L2CAP header.
        &'a [u8],
    ),
    /// Local state-machine input.
    Event(
        /// User decision, OOB material or encryption completion.
        Event,
    ),
}

/// Synchronous side effects used by pairing, with bounded queues and caller-owned cryptographic state.
/// Error returns need not undo earlier packet, event or bond effects; callers must not infer encryption
/// completion from successful queue admission.
pub trait PairingOps<P: PacketPool> {
    /// Consumes and queues `packet`; Returns queue/connection admission errors, not wire completion.
    fn try_send_packet(&mut self, packet: TxPacket<P>) -> Result<(), Error>;
    /// Returns a cloned record for the current peer, or None; a record may still be rejected for reuse.
    fn find_bond(&self) -> Option<BondInformation>;
    /// Requests encryption with an eligible retained key; Returns Some record after queue admission,
    /// None for absent/rejected records, or the queue error. Must not silently substitute a fresh key.
    fn try_enable_bonded_encryption(&mut self) -> Result<Option<BondInformation>, Error>;
    /// Requests encryption with `ltk`, intended `security_level` and persistence intent `is_bonded`.
    /// Legacy `ediv`, wire-order `rand` and byte `encryption_key_len` identify/size the key when enabled.
    /// Returns the pending bond record or an admission error; storage may change before a later error.
    fn try_enable_encryption(
        &mut self,
        ltk: &LongTermKey,
        security_level: SecurityLevel,
        is_bonded: bool,
        #[cfg(feature = "legacy-pairing")] ediv: u16,
        #[cfg(feature = "legacy-pairing")] rand: [u8; 8],
        #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
    ) -> Result<BondInformation, Error>;
    /// Inserts/replaces `bond` in host storage; Returns capacity/admission errors, without external durability.
    fn try_update_bond_information(&mut self, bond: &BondInformation) -> Result<(), Error>;
    /// Returns the current controller connection handle.
    fn connection_handle(&mut self) -> ConnHandle;
    /// Queues application `event`; Returns bounded-queue admission failure if unavailable.
    fn try_send_connection_event(&mut self, event: ConnectionEvent) -> Result<(), Error>;
    /// Returns whether this connection permits retaining a newly negotiated bond.
    fn bonding_flag(&self) -> BondingFlag;
    /// Returns whether the application advertises possession of this peer's OOB authentication data.
    fn oob_available(&self) -> bool;
    /// Returns the optional fixed display passkey; implementations/callers must ensure 0 through 999999.
    /// Currently consumed by the Secure Connections peripheral display path only.
    fn passkey(&self) -> Option<u32>;
    /// Returns the persistent LESC private scalar, matching [`Self::public_key`].
    fn secret_key(&self) -> &crate::security_manager::crypto::SecretKey;
    /// Returns the public point matching [`Self::secret_key`] and previously generated local OOB data.
    fn public_key(&self) -> &crate::security_manager::crypto::PublicKey;
    /// Returns little-endian local IRK bytes, or sixteen zeros when privacy is unconfigured.
    fn local_irk(&self) -> [u8; 16];
    /// Returns the public/static-random local identity for distribution, or an error if unconfigured.
    /// This may differ from the RPA used in pairing calculations.
    fn local_identity_address(&self) -> Result<Address, Error>;
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Active role and algorithm; legacy fallback replaces this value but retains shared feature data.
enum State {
    /// Secure Connections initiator.
    Central(
        /// Initiator phase and terminal result.
        central::Pairing,
    ),
    /// Secure Connections responder.
    Peripheral(
        /// Responder phase and terminal result.
        peripheral::Pairing,
    ),
    #[cfg(feature = "legacy-pairing")]
    /// Legacy initiator after feature-based downgrade.
    LegacyCentral(
        /// Legacy confirm/random/key-distribution state.
        legacy_central::Pairing,
    ),
    #[cfg(feature = "legacy-pairing")]
    /// Legacy responder selected from peer capabilities.
    LegacyPeripheral(
        /// Legacy confirm/random/key-distribution state.
        legacy_peripheral::Pairing,
    ),
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// One pairing attempt bound to shared link/feature data and a role-specific state machine.
pub struct Pairing {
    /// Negotiated features, addresses, deadline and pending/completed bond.
    pairing_data: PairingData,
    /// Algorithm/role phase and terminal outcome.
    state: State,
}

impl Pairing {
    /// Returns whether this machine initiates pairing, including legacy central mode.
    pub(crate) fn is_central(&self) -> bool {
        match &self.state {
            State::Central(_) => true,
            #[cfg(feature = "legacy-pairing")]
            State::LegacyCentral(_) => true,
            _ => false,
        }
    }

    #[cfg(feature = "legacy-pairing")]
    /// Returns whether the machine currently uses the Secure Connections central algorithm.
    pub(crate) fn is_lesc_central(&self) -> bool {
        matches!(&self.state, State::Central(_))
    }

    #[cfg(feature = "legacy-pairing")]
    /// Returns whether the machine currently uses the Secure Connections peripheral algorithm.
    pub(crate) fn is_lesc_peripheral(&self) -> bool {
        matches!(&self.state, State::Peripheral(_))
    }

    /// Returns None while unfinished, Some Ok on success, or the retained terminal error.
    pub(crate) fn result(&self) -> Option<Result<(), Error>> {
        match &self.state {
            State::Central(c) => c.result(),
            State::Peripheral(p) => p.result(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyCentral(c) => c.result(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyPeripheral(p) => p.result(),
        }
    }

    /// Advances with peer `command` and body `payload`, performing bounded `ops` and drawing `rng`.
    /// Returns terminal/state/crypto/queue errors; caller must validate exact payload size before entry
    /// because role helpers index fixed fields. Prior effects are not rolled back.
    pub(crate) fn handle_l2cap_command<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        &mut self,
        command: Command,
        payload: &[u8],
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<(), Error> {
        self.handle_input(Input::Command(command, payload), ops, rng)
    }

    /// Advances with local `event`, performing `ops` and drawing `rng` as required.
    /// Returns terminal/state/crypto/queue errors; errors may follow visible earlier side effects.
    pub(crate) fn handle_event<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        &mut self,
        event: Event,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<(), Error> {
        self.handle_input(Input::Event(event), ops, rng)
    }

    /// Routes `input` to the active role with shared state, `ops` and `rng`; Returns its result.
    /// A role records transition failure as terminal; no cross-role retry or rollback occurs here.
    fn handle_input<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        &mut self,
        input: Input<'_>,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<(), Error> {
        match &mut self.state {
            State::Central(central) => central.handle_input(input, &mut self.pairing_data, ops, rng),
            State::Peripheral(peripheral) => peripheral.handle_input(input, &mut self.pairing_data, ops, rng),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyCentral(central) => central.handle_input(input, &mut self.pairing_data, ops, rng),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyPeripheral(peripheral) => peripheral.handle_input(input, &mut self.pairing_data, ops, rng),
        }
    }

    /// Returns the retained bond's level only in a role state reached after encryption completion;
    /// otherwise NoEncryption, including a missing bond or a failed procedure.
    pub(crate) fn security_level(&self) -> SecurityLevel {
        let is_encrypted = match &self.state {
            State::Central(c) => c.is_encrypted(),
            State::Peripheral(p) => p.is_encrypted(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyCentral(c) => c.is_encrypted(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyPeripheral(p) => p.is_encrypted(),
        };
        if is_encrypted {
            self.pairing_data
                .bond_information
                .as_ref()
                .map(|x| x.security_level)
                .unwrap_or(SecurityLevel::NoEncryption)
        } else {
            SecurityLevel::NoEncryption
        }
    }

    /// Returns the borrowed pending/completed encryption record, or None before key admission.
    pub(crate) fn bond_information(&self) -> Option<&BondInformation> {
        self.pairing_data.bond_information.as_ref()
    }

    /// Returns an idle central bound to `local_address`/`peer_address` with advertised `local_io`.
    /// No packets are sent; the deadline starts at the far-future TIMEOUT_DISABLE interval.
    pub(crate) fn new_central(local_address: Address, peer_address: Address, local_io: IoCapabilities) -> Pairing {
        Pairing {
            pairing_data: PairingData::new(
                local_address,
                peer_address,
                local_io,
                crate::security_manager::constants::TIMEOUT_DISABLE,
            ),
            state: State::Central(central::Pairing::new_idle()),
        }
    }

    /// Starts central pairing for link `local_address`/`peer_address` with `local_io` through `ops`.
    /// `user_initiated` permits fresh-pairing fallback after a rejected bond. Returns the machine
    /// with active timeout or an initiation error; earlier queued work is not cancelled on failure.
    pub(crate) fn initiate_central<P: PacketPool, OPS: PairingOps<P>>(
        local_address: Address,
        peer_address: Address,
        ops: &mut OPS,
        local_io: IoCapabilities,
        user_initiated: bool,
    ) -> Result<Self, Error> {
        let mut pairing_data = PairingData::new(
            local_address,
            peer_address,
            local_io,
            crate::security_manager::constants::TIMEOUT_DISABLE,
        );
        let state = central::Pairing::initiate(&mut pairing_data, ops, user_initiated)?;
        pairing_data.timeout_at = Instant::now() + crate::security_manager::constants::TIMEOUT;
        Ok(Pairing {
            pairing_data,
            state: State::Central(state),
        })
    }

    /// Returns a Secure Connections responder awaiting a request for `local_address`/`peer_address`,
    /// advertising `local_io`, with the active timeout running and no packets sent.
    pub(crate) fn new_peripheral(local_address: Address, peer_address: Address, local_io: IoCapabilities) -> Pairing {
        Pairing {
            pairing_data: PairingData::new(
                local_address,
                peer_address,
                local_io,
                crate::security_manager::constants::TIMEOUT,
            ),
            state: State::Peripheral(peripheral::Pairing::new()),
        }
    }

    #[cfg(feature = "legacy-pairing")]
    /// Returns a legacy responder for `local_address`/`peer_address` and `local_io`, with active timeout.
    /// Selection does not send a packet or admit peer features yet.
    pub(crate) fn new_legacy_peripheral(
        local_address: Address,
        peer_address: Address,
        local_io: IoCapabilities,
    ) -> Pairing {
        Pairing {
            pairing_data: PairingData::new(
                local_address,
                peer_address,
                local_io,
                crate::security_manager::constants::TIMEOUT,
            ),
            state: State::LegacyPeripheral(legacy_peripheral::Pairing::new()),
        }
    }

    /// Sends Security Request through `ops` for `local_address`/`peer_address` with `local_io`.
    /// `user_initiated` chooses local-I/O versus retained-bond authentication requirements.
    /// Returns a timed responder awaiting Pairing Request, or packet/queue failure.
    pub(crate) fn initiate_peripheral<P: PacketPool, OPS: PairingOps<P>>(
        local_address: Address,
        peer_address: Address,
        ops: &mut OPS,
        local_io: IoCapabilities,
        user_initiated: bool,
    ) -> Result<Self, Error> {
        let pairing_data = PairingData::new(
            local_address,
            peer_address,
            local_io,
            crate::security_manager::constants::TIMEOUT,
        );
        let state = peripheral::Pairing::initiate(&pairing_data, ops, user_initiated)?;
        Ok(Pairing {
            pairing_data,
            state: State::Peripheral(state),
        })
    }

    /// Consumes a Secure Connections central; Returns legacy state preserving the exact sent request
    /// reconstructed from local features and the existing deadline. Other roles return InvalidState.
    #[cfg(feature = "legacy-pairing")]
    pub(crate) fn switch_to_legacy_central(self) -> Result<Pairing, Error> {
        use crate::codec::Encode;

        let Pairing { pairing_data, state } = self;
        match state {
            State::Central(_) => {
                let mut preq = [0u8; 7];
                preq[0] = u8::from(Command::PairingRequest);
                pairing_data.local_features.encode(&mut preq[1..]).unwrap();
                Ok(Pairing {
                    pairing_data,
                    state: State::LegacyCentral(legacy_central::Pairing::from_lesc_switch(preq)),
                })
            }
            _ => Err(Error::InvalidState),
        }
    }

    /// Consumes a Secure Connections responder; Returns a legacy request-waiting machine preserving
    /// shared data/deadline. Other roles return InvalidState; callers enforce downgrade policy.
    #[cfg(feature = "legacy-pairing")]
    pub(crate) fn switch_to_legacy_peripheral(self) -> Result<Pairing, Error> {
        let Pairing { pairing_data, state } = self;
        match state {
            State::Peripheral(_) => Ok(Pairing {
                pairing_data,
                state: State::LegacyPeripheral(legacy_peripheral::Pairing::new()),
            }),
            _ => Err(Error::InvalidState),
        }
    }

    /// Returns true only for Secure Connections central waiting on a retained-key encryption request.
    pub(crate) fn is_waiting_bonded_encryption(&self) -> bool {
        match &self.state {
            State::Central(c) => c.is_waiting_bonded_encryption(),
            _ => false,
        }
    }

    /// Returns the peer address retained in shared pairing data for manager routing.
    pub(crate) fn peer_address(&self) -> Address {
        self.pairing_data.peer_address
    }

    /// Returns the active absolute deadline, or now plus TIMEOUT_DISABLE after a terminal result.
    pub(crate) fn timeout_at(&self) -> Instant {
        if self.result().is_some() {
            Instant::now() + crate::security_manager::constants::TIMEOUT_DISABLE
        } else {
            self.pairing_data.timeout_at
        }
    }

    /// Replaces the stored deadline with current monotonic time plus the SMP timeout interval.
    pub(crate) fn reset_timeout(&mut self) {
        self.pairing_data.timeout_at = Instant::now() + crate::security_manager::constants::TIMEOUT;
    }

    /// Delegates timeout termination to the current role; completed states remain terminal unchanged.
    pub(crate) fn mark_timeout(&mut self) {
        match &mut self.state {
            State::Central(c) => c.mark_timeout(),
            State::Peripheral(p) => p.mark_timeout(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyCentral(c) => c.mark_timeout(),
            #[cfg(feature = "legacy-pairing")]
            State::LegacyPeripheral(p) => p.mark_timeout(),
        }
    }
}

/// OOB data for BLE pairing, exchanged via an out-of-band channel.
///
/// Values are little-endian. For LESC, random is r and confirm is f4(PKx, PKx, r, 0).
/// Both fields zero denote absent peer data in the current LESC API. Local confirm is not consumed.
/// An absent peer value is rejected with OobNotAvailable if local OOB availability was advertised.
/// For legacy, random is the shared TK and confirm is unused. The application authenticates the
/// external exchange; Debug exposes this material and must not be used in secret-free logs.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OobData {
    /// Sixteen little-endian random bytes (LESC) or shared temporary-key bytes (legacy).
    pub random: [u8; 16],
    /// Sixteen little-endian f4 commitment bytes (peer LESC verification); unused for legacy/local data.
    pub confirm: [u8; 16],
}

/// Local inputs delivered by the manager; successful queueing is distinct from accepting the input.
pub enum Event {
    /// Controller encryption completion or failure.
    LinkEncryptedResult(
        /// True for enabled encryption, false for failure/disabled state.
        bool,
    ),
    /// User accepts the displayed numeric comparison.
    PassKeyConfirm,
    /// User rejects/cancels the displayed numeric comparison.
    PassKeyCancel,
    /// User supplies the passkey; caller must enforce the six-digit range.
    PassKeyInput(
        /// Decimal input, intended 0 through 999999; constructors do not validate it.
        u32,
    ),
    /// Externally exchanged material for the selected OOB phase.
    OobDataReceived {
        /// Local generated random, or legacy TK; local LESC confirm is unused.
        local: OobData,
        /// Peer LESC random/commitment or legacy material; zero LESC fields signal absence.
        peer: OobData,
    },
}

#[cfg(test)]
/// Deterministic in-memory packet exchange verifies pairing algorithms independently of an HCI controller.
pub(super) mod tests {
    use rand_chacha::{ChaCha12Core, ChaCha12Rng};
    use rand_core::SeedableRng;

    use super::*;
    use crate::security_manager::Reason;
    use crate::{Identity, Packet};

    #[derive(Debug)]
    /// Fixed-capacity test packet whose logical size is initialized to the complete pool MTU.
    pub(crate) struct TestPacket(
        /// Up to 128 owned packet bytes, including L2CAP headers.
        pub(crate) heapless::Vec<u8, 128>,
    );

    impl AsRef<[u8]> for TestPacket {
        /// Returns the complete initialized packet byte slice.
        fn as_ref(&self) -> &[u8] {
            self.0.as_slice()
        }
    }

    impl AsMut<[u8]> for TestPacket {
        /// Returns the complete initialized packet bytes for header/payload construction.
        fn as_mut(&mut self) -> &mut [u8] {
            self.0.as_mut_slice()
        }
    }

    impl Packet for TestPacket {}

    #[derive(Debug)]
    /// Test pool allocating independent 128-byte stack-backed packets without a global allocation cap.
    pub(crate) struct HeaplessPool;

    impl PacketPool for HeaplessPool {
        /// Owned initialized packet storage used by all local exchanges.
        type Packet = TestPacket;
        /// Packet capacity in bytes including the L2CAP/SMP headers.
        const MTU: usize = 128;

        /// Returns a zeroed 128-byte packet; never reports pool exhaustion in this fixture.
        fn allocate() -> Option<Self::Packet> {
            let mut ret = TestPacket(heapless::Vec::new());
            ret.0.resize(Self::MTU, 0).unwrap();
            Some(ret)
        }

        /// Returns isize::MAX as the synthetic pool capacity; no global resource limit is simulated.
        fn capacity() -> usize {
            isize::MAX as usize
        }
    }

    /// Per-peer transcript and key material for controller-free pairing tests.
    pub(crate) struct TestOps<const N: usize> {
        /// Accepted outbound packets, bounded by N; test cursors track delivery without removing them.
        pub(crate) sent_packets: heapless::Vec<TxPacket<HeaplessPool>, N>,
        /// Requested LTKs, capacity 10; does not itself establish encryption.
        pub(crate) encryptions: heapless::Vec<LongTermKey, 10>,
        /// User-visible events retained in order, capacity 10.
        pub(crate) connection_events: heapless::Vec<ConnectionEvent, 10>,
        /// Optional previously retained record supplied to resumption paths.
        pub(crate) bond_information: Option<BondInformation>,
        /// Whether local features request persistent bonding.
        pub(crate) bondable: bool,
        /// Whether local features advertise possession of peer OOB data.
        pub(crate) oob_available: bool,
        /// Synthetic local IRK bytes used by identity-distribution fixtures; zero disables local privacy.
        pub(crate) local_irk: [u8; 16],
        /// Optional fixed passkey consumed by the SC peripheral display path.
        pub(crate) passkey: Option<u32>,
        /// Seeded local P-256 private scalar.
        pub(crate) secret_key: crate::security_manager::crypto::SecretKey,
        /// Matching public point used to generate OOB commitments and exchange packets.
        pub(crate) public_key: crate::security_manager::crypto::PublicKey,
    }

    impl<const N: usize> TestOps<N> {
        /// Returns empty transcripts and a deterministic keypair from `seed`; bonding/OOB start disabled.
        pub(crate) fn new(seed: u64) -> Self {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(seed).into();
            let secret_key = crate::security_manager::crypto::SecretKey::new(&mut rng);
            let public_key = secret_key.public_key();
            Self {
                sent_packets: heapless::Vec::new(),
                encryptions: heapless::Vec::new(),
                connection_events: heapless::Vec::new(),
                bond_information: None,
                bondable: false,
                oob_available: false,
                passkey: None,
                local_irk: [0; 16],
                secret_key,
                public_key,
            }
        }
    }

    impl<const N: usize> PairingOps<HeaplessPool> for TestOps<N> {
        /// Retains `packet` in send order; Returns OutOfMemory when the N-entry transcript is full.
        fn try_send_packet(&mut self, packet: TxPacket<HeaplessPool>) -> Result<(), Error> {
            self.sent_packets.push(packet).map_err(|_| Error::OutOfMemory)
        }

        /// Records `ltk`; Returns a pending record with `security_level`, `is_bonded`, default identity
        /// and optional legacy `ediv`, wire `rand`, byte `encryption_key_len`. Panics after 10 requests;
        /// no controller completion or persistent bond mutation is simulated.
        fn try_enable_encryption(
            &mut self,
            ltk: &LongTermKey,
            security_level: SecurityLevel,
            is_bonded: bool,
            #[cfg(feature = "legacy-pairing")] ediv: u16,
            #[cfg(feature = "legacy-pairing")] rand: [u8; 8],
            #[cfg(feature = "legacy-pairing")] encryption_key_len: u8,
        ) -> Result<BondInformation, Error> {
            self.encryptions.push(*ltk).unwrap();
            Ok(BondInformation {
                security_level,
                identity: Identity::default(),
                ltk: *ltk,
                is_bonded,
                #[cfg(feature = "legacy-pairing")]
                ediv,
                #[cfg(feature = "legacy-pairing")]
                rand,
                #[cfg(feature = "legacy-pairing")]
                encryption_key_len,
            })
        }

        /// Returns the configured retained record clone, or None; no peer filtering is simulated.
        fn find_bond(&self) -> Option<BondInformation> {
            self.bond_information.clone()
        }

        /// Records the retained LTK and Returns its cloned record, or None when absent.
        /// Panics when the 10-entry encryption transcript is full.
        fn try_enable_bonded_encryption(&mut self) -> Result<Option<BondInformation>, Error> {
            if let Some(bond) = &self.bond_information {
                self.encryptions.push(bond.ltk).unwrap();
                Ok(Some(bond.clone()))
            } else {
                Ok(None)
            }
        }

        /// Ignores `bond`; Returns Ok without changing the fixture's configured retained record.
        fn try_update_bond_information(&mut self, bond: &BondInformation) -> Result<(), Error> {
            Ok(())
        }

        /// Returns synthetic controller handle 2 without mutation.
        fn connection_handle(&mut self) -> ConnHandle {
            ConnHandle::new(2)
        }

        /// Appends `event` to its transcript; Returns Ok or panics when ten events are already retained.
        fn try_send_connection_event(&mut self, event: ConnectionEvent) -> Result<(), Error> {
            self.connection_events.push(event).unwrap();
            Ok(())
        }

        /// Returns the fixture's requested bonding policy.
        fn bonding_flag(&self) -> BondingFlag {
            if self.bondable {
                BondingFlag::Bonding
            } else {
                BondingFlag::NoBonding
            }
        }

        /// Returns the fixture's advertised possession of peer OOB data.
        fn oob_available(&self) -> bool {
            self.oob_available
        }

        /// Returns the configured fixed passkey without validation, or None for random display.
        fn passkey(&self) -> Option<u32> {
            self.passkey
        }

        /// Returns the borrowed deterministic private scalar.
        fn secret_key(&self) -> &crate::security_manager::crypto::SecretKey {
            &self.secret_key
        }

        /// Returns the borrowed public point corresponding to the fixture's private scalar.
        fn public_key(&self) -> &crate::security_manager::crypto::PublicKey {
            &self.public_key
        }

        /// Returns configured synthetic IRK bytes; the default zeros simulate no local privacy.
        fn local_irk(&self) -> [u8; 16] {
            self.local_irk
        }

        /// Returns the fixed static-random identity used by distribution fixtures.
        fn local_identity_address(&self) -> Result<Address, Error> {
            Ok(Address::random([0xff, 0x8f, 0x08, 0x05, 0xe4, 0xff]))
        }
    }

    #[test]
    /// Just Works produces equal LTKs and reports unauthenticated encryption only after HCI success.
    fn just_works() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::Encrypted,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::Encrypted,
                bond: None
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::Encrypted);
        assert_eq!(peripheral_pairing.security_level(), SecurityLevel::Encrypted);
    }

    #[test]
    /// Both displayed comparison values agree; accepting both produces one authenticated LTK/level.
    fn numeric_compare() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::DisplayYesNo);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::DisplayYesNo,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        let (central_numeric, peripheral_numeric) = {
            let central = match &central_ops.connection_events[0] {
                ConnectionEvent::PassKeyConfirm(n) => n,
                _ => panic!("Unexpected connection event"),
            };

            let peripheral = match &peripheral_ops.connection_events[0] {
                ConnectionEvent::PassKeyConfirm(n) => n,
                _ => panic!("Unexpected connection event"),
            };

            (*central, *peripheral)
        };

        assert_eq!(central_numeric, peripheral_numeric);
        central_pairing
            .handle_event(Event::PassKeyConfirm, &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::PassKeyConfirm, &mut peripheral_ops, &mut rng)
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    #[test]
    /// Matching user entry at two keyboards completes all 20 rounds and yields authenticated encryption.
    fn pass_key_entry_keyboard_only() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::KeyboardOnly);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::KeyboardOnly,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PassKeyInput
        ));
        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PassKeyInput
        ));

        central_pairing
            .handle_event(Event::PassKeyInput(123456), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::PassKeyInput(123456), &mut peripheral_ops, &mut rng)
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    #[test]
    /// A peripheral-displayed passkey entered by the central yields matching authenticated keys.
    fn pass_key_entry_peripheral_display() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::DisplayOnly);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::KeyboardOnly,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        let pass_key = match &peripheral_ops.connection_events[0] {
            ConnectionEvent::PassKeyDisplay(pk) => *pk,
            _ => panic!("Unexpected connection event"),
        };

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PassKeyInput
        ));

        central_pairing
            .handle_event(Event::PassKeyInput(pass_key.value()), &mut central_ops, &mut rng)
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    #[test]
    /// A central-displayed passkey entered by the peripheral yields matching authenticated keys.
    fn pass_key_entry_central_display() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::KeyboardOnly);
        let mut central_pairing =
            Pairing::initiate_central(central, peripheral, &mut central_ops, IoCapabilities::DisplayOnly, true)
                .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        let pass_key = match &central_ops.connection_events[0] {
            ConnectionEvent::PassKeyDisplay(pk) => *pk,
            _ => panic!("Unexpected connection event"),
        };

        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PassKeyInput
        ));

        peripheral_pairing
            .handle_event(Event::PassKeyInput(pass_key.value()), &mut peripheral_ops, &mut rng)
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[1],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    #[test]
    /// Bilateral bondable Just Works distributes identities before reporting a retained unauthenticated bond.
    fn bondable_just_works() {
        bondable_just_works_with_irk([0; 16]);
    }

    /// Completes bilateral bondable Just Works using synthetic `irk` for both peers; zero omits responder IRK distribution.
    /// Panics unless LTK agreement, both retained bonds and final security levels match the protocol fixture.
    pub(crate) fn bondable_just_works_with_irk(irk: [u8; 16]) {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);
        peripheral_ops.bondable = true;
        central_ops.bondable = true;
        peripheral_ops.local_irk = irk;
        central_ops.local_irk = irk;

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        // Exchange identity keys after encryption
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::Encrypted,
                bond: Some(BondInformation {
                    is_bonded: true,
                    security_level: SecurityLevel::Encrypted,
                    ..
                })
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::Encrypted,
                bond: Some(BondInformation {
                    is_bonded: true,
                    security_level: SecurityLevel::Encrypted,
                    ..
                })
            }
        ));
        assert_eq!(central_pairing.security_level(), SecurityLevel::Encrypted);
        assert_eq!(peripheral_pairing.security_level(), SecurityLevel::Encrypted);
    }

    #[test]
    /// Central resumption requests the retained key without SMP packets and reports authentication on HCI success.
    fn bonded_central_initiates() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);
        central_ops.bond_information = Some(BondInformation {
            security_level: SecurityLevel::EncryptedAuthenticated,
            is_bonded: true,
            ltk: LongTermKey(1),
            identity: peripheral.into(),
            #[cfg(feature = "legacy-pairing")]
            ediv: 0,
            #[cfg(feature = "legacy-pairing")]
            rand: [0; 8],
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 16,
        });

        peripheral_ops.bond_information = Some(BondInformation {
            security_level: SecurityLevel::EncryptedAuthenticated,
            is_bonded: true,
            ltk: LongTermKey(1),
            identity: central.into(),
            #[cfg(feature = "legacy-pairing")]
            ediv: 0,
            #[cfg(feature = "legacy-pairing")]
            rand: [0; 8],
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 16,
        });

        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        let mut peripheral_pairing = Pairing::new_peripheral(peripheral, central, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central,
            peripheral,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();
        assert_eq!(central_ops.sent_packets.len(), 0);
        assert_eq!(peripheral_ops.sent_packets.len(), 0);
        assert_eq!(central_ops.encryptions.len(), 1);
        assert_eq!(central_ops.encryptions[0], LongTermKey(1));

        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_ops.connection_events.len(), 1);
        assert_eq!(peripheral_ops.connection_events.len(), 1);
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    #[test]
    /// Peripheral Security Request triggers central retained-key reuse without a fresh Pairing Request.
    fn bonded_peripheral_initiates() {
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<80>::new(0xDEAD);
        let mut central_ops = TestOps::<80>::new(0xBEEF);
        central_ops.bond_information = Some(BondInformation {
            security_level: SecurityLevel::EncryptedAuthenticated,
            is_bonded: true,
            ltk: LongTermKey(1),
            identity: peripheral.into(),
            #[cfg(feature = "legacy-pairing")]
            ediv: 0,
            #[cfg(feature = "legacy-pairing")]
            rand: [0; 8],
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 16,
        });

        peripheral_ops.bond_information = Some(BondInformation {
            security_level: SecurityLevel::EncryptedAuthenticated,
            is_bonded: true,
            ltk: LongTermKey(1),
            identity: central.into(),
            #[cfg(feature = "legacy-pairing")]
            ediv: 0,
            #[cfg(feature = "legacy-pairing")]
            rand: [0; 8],
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 16,
        });

        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        let mut peripheral_pairing = Pairing::initiate_peripheral(
            peripheral,
            central,
            &mut peripheral_ops,
            IoCapabilities::NoInputNoOutput,
            false,
        )
        .unwrap();
        let mut central_pairing = Pairing::new_central(central, peripheral, IoCapabilities::NoInputNoOutput);

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.sent_packets.len(), 0);
        assert_eq!(peripheral_ops.sent_packets.len(), 1);
        assert_eq!(central_ops.encryptions.len(), 1);
        assert_eq!(central_ops.encryptions[0], LongTermKey(1));

        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(matches!(
            central_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert!(matches!(
            peripheral_ops.connection_events[0],
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                bond: None
            }
        ));
        assert_eq!(central_ops.connection_events.len(), 1);
        assert_eq!(peripheral_ops.connection_events.len(), 1);
        assert_eq!(central_pairing.security_level(), SecurityLevel::EncryptedAuthenticated);
        assert_eq!(
            peripheral_pairing.security_level(),
            SecurityLevel::EncryptedAuthenticated
        );
    }

    /// Delivers unconsumed transcripts from `central_ops`/`peripheral_ops` to the opposite
    /// `peripheral_pairing`/`central_pairing`, drawing deterministic `rng` as needed.
    /// Advances `num_central_data_sent`/`num_peripheral_data_sent` after successful deliveries;
    /// stops at quiescence, panics on a transition error or more than 10000 productive passes.
    fn transmit_packets<const N: usize>(
        peripheral_ops: &mut TestOps<N>,
        central_ops: &mut TestOps<N>,
        rng: &mut ChaCha12Rng,
        peripheral_pairing: &mut Pairing,
        central_pairing: &mut Pairing,
        num_central_data_sent: &mut usize,
        num_peripheral_data_sent: &mut usize,
    ) {
        let mut loop_count = 0;
        loop {
            let saved_num_central_data_sent = *num_central_data_sent;
            let saved_num_peripheral_data_sent = *num_peripheral_data_sent;

            while *num_central_data_sent < central_ops.sent_packets.len() {
                peripheral_pairing
                    .handle_l2cap_command(
                        central_ops.sent_packets[*num_central_data_sent].command,
                        central_ops.sent_packets[*num_central_data_sent].payload(),
                        peripheral_ops,
                        rng,
                    )
                    .unwrap();
                *num_central_data_sent += 1;
            }

            while *num_peripheral_data_sent < peripheral_ops.sent_packets.len() {
                central_pairing
                    .handle_l2cap_command(
                        peripheral_ops.sent_packets[*num_peripheral_data_sent].command,
                        peripheral_ops.sent_packets[*num_peripheral_data_sent].payload(),
                        central_ops,
                        rng,
                    )
                    .unwrap();
                *num_peripheral_data_sent += 1;
            }

            if saved_num_central_data_sent == *num_central_data_sent
                && saved_num_peripheral_data_sent == *num_peripheral_data_sent
            {
                break;
            }

            loop_count += 1;
            if loop_count > 10000 {
                panic!("Too many loops");
            }
        }
    }

    #[test]
    /// Bilateral OOB commitments bind both public keys and yield matching authenticated encryption.
    fn oob_lesc_bilateral() {
        use crate::security_manager::crypto::Nonce;

        let peripheral_addr = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central_addr = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);
        peripheral_ops.oob_available = true;
        central_ops.oob_available = true;

        // Generate OOB data for each side from their persistent keypair
        let central_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(100).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(central_ops.public_key.x(), central_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };
        let peripheral_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(200).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(peripheral_ops.public_key.x(), peripheral_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };

        let mut peripheral_pairing =
            Pairing::new_peripheral(peripheral_addr, central_addr, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central_addr,
            peripheral_addr,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        // Exchange packets until both sides pause for OOB data
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        // Both sides should have sent OobRequest
        assert!(peripheral_ops
            .connection_events
            .iter()
            .any(|e| matches!(e, ConnectionEvent::OobRequest)));
        assert!(central_ops
            .connection_events
            .iter()
            .any(|e| matches!(e, ConnectionEvent::OobRequest)));

        // Provide OOB data to both sides
        // Central gets: local=central_oob, peer=peripheral_oob
        central_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: central_oob,
                    peer: peripheral_oob,
                },
                &mut central_ops,
                &mut rng,
            )
            .unwrap();
        // Peripheral gets: local=peripheral_oob, peer=central_oob
        peripheral_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: peripheral_oob,
                    peer: central_oob,
                },
                &mut peripheral_ops,
                &mut rng,
            )
            .unwrap();

        // Continue packet exchange
        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        // Both sides should agree on the LTK
        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);

        // Simulate encryption success
        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        // OOB is authenticated
        assert!(central_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
        assert!(peripheral_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
    }

    /// OOB pairing where only the central has OOB data (central OOB=1, peripheral OOB=0).
    /// Per spec 2.3.5.6.3, the peripheral skips the confirm check for the central's OOB
    /// and sets ra=0 since it didn't receive the central's OOB data.
    #[test]
    fn oob_lesc_central_only() {
        use crate::security_manager::crypto::Nonce;

        let peripheral_addr = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central_addr = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);
        // Only central has OOB
        peripheral_ops.oob_available = false;
        central_ops.oob_available = true;

        // Generate OOB data for central (the side that has it)
        let central_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(100).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(central_ops.public_key.x(), central_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };
        // Peripheral generates local OOB but has no peer OOB data
        let peripheral_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(200).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(peripheral_ops.public_key.x(), peripheral_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };
        let no_oob = OobData {
            random: [0; 16],
            confirm: [0; 16],
        };

        let mut peripheral_pairing =
            Pairing::new_peripheral(peripheral_addr, central_addr, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central_addr,
            peripheral_addr,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        // Central has peripheral's OOB data (received out of band)
        central_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: central_oob,
                    peer: peripheral_oob,
                },
                &mut central_ops,
                &mut rng,
            )
            .unwrap();
        // Peripheral has no peer OOB data — zeros for peer
        peripheral_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: peripheral_oob,
                    peer: no_oob,
                },
                &mut peripheral_ops,
                &mut rng,
            )
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);

        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(central_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
        assert!(peripheral_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
    }

    /// OOB pairing where only the peripheral has OOB data (central OOB=0, peripheral OOB=1).
    #[test]
    fn oob_lesc_peripheral_only() {
        use crate::security_manager::crypto::Nonce;

        let peripheral_addr = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central_addr = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);
        // Only peripheral has OOB
        peripheral_ops.oob_available = true;
        central_ops.oob_available = false;

        // Peripheral generates local OOB data
        let peripheral_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(200).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(peripheral_ops.public_key.x(), peripheral_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };
        // Central generates local OOB but has no peer OOB data
        let central_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(100).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(central_ops.public_key.x(), central_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };
        let no_oob = OobData {
            random: [0; 16],
            confirm: [0; 16],
        };

        let mut peripheral_pairing =
            Pairing::new_peripheral(peripheral_addr, central_addr, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central_addr,
            peripheral_addr,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        // Central has no peer OOB data — zeros for peer
        central_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: central_oob,
                    peer: no_oob,
                },
                &mut central_ops,
                &mut rng,
            )
            .unwrap();
        // Peripheral has central's OOB data (received out of band)
        peripheral_pairing
            .handle_event(
                Event::OobDataReceived {
                    local: peripheral_oob,
                    peer: central_oob,
                },
                &mut peripheral_ops,
                &mut rng,
            )
            .unwrap();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        assert_eq!(central_ops.encryptions[0], peripheral_ops.encryptions[0]);

        central_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut central_ops, &mut rng)
            .unwrap();
        peripheral_pairing
            .handle_event(Event::LinkEncryptedResult(true), &mut peripheral_ops, &mut rng)
            .unwrap();

        assert!(central_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
        assert!(peripheral_ops.connection_events.iter().any(|e| matches!(
            e,
            ConnectionEvent::PairingComplete {
                security_level: SecurityLevel::EncryptedAuthenticated,
                ..
            }
        )));
    }

    #[test]
    /// A wrong peer OOB commitment terminates the central before any encryption request.
    fn oob_lesc_confirm_mismatch() {
        use crate::security_manager::crypto::Nonce;

        let peripheral_addr = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central_addr = Address::random([0xff, 2, 2, 3, 4, 5]);

        let mut peripheral_ops = TestOps::<10>::new(0xDEAD);
        let mut central_ops = TestOps::<10>::new(0xBEEF);
        peripheral_ops.oob_available = true;
        central_ops.oob_available = true;

        // Generate valid central OOB
        let central_oob = {
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(100).into();
            let r = Nonce::new(&mut rng);
            let c = r.f4(central_ops.public_key.x(), central_ops.public_key.x(), 0);
            OobData {
                random: r.0.to_le_bytes(),
                confirm: c.0.to_le_bytes(),
            }
        };

        // Generate peripheral OOB with WRONG confirm
        let peripheral_oob = OobData {
            random: [1; 16],
            confirm: [2; 16], // Wrong confirm
        };

        let mut peripheral_pairing =
            Pairing::new_peripheral(peripheral_addr, central_addr, IoCapabilities::NoInputNoOutput);
        let mut central_pairing = Pairing::initiate_central(
            central_addr,
            peripheral_addr,
            &mut central_ops,
            IoCapabilities::NoInputNoOutput,
            true,
        )
        .unwrap();

        let mut num_central_data_sent = 0;
        let mut num_peripheral_data_sent = 0;
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();

        transmit_packets(
            &mut peripheral_ops,
            &mut central_ops,
            &mut rng,
            &mut peripheral_pairing,
            &mut central_pairing,
            &mut num_central_data_sent,
            &mut num_peripheral_data_sent,
        );

        // Central tries to verify the bad peripheral OOB — should fail
        let result = central_pairing.handle_event(
            Event::OobDataReceived {
                local: central_oob,
                peer: peripheral_oob,
            },
            &mut central_ops,
            &mut rng,
        );

        assert!(matches!(
            result,
            Err(Error::Security(crate::security_manager::Reason::ConfirmValueFailed))
        ));
    }

    /// Returns each role's admission result when both advertised peer OOB but receive no peer material.
    /// Valid local commitments isolate the missing-peer case; neither role may request encryption.
    fn missing_advertised_peer_oob_results() -> (Result<(), Error>, Result<(), Error>) {
        use crate::security_manager::crypto::Nonce;
        let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
        let central = Address::random([0xff, 2, 2, 3, 4, 5]);
        let mut p_ops = TestOps::<10>::new(0xDEAD);
        let mut c_ops = TestOps::<10>::new(0xBEEF);
        p_ops.oob_available = true;
        c_ops.oob_available = true;
        let local_c = OobData {
            random: 1u128.to_le_bytes(),
            confirm: Nonce(1)
                .f4(c_ops.public_key.x(), c_ops.public_key.x(), 0)
                .0
                .to_le_bytes(),
        };
        let local_p = OobData {
            random: 2u128.to_le_bytes(),
            confirm: Nonce(2)
                .f4(p_ops.public_key.x(), p_ops.public_key.x(), 0)
                .0
                .to_le_bytes(),
        };
        let missing = OobData {
            random: [0; 16],
            confirm: [0; 16],
        };
        let mut p = Pairing::new_peripheral(peripheral, central, IoCapabilities::NoInputNoOutput);
        let mut c =
            Pairing::initiate_central(central, peripheral, &mut c_ops, IoCapabilities::NoInputNoOutput, true).unwrap();
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
        transmit_packets(&mut p_ops, &mut c_ops, &mut rng, &mut p, &mut c, &mut 0, &mut 0);
        let central_result = c.handle_event(
            Event::OobDataReceived {
                local: local_c,
                peer: missing,
            },
            &mut c_ops,
            &mut rng,
        );
        let peripheral_result = p.handle_event(
            Event::OobDataReceived {
                local: local_p,
                peer: missing,
            },
            &mut p_ops,
            &mut rng,
        );
        assert!(c_ops.encryptions.is_empty());
        assert!(p_ops.encryptions.is_empty());
        (central_result, peripheral_result)
    }

    /// A central advertising peer OOB cannot silently skip authentication when that material is absent.
    #[test]
    fn central_rejects_missing_advertised_peer_oob() {
        assert_eq!(
            missing_advertised_peer_oob_results().0,
            Err(Error::Security(Reason::OobNotAvailable))
        );
    }

    /// A peripheral advertising peer OOB cannot silently skip authentication when that material is absent.
    #[test]
    fn peripheral_rejects_missing_advertised_peer_oob() {
        assert_eq!(
            missing_advertised_peer_oob_results().1,
            Err(Error::Security(Reason::OobNotAvailable))
        );
    }

    /// Legacy peers agree on masked STK and durable responder key/EDIV/Rand for every admitted byte size.
    /// HCI completion separates temporary encryption admission from bonded key distribution.
    #[cfg(feature = "legacy-pairing")]
    #[test]
    fn legacy_pairing_preserves_negotiated_key_size_and_distributed_bond() {
        for key_len in 7..=16 {
            let peripheral = Address::random([0xff, 1, 2, 3, 4, 5]);
            let central = Address::random([0xff, 2, 2, 3, 4, 5]);
            let mut p_ops = TestOps::<20>::new(0xDEAD);
            let mut c_ops = TestOps::<20>::new(0xBEEF);
            p_ops.bondable = true;
            c_ops.bondable = true;
            let mut p = Pairing::new_legacy_peripheral(peripheral, central, IoCapabilities::NoInputNoOutput);
            p.pairing_data.local_features.maximum_encryption_key_size = key_len;
            let mut c =
                Pairing::initiate_central(central, peripheral, &mut c_ops, IoCapabilities::NoInputNoOutput, true)
                    .unwrap()
                    .switch_to_legacy_central()
                    .unwrap();
            let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(1).into();
            let (mut c_sent, mut p_sent) = (0, 0);
            transmit_packets(
                &mut p_ops,
                &mut c_ops,
                &mut rng,
                &mut p,
                &mut c,
                &mut c_sent,
                &mut p_sent,
            );
            assert_eq!(c_ops.encryptions.len(), 1);
            assert_eq!(p_ops.encryptions.len(), 1);
            assert_eq!(c_ops.encryptions[0], p_ops.encryptions[0]);
            assert_eq!(c.security_level(), SecurityLevel::NoEncryption);
            assert_eq!(p.security_level(), SecurityLevel::NoEncryption);
            assert_eq!(c.result(), None);
            assert_eq!(p.result(), None);
            if key_len < 16 {
                assert_eq!(c_ops.encryptions[0].0 >> (u32::from(key_len) * 8), 0);
            }
            c.handle_event(Event::LinkEncryptedResult(true), &mut c_ops, &mut rng)
                .unwrap();
            p.handle_event(Event::LinkEncryptedResult(true), &mut p_ops, &mut rng)
                .unwrap();
            transmit_packets(
                &mut p_ops,
                &mut c_ops,
                &mut rng,
                &mut p,
                &mut c,
                &mut c_sent,
                &mut p_sent,
            );
            assert_eq!(c.result(), Some(Ok(())));
            assert_eq!(p.result(), Some(Ok(())));
            let c_bond = c.bond_information().unwrap();
            let p_bond = p.bond_information().unwrap();
            assert!(c_bond.is_bonded && p_bond.is_bonded);
            assert_eq!(c_bond.encryption_key_len, key_len);
            assert_eq!(p_bond.encryption_key_len, key_len);
            assert_eq!(
                (c_bond.ltk, c_bond.ediv, c_bond.rand),
                (p_bond.ltk, p_bond.ediv, p_bond.rand)
            );
            if key_len < 16 {
                assert_eq!(c_bond.ltk.0 >> (u32::from(key_len) * 8), 0);
            }
        }
    }
}
