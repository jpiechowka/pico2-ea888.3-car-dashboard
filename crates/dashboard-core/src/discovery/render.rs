//! Fixed landscape photographic UI. Pagination walks bounded records without allocating a document.
use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_7X13, FONT_9X15};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use heapless::String;

use super::*;
const BG: Rgb565 = Rgb565::new(2, 3, 4);
const PANEL: Rgb565 = Rgb565::new(4, 6, 7);
const WHITE: Rgb565 = Rgb565::new(30, 61, 30);
const MUTED: Rgb565 = Rgb565::new(20, 42, 22);
const RED: Rgb565 = Rgb565::new(31, 8, 6);
const LINES: usize = 10;
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
fn gatt_requested(probe: &Probe) -> bool {
    matches!(probe.request.action, Action::Inspect(peer) if probe.selected_device().is_some_and(|d| d.peer == peer))
}

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
                "Turn/A/B: pages  GATT stale; Y to rescan"
            } else {
                "Turn/A/B: select  GATT stale; rescan"
            }
        } else if details {
            if gatt_requested(probe) {
                "Turn/A/B: pages  Click/X: Repeat GATT"
            } else {
                "Turn/A/B: pages  Click/X: Inspect GATT"
            }
        } else {
            "Turn/A/B: select  Click: Inspect GATT"
        }
    } else if details {
        "Turn/A/B: pages  No peer to inspect"
    } else {
        "Turn/A/B: select  No peer selected"
    };
    let second = if details {
        if busy && gatt_requested(probe) {
            "Y: Cancel / back to scan"
        } else {
            "Y: Back to scan"
        }
    } else if probe.selected_device().is_none() {
        if probe.scan_diagnostics.last.is_some() {
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
fn line(args: core::fmt::Arguments<'_>) -> String<50> {
    let mut s = String::new();
    let _ = s.write_fmt(args);
    s
}
fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    x: i32,
    y: i32,
    s: &str,
    c: Rgb565,
) -> Result<(), D::Error> {
    Text::with_baseline(s, Point::new(x, y), MonoTextStyle::new(&FONT_6X10, c), Baseline::Top)
        .draw(d)
        .map(|_| ())
}

fn rate(value: Option<f32>) -> String<8> {
    let mut s = String::new();
    match value {
        Some(v) if v.is_finite() && (0.0..1000.0).contains(&v) => {
            // Bounded one-decimal rounding avoids pulling the general float
            // formatter's large conversion frames into the utility image.
            let tenths = (v * 10.0 + 0.5) as u32;
            let _ = write!(s, "{}.{:01}", tenths / 10, tenths % 10);
        }
        Some(v) if v.is_finite() && v >= 1000.0 => {
            let _ = s.push_str(">999");
        }
        _ => {
            let _ = s.push_str("--");
        }
    }
    s
}

fn work(value: Option<u16>) -> String<8> {
    let mut s = String::new();
    if let Some(v) = value.filter(|&v| v <= 1000) {
        let _ = write!(s, "{}.{:01}%", v / 10, v % 10);
    } else {
        let _ = s.push_str("--%");
    }
    s
}

fn memory(value: Option<MemoryUse>) -> String<12> {
    let mut s = String::new();
    if let Some(v) = value.filter(|v| v.valid()) {
        // Both are rounded KiB labels; byte-exact bounds remain in MemoryUse.
        let _ = write!(s, "{}/{}K", v.used.div_ceil(1024), v.total.div_ceil(1024));
    } else {
        let _ = s.push_str("--/--K");
    }
    s
}

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
            rate(values.fps.map(|v| v.current)),
            rate(values.fps.map(|v| v.average))
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
            work(values.work[0]),
            work(values.work[1])
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
            memory(values.ram),
            memory(values.stacks[0]),
            memory(values.stacks[1])
        )),
        MUTED,
    )
}
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
fn uuid_text(uuid: Uuid) -> String<50> {
    let mut s = String::new();
    for byte in uuid.bytes[..usize::from(uuid.len).min(16)].iter().rev() {
        let _ = write!(s, "{byte:02X}");
    }
    s
}
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
fn hex(
    bytes: &[u8],
    emit: &mut impl FnMut(&str),
) {
    for chunk in bytes.chunks(16) {
        let mut s = String::<50>::new();
        for (i, b) in chunk.iter().enumerate() {
            if i > 0 {
                let _ = s.push(' ');
            }
            let _ = write!(s, "{b:02X}");
        }
        emit(&s);
    }
}
fn packet(
    title: &str,
    payload: &Payload<31>,
    emit: &mut impl FnMut(&str),
) {
    emit(&line(format_args!(
        "{title}: {} bytes{}",
        payload.len,
        if payload.truncated { " [TRUNCATED]" } else { "" }
    )));
    if payload.as_slice().is_empty() {
        emit("No payload received");
        return;
    }
    emit("RAW HEX (wire order)");
    hex(payload.as_slice(), emit);
    for field in AdFields::new(payload.as_slice()) {
        let Ok(f) = field else {
            emit("! MALFORMED advertisement length");
            break;
        };
        match f.kind {
            1 if f.data.len() == 1 => emit(&line(format_args!("Flags: 0x{:02X}", f.data[0]))),
            8 | 9 => {
                emit(if f.kind == 9 {
                    "Complete name:"
                } else {
                    "Shortened name:"
                });
                emit(&printable::<50>(f.data));
            }
            0x0A if f.data.len() == 1 => emit(&line(format_args!("TX power: {} dBm", f.data[0] as i8))),
            0xFF if f.data.len() >= 2 => {
                emit(&line(format_args!(
                    "Manufacturer ID: 0x{:04X}",
                    u16::from_le_bytes([f.data[0], f.data[1]])
                )))
            }
            2..=7 => {
                let width = match f.kind {
                    2 | 3 => 2,
                    4 | 5 => 4,
                    _ => 16,
                };
                emit(if f.kind % 2 == 1 {
                    "Service UUIDs (complete list):"
                } else {
                    "Service UUIDs (incomplete list):"
                });
                for chunk in f.data.chunks(width) {
                    if let Some(uuid) = Uuid::from_wire(chunk).filter(|_| chunk.len() == width) {
                        emit(&uuid_text(uuid));
                        let label = uuid_label(uuid);
                        if !label.is_empty() {
                            emit(label);
                        }
                    } else {
                        emit("! MALFORMED UUID field");
                    }
                }
            }
            _ => {
                emit(&line(format_args!(
                    "AD type 0x{:02X}: {} bytes (see raw)",
                    f.kind,
                    f.data.len()
                )))
            }
        }
    }
}
fn peer_details(
    probe: &Probe,
    now: u64,
    emit: &mut impl FnMut(&str),
) {
    let Some(d) = probe.selected_device() else {
        emit("No selected device");
        return;
    };
    emit("PEER / ADVERTISEMENT EVIDENCE");
    emit(&d.name());
    emit(&line(format_args!("{} / {}", d.peer.to_string(), d.peer.kind_label())));
    emit(&line(format_args!(
        "Seen {}s ago / {} reports",
        d.age_ms(now) / 1000,
        d.reports
    )));
    emit(&match d.rssi {
        Some(rssi) => line(format_args!("RSSI: {rssi} dBm (not distance)")),
        None => line(format_args!("RSSI unavailable")),
    });
    packet("ADVERTISEMENT", &d.advertisement, emit);
    packet("SCAN RESPONSE", &d.response, emit);
    if !gatt_requested(probe) {
        emit("GATT not inspected");
        emit("Advertisement only / no connection requested");
        emit("When ready: Click/X connects + inspects GATT");
        return;
    }
    let i = &probe.inspection;
    emit("GATT / EXPLICIT INSPECTION");
    emit(if i.complete && i.omitted == 0 && i.malformed == 0 {
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
    emit(&line(format_args!(
        "Omitted {} / rejected records {}",
        i.omitted, i.malformed
    )));
    emit(&line(format_args!(
        "{} primary services / {} characteristics",
        i.services.len(),
        i.characteristics.len()
    )));
    for s in &i.services {
        emit(&line(format_args!("SERVICE {:04X}-{:04X}", s.start, s.end)));
        emit(&uuid_text(s.uuid));
        let label = uuid_label(s.uuid);
        if !label.is_empty() {
            emit(label);
        }
    }
    for c in &i.characteristics {
        emit(&line(format_args!(
            "CHAR {:04X} / service {:04X}",
            c.handle, c.service_start
        )));
        emit(&uuid_text(c.uuid));
        let label = uuid_label(c.uuid);
        if !label.is_empty() {
            emit(label);
        }
        emit(&line(format_args!(
            "Decl {:04X} / properties 0x{:02X}",
            c.declaration, c.properties
        )));
        emit(&properties_text(c.properties));
    }
    for v in &i.values {
        emit(&line(format_args!(
            "{} ({:04X}) / {:?}",
            info_label(v.uuid).unwrap_or("Value"),
            v.uuid,
            v.result
        )));
        if v.result == ReadResult::Ok {
            for chunk in v.value.as_slice().chunks(48) {
                emit(&printable::<50>(chunk));
            }
            emit("Value RAW HEX:");
            hex(v.value.as_slice(), emit);
        }
        if v.value.truncated {
            emit("! VALUE TRUNCATED");
        }
    }
}
fn details(
    probe: &Probe,
    now: u64,
    emit: &mut impl FnMut(&str),
) {
    if let Some(failure) = probe.failure.filter(|_| gatt_requested(probe)) {
        emit(&line(format_args!("GATT FAILURE / {}", failure.stage.label())));
        if let Some(code) = failure.reason.code() {
            emit(&line(format_args!("Reason: {} 0x{code:02X}", failure.reason.label())));
        } else {
            emit(&line(format_args!("Reason: {}", failure.reason.label())));
        }
        if let Some(elapsed) = failure.elapsed_ms {
            emit(&line(format_args!("Stage elapsed: {elapsed} ms")));
        }
        if let Some((start, end)) = failure.range {
            emit(&line(format_args!("Query range {start:04X}-{end:04X}")));
        }
        if let Some((decl, value)) = failure.last_accepted {
            emit(&line(format_args!("Last accepted decl {decl:04X} / value {value:04X}")));
        }
        if let Some(error) = probe.failure_context.local_error {
            if let Some(code) = error.code() {
                emit(&line(format_args!("Host ingress: {} 0x{code:02X}", error.label())));
            } else {
                emit(&line(format_args!("Host ingress: {}", error.label())));
            }
        }
        match probe.failure_context.disconnect_reason {
            Some(code) => emit(&line(format_args!("HCI disconnect: 0x{code:02X}"))),
            None => emit("HCI disconnect: unavailable"),
        }
        emit("Partial records retained; Y returns to scan");
    }
    peer_details(probe, now, emit);
    if let Some(last) = probe.scan_diagnostics.last {
        let scan = probe.scan_diagnostics;
        emit("GLOBAL HCI SCAN ERRORS / since boot");
        emit("Not attributed to the selected device");
        emit(&line(format_args!(
            "Bad batches {} / size {}",
            scan.total, scan.invalid_size
        )));
        emit(&line(format_args!("Invalid value {}", scan.invalid_value)));
        emit(&line(format_args!(
            "Last {:?} / {}s ago",
            last.kind,
            now.saturating_sub(last.at_ms) / 1000
        )));
        emit(&line(format_args!(
            "Scan revision {} / entry {}/{}",
            last.revision,
            u16::from(last.index) + 1,
            last.total_reports
        )));
        emit("Parsing stops at the first bad entry in a batch");
        emit(&line(format_args!(
            "Report bytes {} / remaining {}",
            last.report_bytes, last.remaining_bytes
        )));
        match last.declared_data_len {
            Some(length) => emit(&line(format_args!("Declared data length: {length}"))),
            None => emit("Declared data length: missing header byte"),
        }
        emit("One record needs 10 + data length bytes");
    }
}
/// Detail page count, independent of the current screen and page index.
pub fn page_count(probe: &Probe) -> usize {
    let mut count: usize = 0;
    details(probe, 0, &mut |_| count += 1);
    count.div_ceil(LINES).max(1)
}
/// Draws the same RGB565 scene for host screenshots and the hardware framebuffer.
pub fn render<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    probe: &Probe,
    now_ms: u64,
) -> Result<(), D::Error> {
    let mut clipped = target.clipped(&Rectangle::new(Point::zero(), Size::new(320, 240)));
    let d = &mut clipped;
    d.clear(BG)?;
    rect(d, 0, 0, 320, 3, RED)?;
    Text::with_baseline(
        "BLE PROBE",
        Point::new(10, 7),
        MonoTextStyle::new(&FONT_9X15, WHITE),
        Baseline::Top,
    )
    .draw(d)?;
    let active = !matches!(probe.phase, RadioPhase::Paused | RadioPhase::Fault);
    let spin = if active {
        ["|", "/", "-", "\\"][((now_ms / 200) % 4) as usize]
    } else {
        " "
    };
    text(d, 112, 10, &line(format_args!("{spin} {:?}", probe.phase)), WHITE)?;
    telemetry(d, probe.diagnostics)?;
    text(
        d,
        10,
        49,
        &if probe.screen == Screen::Details
            && let Some(peer) = probe.selected_device()
        {
            line(format_args!("{} / {}", peer.peer.to_string(), peer.peer.kind_label()))
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
            error_copy(probe.error),
            probe.malformed_reports
        )),
        if probe.error == ProbeError::None { MUTED } else { RED },
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
            Text::with_baseline(
                label,
                Point::new(14, y + 1),
                MonoTextStyle::new(&FONT_7X13, WHITE),
                Baseline::Top,
            )
            .draw(d)?;
            text(
                d,
                14,
                y + 16,
                &line(format_args!(
                    "{} {} {}s",
                    device.peer.to_string(),
                    device
                        .rssi
                        .map(|r| line(format_args!("{r}dBm")))
                        .unwrap_or_else(|| line(format_args!("RSSI?"))),
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
        details(probe, now_ms, &mut |s| {
            if (page * LINES..(page + 1) * LINES).contains(&index)
                && error.is_none()
                && let Err(e) = text(d, 10, 88 + (index % LINES) as i32 * 11, s, WHITE)
            {
                error = Some(e);
            }
            index += 1;
        });
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
            "Hold Y 2s: clear + recover scan"
        } else {
            "Hold Y 2s: recover | ENCODER MISSING"
        },
        if probe.encoder_connected { MUTED } else { RED },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
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
        assert!(line(format_args!("FPS {} AVG {}", rate(Some(999.99)), rate(Some(999.99)))).len() * 6 <= 152);
        assert!(line(format_args!("W C0 {} C1 {}", work(Some(1000)), work(Some(1000)))).len() * 6 <= 144);
        let max = memory(Some(MemoryUse {
            used: 524288,
            total: 524288,
        }));
        assert!(line(format_args!("Static {max} SP0 {max} SP1 {max}")).len() * 6 <= 300);
    }
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
    fn lines(p: &Probe) -> std::vec::Vec<std::string::String> {
        let mut lines = std::vec::Vec::new();
        details(p, 0, &mut |s| lines.push(s.into()));
        lines
    }
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
    #[test]
    fn all_detail_lines_fit_without_losing_value_bytes() {
        let mut p = probe();
        p.input(Input::Click, 0).unwrap();
        p.inspection
            .values
            .push(InfoValue {
                uuid: 0x2A29,
                value: Payload::new(&[0xFE; 64]),
                result: ReadResult::Ok,
            })
            .unwrap();
        let l = lines(&p);
        assert!(l.iter().all(|l| l.len() <= 50));
        assert_eq!(l.iter().filter(|l| l.starts_with("FE FE")).count(), 4);
    }

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
        details(&p, u64::MAX, &mut |s| l.push(s.into()));
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
}
