//! Read-only probe capture recovery, separate from hardware discovery.
/// Derived termination policy kept separate from historical capture flags.
mod assessment;
/// Disabled connection hints derived from validated inventory and shared address rules.
mod handoff;
/// Content identity and portable names preserving raw observed-name provenance.
mod identity;

use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use dashboard_core::discovery::FailureReason;
use dashboard_core::discovery::capture::{self, PacketEvidence, Record};
use dashboard_core::discovery::capture_wire::{FrameHeader, FrameKind, HEADER_BYTES, Request};
use handoff::uuid;
use serde_json::{Value, json};

/// One admitted framed USB response, excluding the terminating End frame.
pub struct Reply {
    /// Validated kind, slot, generation, length and CRC envelope.
    pub header: FrameHeader,
    /// Owned payload whose length/CRC match the header; captures also passed schema decoding.
    pub payload: Vec<u8>,
}
/// Fills `bytes` from `io` before absolute `deadline`, retrying interrupted/timeout/would-block reads.
/// Returns Err for expiry, EOF or other I/O failure; a prefix may already have been overwritten.
/// Individual reads must have finite timeouts because the deadline is checked only between calls.
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
/// Sends `request` over `io` within total `timeout`, then returns four unique List replies or one Get reply.
/// Requires a valid End frame and validates every header/payload plus capture schema for Get.
/// Returns descriptive errors on bounds, I/O, remote rejection, framing or sequencing failures;
/// command bytes may already have been sent and partial responses consumed. Does not flush or resynchronize.
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

/// Returns two lowercase hexadecimal characters per input byte in `bytes`, preserving byte order.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}
/// Returns JSON null for absent `reason`, otherwise its category and optional raw HCI/ATT/hardware byte.
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
/// Returns raw hex, truncation and optional boot-time/RSSI/kind observation from packet evidence `p`.
fn packet(p: &PacketEvidence) -> Value {
    json!({"raw_hex":hex(p.bytes()),"truncated":p.truncated,"observation":p.observation.map(|o|json!({"at_boot_ms":o.at_ms,"rssi_dbm":o.rssi,"kind":format!("{:?}",o.kind)}))})
}

/// Returns a structured evidence export for fully validated capture `bytes`, or a schema error string.
/// Preserves frozen/raw observations; derived identity and disabled connection hints establish no authenticity.
pub fn decode_json(bytes: &[u8]) -> Result<Value, String> { decode_export(bytes).map(|(value, _)| value) }

/// Returns JSON and artifact identity from validated capture `bytes`, hashing the entire binary.
/// Schema failures return Err before projection; raw metadata is retained alongside explicitly derived assessments.
fn decode_export(bytes: &[u8]) -> Result<(Value, PublishedCapture), String> {
    let view = capture::decode(bytes).map_err(|e| format!("capture schema rejected: {e:?}"))?;
    let digest = hex(&sha2_const_stable::Sha256::new().update(bytes).finalize());
    let (identity, publication) = identity::derive(&view.view(), &digest);
    let m = view.metadata();
    let records:Vec<Value>=view.records().map(|r|match r{
        Record::Service(v)=>json!({"type":"service","start_handle":v.start,"end_handle":v.end,"uuid":uuid(v.uuid)}),
        Record::Characteristic(v)=>json!({"type":"characteristic","service_start_handle":v.service_start,"declaration_handle":v.declaration,"value_handle":v.handle,"properties":v.properties,"property_flags":handoff::properties(v.properties),"uuid":uuid(v.uuid)}),
        Record::Descriptor(v)=>json!({"type":"descriptor","service_start_handle":v.service_start,"characteristic_value_handle":v.characteristic_handle,"handle":v.handle,"uuid":uuid(v.uuid)}),
        Record::Value(v)=>json!({"type":"standard_read","service_start_handle":v.service_start,"handle":v.handle,"uuid_u16":v.uuid,"result":format!("{:?}",v.result),"failure":reason(v.failure),"elapsed_ms":v.elapsed_ms,"raw_hex":hex(v.bytes),"utf8":core::str::from_utf8(v.bytes).ok(),"truncated":v.truncated,"standard_length_valid":v.length_valid()})
    }).collect();
    let mut value = json!({"format":"BLECAP01","schema":m.schema_version,"export_schema":1,
        "binary":{"byte_length":bytes.len(),"sha256":digest},
        "connection_handoff":handoff::connection(&view),
        "policy":"manual-selected-peer-no-pairing-standard-read-allowlist-v1","time_basis":"device boot monotonic milliseconds; no UTC recorded","captured_at_boot_ms":m.captured_at_ms,
        "build":{"source_digest_hex":hex(&m.build.source_digest),"source_identity_available":m.build.source_identity_available,"profile":format!("{:?}",m.build.profile),"cpu_hz":m.build.cpu_hz,"spi_hz":m.build.spi_hz,"radio_gspi_hz":m.radio_bus_hz},
        "led_latch":m.fault_latch.map(|l|json!({"scope":"global since recovery; first is atomic recording order, not proven root cause","mask":l.bits(),"first":l.first().map(|f|f.label()),"sources":dashboard_core::discovery::Fault::ALL.into_iter().filter(|f|l.contains(*f)).map(|f|f.label()).collect::<Vec<_>>()})),
        "revision":m.revision,"gatt_requested":m.gatt_requested,"phase":format!("{:?}",m.phase),"error":format!("{:?}",m.error),
        "peer":{"address_display_hex":hex(&m.peer.address),"address_display":m.peer.to_string().as_str(),"random":m.peer.random,"address_kind":m.peer.kind_label(),"first_seen_boot_ms":m.first_seen_ms,"last_seen_boot_ms":m.last_seen_ms,"reports":m.reports,"rssi_dbm":m.rssi},
        "advertisement":packet(&m.advertisement),"scan_response":packet(&m.response),
        "link":m.link.map(|l|json!({"connected_at_boot_ms":l.connected_at_ms,"connect_elapsed_ms":l.connect_elapsed_ms,"interval_us":l.interval_us,"latency":l.latency,"supervision_timeout_us":l.supervision_timeout_us,"att_mtu":l.att_mtu})),
        "inspection":{"termination_assessment":assessment::termination(m),"started_at_boot_ms":m.started_at_ms,"ended_at_boot_ms":m.ended_at_ms,"disconnect_reason":m.disconnect_reason,"cleanup_complete":m.cleanup_complete,"stages_complete":{"services":m.stages[0],"characteristics":m.stages[1],"descriptors":m.stages[2],"reads":m.stages[3]},"omitted":m.omitted,"malformed":m.malformed,"complete":m.complete,
          "failure":m.failure.map(|f|json!({"stage":format!("{:?}",f.stage),"reason":reason(Some(f.reason)),"range":f.range,"last_accepted":f.last_accepted,"rejected":f.rejected.map(|r|json!({"declaration_handle":r.handle,"value_handle":r.value_handle(),"length":r.length(),"raw_prefix_hex":hex(r.bytes()),"prefix_truncated":usize::from(r.length())>r.bytes().len()})),"elapsed_ms":f.elapsed_ms})),"failure_context":{"local_error":reason(m.failure_context.local_error),"disconnect_reason":m.failure_context.disconnect_reason}},
        "global_scan":{"attribution":"global counters; not proven to belong to selected peer","dropped_reports":m.dropped_reports,"malformed_reports":m.malformed_reports,"catalog_overflow_reports":m.catalog_overflow_reports,"total_errors":m.scan_diagnostics.total,"invalid_size":m.scan_diagnostics.invalid_size,"invalid_value":m.scan_diagnostics.invalid_value,"last_error":m.scan_diagnostics.last.map(|s|json!({"kind":format!("{:?}",s.kind),"revision":s.revision,"index":s.index,"total_reports":s.total_reports,"report_bytes":s.report_bytes,"remaining_bytes":s.remaining_bytes,"declared_data_len":s.declared_data_len,"at_boot_ms":s.at_ms}))},
        "record_counts":{"services":view.counts()[0],"characteristics":view.counts()[1],"descriptors":view.counts()[2],"reads":view.counts()[3]},"records":records,
        "limits":"Inventory and standard reads do not establish proprietary authentication, L2CAP PSM acceptance, application commands, ECU support or vehicle data."});
    value["identity"] = identity;
    value["cached_name"] = m.cached_name.map(|n|json!({"source":n.source.label(),"raw_hex":hex(n.data.as_slice()),"utf8":core::str::from_utf8(n.data.as_slice()).ok(),"complete":n.complete,"truncated":n.data.truncated,"observed_at_boot_ms":n.at_ms})).unwrap_or(Value::Null);
    value["boot"] = m.boot.map(|b|json!({"raw_watchdog_reason":b.raw_reason,"rom_boot_type":b.boot_type,"classification":b.label(),"watchdog_fault":b.watchdog_fault(),"previous":b.previous.map(|p|json!({"stage_code":p.stage.code(),"stage":p.stage.label(),"observed_at_previous_boot_ms":p.elapsed_ms})),"scope":"Last retained observation, not proven reset cause or exact reset time. Power removal may erase it; producer image unknown."})).unwrap_or(Value::Null);
    Ok((value, publication))
}

/// Returns a flattened human-readable report of `value`, prefixed with byte-order and evidence limitations.
/// JSON formatting escapes scalar strings; this projection does not itself validate an arbitrary input value.
pub fn text_report(value: &Value) -> String {
    /// Appends JSON `v` to `out` as dotted/indexed fields under `path`; empty collections remain scalar lines.
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
         addresses use display order. Canonical UUID text uses expanded display order.\nHexadecimal byte fields are \
         lowercase. Connection hints require review and live rediscovery.\n\n",
    );
    flatten("", value, &mut out);
    out
}

/// Content-derived artifact labels for a successfully decoded or published capture.
pub struct PublishedCapture {
    /// Portable ASCII basename shared by binary, JSON and text artifacts, without an extension.
    pub file_stem: String,
    /// Full SHA256 content identity prefixed by `sha256:`, not a device-authenticity claim.
    pub capture_id: String,
}

/// A fresh directory and create_new files prevent overwrites. COMPLETE is created
/// only after every artifact is synced; marker write/sync failure still returns an
/// error. A marker alone does not prove durability against storage or power failure.
/// Slot/generation are either both known and valid, or both absent for offline decode.
/// Publishes validated `bytes` under new `directory`; `slot` is zero-based 0..=3 and `generation` nonzero.
/// Returns content-derived artifact labels, or admission/filesystem errors. Parent directory must exist.
/// Errors after directory creation retain partial output for inspection; no existing files are overwritten.
pub fn publish(
    directory: &std::path::Path,
    bytes: &[u8],
    slot: Option<u8>,
    generation: Option<u64>,
) -> Result<PublishedCapture, String> {
    use std::fs::{self, OpenOptions};
    if !matches!((slot, generation), (None, None) | (Some(0..=3), Some(1..))) {
        return Err("invalid export envelope: expected slot 0..3 and nonzero generation, or neither".into());
    }
    let (mut value, publication) = decode_export(bytes)?;
    let binary_name = format!("{}.blecap", publication.file_stem);
    let json_name = format!("{}.json", publication.file_stem);
    let text_name = format!("{}.txt", publication.file_stem);
    value["export"] = json!({"slot":slot.map(|s|s+1),"generation":generation,
        "artifacts":{"binary":binary_name,"json":json_name,"text":text_name}});
    let json = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    let text = text_report(&value);
    fs::create_dir(directory).map_err(|e| format!("create new output directory {}: {e}", directory.display()))?;
    for (name, data) in [
        (binary_name.as_str(), bytes),
        (json_name.as_str(), json.as_slice()),
        (text_name.as_str(), text.as_bytes()),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))
            .map_err(|e| format!("create {name}; partial output remains in {}: {e}", directory.display()))?;
        file.write_all(data).and_then(|()| file.sync_all()).map_err(|e| {
            format!(
                "write/sync {name}; partial output remains in {}: {e}",
                directory.display()
            )
        })?;
    }
    let mut complete = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("COMPLETE"))
        .map_err(|e| format!("create COMPLETE in {}: {e}", directory.display()))?;
    complete
        .write_all(b"Validated binary, JSON and text export complete.\n")
        .and_then(|()| complete.sync_all())
        .map_err(|e| format!("write/sync COMPLETE in {}: {e}", directory.display()))?;
    Ok(publication)
}

/// Raspberry Pi USB vendor identifier used by the capture firmware descriptor.
pub const USB_VID: u16 = 0x2E8A;
/// Capture firmware's USB product identifier; VID/PID alone is only a candidate match.
pub const USB_PID: u16 = 0x0009;
/// Product-string prefix required for automatic unique selection.
pub const USB_PRODUCT: &str = "Pico BLE Probe Captures";
/// Returns whether enumerated `port` is USB with the capture VID/PID, without opening it.
pub fn is_candidate(port: &serialport::SerialPortInfo) -> bool {
    matches!(&port.port_type,serialport::SerialPortType::UsbPort(usb)if usb.vid==USB_VID&&usb.pid==USB_PID)
}
/// Returns an enumerated capture port name from `ports` without opening hardware.
/// `explicit` must exactly match a VID/PID candidate; None additionally requires a unique matching product string.
/// Returns Err for missing, noncandidate or ambiguous selection; generic OS product labels require explicit choice.
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
