//! Derived policy assessment, never a rewrite of historical firmware evidence.
use dashboard_core::ble::termination::{normal_reason, reason_label};
use dashboard_core::discovery::capture::CaptureMetadata;
use serde_json::{Value, json};

pub(super) fn termination(m: &CaptureMetadata) -> Value {
    let reason = m.disconnect_reason;
    let consistent = m
        .failure_context
        .disconnect_reason
        .is_none_or(|code| Some(code) == reason);
    let status = if !m.gatt_requested || (m.link.is_none() && reason.is_none()) {
        "not_observed"
    } else if m.failure_context.local_error.is_some() {
        "local_ingress_error"
    } else if !m.cleanup_complete {
        "cleanup_unconfirmed"
    } else if !consistent {
        "conflicting_reason"
    } else if reason.is_none() {
        "reason_unavailable"
    } else if normal_reason(reason) {
        "normal_reason"
    } else {
        "abnormal_reason"
    };
    json!({"version":1,"status":status,
        "reason_hex":reason.map(|code|format!("0x{code:02X}")),
        "reason_label":reason.map(reason_label),
        "acceptable_for_clean_probe":status == "normal_reason" && m.link.is_some() && m.ended_at_ms.is_some(),
        "scope":"Derived termination assessment only; original completion, failure and LED fields remain historical evidence. Normal termination does not establish successful inspection or adapter compatibility."})
}
