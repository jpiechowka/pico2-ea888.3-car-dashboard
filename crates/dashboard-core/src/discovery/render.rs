//! Fixed landscape photographic UI. Pagination walks bounded records without allocating a document.
use core::fmt::Write;

use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_7X13, FONT_9X15};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Baseline;
use embedded_graphics::text::renderer::TextRenderer;
use heapless::String;

use super::*;
/// Shared live/saved adapter identity evidence rows with bounded text and raw bytes.
mod identity_view;
/// Full-screen dark background, in native RGB565 component ranges.
const BG: Rgb565 = Rgb565::new(2, 3, 4);
/// Highlighted selection/footer background.
const PANEL: Rgb565 = Rgb565::new(4, 6, 7);
/// Primary evidence/header text colour.
const WHITE: Rgb565 = Rgb565::new(30, 61, 30);
/// Secondary context and unavailable-value text colour.
const MUTED: Rgb565 = Rgb565::new(20, 42, 22);
/// Error, active selection and page-heading accent.
const RED: Rgb565 = Rgb565::new(31, 8, 6);
/// Evidence rows per page; 11-pixel spacing leaves the footer unobscured.
const LINES: usize = 10;
/// Returns empty-catalog status and recovery hint matching radio `phase`.
fn empty_copy(phase: RadioPhase) -> (&'static str, &'static str) {
    match phase {
        RadioPhase::Scanning => {
            (
                "Listening for connectable advertisements",
                "Some devices may not advertise.",
            )
        }
        RadioPhase::Paused => ("Scan paused / no recorded peers", "Press Y to resume scanning."),
        RadioPhase::Fault => {
            (
                "Radio stopped / no recorded peers",
                "Automatic restart in about 2 seconds.",
            )
        }
        RadioPhase::Starting => ("Starting Bluetooth radio...", "If stalled, hold Y 2s to recover."),
        RadioPhase::Stopping => ("Waiting for radio to stop...", "Y: back / resume discovery."),
        RadioPhase::Connecting => ("Connecting to selected peer...", "Y: back / cancel inspection."),
        RadioPhase::Discovering => ("Inspecting selected peer...", "Y: back / cancel inspection."),
        RadioPhase::Disconnecting => ("Closing inspection connection...", "Y: back to discovery."),
    }
}
/// Returns the code-preserving status/recovery label for current `error`.
fn error_copy(error: ProbeError) -> &'static str {
    match error {
        ProbeError::None => "Error:None",
        ProbeError::Scan => "Scan: failed; Y to rescan",
        ProbeError::Connect => "Connect: retry / Y back",
        ProbeError::Timeout => "Timeout: deadline; Y back",
        ProbeError::Discovery => "Discovery: failed; Y back",
        ProbeError::Capacity => "Capacity: partial results",
        ProbeError::Disconnected => "Disconnected: see details",
        ProbeError::Cleanup => "Cleanup: restarting",
        ProbeError::Transport => "Transport: failed; Y back",
        ProbeError::InvalidPeer => "InvalidPeer: Y to rescan",
    }
}
/// Returns current `probe` error text first, otherwise its latched fault header.
fn status_copy(probe: &Probe) -> &'static str {
    if probe.error != ProbeError::None {
        return error_copy(probe.error);
    }
    probe.fault_latch.header()
}
/// Returns whether `probe`'s active request explicitly inspects its selected
/// address/type identity; retained records alone do not establish a request.
fn gatt_requested(probe: &Probe) -> bool {
    matches!(probe.request.action, Action::Inspect(peer) if probe.selected_device().is_some_and(|d| d.peer == peer))
}

/// Returns two footer rows for `probe` at boot-relative milliseconds `now_ms`.
/// Busy/fault/stale-advertisement states suppress unavailable Inspect actions;
/// retained GATT evidence is independent of advertisement freshness.
fn control_hints(
    probe: &Probe,
    now_ms: u64,
) -> [&'static str; 2] {
    let details = probe.screen == Screen::Details;
    // These phases ignore both Inspect inputs; keep the footer honest while
    // the radio acknowledges stop/disconnect, including after cancellation.
    let busy = matches!(
        probe.phase,
        RadioPhase::Stopping | RadioPhase::Connecting | RadioPhase::Discovering | RadioPhase::Disconnecting
    );
    let first = if probe.phase == RadioPhase::Fault {
        if details {
            "Turn/A/B: pages  GATT unavailable"
        } else {
            "Turn/A/B: select  GATT unavailable"
        }
    } else if busy {
        if details {
            "Turn/A/B: pages  GATT busy; please wait"
        } else {
            "Turn/A/B: select  GATT busy; please wait"
        }
    } else if let Some(peer) = probe.selected_device() {
        if peer.age_ms(now_ms) > 30_000 {
            if details {
                "Turn/A/B: pages  Advert old; Y to rescan"
            } else {
                "Turn/A/B: select  Advert old; rescan"
            }
        } else if probe.identity_mode {
            if details {
                "Turn/A/B: pages Click/X/Hold A 2s: INFO"
            } else {
                "Turn/A/B: select Click/Hold A 2s: INFO"
            }
        } else if details {
            if gatt_requested(probe) {
                "Turn/A/B: pages Click/X/Hold A 2s: Repeat"
            } else {
                "Turn/A/B: pages Click/X/Hold A 2s: Inspect"
            }
        } else {
            "Turn/A/B: select Click/Hold A 2s: Inspect"
        }
    } else if details {
        "Turn/A/B: pages  No peer to inspect"
    } else {
        "Turn/A/B: select  No peer selected"
    };
    let second = if details {
        if busy && gatt_requested(probe) {
            "Y: Cancel / back to scan"
        } else if gatt_requested(probe) && probe.selected_device().is_some_and(|peer| peer.age_ms(now_ms) > 30_000) {
            "GATT retained here; Y: scan  Hold X: slots"
        } else {
            "Y: Back to scan"
        }
    } else if probe.selected_device().is_none() {
        if probe.fault_latch != FaultLatch::EMPTY {
            if matches!(probe.request.action, Action::Scan(true)) {
                "X: Fault details  Y: Pause scan"
            } else {
                "X: Fault details  Y: Resume scan"
            }
        } else if probe.scan_diagnostics.last.is_some() {
            if matches!(probe.request.action, Action::Scan(true)) {
                "X: Scan diagnostics  Y: Pause scan"
            } else {
                "X: Scan diagnostics  Y: Resume scan"
            }
        } else if matches!(probe.request.action, Action::Scan(true)) {
            "Y: Pause scan"
        } else {
            "Y: Resume scan"
        }
    } else if matches!(probe.request.action, Action::Scan(true)) {
        "X: Advert only (fresh)  Y: Pause scan"
    } else {
        "X: Advert only (fresh)  Y: Resume scan"
    };
    [first, second]
}
/// Returns all set GATT characteristic `properties` bit labels in wire-bit
/// order, or `None` text for zero; all eight labels fit the 50-byte buffer.
fn properties_text(properties: u8) -> String<50> {
    let mut out = String::new();
    for (bit, label) in [
        (1, "Bcast"),
        (2, "R"),
        (4, "WriteCmd"),
        (8, "W"),
        (16, "Notify"),
        (32, "Indicate"),
        (64, "Signed"),
        (128, "Ext"),
    ] {
        if properties & bit != 0 {
            if !out.is_empty() {
                let _ = out.push(' ');
            }
            let _ = out.push_str(label);
        }
    }
    if out.is_empty() {
        let _ = out.push_str("None");
    }
    out
}
/// Returns the prefix successfully formatted from `args` into 50 bytes;
/// formatting/capacity errors are ignored, so callers bound their row content.
fn line(args: core::fmt::Arguments<'_>) -> String<50> {
    let mut s = String::new();
    let _ = s.write_fmt(args);
    s
}
/// Draws single-line sanitized ASCII `s` into `d` at top-left pixel (`x`, `y`)
/// in `c` using 6x10 glyphs. Returns the first target error after possible
/// partial writes, or `Ok(())`; clipping is the caller's responsibility.
fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    x: i32,
    y: i32,
    s: &str,
    c: Rgb565,
) -> Result<(), D::Error> {
    font_text(d, Point::new(x, y), s, c, &FONT_6X10)
}
/// Draws sanitized single-line ASCII `s` into `d` at top-left `position`, in
/// `c` with `font`. Returns `Ok(())` or the first draw error; prior pixels remain.
#[inline(never)]
fn font_text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    position: Point,
    s: &str,
    c: Rgb565,
    font: &MonoFont<'_>,
) -> Result<(), D::Error> {
    // Every row is single-line ASCII (peer bytes are sanitized before here).
    // Use the same glyph renderer without Text's unused multiline/alignment pass.
    MonoTextStyle::new(font, c)
        .draw_string(s, position, Baseline::Top, d)
        .map(|_| ())
}

/// Returns one-decimal completed-frame Hz for `value`, `--` for absent,
/// negative or nonfinite values, and the shared overflow marker above range.
fn rate(value: Option<f32>) -> String<8> { crate::diagnostic_values::rate_text(value) }

/// Returns one-decimal percent for instrumented work `value` in tenths-percent
/// `0..=1000`, otherwise `--%`; this is not whole-core utilization.
fn work(value: Option<u16>) -> String<8> {
    let mut s = crate::diagnostic_values::work_text(value.map(u32::from));
    if s == "--" {
        let _ = s.push('%');
    }
    s
}

/// Returns used/total KiB for byte counts in `value` with shared upward rounding,
/// or `--/--K` when absent or outside admitted memory bounds.
fn memory(value: Option<MemoryUse>) -> String<12> {
    let mut s = String::new();
    if let Some((used, total)) = value.and_then(|v| crate::diagnostic_values::memory_kib(v.used, v.total)) {
        let _ = write!(s, "{used}/{total}K");
    } else {
        let _ = s.push_str("--/--K");
    }
    s
}

/// Paints current-Pico `values` in independent clipped header lanes on `d`.
/// Returns `Ok(())` or the first target error after possible partial writes;
/// stack values remain observed samples, not continuous high-water marks.
/// Each lane lies within the fixed screen bounds, so callers pass the original
/// target without adding a redundant full-screen clip around these lane clips.
fn telemetry<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    values: ProbeDiagnostics,
) -> Result<(), D::Error> {
    let mut fps = d.clipped(&Rectangle::new(Point::new(10, 25), Size::new(152, 10)));
    text(
        &mut fps,
        10,
        25,
        &line(format_args!(
            "FPS {} AVG {}",
            rate(values.fps.map(|v| v.current)).as_str(),
            rate(values.fps.map(|v| v.average)).as_str()
        )),
        WHITE,
    )?;
    let mut cpu = d.clipped(&Rectangle::new(Point::new(166, 25), Size::new(144, 10)));
    text(
        &mut cpu,
        166,
        25,
        &line(format_args!(
            "W C0 {} C1 {}",
            work(values.work[0]).as_str(),
            work(values.work[1]).as_str()
        )),
        WHITE,
    )?;
    let mut ram = d.clipped(&Rectangle::new(Point::new(10, 37), Size::new(300, 10)));
    text(
        &mut ram,
        10,
        37,
        &line(format_args!(
            "Static {} SP0 {} SP1 {}",
            memory(values.ram).as_str(),
            memory(values.stacks[0]).as_str(),
            memory(values.stacks[1]).as_str()
        )),
        MUTED,
    )
}
/// Fills pixel rectangle (`x`, `y`, `w`, `h`) in `c` on `d`. Returns `Ok(())`
/// or the target error with any earlier writes retained; does not add clipping.
fn rect<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    c: Rgb565,
) -> Result<(), D::Error> {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(c))
        .draw(d)
}
/// Returns a recognized standard label for `uuid`, including Bluetooth-base
/// aliases, or empty text for an unrecognized UUID; conveys no compatibility.
fn uuid_label(uuid: Uuid) -> &'static str {
    match uuid.short() {
        Some(0x1800) => "Generic Access",
        Some(0x1801) => "Generic Attribute",
        Some(0x180A) => "Device Information",
        Some(0x180F) => "Battery",
        Some(0x2A00) => "Device Name",
        Some(0x2A01) => "Appearance",
        Some(0x2A19) => "Battery Level",
        Some(v) => info_label(v).unwrap_or(""),
        None => "",
    }
}
// Count and draw share a concrete writer and one formatting buffer across the
// traversal. Keep the callback inside the writer, not as a formatter parameter:
// otherwise LLVM specializes the whole packet/record tree for each callback.
/// Counts or streams bounded evidence without allocating a complete document.
struct EvidenceLines<'a> {
    /// Optional borrowed row consumer; absent mode avoids value formatting.
    output: Option<&'a mut dyn FnMut(&str)>,
    /// Rows visited; bounded protocol inventories keep arithmetic below usize limits.
    count: usize,
    /// Reused 50-byte ASCII row buffer, borrowed only during each callback.
    scratch: String<50>,
}
impl<'a> EvidenceLines<'a> {
    /// Returns an empty traversal with `output` as its optional row sink;
    /// `None` counts rows without invoking formatters.
    fn new(output: Option<&'a mut dyn FnMut(&str)>) -> Self {
        Self {
            output,
            count: 0,
            scratch: String::new(),
        }
    }

    /// Counts one row and sends `value` unchanged to the sink when present.
    /// The caller provides bounded ASCII; this path does not truncate.
    fn text(
        &mut self,
        value: &str,
    ) {
        self.count += 1;
        if let Some(output) = &mut self.output {
            output(value);
        }
    }

    /// Counts one row; formats `args` only with an active sink, retaining any
    /// successfully written prefix on formatting/capacity failure.
    #[inline(never)]
    fn formatted(
        &mut self,
        args: core::fmt::Arguments<'_>,
    ) {
        self.count += 1;
        if let Some(output) = &mut self.output {
            self.scratch.clear();
            let _ = self.scratch.write_fmt(args);
            output(&self.scratch);
        }
    }

    /// Emits/counts `uuid` as reversed wire bytes without separators; malformed
    /// lengths are capped to its 16-byte storage, including zero-length rows.
    fn uuid(
        &mut self,
        uuid: Uuid,
    ) {
        self.hex_line(&uuid.bytes[..usize::from(uuid.len).min(16)], true);
    }

    // Raw rows contain at most 16 bytes. UUID rows reverse those bytes and omit
    // spaces. Fixed nibbles avoid per-byte fmt state and a second line buffer.
    /// Counts/emits `bytes` in uppercase hex; `uuid_order` reverses bytes and
    /// omits spaces. Callers use at most 16 bytes so the 50-byte row cannot fill.
    #[inline(never)]
    fn hex_line(
        &mut self,
        bytes: &[u8],
        uuid_order: bool,
    ) {
        self.count += 1;
        if let Some(output) = &mut self.output {
            self.scratch.clear();
            for index in 0..bytes.len() {
                if !uuid_order && index > 0 {
                    let _ = self.scratch.push(' ');
                }
                let byte = bytes[if uuid_order { bytes.len() - index - 1 } else { index }];
                /// Uppercase ASCII nibble lookup, indexed only by masked four-bit values.
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                let _ = self.scratch.push(char::from(HEX[usize::from(byte >> 4)]));
                let _ = self.scratch.push(char::from(HEX[usize::from(byte & 15)]));
            }
            output(&self.scratch);
        }
    }
}
/// Sends all `bytes` to `emit` in wire-order rows of at most 16 bytes; empty
/// input emits no rows, and partial final rows retain every byte.
fn hex(
    bytes: &[u8],
    emit: &mut EvidenceLines<'_>,
) {
    for chunk in bytes.chunks(16) {
        emit.hex_line(chunk, false);
    }
}
/// Sends `title`, raw `payload` prefix and parsed AD fields to `emit`, retaining
/// its truncation indicator; parsing never triggers radio operations.
fn packet(
    title: &str,
    payload: &Payload<31>,
    emit: &mut EvidenceLines<'_>,
) {
    packet_bytes(title, payload.as_slice(), payload.truncated, emit);
}
/// Sends `title`, raw `bytes`, `truncated` status and decoded AD fields to
/// `emit`. Bad AD lengths stop decoding after an explicit marker; raw bytes
/// remain visible. Names are sanitized and incomplete UUID chunks are marked.
fn packet_bytes(
    title: &str,
    bytes: &[u8],
    truncated: bool,
    emit: &mut EvidenceLines<'_>,
) {
    emit.formatted(format_args!(
        "{title}: {} bytes{}",
        bytes.len(),
        if truncated { " [TRUNCATED]" } else { "" }
    ));
    if bytes.is_empty() {
        emit.text("No payload received");
        return;
    }
    emit.text("RAW HEX (wire order)");
    hex(bytes, emit);
    for field in AdFields::new(bytes) {
        let Ok(f) = field else {
            emit.text("! MALFORMED advertisement length");
            break;
        };
        match f.kind {
            1 if f.data.len() == 1 => emit.formatted(format_args!("Flags: 0x{:02X}", f.data[0])),
            8 | 9 => {
                emit.text(if f.kind == 9 {
                    "Complete name:"
                } else {
                    "Shortened name:"
                });
                emit.text(&printable::<50>(f.data));
            }
            0x0A if f.data.len() == 1 => emit.formatted(format_args!("TX power: {} dBm", f.data[0] as i8)),
            0xFF if f.data.len() >= 2 => {
                emit.formatted(format_args!(
                    "Manufacturer ID: 0x{:04X}",
                    u16::from_le_bytes([f.data[0], f.data[1]])
                ))
            }
            2..=7 => {
                let width = match f.kind {
                    2 | 3 => 2,
                    4 | 5 => 4,
                    _ => 16,
                };
                emit.text(if f.kind % 2 == 1 {
                    "Service UUIDs (complete list):"
                } else {
                    "Service UUIDs (incomplete list):"
                });
                for chunk in f.data.chunks(width) {
                    if let Some(uuid) = Uuid::from_wire(chunk).filter(|_| chunk.len() == width) {
                        emit.uuid(uuid);
                        let label = uuid_label(uuid);
                        if !label.is_empty() {
                            emit.text(label);
                        }
                    } else {
                        emit.text("! MALFORMED UUID field");
                    }
                }
            }
            _ => {
                emit.formatted(format_args!(
                    "AD type 0x{:02X}: {} bytes (see raw)",
                    f.kind,
                    f.data.len()
                ))
            }
        }
    }
}
/// Sends `title` and optional `observation` timestamp/RSSI to `emit`; `now` is
/// the same boot's snapshot milliseconds. A future observation reports age
/// unavailable, and absent evidence never becomes a zero measurement.
fn packet_observation(
    title: &str,
    observation: Option<PacketObservation>,
    now: u64,
    emit: &mut EvidenceLines<'_>,
) {
    emit.formatted(format_args!("{title} observation:"));
    if let Some(value) = observation {
        emit.formatted(format_args!("Type: {:?} / at {} ms", value.kind, value.at_ms));
        match now.checked_sub(value.at_ms) {
            Some(age) => emit.formatted(format_args!("Age at snapshot: {age} ms")),
            None => emit.text("Age unavailable: clock before observation"),
        }
        match value.rssi {
            Some(v) => emit.formatted(format_args!("Packet RSSI: {v} dBm")),
            None => emit.text("Packet RSSI: unavailable"),
        }
    } else {
        emit.text("Not observed");
    }
}
/// Sends optional observed `link`, four inventory `stages`, acknowledged
/// `cleanup` and optional HCI `disconnect` reason to `emit`. Stage completion
/// is not compatibility; acknowledged cleanup can still have abnormal termination.
fn link_details(
    link: Option<LinkInfo>,
    stages: [bool; 4],
    cleanup: bool,
    disconnect: Option<u8>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text("LINK / OBSERVED PARAMETERS");
    if let Some(link) = link {
        emit.formatted(format_args!("Connected at: {} ms", link.connected_at_ms));
        emit.formatted(format_args!("Connect elapsed: {} ms", link.connect_elapsed_ms));
        emit.formatted(format_args!(
            "Interval: {} us / latency {}",
            link.interval_us, link.latency
        ));
        emit.formatted(format_args!("Supervision timeout: {} us", link.supervision_timeout_us));
        match link.att_mtu {
            Some(mtu) => emit.formatted(format_args!("ATT MTU: {mtu} bytes")),
            None => emit.text("ATT MTU: unavailable"),
        }
    } else {
        emit.text("No connection parameters observed");
    }
    emit.text("INVENTORY STAGES / ended is not compatibility");
    for (name, complete) in ["Services", "Characteristics", "Descriptors", "Standard reads"]
        .into_iter()
        .zip(stages)
    {
        emit.formatted(format_args!(
            "{name}: {}",
            if complete { "ended" } else { "partial / not run" }
        ));
    }
    emit.text(if cleanup {
        "Disconnect cleanup: acknowledged"
    } else {
        "Disconnect cleanup: not acknowledged"
    });
    match disconnect {
        Some(code) => {
            emit.formatted(format_args!("Disconnect reason: 0x{code:02X}"));
            emit.text(crate::ble::termination::reason_label(code));
        }
        None => emit.text("Disconnect reason: unavailable"),
    }
    if cleanup && link.is_some() && !crate::ble::termination::normal_reason(disconnect) {
        emit.text("Termination not clean / inspect reason");
    }
}
/// Sends admitted `record` to `emit`, including handles, UUIDs and exact raw
/// successful read bytes. Invalid standard lengths and truncated values stay
/// explicit; readable text is sanitized and no CCCD/write is performed.
fn record_details(
    record: capture::Record<'_>,
    emit: &mut EvidenceLines<'_>,
) {
    use capture::Record;
    match record {
        Record::Service(s) => {
            emit.formatted(format_args!("SERVICE {:04X}-{:04X}", s.start, s.end));
            emit.uuid(s.uuid);
            let label = uuid_label(s.uuid);
            if !label.is_empty() {
                emit.text(label);
            }
        }
        Record::Characteristic(c) => {
            emit.formatted(format_args!("CHAR {:04X} / service {:04X}", c.handle, c.service_start));
            emit.uuid(c.uuid);
            let label = uuid_label(c.uuid);
            if !label.is_empty() {
                emit.text(label);
            }
            emit.formatted(format_args!(
                "Decl {:04X} / properties 0x{:02X}",
                c.declaration, c.properties
            ));
            emit.text(&properties_text(c.properties));
        }
        Record::Descriptor(d) => {
            emit.formatted(format_args!(
                "DESCRIPTOR {:04X} / characteristic {:04X}",
                d.handle, d.characteristic_handle
            ));
            emit.formatted(format_args!("Service {:04X} / UUID:", d.service_start));
            emit.uuid(d.uuid);
            if d.uuid.short() == Some(0x2902) {
                emit.text("Client configuration / not written");
            }
        }
        Record::Value(v) => {
            emit.formatted(format_args!(
                "STANDARD READ / {} ({:04X})",
                info_label(v.uuid).unwrap_or("Value"),
                v.uuid
            ));
            emit.formatted(format_args!(
                "Read service {:04X} / handle {:04X}",
                v.service_start, v.handle
            ));
            emit.formatted(format_args!("Outcome: {:?} / {} ms", v.result, v.elapsed_ms));
            if let Some(error) = v.failure {
                reason_line("Read error", error, emit);
            }
            if v.result == ReadResult::Ok {
                if !v.length_valid() {
                    emit.text("! Unexpected standard length; raw retained");
                }
                for chunk in v.bytes.chunks(48) {
                    emit.text(&printable::<50>(chunk));
                }
                emit.text("Value RAW HEX:");
                hex(v.bytes, emit);
            }
            if v.truncated {
                emit.text("! VALUE TRUNCATED");
            }
        }
    }
}
/// Sends `title` and stable `reason` label plus any numeric code to `emit`.
fn reason_line(
    title: &str,
    reason: FailureReason,
    emit: &mut EvidenceLines<'_>,
) {
    if let Some(code) = reason.code() {
        emit.formatted(format_args!("{title}: {} 0x{code:02X}", reason.label()));
    } else {
        emit.formatted(format_args!("{title}: {}", reason.label()));
    }
}
/// Emits shared fields of borrowed `failure` in wire-evidence order through `emit`.
/// Callers retain their distinct live/saved headings and connection-context policy.
fn failure_details(
    failure: &InspectionFailure,
    emit: &mut EvidenceLines<'_>,
) {
    reason_line("Reason", failure.reason, emit);
    if let Some(elapsed) = failure.elapsed_ms {
        emit.formatted(format_args!("Stage elapsed: {elapsed} ms"));
    }
    if let Some((start, end)) = failure.range {
        emit.formatted(format_args!("Query range {start:04X}-{end:04X}"));
    }
    if let Some((decl, value)) = failure.last_accepted {
        emit.formatted(format_args!("Last accepted decl {decl:04X} / value {value:04X}"));
    }
    rejected_details(failure.rejected, emit);
}
/// Emits global `scan` counts and optional last-error fields through `emit`.
/// `now` is live boot-relative milliseconds (age may be unavailable); None retains
/// the saved observation timestamp. Only live rows include the batch-stop explanation.
fn scan_details(
    scan: &ScanDiagnostics,
    now: Option<u64>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.formatted(format_args!("Bad batches {} / size {}", scan.total, scan.invalid_size));
    emit.formatted(format_args!("Invalid value {}", scan.invalid_value));
    if let Some(last) = scan.last.as_ref() {
        match now {
            Some(now) => {
                if let Some(age_ms) = now.checked_sub(last.at_ms) {
                    emit.formatted(format_args!("Last {:?} / {}s ago", last.kind, age_ms / 1000));
                } else {
                    emit.formatted(format_args!("Last {:?} / age unavailable", last.kind));
                }
            }
            None => emit.formatted(format_args!("Last {:?} / at {} ms", last.kind, last.at_ms)),
        }
        emit.formatted(format_args!(
            "Scan revision {} / entry {}/{}",
            last.revision,
            u16::from(last.index) + 1,
            last.total_reports
        ));
        if now.is_some() {
            emit.text("Parsing stops at the first bad entry in a batch");
        }
        emit.formatted(format_args!(
            "Report bytes {} / remaining {}",
            last.report_bytes, last.remaining_bytes
        ));
        match last.declared_data_len {
            Some(length) => emit.formatted(format_args!("Declared data length: {length}")),
            None => emit.text("Declared data length: missing header byte"),
        }
        emit.text("One record needs 10 + data length bytes");
    }
}
/// Sends selected-peer evidence from `probe` to `emit`, computing advertisement
/// ages at boot-relative milliseconds `now`. GATT rows require an explicit
/// matching inspect request; they remain retained when advertisements age.
fn peer_details(
    probe: &Probe,
    now: u64,
    emit: &mut EvidenceLines<'_>,
) {
    let Some(d) = probe.selected_device() else {
        emit.text("No selected device");
        return;
    };
    emit.text("PEER / ADVERTISEMENT EVIDENCE");
    name_details(names::device_name(d), emit);
    emit.formatted(format_args!(
        "{} / {}",
        d.peer.to_string().as_str(),
        d.peer.kind_label()
    ));
    emit.formatted(format_args!(
        "Seen {}s ago / {} reports",
        d.age_ms(now) / 1000,
        d.reports
    ));
    emit.text(&match d.rssi {
        Some(rssi) => line(format_args!("RSSI: {rssi} dBm (not distance)")),
        None => line(format_args!("RSSI unavailable")),
    });
    packet_observation("Advertisement", d.advertisement_meta, now, emit);
    packet("ADVERTISEMENT", &d.advertisement, emit);
    packet_observation("Scan response", d.response_meta, now, emit);
    packet("SCAN RESPONSE", &d.response, emit);
    if !gatt_requested(probe) {
        emit.text("GATT not inspected");
        emit.text("Advertisement only / no connection requested");
        emit.text("When ready: Click/X connects + inspects GATT");
        return;
    }
    let i = &probe.inspection;
    emit.text("GATT / EXPLICIT INSPECTION");
    emit.text("Advert age does not invalidate these records");
    emit.text(if i.complete && i.omitted == 0 && i.malformed == 0 {
        "Enumeration ended"
    } else {
        match probe.phase {
            RadioPhase::Stopping => "Waiting for scan / connection cleanup",
            RadioPhase::Connecting => "Connecting to selected peer",
            RadioPhase::Discovering => "GATT inspection in progress",
            RadioPhase::Disconnecting => "Disconnecting / partial results",
            _ => "Partial / not completed",
        }
    });
    emit.formatted(format_args!("Omitted {} / rejected records {}", i.omitted, i.malformed));
    emit.formatted(format_args!(
        "{} primary services / {} characteristics",
        i.services.len(),
        i.characteristics.len()
    ));
    emit.formatted(format_args!(
        "{} descriptors / {} standard reads",
        i.descriptors.len(),
        i.values.len()
    ));
    link_details(i.link, i.stages, i.cleanup_complete, i.disconnect_reason, emit);
    for s in &i.services {
        record_details(capture::Record::Service(*s), emit);
    }
    for c in &i.characteristics {
        record_details(capture::Record::Characteristic(*c), emit);
    }
    for d in &i.descriptors {
        record_details(capture::Record::Descriptor(*d), emit);
    }
    for v in &i.values {
        record_details(
            capture::Record::Value(capture::ValueRef {
                service_start: v.service_start,
                handle: v.handle,
                uuid: v.uuid,
                result: v.result,
                failure: v.failure,
                elapsed_ms: v.elapsed_ms,
                bytes: v.value.as_slice(),
                truncated: v.value.truncated,
            }),
            emit,
        );
    }
}
/// Traverses current `probe` evidence into `emit` using snapshot milliseconds
/// `now` for ages. Preserves peer failures separately from global scan/fault/boot
/// evidence and makes no radio or storage requests.
#[inline(never)]
fn details(
    probe: &Probe,
    now: u64,
    emit: &mut EvidenceLines<'_>,
) {
    if probe.identity_mode && gatt_requested(probe) {
        identity_view::details(probe.inspection.identity.as_ref(), emit);
    }
    if let Some(failure) = probe.failure.as_ref().filter(|_| gatt_requested(probe)) {
        emit.formatted(format_args!("GATT FAILURE / {}", failure.stage.label()));
        failure_details(failure, emit);
        if let Some(error) = probe.failure_context.local_error {
            if let Some(code) = error.code() {
                emit.formatted(format_args!("Host ingress: {} 0x{code:02X}", error.label()));
            } else {
                emit.formatted(format_args!("Host ingress: {}", error.label()));
            }
        }
        match probe.failure_context.disconnect_reason {
            Some(code) => emit.formatted(format_args!("HCI disconnect: 0x{code:02X}")),
            None => emit.text("HCI disconnect: unavailable"),
        }
        emit.text("Partial records retained; Y returns to scan");
    }
    peer_details(probe, now, emit);
    fault_details(Some(probe.fault_latch), emit);
    boot_details(probe.boot, emit);
    radio_clock_details(probe.radio_bus_hz, emit);
    if probe.scan_diagnostics.last.is_some() {
        emit.text("GLOBAL HCI SCAN ERRORS / since boot");
        emit.text("Not attributed to the selected device");
        scan_details(&probe.scan_diagnostics, Some(now), emit);
    }
}
/// Sends sanitized chunks of optional `name` and its exact provenance to
/// `emit`; cached/shortened/truncated labels remain untrusted device input.
fn name_details(
    name: Option<names::NameRef<'_>>,
    emit: &mut EvidenceLines<'_>,
) {
    let Some(name) = name else {
        emit.text("Name unavailable in observed evidence");
        emit.text("Explicit Inspect may read a GAP Device Name");
        return;
    };
    for chunk in name.bytes.chunks(50) {
        emit.text(&printable::<50>(chunk));
    }
    emit.formatted(format_args!(
        "Name source: {} / {}",
        name.source.label(),
        if name.cached { "cached" } else { "packet/read" }
    ));
    emit.text(if name.complete && !name.truncated {
        "Name: complete (untrusted device label)"
    } else {
        "Name: shortened/truncated (untrusted label)"
    });
    if let Some(at) = name.observed_at_ms {
        emit.formatted(format_args!("Name observed at: {at} ms"));
    }
}
/// Sends optional `boot` reset metadata to `emit`, keeping retained stage/time
/// distinct from a proven cause and distinguishing absent evidence.
fn boot_details(
    boot: Option<crate::health::reset::BootEvidence>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text("BOOT RESET / hardware and retained evidence");
    let Some(boot) = boot else {
        emit.text("Boot evidence not recorded");
        return;
    };
    emit.formatted(format_args!("Reset: {} / raw 0x{:08X}", boot.label(), boot.raw_reason));
    if let Some(kind) = boot.boot_type {
        emit.formatted(format_args!("ROM boot type: 0x{kind:02X}"));
    } else {
        emit.text("ROM boot type: unavailable");
    }
    if let Some(previous) = boot.previous {
        emit.formatted(format_args!("Previous stage: {}", previous.stage.label()));
        emit.formatted(format_args!("Previous boot observation: {} ms", previous.elapsed_ms));
        emit.text("Last observation; not proven reset cause");
        emit.text("Previous image unknown; time may saturate");
    } else {
        emit.text("Previous watchdog stage: unavailable");
    }
    emit.text("Power removal may erase retained evidence");
}
/// Sends optional `latch` membership and first-recorded source to `emit`;
/// `None` means legacy capture evidence was not recorded, not an empty latch.
fn fault_details(
    latch: Option<FaultLatch>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text("LED FAULTS / global since recovery");
    let Some(latch) = latch else {
        emit.text("Not recorded by this legacy capture");
        return;
    };
    emit.formatted(format_args!("Latched mask: 0x{:03X}", latch.bits()));
    if let Some(first) = latch.first() {
        emit.formatted(format_args!("First recorded: {}", first.label()));
        emit.text("Recording order; not a proven root cause");
        for fault in Fault::ALL {
            if latch.contains(fault) {
                emit.formatted(format_args!("Latched: {}", fault.label()));
            }
        }
        emit.text("Live latch clears only with Hold Y 2s");
    } else {
        emit.text("No faults recorded since recovery");
    }
}
/// Sends configured bus `hz` to `emit`, or an unavailable marker for `None`;
/// this is retained configuration metadata, not a measured frequency.
fn radio_clock_details(
    hz: Option<u32>,
    emit: &mut EvidenceLines<'_>,
) {
    match hz {
        Some(hz) => emit.formatted(format_args!("Radio GSPI: {hz} Hz (configured)")),
        None => emit.text("Radio GSPI: not recorded"),
    }
}
/// Sends optional `rejected` declaration to `emit`, labeling any decoded value
/// handle unvalidated and distinguishing its 19-byte prefix from complete raw data.
fn rejected_details(
    rejected: Option<RejectedDeclaration>,
    emit: &mut EvidenceLines<'_>,
) {
    if let Some(r) = rejected {
        emit.formatted(format_args!("Rejected decl {:04X} / {} bytes", r.handle, r.length()));
        match r.value_handle() {
            Some(v) => emit.formatted(format_args!("Rejected value field: {v:04X} (unvalidated)")),
            None => emit.text("Rejected value field: missing"),
        }
        emit.text(if usize::from(r.length()) > r.bytes().len() {
            "Rejected RAW HEX prefix (first 19 bytes)"
        } else {
            "Rejected RAW HEX (wire order)"
        });
        hex(r.bytes(), emit);
    }
}
/// Returns at least one ten-row detail page for `probe`, independent of current
/// screen/page. Counts without formatting; age text changes do not change rows.
pub fn page_count(probe: &Probe) -> usize {
    let mut lines = EvidenceLines::new(None);
    details(probe, 0, &mut lines);
    lines.count.div_ceil(LINES).max(1)
}
/// Traverses validated saved `view` into `emit` with frozen boot-relative
/// timestamps, source identity and raw record bytes. Current live state never
/// replaces capture evidence; no radio/storage operation is initiated.
#[inline(never)]
fn saved_details(
    view: &capture::CaptureView<'_>,
    emit: &mut EvidenceLines<'_>,
) {
    let m = view.metadata();
    emit.text("SAVED EVIDENCE / frozen boot-relative timestamps");
    name_details(names::capture_name(view), emit);
    emit.formatted(format_args!(
        "{} / {}",
        m.peer.to_string().as_str(),
        m.peer.kind_label()
    ));
    emit.formatted(format_args!("Captured at: {} ms", m.captured_at_ms));
    emit.formatted(format_args!("Request revision: {}", m.revision));
    emit.formatted(format_args!("Radio {:?} / {:?}", m.phase, m.error));
    if m.identity_mode == Some(true) {
        identity_view::details(m.identity.as_ref(), emit);
    }
    fault_details(m.fault_latch, emit);
    boot_details(m.boot, emit);
    radio_clock_details(m.radio_bus_hz, emit);
    emit.formatted(format_args!("First seen: {} ms", m.first_seen_ms));
    emit.formatted(format_args!("Last seen: {} ms", m.last_seen_ms));
    emit.formatted(format_args!("Reports: {}", m.reports));
    match m.rssi {
        Some(v) => emit.formatted(format_args!("Latest RSSI: {v} dBm (not distance)")),
        None => emit.text("Latest RSSI: unavailable"),
    }
    packet_observation("Advertisement", m.advertisement.observation, m.captured_at_ms, emit);
    packet_bytes(
        "ADVERTISEMENT",
        m.advertisement.bytes(),
        m.advertisement.truncated,
        emit,
    );
    packet_observation("Scan response", m.response.observation, m.captured_at_ms, emit);
    packet_bytes("SCAN RESPONSE", m.response.bytes(), m.response.truncated, emit);
    if m.gatt_requested {
        emit.text("GATT / explicit inspection evidence");
        emit.text(if m.complete && m.omitted == 0 && m.malformed == 0 {
            "Enumeration ended"
        } else {
            "Partial / not completed"
        });
        let [services, characteristics, descriptors, values] = view.counts();
        emit.formatted(format_args!("{services} services / {characteristics} characteristics"));
        emit.formatted(format_args!("{descriptors} descriptors / {values} standard reads"));
        emit.formatted(format_args!("Omitted {} / rejected {}", m.omitted, m.malformed));
        if let Some(at) = m.started_at_ms {
            emit.formatted(format_args!("Inspection start: {at} ms"));
        }
        if let Some(at) = m.ended_at_ms {
            emit.formatted(format_args!("Inspection end: {at} ms"));
        }
        link_details(m.link, m.stages, m.cleanup_complete, m.disconnect_reason, emit);
        if let Some(failure) = m.failure.as_ref() {
            emit.formatted(format_args!("FAILURE / {}", failure.stage.label()));
            failure_details(failure, emit);
        }
        if let Some(reason) = m.failure_context.local_error {
            reason_line("Host ingress", reason, emit);
        }
        if let Some(code) = m.failure_context.disconnect_reason {
            emit.formatted(format_args!("HCI disconnect: 0x{code:02X}"));
        }
        for record in view.records() {
            record_details(record, emit);
        }
    } else {
        emit.text("GATT not inspected / no connection requested");
    }
    emit.text("GLOBAL SCAN EVIDENCE / not peer-attributed");
    emit.formatted(format_args!(
        "Reports dropped {} / malformed {}",
        m.dropped_reports, m.malformed_reports
    ));
    emit.formatted(format_args!("Catalogue full reports {}", m.catalog_overflow_reports));
    scan_details(&m.scan_diagnostics, None, emit);
    emit.text("CAPTURE BUILD / configured clocks, not measured");
    emit.formatted(format_args!("Profile {:?}", m.build.profile));
    emit.formatted(format_args!("CPU {} Hz / SPI {} Hz", m.build.cpu_hz, m.build.spi_hz));
    emit.text(if m.build.source_identity_available {
        "Source digest (raw byte order):"
    } else {
        "Source identity unavailable; digest bytes:"
    });
    hex(&m.build.source_digest, emit);
    emit.text(if m.identity_mode == Some(true) {
        "INFO only; no pairing, ECU requests or polling"
    } else {
        "No pairing, arbitrary reads, writes or ECU data"
    });
    emit.text("Enumeration does not prove adapter compatibility");
    emit.text("USB serial export retains the exact capture bytes");
}
/// Returns at least one ten-row page for validated `view` without reconstructing
/// a Probe or formatting dynamic row values.
pub fn saved_page_count(view: &capture::CaptureView<'_>) -> usize {
    let mut lines = EvidenceLines::new(None);
    saved_details(view, &mut lines);
    lines.count.div_ceil(LINES).max(1)
}
/// Clears fixed 320x240 `d` and paints `title` with current-Pico `diagnostics`.
/// Returns `Ok(())` or the first target error, leaving prior writes intact.
fn capture_header<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    title: &str,
    diagnostics: ProbeDiagnostics,
) -> Result<(), D::Error> {
    {
        let mut clipped = d.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
        rect(&mut clipped, 0, 0, 320, 240, BG)?;
        font_text(&mut clipped, Point::new(10, 5), title, WHITE, &FONT_9X15)?;
    }
    telemetry(d, diagnostics)
}
/// Borrows the validated capture; never reconstructs or copies a complete Probe.
/// Paints `view` into the 320x240 clip of `target`. Zero-based `slot` is labelled
/// 1..4 or unknown; `page` clamps to the final evidence page. `diagnostics` are
/// current-Pico header values, distinct from the frozen body. Returns `Ok(())`
/// or the first drawing error after partial writes; capture/selection stay unchanged.
pub fn render_saved<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    view: &capture::CaptureView<'_>,
    slot: u8,
    page: usize,
    diagnostics: ProbeDiagnostics,
) -> Result<(), D::Error> {
    capture_header(target, "SAVED BLE CAPTURE", diagnostics)?;
    // Use the same screen target as live rendering so text/shape code is shared.
    let mut clipped = target.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
    let d = &mut clipped;
    let pages = saved_page_count(view);
    let page = page.min(pages - 1);
    let slot_label = if slot < 4 {
        line(format_args!("Slot {}", slot + 1))
    } else {
        line(format_args!("Slot --"))
    };
    text(
        d,
        10,
        61,
        &line(format_args!(
            "{} / read only / {}/{}",
            slot_label.as_str(),
            page + 1,
            pages
        )),
        RED,
    )?;
    let mut index = 0;
    let mut error = None;
    saved_details(
        view,
        &mut EvidenceLines::new(Some(&mut |s| {
            if (page * LINES..(page + 1) * LINES).contains(&index)
                && error.is_none()
                && let Err(e) = text(d, 10, 88 + (index % LINES) as i32 * 11, s, WHITE)
            {
                error = Some(e);
            }
            index += 1;
        })),
    );
    if let Some(error) = error {
        return Err(error);
    }
    rect(d, 0, 204, 320, 36, PANEL)?;
    text(d, 8, 206, "Turn/A/B: pages  X/Y: back to slots", WHITE)?;
    text(d, 8, 217, "Header: current Pico / body: saved evidence", MUTED)?;
    text(d, 8, 228, "Hold Y 2s: recover scan; saves retained", MUTED)
}
/// Returns the directory-state label for `slot`, preserving unknown, damaged
/// and replacement-blocked states rather than treating them as empty.
fn slot_label(slot: capture_ui::Slot) -> &'static str {
    use capture_ui::Slot;
    match slot {
        Slot::Unknown => "Not loaded",
        Slot::Empty => "Empty",
        Slot::Saved { .. } => "Saved capture",
        Slot::SavedBlocked { .. } => "Saved / replacement blocked",
        Slot::Corrupt => "Damaged capture",
        Slot::Unsupported => "Unsupported format",
        Slot::Unavailable => "Storage unavailable",
    }
}
/// Returns recovery guidance for `error`; `pending` means the storage owner still
/// holds a request. Released timeouts imply neither successful I/O nor zero effects.
fn ui_error(
    error: capture_ui::UiError,
    pending: bool,
) -> &'static str {
    use capture_ui::UiError;
    match error {
        UiError::NoPeer => "Select a peer before saving",
        UiError::Busy => "Wait for the active operation to finish",
        UiError::Empty => "Slot is empty; X saves current evidence",
        UiError::Storage => "Storage failed; refresh slots before retry",
        UiError::Corrupt => "Damaged slot; X offers explicit erase",
        UiError::Unsupported => "Unknown format; export before erasing",
        UiError::Unavailable => "Storage unavailable; hold X to refresh",
        UiError::Timeout if pending => "Timed out; owner must finish before retry",
        UiError::Timeout => "Timed out; Y back to review slots",
        UiError::ConfirmationExpired => "Confirmation expired; no change made",
        UiError::Changed => "Evidence changed; review then confirm again",
        UiError::Clock => "Clock changed; retry the gesture",
        UiError::Exhausted => "Request limit reached; restart required",
    }
}
/// Paints four-slot `ui` state and current-Pico `diagnostics` into the 320x240
/// clip of `target`. Confirmation/pending status is presentation only; no slot
/// is loaded, saved or erased. Returns `Ok(())` or the first draw error with
/// prior pixels retained; `CaptureUi` guarantees its selected index is in range.
pub fn render_slots<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    ui: &capture_ui::CaptureUi,
    diagnostics: ProbeDiagnostics,
) -> Result<(), D::Error> {
    use capture_ui::{Operation, Screen as CaptureScreen};
    capture_header(target, "CAPTURE SLOTS", diagnostics)?;
    let mut clipped = target.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
    let d = &mut clipped;
    let screen = ui.screen();
    let status = match screen {
        CaptureScreen::ConfirmOverwrite => "Replace saved capture?",
        CaptureScreen::ConfirmReset => "Erase this slot and ALL its saved data?",
        CaptureScreen::Busy(Operation::Refresh) => "Loading slot directory...",
        CaptureScreen::Busy(Operation::Load) => "Reading selected capture...",
        CaptureScreen::Busy(Operation::Save) => "Saving frozen evidence; please wait...",
        CaptureScreen::Busy(Operation::Reset) => "Erasing selected slot; please wait...",
        CaptureScreen::Error(error) => ui_error(error, ui.pending().is_some()),
        _ if ui.pending().is_some() => "Operation still running; please wait",
        _ if matches!(
            ui.slots()[usize::from(ui.selected())],
            capture_ui::Slot::SavedBlocked { .. }
        ) =>
        {
            "Readable save; export before erasing this slot"
        }
        _ => "Four saves survive USB power off",
    };
    text(
        d,
        10,
        61,
        status,
        if screen == CaptureScreen::Error(capture_ui::UiError::ConfirmationExpired) {
            MUTED
        } else {
            RED
        },
    )?;
    for (index, &slot) in ui.slots().iter().enumerate() {
        let y = 78 + index as i32 * 30;
        rect(
            d,
            6,
            y,
            308,
            29,
            if usize::from(ui.selected()) == index { PANEL } else { BG },
        )?;
        if usize::from(ui.selected()) == index {
            rect(d, 6, y, 3, 29, RED)?;
        }
        let identity = ui.identities()[index];
        let title = if let Some(identity) = identity {
            let name = printable::<31>(identity.name.as_slice());
            line(format_args!(
                "{} {} | {}",
                index + 1,
                if matches!(slot, capture_ui::Slot::SavedBlocked { .. }) {
                    "Protected"
                } else {
                    "Saved"
                },
                if name.is_empty() { "Unnamed" } else { name.as_str() }
            ))
        } else {
            line(format_args!("Slot {} / {}", index + 1, slot_label(slot)))
        };
        text(d, 14, y + 1, &title, WHITE)?;
        let detail =
            if let capture_ui::Slot::Saved { generation } | capture_ui::Slot::SavedBlocked { generation } = slot {
                if let Some(identity) = identity {
                    line(format_args!(
                        "{} {} / gen {generation}",
                        if identity.peer.random { "R" } else { "P" },
                        identity.peer.to_string().as_str()
                    ))
                } else {
                    line(format_args!("Generation {generation}"))
                }
            } else {
                line(format_args!(
                    "{}",
                    if matches!(slot, capture_ui::Slot::Corrupt | capture_ui::Slot::Unsupported) {
                        "Export first; never erased automatically"
                    } else {
                        "USB serial provides read-only recovery"
                    }
                ))
            };
        text(d, 14, y + 14, &detail, MUTED)?;
    }
    rect(d, 0, 204, 320, 36, PANEL)?;
    let confirm = matches!(screen, CaptureScreen::ConfirmOverwrite | CaptureScreen::ConfirmReset);
    text(
        d,
        8,
        206,
        if confirm {
            "Click/Hold A 2s: confirm  Y: cancel (10s)"
        } else if ui.pending().is_some() {
            "Y: leave view; storage finishes safely"
        } else if matches!(screen, CaptureScreen::Error(_)) {
            "Y: back to slots before retrying"
        } else {
            "A/B: slot Click/Hold A 2s: open X: save/reset"
        },
        WHITE,
    )?;
    text(d, 8, 217, "Y: back  Hold X 2s: refresh slot directory", MUTED)?;
    text(d, 8, 228, "Hold Y 2s: recover scan; saves retained", MUTED)
}
/// Draws the same RGB565 scene for host screenshots and the hardware framebuffer.
/// Paints immutable `probe` into the 320x240 clip of `target`, using monotonic
/// boot-relative `now_ms` for the 200ms spinner and evidence ages. Selection and
/// page are defensively clamped. Returns `Ok(())` or the first drawing error
/// after partial writes; rendering never advances state or initiates radio I/O.
pub fn render<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    probe: &Probe,
    now_ms: u64,
) -> Result<(), D::Error> {
    {
        let mut clipped = target.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
        let d = &mut clipped;
        d.clear(BG)?;
        rect(d, 0, 0, 320, 3, RED)?;
        font_text(
            d,
            Point::new(10, 7),
            if probe.identity_mode { "NGM INFO" } else { "BLE PROBE" },
            WHITE,
            &FONT_9X15,
        )?;
        let active = !matches!(probe.phase, RadioPhase::Paused | RadioPhase::Fault);
        let spin = if active {
            ["|", "/", "-", "\\"][((now_ms / 200) % 4) as usize]
        } else {
            " "
        };
        text(d, 112, 10, &line(format_args!("{spin} {:?}", probe.phase)), WHITE)?;
    }
    telemetry(target, probe.diagnostics)?;
    let mut clipped = target.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
    let d = &mut clipped;
    text(
        d,
        10,
        49,
        &if probe.screen == Screen::Details
            && let Some(peer) = probe.selected_device()
        {
            line(format_args!(
                "{} / {}",
                peer.peer.to_string().as_str(),
                peer.peer.kind_label()
            ))
        } else {
            line(format_args!(
                "{}/{} peers lost:{} full:{}",
                if probe.catalog.devices().is_empty() {
                    0
                } else {
                    probe.selected.min(probe.catalog.devices().len() - 1) + 1
                },
                probe.catalog.devices().len(),
                probe.dropped_reports,
                probe.catalog.overflow_reports
            ))
        },
        MUTED,
    )?;
    text(
        d,
        10,
        61,
        &line(format_args!(
            "{} HCIbad:{}",
            status_copy(probe),
            probe.malformed_reports
        )),
        if probe.error == ProbeError::None && probe.fault_latch.category() == ErrorLatch::None {
            MUTED
        } else {
            RED
        },
    )?;
    if probe.screen == Screen::List {
        let devices = probe.catalog.devices();
        let selected = probe.selected.min(devices.len().saturating_sub(1));
        let start = selected / 4 * 4;
        if devices.is_empty() {
            let (message, action) = empty_copy(probe.phase);
            text(d, 14, 94, message, WHITE)?;
            text(d, 14, 112, action, MUTED)?;
        }
        for (row, device) in devices.iter().enumerate().skip(start).take(4) {
            let y = 76 + (row - start) as i32 * 31;
            let focus = row == selected;
            rect(d, 6, y, 308, 30, if focus { PANEL } else { BG })?;
            if focus {
                rect(d, 6, y, 3, 30, RED)?;
            }
            let name = device.name();
            let label = if name.is_empty() { "Unnamed device" } else { &name };
            font_text(d, Point::new(14, y + 1), label, WHITE, &FONT_7X13)?;
            text(
                d,
                14,
                y + 16,
                &line(format_args!(
                    "{} {} {}s",
                    device.peer.to_string().as_str(),
                    device
                        .rssi
                        .map(|r| line(format_args!("{r}dBm")))
                        .unwrap_or_else(|| line(format_args!("RSSI?")))
                        .as_str(),
                    device.age_ms(now_ms) / 1000
                )),
                MUTED,
            )?;
        }
    } else {
        let pages = page_count(probe);
        let page = probe.page.min(pages - 1);
        text(
            d,
            10,
            75,
            &line(format_args!(
                "{}  {}/{}",
                if probe.selected_device().is_none() {
                    "SCAN DIAGNOSTICS"
                } else if gatt_requested(probe) && probe.identity_mode {
                    "GATT + ADAPTER IDENTITY"
                } else if gatt_requested(probe) {
                    "ADVERTISEMENT + GATT"
                } else {
                    "ADVERTISEMENT"
                },
                page + 1,
                pages
            )),
            RED,
        )?;
        let mut index = 0;
        let mut error = None;
        details(
            probe,
            now_ms,
            &mut EvidenceLines::new(Some(&mut |s| {
                if (page * LINES..(page + 1) * LINES).contains(&index)
                    && error.is_none()
                    && let Err(e) = text(d, 10, 88 + (index % LINES) as i32 * 11, s, WHITE)
                {
                    error = Some(e);
                }
                index += 1;
            })),
        );
        if let Some(e) = error {
            return Err(e);
        }
    }
    rect(d, 0, 204, 320, 36, PANEL)?;
    let hints = control_hints(probe, now_ms);
    text(d, 8, 206, hints[0], WHITE)?;
    text(d, 8, 217, hints[1], MUTED)?;
    text(
        d,
        8,
        228,
        if probe.encoder_connected {
            "Hold X 2s: slots | Hold Y 2s: recover"
        } else {
            "Hold X/Y 2s: slots/recover | NO ENCODER"
        },
        if probe.encoder_connected { MUTED } else { RED },
    )
}

#[cfg(test)]
/// Pixel parity, evidence preservation, pagination and truthful recovery-copy regressions.
mod tests {
    use super::*;
    extern crate std;
    /// INFO image exposes candidate provenance and partial raw evidence without
    /// rendering standard-only success or changing its manual request.
    #[test]
    fn identity_details_preserve_configured_candidate_and_partial_failure() {
        let mut p = probe();
        p.identity_mode = true;
        p.input(Input::Click, 0).unwrap();
        p.phase = RadioPhase::Paused;
        let mut evidence = identity::Evidence::new(0);
        evidence.fault = Some(crate::ble::canyonero::Fault {
            stage: crate::ble::canyonero::Stage::OpeningCoc,
            reason: crate::ble::canyonero::Reason::Transport(crate::ble::canyonero::TransportFault {
                domain: crate::ble::canyonero::TransportDomain::L2cap,
                code: Some(2),
                partial_send: false,
            }),
            at_ms: 1,
        });
        p.inspection.identity = Some(evidence);
        let rows = lines(&p);
        assert!(rows.iter().any(|r| r == "Candidate SPSM 129 / source configured"));
        assert!(rows.iter().any(|r| r == "Discovered SPSM: unavailable"));
        assert!(rows.iter().any(|r| r == "Transport domain: L2CAP"));
        assert!(rows.iter().any(|r| r == "Transport code: 0x0002"));
        assert!(control_hints(&p, 0)[0].contains("INFO"));
        assert_eq!(p.request.revision, 2);
    }
    /// Checks all printable ASCII glyphs in three fonts against Text's reference rasterizer.
    #[test]
    fn single_line_glyph_rendering_matches_text_layout_for_all_printable_ascii() {
        use embedded_graphics::mock_display::MockDisplay;
        use embedded_graphics::text::Text;
        for font in [&FONT_6X10, &FONT_7X13, &FONT_9X15] {
            for byte in 32..=126 {
                let mut value = String::<1>::new();
                value.push(char::from(byte)).unwrap();
                let mut expected = MockDisplay::new();
                let mut actual = MockDisplay::new();
                Text::with_baseline(&value, Point::new(4, 7), MonoTextStyle::new(font, WHITE), Baseline::Top)
                    .draw(&mut expected)
                    .unwrap();
                font_text(&mut actual, Point::new(4, 7), &value, WHITE, font).unwrap();
                assert_eq!(actual, expected, "ASCII {byte}");
            }
        }
    }
    /// Counting and drawing visit identical rows, but counting must not invoke Display.
    #[test]
    fn page_count_does_not_format_values_but_draw_emits_each_value() {
        /// Formatter side-effect witness for the no-format count traversal.
        struct Observed<'a>(
            /// Number of Display invocations observed by the test.
            &'a core::cell::Cell<usize>,
        );
        impl core::fmt::Display for Observed<'_> {
            /// Increments the witness before writing to `f`. Returns the sink's
            /// formatting result; an error does not undo the observed call.
            fn fmt(
                &self,
                f: &mut core::fmt::Formatter<'_>,
            ) -> core::fmt::Result {
                self.0.set(self.0.get() + 1);
                f.write_str("observed")
            }
        }
        let calls = core::cell::Cell::new(0);
        let mut count = EvidenceLines::new(None);
        count.text("heading");
        count.formatted(format_args!("{}", Observed(&calls)));
        assert_eq!(count.count, 2);
        assert_eq!(calls.get(), 0);

        let mut rows = std::vec::Vec::<std::string::String>::new();
        let mut collect = |line: &str| rows.push(line.into());
        let mut draw = EvidenceLines::new(Some(&mut collect));
        draw.text("heading");
        draw.formatted(format_args!("{}", Observed(&calls)));
        assert_eq!(draw.count, count.count);
        assert_eq!(calls.get(), 1);
        assert_eq!(rows, ["heading", "observed"]);
    }
    /// Covers all byte values, partial rows and malformed UUID lengths against independent hex formatting.
    #[test]
    fn raw_hex_and_uuid_rows_preserve_all_byte_values_and_wire_order() {
        let bytes: std::vec::Vec<u8> = (0..=255).collect();
        for length in [0, 1, 15, 16, 17, 31, 32, 255, 256] {
            let mut rows = std::vec::Vec::<std::string::String>::new();
            hex(
                &bytes[..length],
                &mut EvidenceLines::new(Some(&mut |line| rows.push(line.into()))),
            );
            let expected: std::vec::Vec<_> = bytes[..length]
                .chunks(16)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|byte| std::format!("{byte:02X}"))
                        .collect::<std::vec::Vec<_>>()
                        .join(" ")
                })
                .collect();
            assert_eq!(rows, expected);
            let mut count = EvidenceLines::new(None);
            hex(&bytes[..length], &mut count);
            assert_eq!(count.count, rows.len());
        }
        for len in [0, 2, 4, 16, 255] {
            let uuid = Uuid {
                bytes: core::array::from_fn(|i| i as u8 * 17),
                len,
            };
            let expected: std::string::String = uuid.bytes[..usize::from(len).min(16)]
                .iter()
                .rev()
                .map(|byte| std::format!("{byte:02X}"))
                .collect();
            let mut rows = std::vec::Vec::<std::string::String>::new();
            EvidenceLines::new(Some(&mut |line| rows.push(line.into()))).uuid(uuid);
            assert_eq!(rows, [expected]);
        }
    }
    /// Checks rounding, invalid/nonfinite markers, memory bounds and worst-case telemetry lane widths.
    #[test]
    fn telemetry_values_are_bounded_and_invalid_is_not_zero() {
        for v in [None, Some(f32::NAN), Some(f32::INFINITY), Some(-1.0)] {
            assert_eq!(rate(v).as_str(), "--");
        }
        assert_eq!(rate(Some(19.875)).as_str(), "19.9");
        assert_eq!(rate(Some(0.0)).as_str(), "0.0");
        assert_eq!(rate(Some(19.94)).as_str(), "19.9");
        assert_eq!(rate(Some(19.96)).as_str(), "20.0");
        assert_eq!(rate(Some(999.99)).as_str(), "1000.0");
        assert_eq!(rate(Some(f32::MAX)).as_str(), ">999");
        assert_eq!(work(Some(1000)).as_str(), "100.0%");
        assert_eq!(work(Some(1001)).as_str(), "--%");
        assert_eq!(
            memory(Some(MemoryUse {
                used: 4097,
                total: 32768
            }))
            .as_str(),
            "5/32K"
        );
        for v in [
            MemoryUse { used: 1, total: 0 },
            MemoryUse {
                used: 32769,
                total: 32768,
            },
            MemoryUse {
                used: 0,
                total: u32::MAX,
            },
        ] {
            assert_eq!(memory(Some(v)).as_str(), "--/--K");
        }
        // Worst supported values fit independent columns without clipping content.
        assert!(
            line(format_args!(
                "FPS {} AVG {}",
                rate(Some(999.99)).as_str(),
                rate(Some(999.99)).as_str()
            ))
            .len()
                * 6
                <= 152
        );
        assert!(
            line(format_args!(
                "W C0 {} C1 {}",
                work(Some(1000)).as_str(),
                work(Some(1000)).as_str()
            ))
            .len()
                * 6
                <= 144
        );
        let max = memory(Some(MemoryUse {
            used: 524288,
            total: 524288,
        }));
        assert!(
            line(format_args!(
                "Static {} SP0 {} SP1 {}",
                max.as_str(),
                max.as_str(),
                max.as_str()
            ))
            .len()
                * 6
                <= 300
        );
    }
    /// Expired advertisements suppress another Inspect without invalidating already retained GATT evidence.
    #[test]
    fn old_advertisement_does_not_label_retained_gatt_as_stale() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.phase = RadioPhase::Paused;
        let hints = control_hints(&p, 30_001);
        assert!(hints[0].contains("Advert old"));
        assert!(!hints[0].contains("GATT stale"));
        assert!(hints[1].contains("retained"));
    }
    /// Checks observed MTU/stage/descriptor/read failure details survive formatting with exact identifiers.
    #[test]
    fn link_stages_descriptor_and_exact_read_evidence_are_retained_in_lines() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.inspection.link = Some(LinkInfo {
            connected_at_ms: 123,
            connect_elapsed_ms: 456,
            interval_us: 30_000,
            latency: 4,
            supervision_timeout_us: 2_000_000,
            att_mtu: Some(247),
        });
        p.inspection.stages = [true, true, false, false];
        p.inspection
            .descriptors
            .push(Descriptor {
                service_start: 1,
                characteristic_handle: 3,
                handle: 4,
                uuid: Uuid::from_wire(&[2, 0x29]).unwrap(),
            })
            .unwrap();
        p.inspection
            .values
            .push(InfoValue {
                service_start: 1,
                handle: 3,
                uuid: 0x2A29,
                result: ReadResult::Protected,
                failure: Some(FailureReason::Att(5)),
                elapsed_ms: 55,
                value: Payload::new(&[]),
            })
            .unwrap();
        let rows = lines(&p);
        for expected in [
            "ATT MTU: 247 bytes",
            "Services: ended",
            "Descriptors: partial / not run",
            "DESCRIPTOR 0004 / characteristic 0003",
            "Read service 0001 / handle 0003",
            "Outcome: Protected / 55 ms",
            "Read error: ATT error 0x05",
        ] {
            assert!(rows.iter().any(|row| row == expected), "missing {expected}");
        }
    }
    /// Round-trips a capture with maximal counters and invalid-length raw data, then checks complete bounded rows.
    #[test]
    fn saved_lines_preserve_extreme_numbers_raw_values_and_capture_provenance() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.inspection
            .services
            .push(Service {
                start: 1,
                end: 9,
                uuid: Uuid::from_wire(&[0, 0x18]).unwrap(),
            })
            .unwrap();
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: 1,
                declaration: 2,
                handle: 3,
                properties: 2,
                uuid: Uuid::from_wire(&[1, 0x2A]).unwrap(),
            })
            .unwrap();
        p.inspection
            .values
            .push(InfoValue {
                service_start: 1,
                handle: 3,
                uuid: 0x2A01,
                result: ReadResult::Ok,
                failure: None,
                elapsed_ms: u32::MAX,
                value: Payload::new(&[0xAB]),
            })
            .unwrap();
        p.inspection.link = Some(LinkInfo {
            connected_at_ms: u64::MAX,
            connect_elapsed_ms: u32::MAX,
            interval_us: u32::MAX,
            latency: u16::MAX,
            supervision_timeout_us: u32::MAX,
            att_mtu: Some(517),
        });
        p.dropped_reports = u32::MAX;
        p.malformed_reports = u32::MAX;
        let mut bytes = [0; 8192];
        let len = capture::encode(
            &p,
            capture::BuildIdentity {
                source_digest: [0xFF; 32],
                cpu_hz: u32::MAX,
                spi_hz: u32::MAX,
                profile: capture::BuildProfile::Stock,
                source_identity_available: true,
            },
            u64::MAX,
            &mut bytes,
        )
        .unwrap();
        let mut decoder = capture::Decoder::new();
        let view = decoder.decode(&bytes[..len]).unwrap();
        let mut rows = std::vec::Vec::<std::string::String>::new();
        saved_details(&view, &mut EvidenceLines::new(Some(&mut |line| rows.push(line.into()))));
        for expected in [
            "Captured at: 18446744073709551615 ms",
            "Connected at: 18446744073709551615 ms",
            "Interval: 4294967295 us / latency 65535",
            "CPU 4294967295 Hz / SPI 4294967295 Hz",
            "Outcome: Ok / 4294967295 ms",
            "! Unexpected standard length; raw retained",
            "AB",
            "Reports dropped 4294967295 / malformed 4294967295",
        ] {
            assert!(
                rows.iter().any(|row| row == expected),
                "missing or truncated {expected}"
            );
        }
        assert_eq!(rows.iter().filter(|row| row.starts_with("FF FF FF")).count(), 2);
        assert!(rows.iter().all(|row| row.is_ascii() && row.len() * 6 <= 300));
        assert_eq!(saved_page_count(&view), rows.len().div_ceil(LINES));
    }
    /// Only Scanning claims active listening; paused/fault states offer their distinct recovery paths.
    #[test]
    fn empty_catalog_copy_matches_radio_state() {
        assert!(empty_copy(RadioPhase::Scanning).0.starts_with("Listening"));
        for phase in [
            RadioPhase::Paused,
            RadioPhase::Fault,
            RadioPhase::Starting,
            RadioPhase::Stopping,
            RadioPhase::Connecting,
            RadioPhase::Discovering,
            RadioPhase::Disconnecting,
        ] {
            assert!(!empty_copy(phase).0.contains("Listening"));
        }
        assert!(empty_copy(RadioPhase::Paused).1.contains("Y"));
        assert!(empty_copy(RadioPhase::Fault).1.contains("Automatic restart"));
    }
    /// Status labels retain error codes/recovery clues and leave enough space for bounded header counters.
    #[test]
    fn error_copy_preserves_code_and_explains_recovery() {
        for (error, code, clue) in [
            (ProbeError::Timeout, "Timeout", "deadline"),
            (ProbeError::Connect, "Connect", "retry"),
            (ProbeError::Capacity, "Capacity", "partial"),
            (ProbeError::InvalidPeer, "InvalidPeer", "rescan"),
            (ProbeError::Cleanup, "Cleanup", "restarting"),
        ] {
            let hint = error_copy(error);
            assert!(hint.starts_with(code));
            assert!(hint.contains(clue));
            assert!(hint.len() + 18 <= 50);
        }
    }
    /// Checks signed RSSI, every GATT property bit and immediate-back recovery labels.
    #[test]
    fn rssi_and_characteristic_flags_are_photo_readable() {
        let p = probe();
        assert!(lines(&p).iter().any(|l| l.contains("RSSI: -50 dBm")));
        assert_eq!(properties_text(0).as_str(), "None");
        for (bits, name) in [
            (1, "Bcast"),
            (2, "R"),
            (4, "WriteCmd"),
            (8, "W"),
            (16, "Notify"),
            (32, "Indicate"),
            (64, "Signed"),
            (128, "Ext"),
        ] {
            assert_eq!(properties_text(bits).as_str(), name);
        }
        assert_eq!(
            properties_text(255).as_str(),
            "Bcast R WriteCmd W Notify Indicate Signed Ext"
        );
        for error in [
            ProbeError::Connect,
            ProbeError::Timeout,
            ProbeError::Discovery,
            ProbeError::Scan,
        ] {
            assert!(error_copy(error).contains("Y"));
            assert!(!error_copy(error).contains("hold"));
        }
    }
    /// Returns every live evidence row for `p` at time zero in owned test strings.
    fn lines(p: &Probe) -> std::vec::Vec<std::string::String> {
        let mut lines = std::vec::Vec::new();
        details(p, 0, &mut EvidenceLines::new(Some(&mut |s| lines.push(s.into()))));
        lines
    }

    /// Rejected prefixes retain lengths/unvalidated handles, and current errors outrank latched headers.
    #[test]
    fn rejected_wire_evidence_is_bounded_explicit_and_paginated() {
        for length in [0, 1, 2, 3, 5, 19, 20, 253, 65535] {
            let raw = std::vec![0xAB; length];
            let rejected = RejectedDeclaration::from_wire(0xFFFF, &raw);
            let mut rows = std::vec::Vec::<std::string::String>::new();
            rejected_details(
                Some(rejected),
                &mut EvidenceLines::new(Some(&mut |s| rows.push(s.into()))),
            );
            assert_eq!(rows[0], std::format!("Rejected decl FFFF / {length} bytes"));
            assert_eq!(
                rows[1],
                if length >= 3 {
                    "Rejected value field: ABAB (unvalidated)"
                } else {
                    "Rejected value field: missing"
                }
            );
            assert_eq!(rows.len(), 3 + length.min(19).div_ceil(16));
            assert!(rows.iter().all(|s| s.is_ascii() && s.len() * 6 <= 300));
            let mut counted = EvidenceLines::new(None);
            rejected_details(Some(rejected), &mut counted);
            assert_eq!(counted.count, rows.len());
        }
        let mut p = Probe::new();
        p.fault_latch = FaultLatch::single(Fault::Scan);
        assert_eq!(status_copy(&p), "Slow red: scan errors");
        p.error = ProbeError::Discovery;
        assert_eq!(status_copy(&p), "Discovery: failed; Y back");
        p.error = ProbeError::None;
        p.fault_latch = FaultLatch::single(Fault::Probe);
        assert_eq!(status_copy(&p), "Red: probe error");
        p.fault_latch = FaultLatch::EMPTY;
        p.malformed_reports = u32::MAX;
        assert_eq!(status_copy(&p), "Error:None");
    }
    /// Returns one public-address peer with flags, negative TX power, manufacturer and standard-service AD fields.
    fn probe() -> Probe {
        let mut p = Probe::new();
        p.catalog.observe(Advertisement::new(
            Peer {
                address: [0; 6],
                random: false,
            },
            ReportKind::Connectable,
            -50,
            &[2, 1, 6, 2, 0x0A, 0xF6, 3, 0xFF, 0x34, 0x12, 3, 3, 0x0A, 0x18],
            0,
        ));
        p
    }
    /// Every fault remains visible, legacy absence stays unknown, and first-recorded scan cannot hide a later fault.
    #[test]
    fn fault_ledger_keeps_every_source_visible_and_legacy_unknown() {
        let mut p = probe();
        p.fault_latch = FaultLatch::from_parts(0x1FF, Fault::Timing as u8).unwrap();
        p.radio_bus_hz = Some(2_500_000);
        let rows = lines(&p);
        assert_eq!(status_copy(&p), "Red: timing");
        assert!(rows.iter().any(|r| r == "First recorded: display completion timeline"));
        for fault in Fault::ALL {
            assert!(rows.iter().any(|r| r == &std::format!("Latched: {}", fault.label())));
        }
        assert!(rows.iter().all(|r| r.len() * 6 <= 300));
        assert_eq!(page_count(&p), rows.len().div_ceil(LINES));
        let mut legacy = std::vec::Vec::<std::string::String>::new();
        fault_details(None, &mut EvidenceLines::new(Some(&mut |s| legacy.push(s.into()))));
        assert_eq!(legacy[1], "Not recorded by this legacy capture");
        // A scan recorded first must not hide a later steady-red cause.
        p.fault_latch = FaultLatch::from_parts(Fault::Scan.bit() | Fault::Encoder.bit(), Fault::Scan as u8).unwrap();
        assert_eq!(status_copy(&p), "Red: encoder I2C");
        p.catalog = Catalog::new();
        assert_eq!(control_hints(&p, 0)[1], "X: Fault details  Y: Pause scan");
        let rows = lines(&p);
        assert!(rows.iter().any(|r| r == "Latched: encoder I2C"));
        assert!(rows.iter().any(|r| r == "No selected device"));
    }

    /// Cached-name provenance and retained reset observations remain bounded without claiming causation.
    #[test]
    fn name_and_reset_evidence_are_visible_bounded_and_honest() {
        use crate::health::reset::{BootEvidence, RecoveryCause, ResetRecord, ResetStage};
        let mut p = probe();
        let peer = p.selected_device().unwrap().peer;
        p.catalog
            .observe(Advertisement::new(peer, ReportKind::Connectable, -50, b"\x03\x09TV", 1));
        p.catalog
            .observe(Advertisement::new(peer, ReportKind::Connectable, -50, &[2, 1, 6], 2));
        p.boot = Some(BootEvidence {
            raw_reason: 1,
            boot_type: Some(0),
            previous: Some(ResetRecord {
                stage: ResetStage::Failure(RecoveryCause::RadioStartup),
                elapsed_ms: u32::MAX,
            }),
        });
        let rows = lines(&p);
        assert!(rows.iter().any(|r| r == "Name source: advertisement / cached"));
        assert!(rows.iter().any(|r| r == "Previous stage: radio startup"));
        assert!(rows.iter().any(|r| r.contains("not proven reset cause")));
        assert!(rows.iter().all(|r| r.is_ascii() && r.len() * 6 <= 300));
        p.boot = None;
        let rows = lines(&p);
        assert!(!rows.iter().any(|r| r == "Previous stage: radio startup"));
    }

    /// Checks signed and little-endian AD decoding beside the unchanged wire-order hexadecimal payload.
    #[test]
    fn parsed_fields_and_raw_wire_bytes_are_preserved() {
        let p = probe();
        let l = lines(&p);
        assert!(l.iter().any(|l| l == "Flags: 0x06"));
        assert!(l.iter().any(|l| l == "TX power: -10 dBm"));
        assert!(l.iter().any(|l| l == "Manufacturer ID: 0x1234"));
        assert!(l.iter().any(|l| l == "180A"));
        assert!(l.iter().any(|l| l == "Device Information"));
        assert!(l.iter().any(|l| l == "02 01 06 02 0A F6 03 FF 34 12 03 03 0A 18"));
    }
    /// Advertisement-only viewing makes no GATT completion claim; explicit inspection displays actual phases.
    #[test]
    fn advertisement_view_does_not_claim_a_failed_or_empty_gatt_inspection() {
        let mut p = probe();
        assert_eq!(p.input(Input::Details, 0).unwrap().action, Action::Scan(false));
        let l = lines(&p);
        assert!(l.iter().any(|l| l.contains("GATT not inspected")));
        assert!(
            !l.iter()
                .any(|l| l.contains("Partial") || l.contains("0 primary services"))
        );
        assert!(l.iter().any(|l| l.contains("no connection")));

        p.event(Event::State {
            revision: p.request.revision,
            phase: RadioPhase::Paused,
            error: ProbeError::None,
        });
        assert!(matches!(p.input(Input::Click, 1).unwrap().action, Action::Inspect(_)));
        for (phase, expected) in [
            (RadioPhase::Stopping, "Waiting for scan / connection cleanup"),
            (RadioPhase::Connecting, "Connecting to selected peer"),
            (RadioPhase::Discovering, "GATT inspection in progress"),
            (RadioPhase::Paused, "Partial / not completed"),
        ] {
            p.event(Event::State {
                revision: p.request.revision,
                phase,
                error: ProbeError::Timeout,
            });
            let l = lines(&p);
            assert!(l.iter().any(|l| l == expected), "wrong status for {phase:?}: {l:?}");
            assert!(!l.iter().any(|l| l.contains("GATT not inspected")));
        }
    }
    /// Cross-checks footer Inspect offers against actual input admission across screens, phases and freshness edges.
    #[test]
    fn footer_only_offers_inspection_when_the_input_path_accepts_it_and_fits_its_rows() {
        for screen in [Screen::List, Screen::Details] {
            for phase in [
                RadioPhase::Starting,
                RadioPhase::Scanning,
                RadioPhase::Paused,
                RadioPhase::Stopping,
                RadioPhase::Connecting,
                RadioPhase::Discovering,
                RadioPhase::Disconnecting,
                RadioPhase::Fault,
            ] {
                for now in [0, 30_000, 30_001] {
                    let mut p = probe();
                    p.screen = screen;
                    p.phase = phase;
                    let hints = control_hints(&p, now);
                    let accepts_inspect = p.input(Input::Click, now).is_some();
                    assert_eq!(
                        hints[0].contains("Click"),
                        accepts_inspect,
                        "{screen:?}/{phase:?}/{now}"
                    );
                    assert!(hints.iter().all(|s| s.is_ascii() && s.len() * 6 <= 304));
                    assert!(hints[1].contains("Y:"));
                }
            }
        }
        let empty = Probe::new();
        assert!(!control_hints(&empty, 0)[0].contains("Click"));
        assert!(!control_hints(&empty, 0)[1].contains("X:"));
    }
    /// Omitted or rejected records prevent the otherwise completed inspection from claiming enumeration ended.
    #[test]
    fn incomplete_or_malformed_inspection_never_claims_enumeration_ended() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.phase = RadioPhase::Paused;
        p.inspection.complete = true;
        assert!(lines(&p).iter().any(|l| l == "Enumeration ended"));
        for (omitted, malformed) in [(1, 0), (0, 1)] {
            p.inspection.omitted = omitted;
            p.inspection.malformed = malformed;
            let l = lines(&p);
            assert!(!l.iter().any(|l| l == "Enumeration ended"));
            assert!(l.iter().any(|l| l == "Partial / not completed"));
        }
    }
    /// A full 64-byte read retains four raw rows while every evidence row stays within 50 characters.
    #[test]
    fn all_detail_lines_fit_without_losing_value_bytes() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.inspection
            .values
            .push(InfoValue {
                service_start: 1,
                handle: 3,
                failure: None,
                elapsed_ms: 1,
                uuid: 0x2A29,
                value: Payload::new(&[0xFE; 64]),
                result: ReadResult::Ok,
            })
            .unwrap();
        let l = lines(&p);
        assert!(l.iter().all(|l| l.len() <= 50));
        assert_eq!(l.iter().filter(|l| l.starts_with("FE FE")).count(), 4);
    }

    /// Exercises failure-stage/reason combinations, maximum fields and all byte codes without row truncation.
    #[test]
    fn every_failure_label_and_maximum_evidence_field_is_visible_and_fits() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        for stage in [
            FailureStage::Connect,
            FailureStage::Mtu,
            FailureStage::Services,
            FailureStage::Characteristics,
            FailureStage::DeviceInfo,
            FailureStage::ConnectionEvents,
            FailureStage::ClientTask,
            FailureStage::Inspection,
        ] {
            for reason in [
                FailureReason::Controller,
                FailureReason::Timeout,
                FailureReason::Disconnected,
                FailureReason::DisconnectCode(255),
                FailureReason::Capacity,
                FailureReason::Att(255),
                FailureReason::Hci(255),
                FailureReason::HciDecode,
                FailureReason::UnexpectedResponse,
                FailureReason::InvalidValue,
                FailureReason::Codec,
                FailureReason::InvalidState,
                FailureReason::NotFound,
                FailureReason::Unsupported,
                FailureReason::Busy,
                FailureReason::Other,
                FailureReason::DeclarationLength,
                FailureReason::DeclarationOrder,
                FailureReason::ValueHandle,
                FailureReason::PeerMismatch,
            ] {
                p.failure = Some(InspectionFailure {
                    stage,
                    reason,
                    range: Some((1, 0xFFFF)),
                    last_accepted: Some((0xFFFE, 0xFFFF)),
                    rejected: None,
                    elapsed_ms: Some(u32::MAX),
                });
                let l = lines(&p);
                assert_eq!(l[0], std::format!("GATT FAILURE / {}", stage.label()));
                assert!(l[1].ends_with(if reason.code().is_some() {
                    "0xFF"
                } else {
                    reason.label()
                }));
                assert_eq!(l[2], "Stage elapsed: 4294967295 ms");
                assert_eq!(l[3], "Query range 0001-FFFF");
                assert_eq!(l[4], "Last accepted decl FFFE / value FFFF");
                assert!(l.iter().all(|s| s.is_ascii() && s.len() * 6 <= 300));
            }
        }
        p.failure = Some(InspectionFailure::new(
            FailureStage::ClientTask,
            FailureReason::Disconnected,
        ));
        let l = lines(&p);
        assert!(
            l.iter()
                .all(|s| !s.contains("Stage elapsed") && !s.contains("Query range") && !s.contains("Last accepted"))
        );
        for code in 0..=u8::MAX {
            p.failure_context = ConnectionFailureContext {
                local_error: Some(FailureReason::Att(code)),
                disconnect_reason: Some(code),
            };
            let l = lines(&p);
            assert!(
                l.iter()
                    .any(|s| s == &std::format!("Host ingress: ATT error 0x{code:02X}"))
            );
            assert!(l.iter().any(|s| s == &std::format!("HCI disconnect: 0x{code:02X}")));
            assert!(l.iter().all(|s| s.len() * 6 <= 300));
        }
    }

    /// Maximum global scan counters remain complete and are explicitly not attributed to a peer.
    #[test]
    fn scan_evidence_keeps_maximum_counts_and_does_not_blame_a_peer() {
        let mut p = Probe::new();
        p.scan_diagnostics = ScanDiagnostics {
            total: u32::MAX,
            invalid_size: u32::MAX,
            invalid_value: u32::MAX,
            last: Some(ScanFailure {
                kind: ScanErrorKind::InvalidValue,
                revision: u32::MAX,
                index: 254,
                total_reports: 255,
                report_bytes: u16::MAX,
                remaining_bytes: u16::MAX,
                declared_data_len: Some(u8::MAX),
                at_ms: 1,
            }),
        };
        p.input(Input::Details, 2).unwrap();
        p.phase = RadioPhase::Paused;
        let hints = control_hints(&p, 2);
        assert!(hints[0].contains("pages") && !hints[0].contains("Click"));
        let mut l = std::vec::Vec::<std::string::String>::new();
        details(&p, u64::MAX, &mut EvidenceLines::new(Some(&mut |s| l.push(s.into()))));
        assert!(l.iter().any(|s| s == "Not attributed to the selected device"));
        assert!(l.iter().any(|s| s == "Bad batches 4294967295 / size 4294967295"));
        assert!(l.iter().any(|s| s == "Invalid value 4294967295"));
        assert!(l.iter().any(|s| s == "Scan revision 4294967295 / entry 255/255"));
        assert!(l.iter().any(|s| s == "Last InvalidValue / 18446744073709551s ago"));
        assert!(l.iter().any(|s| s == "Report bytes 65535 / remaining 65535"));
        assert!(l.iter().any(|s| s == "Declared data length: 255"));
        assert!(l.iter().all(|s| s.is_ascii() && s.len() * 6 <= 300));
        let pages = page_count(&p);
        assert_eq!(pages, l.len().div_ceil(10));
        p.input(Input::Back, 2);
        assert!(control_hints(&p, 2)[1].contains("X: Scan diagnostics"));
        p.scan_diagnostics.last.as_mut().unwrap().declared_data_len = None;
        assert!(
            lines(&p)
                .iter()
                .any(|s| s == "Declared data length: missing header byte")
        );
    }

    /// Future observations report unavailable age while equal/later snapshots show zero/elapsed seconds.
    #[test]
    fn scan_observation_newer_than_ui_snapshot_has_unavailable_age() {
        let mut p = Probe::new();
        p.scan_diagnostics.record(ScanFailure {
            kind: ScanErrorKind::InvalidSize,
            revision: 1,
            index: 0,
            total_reports: 1,
            report_bytes: 3,
            remaining_bytes: 3,
            declared_data_len: None,
            at_ms: 5,
        });
        for (now, expected) in [
            (4, "Last InvalidSize / age unavailable"),
            (5, "Last InvalidSize / 0s ago"),
            (1005, "Last InvalidSize / 1s ago"),
        ] {
            let mut rows = std::vec::Vec::<std::string::String>::new();
            details(
                &p,
                now,
                &mut EvidenceLines::new(Some(&mut |line| rows.push(line.into()))),
            );
            assert!(rows.iter().any(|line| line == expected), "{rows:?}");
            assert!(rows.iter().all(|line| line.len() * 6 <= 300));
        }
    }

    /// All HCI reasons preserve cleanup acknowledgment independently of whether termination was normal.
    #[test]
    fn shared_live_and_saved_link_lines_separate_cleanup_ack_from_abnormal_termination() {
        let link = Some(LinkInfo {
            connected_at_ms: 2,
            connect_elapsed_ms: 1,
            interval_us: 30_000,
            latency: 0,
            supervision_timeout_us: 4_000_000,
            att_mtu: Some(23),
        });
        for reason in (0..=u8::MAX).map(Some).chain([None]) {
            let mut rows = std::vec::Vec::<std::string::String>::new();
            link_details(
                link,
                [true; 4],
                true,
                reason,
                &mut EvidenceLines::new(Some(&mut |s| rows.push(s.into()))),
            );
            assert!(rows.iter().any(|s| s == "Disconnect cleanup: acknowledged"));
            assert_eq!(
                rows.iter().any(|s| s == "Termination not clean / inspect reason"),
                !matches!(reason, Some(0x13 | 0x16))
            );
            assert!(rows.iter().all(|s| s.is_ascii() && s.len() * 6 <= 300));
            if reason == Some(0x22) {
                assert!(rows.iter().any(|s| s == "Disconnect reason: 0x22"));
                assert!(rows.iter().any(|s| s == "LL response timeout"));
            }
        }
    }
}
