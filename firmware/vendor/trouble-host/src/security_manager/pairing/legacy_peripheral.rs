//! Legacy responder: validate features, authenticate c1 with a temporary key, mask STK to the
//! negotiated size, then distribute the durable responder key only after HCI encryption completion.
//! Manager entry validates command lengths; packet/bond effects before errors are not rolled back.

use bt_hci::param::{AddrKind, BdAddr};
use rand::Rng;
use rand_core::{CryptoRng, RngCore};

use crate::codec::{Decode, Encode};
use crate::connection::ConnectionEvent;
use crate::security_manager::pairing::util::{
    choose_legacy_pairing_method, make_central_identification_packet, make_encryption_information_packet,
    make_identity_address_information_packet, make_identity_information_packet, make_pairing_random, prepare_packet,
    PairingMethod, PassKeyEntryAction,
};
use crate::security_manager::pairing::{Event, Input, PairingData, PairingOps};
use crate::security_manager::types::{AuthReq, Command, PairingFeatures, PassKey};
use crate::security_manager::{crypto, Reason};
use crate::{Address, Error, IdentityResolvingKey, IoCapabilities, LongTermKey, PacketPool};

/// Data needed during the confirm/random exchange phase of legacy pairing.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(super) struct ConfirmPhaseData {
    /// Numeric temporary key: zero for Just Works, decimal passkey or shared external OOB TK.
    tk: u128,
    /// Pairing Request command bytes (opcode + 6 feature bytes)
    preq: [u8; 7],
    /// Pairing Response command bytes (opcode + 6 feature bytes)
    pres: [u8; 7],
    /// Numeric responder Srand; generated before Sconfirm, replacing zero draws with one.
    local_nonce: u128,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Legacy responder phases with owned c1 transcript and key-distribution state.
pub(super) enum Pairing {
    /// Await Pairing Request or retained-key encryption completion.
    WaitingPairingRequest,
    /// Await user passkey, optionally retaining one early commitment.
    WaitingPassKeyInput {
        /// Latest early 16-byte little-endian Mconfirm, if received.
        confirm_bytes: Option<[u8; size_of::<u128>()]>,
        /// Exact seven-byte wire request including opcode.
        preq: [u8; 7],
        /// Exact seven-byte wire response including opcode.
        pres: [u8; 7],
    },
    /// Await shared temporary key through the application's external channel.
    WaitingOobData {
        /// Exact seven-byte wire Pairing Request.
        preq: [u8; 7],
        /// Exact seven-byte wire Pairing Response.
        pres: [u8; 7],
    },
    /// TK selected; await Mconfirm before generating Srand/Sconfirm.
    WaitingPairingConfirm(
        /// Temporary key and c1 feature transcript.
        ConfirmPhaseData,
    ),
    /// Sconfirm sent; await Mrand and verify Mconfirm.
    WaitingPairingRandom {
        /// Temporary key, c1 transcript and local Srand.
        confirm_data: ConfirmPhaseData,
        /// Numeric initiator Mconfirm to authenticate before sending Srand.
        peer_confirm: u128,
    },
    /// STK encryption requested; await HCI outcome.
    WaitingLinkEncrypted,
    /// Link encrypted; await negotiated initiator IRK.
    WaitingIdentitityInformation,
    /// Initiator IRK accepted; await identity address.
    WaitingIdentitityAddressInformation,
    /// Consume initiator legacy key pair while retaining this responder's durable key for resumption.
    ReceivingKeys {
        /// Zero waits for LTK; one waits for EDIV/Rand.
        phase: u8,
        /// Received initiator LTK, consumed then discarded; zero placeholder in phase 0.
        received_ltk: LongTermKey,
    },
    /// Encryption and agreed key distribution finished.
    Success,
    /// Terminal transition or timeout failure.
    Error(
        /// Retained failure reported to the manager.
        Error,
    ),
}

/// Returns c1 type 0 for PUBLIC `addr`, type 1 otherwise; caller supplies an actual public/random link address.
fn addr_type_flag(addr: &Address) -> u8 {
    if addr.kind == AddrKind::PUBLIC {
        0
    } else {
        1
    }
}

/// Returns `addr`'s six bytes in most-significant-octet order required by c1.
fn addr_bytes_mso(addr: &Address) -> [u8; 6] {
    let mut bytes = addr.addr.into_inner();
    bytes.reverse();
    bytes
}

impl Pairing {
    /// Returns None while unfinished, Some Ok on Success, or the retained terminal error.
    pub fn result(&self) -> Option<Result<(), Error>> {
        match self {
            Self::Success => Some(Ok(())),
            Self::Error(e) => Some(Err(e.clone())),
            _ => None,
        }
    }

    /// Marks an active phase Timeout without packet transmission; existing terminal results remain.
    pub(crate) fn mark_timeout(&mut self) {
        if matches!(self, Self::Success | Self::Error(_)) {
            return;
        }
        *self = Self::Error(Error::Timeout);
    }

    /// Returns a request-waiting legacy responder without allocating or sending a packet.
    pub fn new() -> Self {
        Self::WaitingPairingRequest
    }

    /// Returns true in key/identity distribution phases after HCI success, or on completed Success.
    pub(crate) fn is_encrypted(&self) -> bool {
        matches!(
            self,
            Self::WaitingIdentitityInformation
                | Self::WaitingIdentitityAddressInformation
                | Self::ReceivingKeys { .. }
                | Self::Success
        )
    }

    // --- FSM core ---

    /// Applies `input` using shared `pairing_data`, bounded `ops` and cryptographic `rng`.
    /// Returns terminal failure or Ok; a transition error replaces the phase after earlier effects.
    /// Direct callers must provide validated command sizes to avoid fixed-field indexing panics.
    pub(super) fn handle_input<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        &mut self,
        input: Input<'_>,
        pairing_data: &mut PairingData,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<(), Error> {
        let current = core::mem::replace(self, Self::Error(Error::InvalidState));
        let next = Self::transition::<P, OPS, RNG>(current, input, pairing_data, ops, rng).unwrap_or_else(Self::Error);
        self.enter(next, pairing_data, ops);
        self.result().unwrap_or(Ok(()))
    }

    /// Consumes `current`/`input`, mutating `pairing_data` through `ops`/`rng` effects.
    /// Returns next phase or state/validation/crypto/queue errors. OOB uses only local.random as TK;
    /// unrelated user events and keypress notices leave the phase unchanged.
    fn transition<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        current: Self,
        input: Input<'_>,
        pairing_data: &mut PairingData,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<Self, Error> {
        match (current, input) {
            // --- Command transitions ---
            (Self::WaitingPairingRequest, Input::Command(Command::PairingRequest, payload)) => {
                Self::handle_pairing_request(payload, ops, pairing_data, rng)
            }
            (Self::WaitingOobData { preq, pres }, Input::Event(Event::OobDataReceived { local, .. })) => {
                let tk = u128::from_le_bytes(local.random);
                Ok(Self::WaitingPairingConfirm(ConfirmPhaseData {
                    tk,
                    preq,
                    pres,
                    local_nonce: 0,
                }))
            }
            (Self::WaitingPassKeyInput { preq, pres, .. }, Input::Command(Command::PairingConfirm, payload)) => {
                Self::store_confirm_bytes(payload, preq, pres)
            }
            (Self::WaitingPairingConfirm(confirm_data), Input::Command(Command::PairingConfirm, payload)) => {
                Self::handle_peer_confirm(payload, ops, pairing_data, confirm_data, rng)
            }
            (
                Self::WaitingPairingRandom {
                    confirm_data,
                    peer_confirm,
                },
                Input::Command(Command::PairingRandom, payload),
            ) => Self::handle_pairing_random(payload, ops, pairing_data, &confirm_data, peer_confirm, rng),
            (Self::WaitingIdentitityInformation, Input::Command(Command::IdentityInformation, payload)) => {
                Self::handle_identity_information(payload, pairing_data)
            }
            (
                Self::WaitingIdentitityAddressInformation,
                Input::Command(Command::IdentityAddressInformation, payload),
            ) => Self::handle_identity_address_information(payload, pairing_data),
            (Self::ReceivingKeys { phase: 0, .. }, Input::Command(Command::EncryptionInformation, payload)) => {
                Self::handle_encryption_information(payload)
            }
            (
                Self::ReceivingKeys {
                    phase: 1,
                    received_ltk: _,
                },
                Input::Command(Command::CentralIdentification, payload),
            ) => Self::handle_central_identification(payload, pairing_data),
            (current, Input::Command(Command::KeypressNotification, _)) => Ok(current),

            // --- Event transitions ---
            (
                x @ (Self::WaitingPairingRequest | Self::WaitingLinkEncrypted),
                Input::Event(Event::LinkEncryptedResult(true)),
            ) => Self::handle_link_encrypted_success(x, pairing_data, ops, rng),
            (
                Self::WaitingPairingRequest | Self::WaitingLinkEncrypted,
                Input::Event(Event::LinkEncryptedResult(false)),
            ) => {
                error!("[smp legacy peripheral] Link encryption failed!");
                Err(Error::Security(Reason::KeyRejected))
            }
            (
                Self::WaitingPassKeyInput {
                    confirm_bytes,
                    preq,
                    pres,
                },
                Input::Event(Event::PassKeyInput(input)),
            ) => Self::handle_pass_key_input(input, confirm_bytes, preq, pres, ops, pairing_data, rng),
            (Self::WaitingPassKeyInput { .. }, Input::Event(Event::PassKeyCancel)) => {
                Err(Error::Security(Reason::PasskeyEntryFailed))
            }
            (current, Input::Event(Event::PassKeyConfirm | Event::PassKeyCancel | Event::PassKeyInput(_))) => {
                Ok(current)
            }
            // Handle PairingFailed from peer in any state
            (_, Input::Command(Command::PairingFailed, payload)) => {
                let reason = Reason::try_from(payload[0]).unwrap_or(Reason::UnspecifiedReason);
                warn!("[smp legacy peripheral] Peer sent PairingFailed: {:?}", reason);
                Err(Error::Security(reason))
            }

            // --- Catch-all ---
            _ => Err(Error::InvalidState),
        }
    }

    /// Installs `next`, reporting terminal outcome and agreed bond storage through `pairing_data`/`ops`.
    /// Reporting/storage errors are logged and do not override the selected terminal state.
    fn enter<P: PacketPool, OPS: PairingOps<P>>(&mut self, next: Self, pairing_data: &PairingData, ops: &mut OPS) {
        match &next {
            Self::Error(e) => {
                if let Err(e) = ops.try_send_connection_event(ConnectionEvent::PairingFailed(e.clone())) {
                    error!("[smp] Failed to send PairingFailed event: {:?}", e);
                }
            }
            Self::Success => {
                if let Some(bond) = pairing_data.bond_information.as_ref() {
                    let pairing_bond = if pairing_data.want_bonding() {
                        if let Err(e) = ops.try_update_bond_information(bond) {
                            error!("[smp] Failed to update bond information: {:?}", e);
                        }
                        Some(bond.clone())
                    } else {
                        None
                    };
                    if let Err(e) = ops.try_send_connection_event(ConnectionEvent::PairingComplete {
                        security_level: bond.security_level,
                        bond: pairing_bond,
                    }) {
                        error!("[smp] Failed to send PairingComplete event: {:?}", e);
                    }
                } else {
                    error!("[smp legacy peripheral] No bond information stored");
                }
            }
            _ => {}
        }
        *self = next;
    }

    // --- Transition helpers ---

    /// Handles HCI success from `previous`, loading a retained key or distributing newly bonded keys
    /// using `pairing_data`, `ops` and `rng`. Returns next receive phase/Success or storage/queue errors;
    /// newly bonded responder keys are sent before initiator keys are accepted.
    fn handle_link_encrypted_success<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        previous: Self,
        pairing_data: &mut PairingData,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<Self, Error> {
        debug!("[smp legacy peripheral] Link encrypted!");
        if matches!(previous, Self::WaitingPairingRequest) {
            pairing_data.bond_information = ops.try_enable_bonded_encryption()?;
        }
        if matches!(previous, Self::WaitingLinkEncrypted) && pairing_data.want_bonding() {
            // Peripheral sends its keys first, then receives central's
            Self::send_all_keys(pairing_data, ops, rng)
        } else if pairing_data.peer_features.initiator_key_distribution.identity_key() {
            Ok(Self::WaitingIdentitityInformation)
        } else {
            Ok(Self::Success)
        }
    }

    /// Returns passkey-input wait state retaining wire `preq`/`pres` and copied 16-byte Mconfirm `payload`.
    /// Wrong length returns InvalidValue; a later early commitment replaces the earlier one.
    fn store_confirm_bytes(payload: &[u8], preq: [u8; 7], pres: [u8; 7]) -> Result<Self, Error> {
        let confirm_bytes: [u8; size_of::<u128>()] = payload.try_into().map_err(|_| Error::InvalidValue)?;
        Ok(Self::WaitingPassKeyInput {
            confirm_bytes: Some(confirm_bytes),
            preq,
            pres,
        })
    }

    #[inline]
    /// Reads 16-byte Mconfirm `payload`, generates Sconfirm in `confirm_data` using link `pairing_data`
    /// and `rng`, then queues it through `ops`. Returns Mrand-wait state or length/send errors.
    fn handle_peer_confirm<P: PacketPool, OPS: PairingOps<P>, RNG: RngCore>(
        payload: &[u8],
        ops: &mut OPS,
        pairing_data: &PairingData,
        mut confirm_data: ConfirmPhaseData,
        rng: &mut RNG,
    ) -> Result<Self, Error> {
        let peer_confirm = u128::from_le_bytes(
            payload
                .try_into()
                .map_err(|_| Error::Security(Reason::InvalidParameters))?,
        );
        Self::send_sconfirm(ops, pairing_data, &mut confirm_data, rng)?;
        Ok(Self::WaitingPairingRandom {
            confirm_data,
            peer_confirm,
        })
    }

    #[inline]
    /// Uses decimal `input` as TK with wire `preq`/`pres`; caller enforces 0 through 999999.
    /// Optional early `confirm_bytes` triggers Sconfirm generation via `rng`, `pairing_data` and `ops`.
    /// Returns confirm/random wait state or send errors; this helper does not validate the input range.
    fn handle_pass_key_input<P: PacketPool, OPS: PairingOps<P>, RNG: RngCore>(
        input: u32,
        confirm_bytes: Option<[u8; size_of::<u128>()]>,
        preq: [u8; 7],
        pres: [u8; 7],
        ops: &mut OPS,
        pairing_data: &PairingData,
        rng: &mut RNG,
    ) -> Result<Self, Error> {
        let tk = input as u128;
        match confirm_bytes {
            Some(payload) => {
                let peer_confirm = u128::from_le_bytes(payload);
                let mut confirm_data = ConfirmPhaseData {
                    tk,
                    preq,
                    pres,
                    local_nonce: 0,
                };
                Self::send_sconfirm(ops, pairing_data, &mut confirm_data, rng)?;
                Ok(Self::WaitingPairingRandom {
                    confirm_data,
                    peer_confirm,
                })
            }
            None => Ok(Self::WaitingPairingConfirm(ConfirmPhaseData {
                tk,
                preq,
                pres,
                local_nonce: 0,
            })),
        }
    }

    /// Generates negotiated responder LTK/EDIV/Rand with `rng` and sends them via `ops`, updating
    /// `pairing_data`'s bond with the key masked to negotiated byte size. Sends identity if agreed.
    /// Returns initiator-key/identity wait state or Success; errors preserve earlier packets/bond updates.
    fn send_all_keys<P: PacketPool, OPS: PairingOps<P>, RNG: RngCore>(
        pairing_data: &mut PairingData,
        ops: &mut OPS,
        rng: &mut RNG,
    ) -> Result<Self, Error> {
        if pairing_data.local_features.responder_key_distribution.encryption_key() {
            let mut ltk_bytes = [0u8; 16];
            rng.fill_bytes(&mut ltk_bytes);
            let long_term_key = LongTermKey::from_le_bytes(ltk_bytes);
            let ediv: u16 = rng.r#gen();
            let mut rand = [0u8; 8];
            rng.fill_bytes(&mut rand);

            let packet = make_encryption_information_packet(&long_term_key)?;
            ops.try_send_packet(packet)?;
            let packet = make_central_identification_packet(ediv, &rand)?;
            ops.try_send_packet(packet)?;

            // Update bond with our own distributed LTK (masked to negotiated key size
            // per BT Core Spec Vol 3, Part H, Section 2.4.4), EDIV, and Rand so the
            // central can use them for re-encryption.
            let negotiated_key_size = core::cmp::min(
                pairing_data.peer_features.maximum_encryption_key_size,
                pairing_data.local_features.maximum_encryption_key_size,
            );
            let want_bonding = pairing_data.want_bonding();
            if let Some(ref mut bond) = pairing_data.bond_information {
                bond.ltk = if negotiated_key_size >= 16 {
                    long_term_key
                } else {
                    LongTermKey(long_term_key.0 & ((1u128 << (negotiated_key_size as u32 * 8)) - 1))
                };
                bond.ediv = ediv;
                bond.rand = rand;
                bond.is_bonded = want_bonding;
            }
        }
        if pairing_data.local_features.responder_key_distribution.identity_key() {
            let irk = ops.local_irk();
            let packet = make_identity_information_packet(&irk)?;
            ops.try_send_packet(packet)?;
            // Send the identity address (public or static random), not the RPA used for pairing.
            let identity_address = ops.local_identity_address()?;
            let packet = make_identity_address_information_packet(&identity_address)?;
            ops.try_send_packet(packet)?;
        }

        // After sending our keys, receive central's keys
        if pairing_data.peer_features.initiator_key_distribution.encryption_key() {
            Ok(Self::ReceivingKeys {
                phase: 0,
                received_ltk: LongTermKey(0),
            })
        } else if pairing_data.peer_features.initiator_key_distribution.identity_key() {
            Ok(Self::WaitingIdentitityInformation)
        } else {
            Ok(Self::Success)
        }
    }

    // --- Protocol helpers ---

    /// Admits six-byte feature `payload` into `pairing_data`, negotiates legacy distribution and sends
    /// response/prompts through `ops`; `rng` supplies display passkeys. Returns next phase or feature/
    /// packet/event errors. Local-IRK absence declines its distribution; oversized direct bodies panic
    /// while copying the c1 transcript. Existing-bond notification may precede later rejection.
    fn handle_pairing_request<P: PacketPool, OPS: PairingOps<P>, RNG: CryptoRng + RngCore>(
        payload: &[u8],
        ops: &mut OPS,
        pairing_data: &mut PairingData,
        rng: &mut RNG,
    ) -> Result<Pairing, Error> {
        if ops.find_bond().is_some() {
            if let Err(e) = ops.try_send_connection_event(ConnectionEvent::BondLost) {
                error!("[smp] Failed to send BondLost event: {:?}", e);
            }
        }

        let peer_features = PairingFeatures::decode(payload).map_err(|_| Error::Security(Reason::InvalidParameters))?;
        // Key size validation (7-16 range) is done in PairingFeatures::decode

        // Store the PairingRequest command bytes for c1
        let mut preq = [0u8; 7];
        preq[0] = u8::from(Command::PairingRequest);
        preq[1..7].copy_from_slice(payload);

        // Negotiate key distribution
        if peer_features.initiator_key_distribution.identity_key() {
            pairing_data
                .local_features
                .initiator_key_distribution
                .set_identity_key();
        }
        if peer_features.initiator_key_distribution.encryption_key() {
            pairing_data
                .local_features
                .initiator_key_distribution
                .set_encryption_key();
        }
        // Without a local IRK, decline identity key distribution instead of sending a zero IRK.
        if peer_features.responder_key_distribution.identity_key() && ops.local_irk() != [0; 16] {
            pairing_data
                .local_features
                .responder_key_distribution
                .set_identity_key();
        }
        if peer_features.responder_key_distribution.encryption_key() {
            pairing_data
                .local_features
                .responder_key_distribution
                .set_encryption_key();
        }

        pairing_data.peer_features = peer_features;
        let mut auth_req = AuthReq::new_legacy(ops.bonding_flag());
        if pairing_data.local_features.io_capabilities != IoCapabilities::NoInputNoOutput {
            auth_req = auth_req.with_mitm();
        }
        pairing_data.local_features.security_properties = auth_req;
        if ops.oob_available() {
            pairing_data.local_features.use_oob = crate::security_manager::types::UseOutOfBand::Present;
        }
        pairing_data.pairing_method =
            choose_legacy_pairing_method(pairing_data.peer_features, pairing_data.local_features);
        debug!("[smp legacy] Pairing method {:?}", pairing_data.pairing_method);

        // Send PairingResponse and store the command bytes for c1
        let mut packet = prepare_packet::<P>(Command::PairingResponse)?;
        let response = packet.payload_mut();
        pairing_data
            .local_features
            .encode(response)
            .map_err(|_| Error::InvalidValue)?;
        let mut pres = [0u8; 7];
        pres[0] = u8::from(Command::PairingResponse);
        pres[1..7].copy_from_slice(response);
        ops.try_send_packet(packet)?;

        match pairing_data.pairing_method {
            PairingMethod::OutOfBand => {
                ops.try_send_connection_event(ConnectionEvent::OobRequest)?;
                Ok(Self::WaitingOobData { preq, pres })
            }
            PairingMethod::PassKeyEntry { peripheral, .. } => {
                if peripheral == PassKeyEntryAction::Display {
                    let tk = rng.sample(rand::distributions::Uniform::new_inclusive(0u32, 999999)) as u128;
                    ops.try_send_connection_event(ConnectionEvent::PassKeyDisplay(PassKey(tk as u32)))?;
                    Ok(Self::WaitingPairingConfirm(ConfirmPhaseData {
                        tk,
                        preq,
                        pres,
                        local_nonce: 0,
                    }))
                } else {
                    ops.try_send_connection_event(ConnectionEvent::PassKeyInput)?;
                    Ok(Self::WaitingPassKeyInput {
                        confirm_bytes: None,
                        preq,
                        pres,
                    })
                }
            }
            PairingMethod::JustWorks => Ok(Self::WaitingPairingConfirm(ConfirmPhaseData {
                tk: 0,
                preq,
                pres,
                local_nonce: 0,
            })),
            PairingMethod::NumericComparison => {
                // Should not happen in legacy pairing
                Err(Error::Security(Reason::AuthenticationRequirements))
            }
        }
    }

    /// Draws Srand from `rng` into `confirm_data` (big-endian numeric, zero replaced by one), computes
    /// c1 using `pairing_data` addresses and queues Sconfirm through `ops`. Returns allocation/send
    /// errors after nonce mutation; infallible RNG failure can panic.
    fn send_sconfirm<P: PacketPool, OPS: PairingOps<P>, RNG: RngCore>(
        ops: &mut OPS,
        pairing_data: &PairingData,
        confirm_data: &mut ConfirmPhaseData,
        rng: &mut RNG,
    ) -> Result<(), Error> {
        // Generate Srand
        let mut srand_bytes = [0u8; 16];
        rng.fill_bytes(&mut srand_bytes);
        confirm_data.local_nonce = u128::from_be_bytes(srand_bytes);
        if confirm_data.local_nonce == 0 {
            confirm_data.local_nonce = 1;
        }

        // Compute Sconfirm = c1(TK, Srand, preq, pres, iat, ia, rat, ra)
        let sconfirm = crypto::c1(
            confirm_data.tk,
            confirm_data.local_nonce,
            &confirm_data.preq,
            &confirm_data.pres,
            addr_type_flag(&pairing_data.peer_address),
            &addr_bytes_mso(&pairing_data.peer_address),
            addr_type_flag(&pairing_data.local_address),
            &addr_bytes_mso(&pairing_data.local_address),
        );

        let mut packet = prepare_packet(Command::PairingConfirm)?;
        packet.payload_mut().copy_from_slice(&sconfirm.to_le_bytes());
        ops.try_send_packet(packet)?;
        Ok(())
    }

    #[inline]
    /// Verifies 16-byte Mrand `payload` against numeric `peer_confirm` with `confirm_data`/`pairing_data`.
    /// Queues Srand and negotiated-size STK encryption through `ops`; `rng` is unused.
    /// Returns HCI-wait state or length/check/storage/queue error; no encryption precedes verification.
    fn handle_pairing_random<P: PacketPool, OPS: PairingOps<P>, RNG: RngCore>(
        payload: &[u8],
        ops: &mut OPS,
        pairing_data: &mut PairingData,
        confirm_data: &ConfirmPhaseData,
        peer_confirm: u128,
        rng: &mut RNG,
    ) -> Result<Pairing, Error> {
        // Parse Mrand from central
        let mrand_le: [u8; 16] = payload
            .try_into()
            .map_err(|_| Error::Security(Reason::InvalidParameters))?;
        let peer_nonce = u128::from_le_bytes(mrand_le);

        // Verify: c1(TK, Mrand, preq, pres, iat, ia, rat, ra) == stored Mconfirm
        let expected_mconfirm = crypto::c1(
            confirm_data.tk,
            peer_nonce,
            &confirm_data.preq,
            &confirm_data.pres,
            addr_type_flag(&pairing_data.peer_address),
            &addr_bytes_mso(&pairing_data.peer_address),
            addr_type_flag(&pairing_data.local_address),
            &addr_bytes_mso(&pairing_data.local_address),
        );

        if expected_mconfirm != peer_confirm {
            error!("[smp legacy] Confirm value mismatch");
            return Err(Error::Security(Reason::ConfirmValueFailed));
        }

        // Send Srand
        let packet = make_pairing_random(&crypto::Nonce(confirm_data.local_nonce))?;
        ops.try_send_packet(packet)?;

        // Compute STK = s1(TK, Srand, Mrand)
        let stk = crypto::s1(confirm_data.tk, confirm_data.local_nonce, peer_nonce);

        // Mask STK to negotiated key size (Bluetooth Core Spec Vol 3, Part H, Section 2.4.4):
        // Zero out the most significant (16 - key_size) bytes.
        let negotiated_key_size = core::cmp::min(
            pairing_data.peer_features.maximum_encryption_key_size,
            pairing_data.local_features.maximum_encryption_key_size,
        );
        let masked_stk = if negotiated_key_size >= 16 {
            stk
        } else {
            stk & ((1u128 << (negotiated_key_size as u32 * 8)) - 1)
        };
        let stk = LongTermKey(masked_stk);

        // Enable encryption with STK (not yet bonded — real LTK comes via key distribution)
        let bond = ops.try_enable_encryption(
            &stk,
            pairing_data.pairing_method.security_level(),
            false,
            0,
            [0; 8],
            negotiated_key_size,
        )?;
        pairing_data.bond_information = Some(bond);

        Ok(Self::WaitingLinkEncrypted)
    }

    /// Returns EDIV/Rand-wait state after decoding 16-byte little-endian initiator LTK `payload`.
    /// Wrong length returns InvalidValue; the initiator key is later discarded in this fixed-role host.
    fn handle_encryption_information(payload: &[u8]) -> Result<Pairing, Error> {
        let ltk = LongTermKey::from_le_bytes(payload.try_into().map_err(|_| Error::InvalidValue)?);
        trace!("[smp legacy] Received LTK from central");
        Ok(Self::ReceivingKeys {
            phase: 1,
            received_ltk: ltk,
        })
    }

    /// Consumes initiator EDIV/Rand `payload` without replacing the responder's retained bond key.
    /// Returns identity-wait or Success from `pairing_data`'s negotiated flags; fewer than 10 bytes
    /// return InvalidParameters, and trailing bytes are ignored by this direct helper.
    fn handle_central_identification(payload: &[u8], pairing_data: &mut PairingData) -> Result<Pairing, Error> {
        if payload.len() < 10 {
            return Err(Error::Security(Reason::InvalidParameters));
        }
        // Central's EDIV/Rand received but not stored in bond — the bond retains the
        // peripheral's own LTK/EDIV/Rand (set in send_all_keys) for re-encryption.
        trace!("[smp legacy] Received EDIV/Rand from central");
        if pairing_data.peer_features.initiator_key_distribution.identity_key() {
            Ok(Self::WaitingIdentitityInformation)
        } else {
            Ok(Self::Success)
        }
    }

    /// Stores 16-byte IRK `payload` into any pending `pairing_data` bond; zero means None.
    /// Returns identity-address wait state or InvalidValue on wrong length; absent bond is tolerated.
    fn handle_identity_information(payload: &[u8], pairing_data: &mut PairingData) -> Result<Pairing, Error> {
        let irk = IdentityResolvingKey::new(u128::from_le_bytes(
            payload.try_into().map_err(|_| Error::InvalidValue)?,
        ));
        if let Some(ref mut bond) = &mut pairing_data.bond_information {
            bond.identity.irk = irk;
        }
        trace!("[smp legacy] Received IRK");
        Ok(Self::WaitingIdentitityAddressInformation)
    }

    /// Stores validated seven-byte identity `payload` into `pairing_data`'s peer/bond address.
    /// Returns Success or InvalidValue for a reserved type; short direct payloads panic and random
    /// address subtype bits are not validated here.
    fn handle_identity_address_information(payload: &[u8], pairing_data: &mut PairingData) -> Result<Pairing, Error> {
        let addr_type = payload[0];
        let kind = if addr_type == 0 {
            AddrKind::PUBLIC
        } else if addr_type == 1 {
            AddrKind::RANDOM
        } else {
            return Err(Error::InvalidValue);
        };
        let addr = BdAddr::new(payload[1..7].try_into().map_err(|_| Error::InvalidValue)?);
        pairing_data.peer_address = Address::new(kind, addr);

        if let Some(ref mut bond) = &mut pairing_data.bond_information {
            bond.identity.addr = Address::new(kind, addr);
        }

        trace!("[smp legacy] Received identity address {:?}", addr);
        Ok(Self::Success)
    }
}

#[cfg(test)]
/// Legacy responder request admission with MITM requested but no available I/O authentication.
mod tests {
    use embassy_time::Instant;
    use rand_chacha::{ChaCha12Core, ChaCha12Rng};
    use rand_core::SeedableRng;

    use super::Pairing;
    use crate::security_manager::pairing::tests::{HeaplessPool, TestOps};
    use crate::security_manager::pairing::util::PairingMethod;
    use crate::security_manager::pairing::{Input, PairingData};
    use crate::security_manager::types::{Command, PairingFeatures};
    use crate::{Address, IoCapabilities};

    /// Returns fresh Just Works data for `local_address`/`peer_address`, advertised `local_io` and active timeout.
    fn make_default_pairing_data(
        local_address: Address,
        peer_address: Address,
        local_io: IoCapabilities,
    ) -> PairingData {
        PairingData {
            local_address,
            peer_address,
            local_features: PairingFeatures {
                io_capabilities: local_io,
                ..Default::default()
            },
            pairing_method: PairingMethod::JustWorks,
            peer_features: PairingFeatures::default(),
            timeout_at: Instant::now() + crate::security_manager::constants::TIMEOUT,
            bond_information: None,
        }
    }

    #[test]
    /// No-I/O legacy peers answer the request and wait for confirm under Just Works despite MITM intent.
    fn just_works() {
        let mut ops: TestOps<10> = TestOps::new(0xDEAD);
        let mut pairing_data = make_default_pairing_data(
            Address::random([1, 2, 3, 4, 5, 6]),
            Address::random([7, 8, 9, 10, 11, 12]),
            IoCapabilities::NoInputNoOutput,
        );
        let mut pairing = Pairing::new();
        let mut rng: ChaCha12Rng = ChaCha12Core::seed_from_u64(42).into();

        // Central sends PairingRequest (without SC flag)
        // AuthReq = 0x04 (MITM, no bonding or SC).
        pairing
            .handle_input::<HeaplessPool, _, _>(
                Input::Command(Command::PairingRequest, &[0x03, 0, 0x04, 16, 0, 0]),
                &mut pairing_data,
                &mut ops,
                &mut rng,
            )
            .unwrap();

        assert_eq!(ops.sent_packets.len(), 1);
        assert_eq!(ops.sent_packets[0].command(), Command::PairingResponse);

        // Verify it's JustWorks (NoInputNoOutput)
        assert!(matches!(pairing, Pairing::WaitingPairingConfirm(_)));
    }
}
