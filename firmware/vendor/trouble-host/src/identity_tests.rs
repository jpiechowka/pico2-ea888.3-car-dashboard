//! Address resolution must retain the HCI public/random distinction.

use core::cell::RefCell;

use bt_hci::param::{AddrKind, BdAddr};

use crate::connection::SecurityLevel;
use crate::security_manager::SecurityManager;
use crate::{Address, BondInformation, Identity, IdentityResolvingKey, LongTermKey};

/// Core Vol 3 Part H ah sample in HCI least-significant-byte-first order.
const RPA: [u8; 6] = [0xaa, 0xfb, 0x0d, 0x94, 0x81, 0x70];

/// Returns the sample IRK attached to a distinct static random identity address.
fn known_identity() -> Identity {
    Identity {
        addr: Address::random([1, 2, 3, 4, 5, 0xc6]),
        irk: Some(IdentityResolvingKey::new(0xec0234a3_57c8ad05_341010a6_0a397d9b).unwrap()),
    }
}

/// Returns the frozen resolving bytes tagged with raw HCI `kind`.
fn observed(kind: u8) -> Address {
    Address::new(AddrKind::new(kind), BdAddr::new(RPA))
}

#[test]
/// The same valid resolving bytes match only when reported as a random device address.
fn address_resolution_requires_random_kind() {
    let known = known_identity();
    assert!(known.irk.unwrap().resolve_address(&BdAddr::new(RPA)));
    assert!(known.match_address(&observed(1)));
    for kind in [0, 2, 3, 0xff] {
        assert!(!known.match_address(&observed(kind)), "raw HCI kind {kind}");
    }
}

#[test]
/// Forward identity matching applies the same address-kind gate as address matching.
fn forward_identity_resolution_requires_random_kind() {
    let known = known_identity();
    assert!(known.match_identity(&observed(1).into()));
    for kind in [0, 2, 3, 0xff] {
        assert!(!known.match_identity(&observed(kind).into()), "raw HCI kind {kind}");
    }
}

#[test]
/// Reverse IRK resolution also rejects public, resolved-identity and anonymous kinds.
fn reverse_identity_resolution_requires_random_kind() {
    let known = known_identity();
    assert!(Identity::from(observed(1)).match_identity(&known));
    for kind in [0, 2, 3, 0xff] {
        assert!(
            !Identity::from(observed(kind)).match_identity(&known),
            "raw HCI kind {kind}"
        );
    }
}

#[test]
/// Exact normalized identities and equal IRKs remain valid independently of RPA resolution.
fn normalized_addresses_and_equal_irks_remain_matching() {
    for (plain, resolved) in [(0, 2), (1, 3)] {
        let bytes = BdAddr::new([7, 8, 9, 10, 11, 0xcc]);
        let address = Address::new(AddrKind::new(plain), bytes);
        let identity = Identity::from(address);
        let normalized = Address::new(AddrKind::new(resolved), bytes);
        assert!(identity.match_address(&normalized));
        assert!(identity.match_identity(&normalized.into()));
        assert!(Identity::from(normalized).match_identity(&identity));
        assert!(!identity.match_address(&Address::new(AddrKind::new(plain ^ 1), bytes)));
    }
    let known = known_identity();
    let same_key = Identity {
        addr: Address::random([6, 5, 4, 3, 2, 0xc1]),
        irk: known.irk,
    };
    assert!(known.match_identity(&same_key));
    assert!(same_key.match_identity(&known));
    let wrong_key = Identity {
        irk: IdentityResolvingKey::new(1),
        ..same_key
    };
    assert!(!known.match_identity(&wrong_key));
    assert!(!wrong_key.match_address(&observed(1)));
}

#[test]
/// Actual bond selection accepts the random RPA but not identical bytes in other address namespaces.
fn bond_lookup_preserves_address_kind() {
    let known = known_identity();
    let mut records = heapless::Vec::<BondInformation, 1>::new();
    records
        .push(BondInformation::new(
            known,
            LongTermKey::new(7),
            SecurityLevel::Encrypted,
            true,
        ))
        .unwrap();
    let bonds = RefCell::new(records);
    let security = SecurityManager::new(&bonds);
    let selected = security.get_peer_bond_information(&observed(1).into()).unwrap();
    assert_eq!(selected.identity, known);
    for kind in [0, 2, 3, 0xff] {
        assert!(
            security.get_peer_bond_information(&observed(kind).into()).is_none(),
            "raw HCI kind {kind}"
        );
    }
}
