//! Human-readable labels for saved evidence, never authenticated device identity.
use dashboard_core::discovery::capture::CaptureView;
use dashboard_core::discovery::names::capture_name;
use serde_json::{Value, json};

use super::{PublishedCapture, hex};

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
    let name = capture_name(view);
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
            "source": n.source.label(), "raw_hex": hex(n.bytes), "utf8": utf8,
            "complete": n.complete, "truncated": n.truncated, "record_index": n.record_index,
            "cached": n.cached, "observed_at_boot_ms": n.observed_at_ms,
        })),
        "scope": "Observed address and advertised/read name are untrusted labels, not permanent physical identity. The content hash identifies captured bytes, not authenticity."
    });
    (identity, PublishedCapture { file_stem, capture_id })
}
