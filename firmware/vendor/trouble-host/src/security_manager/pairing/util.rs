//! Association-model selection and fixed-size SMP payload construction.
//!
//! Selection follows Bluetooth Core Vol 3, Part H, §§2.3.3 and 2.3.5
//! ([Security Manager specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html)).
//! OOB selection does not establish that the application's out-of-band channel is trusted.

use crate::prelude::SecurityLevel;
use crate::security_manager::crypto::{Check, Confirm, DHKey, MacKey, Nonce, PublicKey};
use crate::security_manager::types::{Command, PairingFeatures, UseOutOfBand};
use crate::security_manager::TxPacket;
use crate::{Address, Error, IoCapabilities, LongTermKey, PacketPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// User interaction assigned to one peer during passkey entry.
pub enum PassKeyEntryAction {
    /// Show the six-digit value for entry at the other peer.
    Display,
    /// Obtain the value entered by the user.
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Association model selected from both peers' advertised capabilities.
pub enum PairingMethod {
    /// Encryption without authenticated peer identity.
    JustWorks,
    /// User confirms that both displays show the same six-digit value.
    NumericComparison,
    /// Passkey entry with the indicated role at each peer.
    PassKeyEntry {
        /// Initiating peer's user interaction.
        central: PassKeyEntryAction,
        /// Responding peer's user interaction.
        peripheral: PassKeyEntryAction,
    },
    /// Authenticate using externally exchanged data; transfer trust is application-owned.
    OutOfBand,
}

impl PairingMethod {
    /// Central displays and peripheral accepts entry.
    const CENTRAL_DISPLAYS: Self = Self::PassKeyEntry {
        central: PassKeyEntryAction::Display,
        peripheral: PassKeyEntryAction::Input,
    };

    /// Peripheral displays and central accepts entry.
    const PERIPHERAL_DISPLAYS: Self = Self::PassKeyEntry {
        central: PassKeyEntryAction::Input,
        peripheral: PassKeyEntryAction::Display,
    };

    /// Both peers obtain the same externally known passkey through input.
    const BOTH_INPUT: Self = Self::PassKeyEntry {
        central: PassKeyEntryAction::Input,
        peripheral: PassKeyEntryAction::Input,
    };

    /// Returns the nominal completed level: unauthenticated for Just Works, authenticated otherwise.
    /// OOB callers must ensure their external authentication channel warrants this level.
    pub fn security_level(&self) -> SecurityLevel {
        match self {
            PairingMethod::JustWorks => SecurityLevel::Encrypted,
            _ => SecurityLevel::EncryptedAuthenticated,
        }
    }
}

/// Returns the Secure Connections model for `central` and `peripheral` feature advertisements.
/// Either OOB flag takes precedence; absent OOB, neither requesting MITM selects Just Works.
/// This selects a model only; it does not validate the negotiated key size or execute pairing.
pub fn choose_pairing_method(central: PairingFeatures, peripheral: PairingFeatures) -> PairingMethod {
    if matches!(central.use_oob, UseOutOfBand::Present) || matches!(peripheral.use_oob, UseOutOfBand::Present) {
        PairingMethod::OutOfBand
    } else if !central.security_properties.man_in_the_middle() && !peripheral.security_properties.man_in_the_middle() {
        PairingMethod::JustWorks
    } else {
        match (central.io_capabilities, peripheral.io_capabilities) {
            (IoCapabilities::NoInputNoOutput, _) | (_, IoCapabilities::NoInputNoOutput) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::DisplayYesNo) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayOnly, IoCapabilities::KeyboardDisplay) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayYesNo, IoCapabilities::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayYesNo, IoCapabilities::DisplayYesNo) => PairingMethod::NumericComparison,
            (IoCapabilities::DisplayYesNo, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayYesNo, IoCapabilities::KeyboardDisplay) => PairingMethod::NumericComparison,
            (IoCapabilities::KeyboardOnly, IoCapabilities::DisplayOnly) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardOnly, IoCapabilities::DisplayYesNo) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardOnly, IoCapabilities::KeyboardOnly) => PairingMethod::BOTH_INPUT,
            (IoCapabilities::KeyboardOnly, IoCapabilities::KeyboardDisplay) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::DisplayOnly) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::DisplayYesNo) => PairingMethod::NumericComparison,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::KeyboardDisplay) => PairingMethod::NumericComparison,
        }
    }
}

#[cfg(feature = "legacy-pairing")]
/// Returns the legacy model for `central` and `peripheral` feature advertisements.
/// OOB requires both flags; numeric comparison is unavailable, including two DisplayYesNo peers.
pub fn choose_legacy_pairing_method(central: PairingFeatures, peripheral: PairingFeatures) -> PairingMethod {
    if matches!(central.use_oob, UseOutOfBand::Present) && matches!(peripheral.use_oob, UseOutOfBand::Present) {
        PairingMethod::OutOfBand
    } else if !central.security_properties.man_in_the_middle() && !peripheral.security_properties.man_in_the_middle() {
        PairingMethod::JustWorks
    } else {
        match (central.io_capabilities, peripheral.io_capabilities) {
            (IoCapabilities::NoInputNoOutput, _) | (_, IoCapabilities::NoInputNoOutput) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::DisplayYesNo) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayOnly, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayOnly, IoCapabilities::KeyboardDisplay) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayYesNo, IoCapabilities::DisplayOnly) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayYesNo, IoCapabilities::DisplayYesNo) => PairingMethod::JustWorks,
            (IoCapabilities::DisplayYesNo, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::DisplayYesNo, IoCapabilities::KeyboardDisplay) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::KeyboardOnly, IoCapabilities::DisplayOnly) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardOnly, IoCapabilities::DisplayYesNo) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardOnly, IoCapabilities::KeyboardOnly) => PairingMethod::BOTH_INPUT,
            (IoCapabilities::KeyboardOnly, IoCapabilities::KeyboardDisplay) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::DisplayOnly) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::DisplayYesNo) => PairingMethod::PERIPHERAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::KeyboardOnly) => PairingMethod::CENTRAL_DISPLAYS,
            (IoCapabilities::KeyboardDisplay, IoCapabilities::KeyboardDisplay) => PairingMethod::CENTRAL_DISPLAYS,
        }
    }
}

/// Returns an allocated packet with the L2CAP/SMP header for `command`; payload bytes remain unset.
/// Returns OutOfMemory if the pool is exhausted. Panics if a pool packet cannot hold the header;
/// callers must also provide capacity for the command's complete fixed-size payload.
pub fn prepare_packet<P: PacketPool>(command: Command) -> Result<TxPacket<P>, Error> {
    let packet = P::allocate().ok_or(Error::OutOfMemory)?;
    TxPacket::new(packet, command)
}

/// Returns Pairing Random containing `nonce` in little-endian wire order.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_pairing_random<P: PacketPool>(nonce: &Nonce) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::PairingRandom)?;
    let response = packet.payload_mut();
    response.copy_from_slice(&nonce.0.to_le_bytes());
    Ok(packet)
}

/// Returns Pairing Public Key with `public_key` as little-endian X followed by Y, without a SEC1 tag.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`]; no curve validation occurs.
pub fn make_public_key_packet<P: PacketPool>(public_key: &PublicKey) -> Result<TxPacket<P>, Error> {
    let mut x = [0u8; 32];
    let mut y = [0u8; 32];
    x.copy_from_slice(public_key.x.as_be_bytes());
    y.copy_from_slice(public_key.y.as_be_bytes());
    x.reverse();
    y.reverse();
    let mut packet = prepare_packet(Command::PairingPublicKey)?;

    let response = packet.payload_mut();

    response[..x.len()].copy_from_slice(&x);
    response[x.len()..y.len() + x.len()].copy_from_slice(&y);
    Ok(packet)
}

/// Returns Pairing DHKey Check containing `check` in little-endian wire order.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_dhkey_check_packet<P: PacketPool>(check: &Check) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet(Command::PairingDhKeyCheck)?;
    let response = packet.payload_mut();
    let bytes = check.0.to_le_bytes();
    response[..bytes.len()].copy_from_slice(&bytes);
    Ok(packet)
}

/// Returns f5's (MacKey, LTK) from shared `dh_key`, initiator `central_nonce` and responder
/// `peripheral_nonce`, binding `central_address` and `peripheral_address` in that order.
/// Addresses are the actual pairing-link addresses, which may differ from stored identities.
pub fn make_mac_and_ltk(
    dh_key: &DHKey,
    central_nonce: &Nonce,
    peripheral_nonce: &Nonce,
    central_address: Address,
    peripheral_address: Address,
) -> (MacKey, LongTermKey) {
    dh_key.f5(*central_nonce, *peripheral_nonce, central_address, peripheral_address)
}

/// Returns Pairing Confirm containing `confirm` in little-endian wire order.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_confirm_packet<P: PacketPool>(confirm: &Confirm) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::PairingConfirm)?;
    let response = packet.payload_mut();
    response.copy_from_slice(&confirm.0.to_le_bytes());
    Ok(packet)
}

#[cfg(feature = "legacy-pairing")]
/// Returns Encryption Information containing `ltk` in little-endian wire order.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_encryption_information_packet<P: PacketPool>(ltk: &LongTermKey) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::EncryptionInformation)?;
    let payload = packet.payload_mut();
    payload.copy_from_slice(&ltk.to_le_bytes());
    Ok(packet)
}

#[cfg(feature = "legacy-pairing")]
/// Returns Central Identification with little-endian `ediv` and the eight wire-order `rand` bytes.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_central_identification_packet<P: PacketPool>(ediv: u16, rand: &[u8; 8]) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::CentralIdentification)?;
    let payload = packet.payload_mut();
    payload[0..2].copy_from_slice(&ediv.to_le_bytes());
    payload[2..10].copy_from_slice(rand);
    Ok(packet)
}

/// Returns Identity Information containing the already little-endian `irk` bytes unchanged.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_identity_information_packet<P: PacketPool>(irk: &[u8; 16]) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::IdentityInformation)?;
    let payload = packet.payload_mut();
    payload.copy_from_slice(irk);
    Ok(packet)
}

/// Returns Identity Address Information using `addr`'s wire-order bytes, type 0 for PUBLIC and type 1
/// for every other kind. Caller must supply a public/static-random identity, not an RPA or HCI alias.
/// Allocation failure and undersized-pool panics follow [`prepare_packet`].
pub fn make_identity_address_information_packet<P: PacketPool>(addr: &Address) -> Result<TxPacket<P>, Error> {
    let mut packet = prepare_packet::<P>(Command::IdentityAddressInformation)?;
    let payload = packet.payload_mut();
    let addr_type = if addr.kind == bt_hci::param::AddrKind::PUBLIC {
        0u8
    } else {
        1u8
    };
    payload[0] = addr_type;
    payload[1..7].copy_from_slice(addr.addr.raw());
    Ok(packet)
}

#[cfg(test)]
/// Association tables distinguish legacy and Secure Connections OOB/display rules.
mod tests {
    use super::*;
    use crate::security_manager::types::{AuthReq, BondingFlag};

    #[cfg(feature = "legacy-pairing")]
    #[test]
    /// Two yes/no displays require Just Works in legacy but numeric comparison in Secure Connections.
    fn legacy_no_numeric_comparison() {
        // In legacy pairing, DisplayYesNo + DisplayYesNo should be JustWorks (not NumericComparison)
        let central = PairingFeatures {
            io_capabilities: IoCapabilities::DisplayYesNo,
            use_oob: UseOutOfBand::NotPresent,
            security_properties: AuthReq::new_legacy(BondingFlag::NoBonding).with_mitm(),
            initiator_key_distribution: 0.into(),
            responder_key_distribution: 0.into(),
            maximum_encryption_key_size: 16,
        };
        let peripheral = PairingFeatures {
            io_capabilities: IoCapabilities::DisplayYesNo,
            use_oob: UseOutOfBand::NotPresent,
            security_properties: AuthReq::new_legacy(BondingFlag::NoBonding).with_mitm(),
            initiator_key_distribution: 0.into(),
            responder_key_distribution: 0.into(),
            maximum_encryption_key_size: 16,
        };
        assert_eq!(
            choose_legacy_pairing_method(central, peripheral),
            PairingMethod::JustWorks
        );
        // LESC would produce NumericComparison
        assert_eq!(
            choose_pairing_method(central, peripheral),
            PairingMethod::NumericComparison
        );
    }

    #[cfg(feature = "legacy-pairing")]
    #[test]
    /// Legacy mixed keyboard/display roles follow the initiator/responder association table.
    fn legacy_keyboard_display_combos() {
        let make = |io: IoCapabilities| PairingFeatures {
            io_capabilities: io,
            use_oob: UseOutOfBand::NotPresent,
            security_properties: AuthReq::new_legacy(BondingFlag::NoBonding).with_mitm(),
            initiator_key_distribution: 0.into(),
            responder_key_distribution: 0.into(),
            maximum_encryption_key_size: 16,
        };
        // KeyboardDisplay + DisplayYesNo -> PERIPHERAL_DISPLAYS in legacy (NumericComparison in LESC)
        assert_eq!(
            choose_legacy_pairing_method(
                make(IoCapabilities::KeyboardDisplay),
                make(IoCapabilities::DisplayYesNo)
            ),
            PairingMethod::PERIPHERAL_DISPLAYS
        );
        // KeyboardDisplay + KeyboardDisplay -> CENTRAL_DISPLAYS in legacy (NumericComparison in LESC)
        assert_eq!(
            choose_legacy_pairing_method(
                make(IoCapabilities::KeyboardDisplay),
                make(IoCapabilities::KeyboardDisplay)
            ),
            PairingMethod::CENTRAL_DISPLAYS
        );
    }

    #[test]
    /// Every I/O pair selects Secure Connections OOB iff either peer advertises peer OOB data.
    fn oob_used() {
        for p_oob in 0..=1 {
            for c_oob in 0..=1 {
                let p_oob = if p_oob == 1 {
                    UseOutOfBand::Present
                } else {
                    UseOutOfBand::NotPresent
                };
                let c_oob = if c_oob == 1 {
                    UseOutOfBand::Present
                } else {
                    UseOutOfBand::NotPresent
                };
                for p in 0u8..5 {
                    for c in 0u8..5 {
                        let peripheral = PairingFeatures {
                            io_capabilities: p.try_into().unwrap(),
                            use_oob: p_oob,
                            security_properties: AuthReq::new(BondingFlag::NoBonding),
                            initiator_key_distribution: 0.into(),
                            responder_key_distribution: 0.into(),
                            maximum_encryption_key_size: 16,
                        };
                        let mut central = peripheral;
                        central.use_oob = c_oob;
                        central.io_capabilities = c.try_into().unwrap();
                        if p_oob == UseOutOfBand::NotPresent && c_oob == UseOutOfBand::NotPresent {
                            assert_ne!(choose_pairing_method(central, peripheral), PairingMethod::OutOfBand);
                        } else {
                            assert_eq!(choose_pairing_method(central, peripheral), PairingMethod::OutOfBand);
                        }
                        #[cfg(feature = "legacy-pairing")]
                        assert_eq!(
                            choose_legacy_pairing_method(central, peripheral) == PairingMethod::OutOfBand,
                            p_oob == UseOutOfBand::Present && c_oob == UseOutOfBand::Present,
                        );
                    }
                }
            }
        }
    }
}
