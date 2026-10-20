//! Human-readable labels for saved evidence, never authenticated device identity.
use dashboard_core::discovery::capture::{CaptureView, PacketEvidence, Record};
use dashboard_core::discovery::{AdFields, ReadResult};
use serde_json::{Value, json};

use super::{PublishedCapture, hex};

struct Name<'a> {
    bytes: &'a [u8],
    source: &'static str,
    complete: bool,
    truncated: bool,
    record_index: Option<usize>,
}

fn packet_name<'a>(
    packet: &'a PacketEvidence,
    source: &'static str,
    kind: u8,
) -> Option<Name<'a>> {
    // AdFields stops at malformed lengths or a zero terminator. Keep the raw
    // packet in the export even when only its valid prefix supplies a name.
    AdFields::new(packet.bytes())
        .flatten()
        .find(|field| field.kind == kind && !field.data.is_empty())
        .map(|field| {
            Name {
                bytes: field.data,
                source,
                complete: kind == 9,
                truncated: packet.truncated,
                record_index: None,
            }
        })
}

fn selected_name<'a>(view: &'a CaptureView<'_>) -> Option<Name<'a>> {
    let m = view.metadata();
    let packets = [(&m.response, "scan_response"), (&m.advertisement, "advertisement")];
    packets
        .iter()
        .find_map(|(p, source)| packet_name(p, source, 9))
        .or_else(|| {
            view.records().enumerate().find_map(|(index, record)| {
                match record {
                    // Capture admission already binds this allowlisted read to its GAP
                    // service and characteristic. No additional read is performed here.
                    Record::Value(v)
                        if v.uuid == 0x2A00 && v.result == ReadResult::Ok && !v.truncated && !v.bytes.is_empty() =>
                    {
                        Some(Name {
                            bytes: v.bytes,
                            source: "gap_device_name",
                            complete: true,
                            truncated: false,
                            record_index: Some(index),
                        })
                    }
                    _ => None,
                }
            })
        })
        .or_else(|| packets.iter().find_map(|(p, source)| packet_name(p, source, 8)))
}

fn slug(name: Option<&str>) -> String {
    let mut slug = String::with_capacity(32);
    for c in name.unwrap_or("").chars() {
        if slug.len() == 32 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    if slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("unnamed");
    }
    slug
}

/// `digest` is the complete lowercase SHA256 of the admitted binary, including
/// frozen metadata. The short suffix is a label; use the full ID for comparison.
pub(super) fn derive(
    view: &CaptureView<'_>,
    digest: &str,
) -> (Value, PublishedCapture) {
    let peer = view.metadata().peer;
    let name = selected_name(view);
    let utf8 = name.as_ref().and_then(|n| std::str::from_utf8(n.bytes).ok());
    let device_key = format!("{}-{}", if peer.random { "r" } else { "p" }, hex(&peer.address));
    let short_id = &digest[..16];
    let capture_id = format!("sha256:{digest}");
    // Fixed prefix and ASCII slug avoid reserved Windows names, path separators,
    // controls and trailing dots/spaces, including names such as CON or ../NUL.
    let file_stem = format!("blecap-{}-{device_key}-{short_id}", slug(utf8));
    let identity = json!({
        "version": 1, "capture_id": capture_id, "short_id": short_id,
        "device_key": device_key, "file_stem": file_stem,
        "device_name": name.as_ref().map(|n| json!({
            "source": n.source, "raw_hex": hex(n.bytes), "utf8": utf8,
            "complete": n.complete, "truncated": n.truncated, "record_index": n.record_index,
        })),
        "scope": "Observed address and advertised/read name are untrusted labels, not permanent physical identity. The content hash identifies captured bytes, not authenticity."
    });
    (identity, PublishedCapture { file_stem, capture_id })
}
