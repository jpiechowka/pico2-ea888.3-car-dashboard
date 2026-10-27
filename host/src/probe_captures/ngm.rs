//! INFO evidence projected from validated capture bytes; never live device authentication.
use dashboard_core::ble::canyonero::{DecodeError, FrameDecoder, MAX_FIELD_BYTES, MAX_FRAME_BYTES, Reason};
use dashboard_core::discovery::identity::{CANDIDATE_SPSM, Evidence, SOURCE_COMMIT, VENDOR_UUIDS};
use serde_json::{Value, json};

use super::{hex, reason};

/// Fixed local policy for the explicit selected-peer, source-configured INFO-only trial.
pub(super) const POLICY: &str = "manual-selected-peer-no-pairing-configured-spsm129-info-only-v1";

/// Returns an explicit decoder category and optional received message byte from `error`.
fn protocol_error(error: DecodeError) -> Value {
    let code = match error {
        DecodeError::UnexpectedType(code) | DecodeError::Remote(code) => Some(code),
        _ => None,
    };
    json!({"kind":format!("{error:?}").split('(').next().unwrap_or("Unknown"),"raw_message_type":code})
}

/// Returns JSON null for absent evidence, otherwise lossless bounded observations and derived parsing.
/// `evidence` must have passed the capture decoder; absent fields remain null and no I/O is performed.
pub(super) fn project(evidence: Option<&Evidence>) -> Value {
    let Some(e) = evidence else { return Value::Null };
    let mut decoder = FrameDecoder::new();
    let parsed = decoder.push(e.raw());
    let fields = if e.raw_incomplete() { None } else { decoder.identity() };
    let fault = e.fault.map(|f| {
        let (protocol, transport) = match f.reason {
            Reason::Protocol(error) => (protocol_error(error), Value::Null),
            Reason::Transport(error) => (Value::Null,json!({"domain":format!("{:?}",error.domain),"raw_code_u16":error.code,"partial_send":error.partial_send})),
            _ => (Value::Null,Value::Null),
        };
        json!({"stage":format!("{:?}",f.stage),"at_boot_ms":f.at_ms,
            "kind":format!("{:?}",f.reason).split('(').next().unwrap_or("Unknown"),
            "protocol":protocol,"transport":transport})
    });
    let vendors: Vec<Value> = VENDOR_UUIDS.into_iter().zip(e.vendor_values).map(|(uuid,v)|json!({
        "uuid_u16":uuid,"read":v.map(|v|json!({"service_start_handle":v.service_start,"handle":v.handle,
            "result":format!("{:?}",v.result),"failure":reason(v.failure),"elapsed_ms":v.elapsed_ms,
            "raw_hex":hex(v.value.as_slice()),"retained_bytes":v.value.as_slice().len(),
            "utf8":core::str::from_utf8(v.value.as_slice()).ok(),"truncated":v.value.truncated,"capacity_bytes":64}))
    })).collect();
    json!({
        "version":1,"policy":POLICY,"source_commit":SOURCE_COMMIT,
        "scope":"Untrusted identity text derived from captured INFO bytes; not authentication, ECU support or vehicle data.",
        "candidate":{"configured_spsm":CANDIDATE_SPSM,"discovered_spsm":null,"provenance":"source-configured compatibility trial; no SPSM enumeration"},
        "stage":format!("{:?}",e.stage),"fault":fault,"cleanup":format!("{:?}",e.cleanup),
        "started_at_boot_ms":e.started_at_ms,"updated_at_boot_ms":e.updated_at_ms,"ended_at_boot_ms":e.ended_at_ms,
        "elapsed_ms":e.ended_at_ms.map(|end|end.saturating_sub(e.started_at_ms)),
        "successful":e.successful(),"vendor_reads":vendors,
        "channel":e.channel.map(|c|json!({
            "offered_local_receive":{"mtu_bytes":c.offered_mtu,"mps_bytes":c.offered_mps,"initial_credits":c.offered_credits},
            "observed_local_receive":{"mtu_bytes":c.local_mtu,"mps_bytes":c.local_mps,"cid":c.local_cid,"remaining_credits":c.local_credits},
            "observed_peer_receive":{"mtu_bytes":c.peer_mtu,"mps_bytes":c.peer_mps,"cid":c.peer_cid,"remaining_credits":c.peer_credits}
        })),
        "request":{"allowed_hex":"1f110000","submitted":e.request_submitted_at_ms.is_some(),"submitted_at_boot_ms":e.request_submitted_at_ms,
            "scope":"Complete local SDU submission only; not peer processing acknowledgment."},
        "reply":{"raw_prefix_hex":hex(e.raw()),"retained_bytes":e.raw().len(),"observed_bytes":e.observed_bytes,
            "omitted_observed_bytes":e.observed_bytes.saturating_sub(e.raw().len() as u64),"prefix_incomplete":e.raw_incomplete(),
            "capacity_bytes":MAX_FRAME_BYTES,"observed_sdu_count":e.sdu_count,"max_observed_sdu_bytes":(e.sdu_count>0).then_some(e.max_sdu_bytes),
            "declared_body_bytes":decoder.declared_body_bytes(),
            "retained_prefix_parse_status":parsed.ok().map(|s|format!("{s:?}")),"retained_prefix_parse_error":decoder.error().map(protocol_error),
            "derived_fields":fields.map(|f|json!({"vendor":f.vendor,"model":f.model,"hardware":f.hardware,"serial":f.serial,"firmware":f.firmware,"trailing_lf":f.trailing_lf})),
            "field_limit_bytes":MAX_FIELD_BYTES,
            "observation_scope":"Only SDUs delivered by the transport owner are counted; lower-layer rejected ingress may be unavailable. Parsed fields require all observed bytes retained."}
    })
}
