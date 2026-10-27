//! Host-only projection of validated evidence for a future connection editor.
//! Hints are not settings, protocol identification, permissions or cached handles.
use dashboard_config::{BleAddressKind, BlePeer};
use dashboard_core::discovery::capture::{DecodedCapture, Record};
use dashboard_core::discovery::{ReadResult, Uuid};
use serde_json::{Value, json};

/// Returns canonical display-order text for validated wire-order `uuid`, expanding 16/32-bit Bluetooth UUIDs.
/// Panics for widths other than 2, 4 or 16; admitted captures already enforce those widths.
fn canonical(uuid: Uuid) -> String {
    // Expand short UUIDs with the Bluetooth base, then reverse wire byte order.
    let mut wire = [0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0, 0, 0, 0];
    match uuid.len {
        2 | 4 => wire[12..12 + usize::from(uuid.len)].copy_from_slice(&uuid.bytes[..usize::from(uuid.len)]),
        16 => wire = uuid.bytes,
        _ => unreachable!("capture decoder validates UUID width"),
    }
    wire.reverse();
    let h = super::hex(&wire);
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Returns raw wire hex, width, optional assigned 16-bit value and canonical display text for `uuid`.
/// Panics for invalid width; callers supply capture-validated UUIDs.
pub(super) fn uuid(uuid: Uuid) -> Value {
    json!({"wire_hex":super::hex(&uuid.bytes[..usize::from(uuid.len)]),
        "width_bytes":uuid.len,"assigned_u16":uuid.short(),"canonical":canonical(uuid)})
}

/// Returns all eight GATT characteristic-property booleans from `bits`; advertised properties prove no access rights.
pub(super) fn properties(bits: u8) -> Value {
    json!({
        "broadcast":bits & 0x01 != 0,
        "read":bits & 0x02 != 0,
        "write_without_response":bits & 0x04 != 0,
        "write_request":bits & 0x08 != 0,
        "notify":bits & 0x10 != 0,
        "indicate":bits & 0x20 != 0,
        "authenticated_signed_writes":bits & 0x40 != 0,
        "extended_properties":bits & 0x80 != 0,
    })
}

/// Returns disabled, evidence-only connection hints from validated `view`, preserving original record indexes.
/// Uses shared address admission and UUID-pair uniqueness; saved handles remain explicitly nonreusable.
/// Scope and unobserved categories cover standard advertisement/GATT hints only;
/// independent INFO/CoC evidence is referenced at the separate ngm_identity export path.
/// Panics only if decoder guarantees about UUID widths or characteristic parent services are violated.
pub(super) fn connection(view: &DecodedCapture<'_>) -> Value {
    let m = view.metadata();
    let kind = if !m.peer.random {
        Some(BleAddressKind::Public)
    } else if m.peer.address[0] >> 6 == 3 {
        Some(BleAddressKind::RandomStatic)
    } else {
        None
    };
    // Share actual Studio/firmware admission. A static address is still only an
    // observation, and can change after a peer power cycle.
    let peer = kind
        .map(|kind| {
            BlePeer {
                address: m.peer.address,
                kind,
            }
        })
        .filter(|p| p.valid(true));
    let blocker = if peer.is_some() {
        None
    } else if kind.is_none() {
        Some("random_address_not_supported")
    } else {
        Some("invalid_dashboard_address")
    };
    // Indexes refer to the original JSON records, including duplicate UUIDs.
    // At most 24 services, 64 characteristics, 128 descriptors and 16 reads.
    let services: Vec<_> = view
        .records()
        .enumerate()
        .filter_map(|(i, r)| {
            match r {
                Record::Service(s) => Some((i, s, canonical(s.uuid))),
                _ => None,
            }
        })
        .collect();
    let chars: Vec<_> = view
        .records()
        .enumerate()
        .filter_map(|(i, r)| {
            match r {
                Record::Characteristic(c) => Some((i, c, canonical(c.uuid))),
                _ => None,
            }
        })
        .collect();
    let endpoints: Vec<_> = chars
        .iter()
        .map(|(i, c, key)| {
            let (service_index, _, service_key) = services
                .iter()
                .find(|(_, s, _)| s.start == c.service_start)
                .expect("capture decoder validates characteristic parent");
            let unique = services.iter().filter(|(_, _, k)| k == service_key).count() == 1
                && chars
                    .iter()
                    .filter(|(_, other, k)| other.service_start == c.service_start && k == key)
                    .count()
                    == 1;
            let cccds: Vec<_> = view
                .records()
                .enumerate()
                .filter_map(|(i, r)| {
                    match r {
                        Record::Descriptor(d)
                            if d.service_start == c.service_start
                                && d.characteristic_handle == c.handle
                                && d.uuid.short() == Some(0x2902) =>
                        {
                            Some(i)
                        }
                        _ => None,
                    }
                })
                .collect();
            let subscription = if c.properties & 0x30 == 0 {
                "not_advertised"
            } else {
                match cccds.len() {
                    0 => "cccd_not_observed",
                    1 => "cccd_observed",
                    _ => "multiple_cccd_observed",
                }
            };
            json!({"service_record_index":service_index,"characteristic_record_index":i,
            "service_uuid":service_key,"characteristic_uuid":key,
            "uuid_pair_unique_in_capture":unique,"cccd_record_indices":cccds,
            "subscription_evidence":subscription})
        })
        .collect();
    let hashes: Vec<_> = view
        .records()
        .enumerate()
        .filter_map(|(i, r)| {
            match r {
                Record::Value(v)
                    if v.uuid == 0x2B2A && v.result == ReadResult::Ok && v.length_valid() && !v.truncated =>
                {
                    Some(i)
                }
                _ => None,
            }
        })
        .collect();
    json!({"version":1,"status":"evidence_only","enabled":false,"profile":null,
        "observation_scope":"standard_advertisement_and_gatt_hints_only",
        "independent_identity_evidence_path":"ngm_identity",
        "address_candidate":peer,"address_blocker":blocker,
        "inventory_complete":m.gatt_requested && m.stages[..3].iter().all(|v| *v)
            && m.omitted == 0 && m.malformed == 0,
        "saved_handles_reusable":false,"endpoints":endpoints,
        "database_hash_read_record_indices":hashes,
        "unobserved":["application_protocol","application_authentication","l2cap_spsm_and_channel_limits",
            "vendor_write_acceptance","notification_payloads"],
        "review_required":"Identify a supported protocol, verify peer identity, rediscover live endpoints and validate security before use; never enable from UUID/name matches alone."})
}
