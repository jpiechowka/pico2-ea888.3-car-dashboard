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
        ProbeError::Timeout => "Timeout: no reply; Y back",
        ProbeError::Discovery => "Discovery: failed; Y back",
        ProbeError::Capacity => "Capacity: partial results",
        ProbeError::Disconnected => "Disconnected: peer left",
        ProbeError::Cleanup => "Cleanup: restarting",
        ProbeError::Transport => "Transport: restarting",
        ProbeError::InvalidPeer => "InvalidPeer: stale; rescan with Y",
    }
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
fn details(
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
    let i = &probe.inspection;
    emit("GATT / EXPLICIT INSPECTION");
    emit(if i.complete && i.omitted == 0 && i.malformed == 0 {
        "Enumeration ended"
    } else {
        "Partial / not completed"
    });
    emit(&line(format_args!("Omitted {} / malformed {}", i.omitted, i.malformed)));
    emit(&line(format_args!(
        "{} primary services / {} characteristics",
        i.services.len(),
        i.characteristics.len()
    )));
    if i.services.is_empty() && i.characteristics.is_empty() {
        emit("Click encoder to inspect selected peer");
    }
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
    if !probe.encoder_connected {
        text(d, 10, 25, "ENCODER MISSING - use A/B/X/Y", RED)?;
    } else {
        text(d, 10, 25, "Connectable BLE / explicit inspection", MUTED)?;
    }
    text(
        d,
        10,
        39,
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
        52,
        &line(format_args!(
            "{} bad:{}",
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
            let y = 70 + (row - start) as i32 * 32;
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
            67,
            &line(format_args!("DETAILS   page {} / {}", page + 1, pages)),
            RED,
        )?;
        let mut index = 0;
        let mut error = None;
        details(probe, now_ms, &mut |s| {
            if (page * LINES..(page + 1) * LINES).contains(&index)
                && error.is_none()
                && let Err(e) = text(d, 10, 81 + (index % LINES) as i32 * 12, s, WHITE)
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
    text(d, 8, 206, "Turn / A B: scroll   Click: Inspect", WHITE)?;
    text(d, 8, 217, "X: detail / inspect   Y: back / pause", MUTED)?;
    text(d, 8, 228, "Hold Y 2s: clear + recover scan", MUTED)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
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
            (ProbeError::Timeout, "Timeout", "reply"),
            (ProbeError::Connect, "Connect", "retry"),
            (ProbeError::Capacity, "Capacity", "partial"),
            (ProbeError::InvalidPeer, "InvalidPeer", "rescan"),
            (ProbeError::Cleanup, "Cleanup", "restarting"),
        ] {
            let hint = error_copy(error);
            assert!(hint.starts_with(code));
            assert!(hint.contains(clue));
            assert!(hint.len() + 15 <= 50);
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
    fn incomplete_or_malformed_inspection_never_claims_enumeration_ended() {
        let mut p = probe();
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
}
