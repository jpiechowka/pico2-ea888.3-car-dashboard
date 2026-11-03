//! Offline raw collection projection; never grants live qualification or replays saved traffic.
use dashboard_core::ble::canyonero::reads::{CandidateValue, ReadId};
use dashboard_core::discovery::capture::{CaptureView, Record};
use serde_json::{Value, json};

use super::hex;

/// Returns an explicitly unqualified family candidate from complete response `raw` for `id`.
/// Unknown schemas, negatives, mismatches and incomplete replies return JSON null.
fn candidate(
    id: ReadId,
    raw: &[u8],
) -> Value {
    let Some(value) = id.candidate(raw) else {
        return Value::Null;
    };
    let (number, unit) = match value {
        CandidateValue::Degrees(v) => (v, "degrees"),
        CandidateValue::AbsoluteKpa(v) => (v, "kPa absolute"),
        CandidateValue::Celsius(v) => (v, "degC"),
        CandidateValue::Volts(v) => (v, "V at ECU module"),
        CandidateValue::Lambda(v) => (v, "lambda"),
    };
    json!({"value":number,"unit":unit,"qualification":"unqualified family interpretation; no sentinel or exact ECU qualification","live_measurement":false})
}
/// Returns the collection-only JSON for admitted `view`, or null for legacy/identity-only captures.
/// All raw bytes remain lossless; candidate values are segregated from raw outcome and completeness.
pub(super) fn project(view: &CaptureView<'_>) -> Value {
    let Some(summary) = view.metadata().collection else {
        return Value::Null;
    };
    let rows: Vec<Value> = view
        .records()
        .filter_map(|record| {
            let Record::Collection(row) = record else {
                return None;
            };
            Some(
                json!({"manifest_index":row.id.index(),"did":format!("{:04X}",row.id.did()),"label":row.id.label(),
            "request_hex":hex(&row.id.request()),"attempted_at_boot_ms":row.attempted_at_ms(),
            "locally_submitted_at_boot_ms":row.submitted_at_ms(),"response_at_boot_ms":row.completed_at_ms(),
            "submission_unknown":row.attempted_at_ms().is_some()&&row.submitted_at_ms().is_none(),
            "result":row.status.map(|s|format!("{s:?}")),"pending_nrc_78_count":row.pending,
            "declared_response_bytes":row.declared_len,"retained_response_bytes":row.raw.len(),
            "response_complete":row.complete(),"raw_hex":hex(row.raw),
            "candidate_expected_data_bytes":row.id.expected_data_len(),
            "classification":if row.complete(){row.id.classify(row.raw).ok().map(|r|format!("{r:?}"))}else{None},
            "candidate":if row.complete(){candidate(row.id,row.raw)}else{Value::Null},"live_measurement":false}),
            )
        })
        .collect();
    let terminal=view.records().find_map(|r|if let Record::CollectionEnvelope(v)=r{Some(json!({"kind":format!("{:?}",v.kind),
        "declared_prefix_bytes":v.declared_len,"retained_prefix_bytes":v.raw.len(),"raw_hex":hex(v.raw),
        "prefix_complete":v.raw.len()==usize::from(v.declared_len),"scope":"latest retained envelope or UDS prefix; not a complete wire transcript"}))}else{None});
    json!({"policy":"explicit-fixed-engine-single-DID-raw-collection-v1","qualification":"unqualified raw evidence; no live vehicle values or alarms",
        "route":{"request_can_id":"7E0","response_can_id":"7E8","protocol":"classical raw CAN00 with host ISO-TP","physical_route_qualified":false},
        "started_at_boot_ms":summary.started_at_ms,"updated_at_boot_ms":summary.updated_at_ms,"ended_at_boot_ms":summary.ended_at_ms,
        "adapter_supply":{"millivolts":summary.adapter_mv,"scope":"adapter-reported supply, not ECU module or battery-post voltage"},
        "fresh_handle_observed":summary.handle,"fixed_arbitration_acknowledged":summary.configured,
        "close_acknowledged":summary.completed,"fault":summary.fault.map(|f|format!("{f:?}")),"cleanup":format!("{:?}",summary.cleanup),
        "last_submission":summary.last_submission.map(|s|json!({"command":format!("{:?}",s.command),"state":format!("{:?}",s.state),"attempted_at_boot_ms":s.attempted_at_ms,"completed_at_boot_ms":s.completed_at_ms})),
        "manifest_count":view.metadata().collection_row_count,"rows":rows,"terminal_prefix":terminal,
        "limits":"Local submission does not prove CAN acceptance. Saved handles and responses grant no command authority. Read requests and ISO-TP flow control are active traffic."})
}
