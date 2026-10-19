//! Read-only probe capture recovery, separate from hardware discovery.
use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use dashboard_core::discovery::capture::{self, PacketEvidence, Record};
use dashboard_core::discovery::capture_wire::{FrameHeader, FrameKind, HEADER_BYTES, Request};
use dashboard_core::discovery::{FailureReason, Uuid};
use serde_json::{Value, json};

pub struct Reply {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}
fn read_exact(
    io: &mut (impl Read + ?Sized),
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), String> {
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err("capture transaction timed out".into());
        }
        match io.read(bytes) {
            Ok(0) => return Err("capture stream ended early".into()),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::Interrupted | ErrorKind::TimedOut | ErrorKind::WouldBlock
                ) => {}
            Err(e) => return Err(format!("capture read failed: {e}")),
        }
    }
    Ok(())
}
/// The caller must configure finite individual I/O timeouts. This deadline bounds
/// partial-progress streams too; no flushing call can wait for a non-reading peer.
pub fn exchange(
    io: &mut (impl Read + Write + ?Sized),
    request: Request,
    timeout: Duration,
) -> Result<Vec<Reply>, String> {
    let deadline = Instant::now().checked_add(timeout).ok_or("invalid timeout")?;
    let command = match request {
        Request::List => "BLECAP LIST\n".to_owned(),
        Request::Get(slot) if slot < 4 => format!("BLECAP GET {}\n", slot + 1),
        _ => return Err("slot must be 1..4".into()),
    };
    let mut pending = command.as_bytes();
    while !pending.is_empty() {
        if Instant::now() >= deadline {
            return Err("capture command timed out".into());
        }
        match io.write(pending) {
            Ok(0) => return Err("capture command write stopped".into()),
            Ok(n) => pending = &pending[n..],
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::Interrupted | ErrorKind::TimedOut | ErrorKind::WouldBlock
                ) => {}
            Err(e) => return Err(format!("capture write failed: {e}")),
        }
    }
    let mut replies = Vec::with_capacity(4);
    let mut seen = 0u8;
    let limit = if request == Request::List { 5 } else { 2 };
    let mut remote_error = None;
    for _ in 0..limit {
        let mut bytes = [0; HEADER_BYTES];
        read_exact(io, &mut bytes, deadline)?;
        let header = FrameHeader::decode(&bytes).map_err(|e| format!("invalid frame header: {e:?}"))?;
        let mut payload = vec![0; usize::from(header.length)];
        read_exact(io, &mut payload, deadline)?;
        header
            .validate_payload(&payload)
            .map_err(|e| format!("invalid frame payload: {e:?}"))?;
        if header.kind == FrameKind::End {
            if let Some(error) = remote_error {
                return Err(error);
            }
            if (request == Request::List && seen == 15) || (matches!(request, Request::Get(_)) && replies.len() == 1) {
                return Ok(replies);
            }
            return Err("incomplete capture response before End".into());
        }
        if remote_error.is_some() {
            return Err("unexpected frame after error".into());
        }
        if header.kind == FrameKind::Error {
            if !replies.is_empty() || header.slot.is_some_and(|slot| request != Request::Get(slot)) {
                return Err("error frame has unexpected slot or position".into());
            }
            remote_error = Some(format!("probe reported {:?}", header.status));
            continue;
        }
        match request {
            Request::List => {
                if header.kind != FrameKind::ListEntry {
                    return Err("expected slot list entry".into());
                }
                let slot = header.slot.ok_or("missing list slot")?;
                if seen & (1 << slot) != 0 {
                    return Err("duplicate list slot".into());
                }
                seen |= 1 << slot;
            }
            Request::Get(slot) => {
                if header.kind != FrameKind::Capture || header.slot != Some(slot) || !replies.is_empty() {
                    return Err("unexpected capture slot or sequence".into());
                }
                capture::decode(&payload).map_err(|e| format!("capture schema rejected: {e:?}"))?;
            }
        }
        replies.push(Reply { header, payload });
    }
    Err("missing End frame".into())
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}
fn uuid(uuid: Uuid) -> Value {
    json!({"wire_hex":hex(&uuid.bytes[..usize::from(uuid.len)]),"width_bytes":uuid.len,"assigned_u16":uuid.short()})
}
fn reason(reason: Option<FailureReason>) -> Value {
    reason.map_or(Value::Null, |reason| {
        let raw = match reason {
            FailureReason::Hardware(v)
            | FailureReason::DisconnectCode(v)
            | FailureReason::Att(v)
            | FailureReason::Hci(v) => Some(v),
            _ => None,
        };
        json!({"kind":format!("{reason:?}").split('(').next().unwrap_or("Unknown"),"raw_code":raw})
    })
}
fn packet(p: &PacketEvidence) -> Value {
    json!({"raw_hex":hex(p.bytes()),"truncated":p.truncated,"observation":p.observation.map(|o|json!({"at_boot_ms":o.at_ms,"rssi_dbm":o.rssi,"kind":format!("{:?}",o.kind)}))})
}

pub fn decode_json(bytes: &[u8]) -> Result<Value, String> {
    let view = capture::decode(bytes).map_err(|e| format!("capture schema rejected: {e:?}"))?;
    let m = view.metadata();
    let records:Vec<Value>=view.records().map(|r|match r{
        Record::Service(v)=>json!({"type":"service","start_handle":v.start,"end_handle":v.end,"uuid":uuid(v.uuid)}),
        Record::Characteristic(v)=>json!({"type":"characteristic","service_start_handle":v.service_start,"declaration_handle":v.declaration,"value_handle":v.handle,"properties":v.properties,"uuid":uuid(v.uuid)}),
        Record::Descriptor(v)=>json!({"type":"descriptor","service_start_handle":v.service_start,"characteristic_value_handle":v.characteristic_handle,"handle":v.handle,"uuid":uuid(v.uuid)}),
        Record::Value(v)=>json!({"type":"standard_read","service_start_handle":v.service_start,"handle":v.handle,"uuid_u16":v.uuid,"result":format!("{:?}",v.result),"failure":reason(v.failure),"elapsed_ms":v.elapsed_ms,"raw_hex":hex(v.bytes),"utf8":core::str::from_utf8(v.bytes).ok(),"truncated":v.truncated,"standard_length_valid":v.length_valid()})
    }).collect();
    Ok(
        json!({"format":"BLECAP01","schema":m.schema_version,"policy":"manual-selected-peer-no-pairing-standard-read-allowlist-v1","time_basis":"device boot monotonic milliseconds; no UTC recorded","captured_at_boot_ms":m.captured_at_ms,
        "build":{"source_digest_hex":hex(&m.build.source_digest),"source_identity_available":m.build.source_identity_available,"profile":format!("{:?}",m.build.profile),"cpu_hz":m.build.cpu_hz,"spi_hz":m.build.spi_hz},
        "revision":m.revision,"gatt_requested":m.gatt_requested,"phase":format!("{:?}",m.phase),"error":format!("{:?}",m.error),
        "peer":{"address_display_hex":hex(&m.peer.address),"address_display":m.peer.to_string().as_str(),"random":m.peer.random,"address_kind":m.peer.kind_label(),"first_seen_boot_ms":m.first_seen_ms,"last_seen_boot_ms":m.last_seen_ms,"reports":m.reports,"rssi_dbm":m.rssi},
        "advertisement":packet(&m.advertisement),"scan_response":packet(&m.response),
        "link":m.link.map(|l|json!({"connected_at_boot_ms":l.connected_at_ms,"connect_elapsed_ms":l.connect_elapsed_ms,"interval_us":l.interval_us,"latency":l.latency,"supervision_timeout_us":l.supervision_timeout_us,"att_mtu":l.att_mtu})),
        "inspection":{"started_at_boot_ms":m.started_at_ms,"ended_at_boot_ms":m.ended_at_ms,"disconnect_reason":m.disconnect_reason,"cleanup_complete":m.cleanup_complete,"stages_complete":{"services":m.stages[0],"characteristics":m.stages[1],"descriptors":m.stages[2],"reads":m.stages[3]},"omitted":m.omitted,"malformed":m.malformed,"complete":m.complete,
          "failure":m.failure.map(|f|json!({"stage":format!("{:?}",f.stage),"reason":reason(Some(f.reason)),"range":f.range,"last_accepted":f.last_accepted,"rejected":f.rejected.map(|r|json!({"declaration_handle":r.handle,"value_handle":r.value_handle(),"length":r.length(),"raw_prefix_hex":hex(r.bytes()),"prefix_truncated":usize::from(r.length())>r.bytes().len()})),"elapsed_ms":f.elapsed_ms})),"failure_context":{"local_error":reason(m.failure_context.local_error),"disconnect_reason":m.failure_context.disconnect_reason}},
        "global_scan":{"attribution":"global counters; not proven to belong to selected peer","dropped_reports":m.dropped_reports,"malformed_reports":m.malformed_reports,"catalog_overflow_reports":m.catalog_overflow_reports,"total_errors":m.scan_diagnostics.total,"invalid_size":m.scan_diagnostics.invalid_size,"invalid_value":m.scan_diagnostics.invalid_value,"last_error":m.scan_diagnostics.last.map(|s|json!({"kind":format!("{:?}",s.kind),"revision":s.revision,"index":s.index,"total_reports":s.total_reports,"report_bytes":s.report_bytes,"remaining_bytes":s.remaining_bytes,"declared_data_len":s.declared_data_len,"at_boot_ms":s.at_ms}))},
        "record_counts":{"services":view.counts()[0],"characteristics":view.counts()[1],"descriptors":view.counts()[2],"reads":view.counts()[3]},"records":records,
        "limits":"Inventory and standard reads do not establish proprietary authentication, L2CAP PSM acceptance, application commands, ECU support or vehicle data."}),
    )
}

pub fn text_report(value: &Value) -> String {
    fn flatten(
        path: &str,
        v: &Value,
        out: &mut String,
    ) {
        use std::fmt::Write;
        match v {
            Value::Object(map) => {
                for (k, v) in map {
                    flatten(
                        &if path.is_empty() {
                            k.clone()
                        } else {
                            format!("{path}.{k}")
                        },
                        v,
                        out,
                    )
                }
            }
            Value::Array(list) if !list.is_empty() => {
                for (i, v) in list.iter().enumerate() {
                    flatten(&format!("{path}[{i}]"), v, out)
                }
            }
            _ => {
                let _ = writeln!(out, "{path}: {v}");
            }
        }
    }
    let mut out = String::from(
        "BLE probe capture: validated offline evidence\nRaw payload and UUID bytes use Bluetooth wire order; peer \
         addresses use display order. All hexadecimal fields are lowercase.\n\n",
    );
    flatten("", value, &mut out);
    out
}

/// A fresh directory and create_new files prevent overwrites. COMPLETE is written
/// only after every artifact is synced. Failed/interrupted exports lack this marker.
pub fn publish(
    directory: &std::path::Path,
    bytes: &[u8],
    slot: Option<u8>,
    generation: Option<u64>,
) -> Result<(), String> {
    use std::fs::{self, OpenOptions};
    let mut value = decode_json(bytes)?;
    value["export"] = json!({"slot":slot.map(|s|s+1),"generation":generation});
    let json = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    let text = text_report(&value);
    fs::create_dir(directory).map_err(|e| format!("create new output directory {}: {e}", directory.display()))?;
    for (name, data) in [
        ("capture.blecap", bytes),
        ("capture.json", json.as_slice()),
        ("capture.txt", text.as_bytes()),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(data)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("partial output remains in {}: {e}", directory.display()))?;
    }
    let mut complete = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("COMPLETE"))
        .map_err(|e| e.to_string())?;
    complete
        .write_all(b"Validated binary, JSON and text export complete.\n")
        .and_then(|()| complete.sync_all())
        .map_err(|e| e.to_string())
}

pub const USB_VID: u16 = 0x2E8A;
pub const USB_PID: u16 = 0x0009;
pub const USB_PRODUCT: &str = "Pico BLE Probe Captures";
pub fn is_candidate(port: &serialport::SerialPortInfo) -> bool {
    matches!(&port.port_type,serialport::SerialPortType::UsbPort(usb)if usb.vid==USB_VID&&usb.pid==USB_PID)
}
pub fn select_port(
    ports: &[serialport::SerialPortInfo],
    explicit: Option<&str>,
) -> Result<String, String> {
    if let Some(name) = explicit {
        return ports
            .iter()
            .find(|p| p.port_name == name && is_candidate(p))
            .map(|p| p.port_name.clone())
            .ok_or_else(|| {
                format!("{name} is not an enumerated {USB_VID:04X}:{USB_PID:04X} capture candidate; run ports")
            });
    }
    let mut matching=ports.iter().filter(|p|is_candidate(p)&&matches!(&p.port_type,serialport::SerialPortType::UsbPort(usb)if usb.product.as_deref().is_some_and(|product|product==USB_PRODUCT||product.starts_with(&format!("{USB_PRODUCT} (")))));
    let port = matching
        .next()
        .ok_or("no uniquely identified capture product; run ports and pass --port COMx explicitly")?;
    if matching.next().is_some() {
        return Err("multiple capture devices; choose --port COMx explicitly".into());
    }
    Ok(port.port_name.clone())
}
