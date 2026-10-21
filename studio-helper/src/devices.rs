//! Descriptor-only BOOTSEL discovery. Never opens a PICOBOOT interface or runs picotool.
use std::path::Path;

use dashboard_helper_protocol::*;
/// Returns nonempty control-free descriptor `s` within `N` UTF-8 bytes, or `None` for absent/invalid/oversized text.
fn text<const N: usize>(s: Option<&str>) -> Option<Text<N>> {
    s.filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .and_then(|s| Text::new(s).ok())
}
/// Returns an incomplete empty descriptor report for `session` with bounded diagnostic `reason`.
/// No absence-of-device or capacity conclusion follows from this failure report.
pub(crate) fn empty(
    session: SessionId,
    reason: &str,
) -> DeviceScanReport {
    DeviceScanReport {
        schema_version: 1,
        session_id: session,
        complete: false,
        reason: crate::diagnostics::summary(reason),
        devices: BoundedVec::new(vec![]).unwrap(),
    }
}
/// Returns a fresh 128-bit lowercase-hex selection identity, or OS entropy/protocol validation error.
/// This ephemeral ID is not a USB serial, chip identity or proof of continued attachment.
fn id() -> Result<DeviceId, String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "OS entropy unavailable")?;
    DeviceId::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).map_err(|e| e.to_string())
}
/// Marks duplicate-serial `devices` Ambiguous and absent/non-ASCII/option-like serials Unsupported.
/// Caller has removed control-bearing text; uniquely selectable descriptors remain DescriptorOnly without hardware
/// qualification.
fn qualify(devices: &mut [DeviceSummary]) {
    for i in 0..devices.len() {
        let serial = devices[i].serial.as_ref().map(|s| s.as_str());
        let ambiguous = serial.is_some_and(|s| {
            devices
                .iter()
                .enumerate()
                .any(|(j, d)| i != j && d.serial.as_ref().is_some_and(|v| v.as_str() == s))
        });
        if ambiguous {
            devices[i].status = DeviceStatus::Ambiguous;
            devices[i].reason = Text::new("Duplicate descriptor serial; device identity is ambiguous").unwrap();
        } else if serial.is_none_or(|s| !s.is_ascii() || s.starts_with('-')) {
            devices[i].status = DeviceStatus::Unsupported;
            devices[i].reason =
                Text::new("Missing or unsupported descriptor serial; no device command is authorized").unwrap();
        }
    }
}
/// Returns SHA-256 over length-prefixed bounded descriptor/topology identity from `d`, including Windows
/// instance/location data. Oversized fields/location count return an error; descriptor identity can change on
/// reattachment and does not measure flash.
fn descriptor_digest(d: &nusb::DeviceInfo) -> Result<Sha256, String> {
    let mut bytes = Vec::new();
    let mut add = |value: &[u8], maximum: usize| -> Result<(), String> {
        if value.len() > maximum {
            return Err("Descriptor identity bound".into());
        }
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value);
        Ok(())
    };
    add(&d.vendor_id().to_le_bytes(), 2)?;
    add(&d.product_id().to_le_bytes(), 2)?;
    add(d.serial_number().unwrap_or("").as_bytes(), 126)?;
    add(d.bus_id().as_bytes(), 1024)?;
    add(d.port_chain(), 16)?;
    add(&[d.device_address()], 1)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        for (value, max) in [(d.instance_id(), 400), (d.parent_instance_id(), 400)] {
            let raw = value
                .encode_wide()
                .take(max / 2 + 1)
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            add(&raw, max)?;
        }
        if d.location_paths().len() > 8 {
            return Err("Device location count bound".into());
        }
        for value in d.location_paths() {
            let raw = value
                .encode_wide()
                .take(1025)
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            add(&raw, 2048)?;
        }
    }
    Sha256::new(crate::build_input::digest(&bytes)).map_err(|e| e.to_string())
}
/// Returns up to 16 RP2350 BOOTSEL VID/PID observations using blocking nusb descriptor enumeration.
/// More matches return an incomplete empty report; enumeration/entropy/identity/schema errors return `Err`.
/// No PICOBOOT interface is opened. Completeness covers returned library observations, not continuous attachment
/// or an assertion that the OS/library exposed every physical device. Caller runs this in an owned child for timeout
/// control.
fn enumerate() -> Result<DeviceScanReport, String> {
    use nusb::MaybeFuture;
    let mut report = empty(
        SessionId::new("0".repeat(32)).unwrap(),
        "Descriptor observation only; capacity, model, security and partition state are unverified",
    );
    let mut devices = Vec::new();
    for d in nusb::list_devices()
        .wait()
        .map_err(|e| format!("Descriptor enumeration failed: {e}"))?
    {
        if d.vendor_id() != 0x2E8A || d.product_id() != 0x000F {
            continue;
        }
        if devices.len() == 16 {
            return Ok(empty(
                report.session_id,
                "More than 16 BOOTSEL descriptors; scan is incomplete and no devices are admitted",
            ));
        }
        devices.push(DeviceSummary {
            device_id: id()?,
            descriptor_sha256: descriptor_digest(&d)?,
            vid: d.vendor_id(),
            pid: d.product_id(),
            serial: text(d.serial_number()),
            product: text(d.product_string()),
            status: DeviceStatus::DescriptorOnly,
            reason: Text::new("BOOTSEL VID/PID observed; descriptor identity only, not confirmed Pico 2 W hardware")
                .unwrap(),
            capacity: DeviceCapacity {
                state: DeviceCapacityState::Unverified,
                bytes: None,
                provenance: None,
            },
            flash_ready: false,
        });
    }
    qualify(&mut devices);
    report.complete = true;
    report.devices = BoundedVec::new(devices).map_err(|e| e.to_string())?;
    report.validate().map_err(|e| e.to_string())?;
    Ok(report)
}
/// Fixed child entrypoint is isolated because nusb's Windows enumeration is blocking.
/// Returns exit 0 only after encoding/writing a descriptor report to stdout; otherwise emits a generic stderr failure
/// and returns 1.
pub fn enumerate_child() -> i32 {
    match enumerate().and_then(|r| encode(&r).map_err(|e| e.to_string())) {
        Ok(bytes) => {
            use std::io::Write;
            if std::io::stdout().write_all(&bytes).is_ok() {
                0
            } else {
                1
            }
        }
        Err(_) => {
            eprintln!("Descriptor enumeration failed; no devices admitted");
            1
        }
    }
}
/// Returns a descriptor report bound to `session`, running this executable's fixed child mode in `repo` under
/// `process`. Child/timeout/cancellation/schema failures become bounded incomplete reports; decoded raw output
/// preserves exact wire values. This scan never runs picotool, measures capacity or authorizes a device command.
pub(crate) fn scan(
    process: &mut crate::process::ProcessContext,
    repo: &Path,
    session: SessionId,
) -> DeviceScanReport {
    let result = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|exe| process.run(&exe, &["--internal-device-descriptors"], repo))
        .and_then(|o| {
            if o.code != Some(0) {
                return Err(format!("Descriptor child exited {:?}: {}", o.code, o.stderr));
            }
            decode::<DeviceScanReport>(&o.raw_stdout).map_err(|e| format!("Invalid descriptor report: {e}"))
        });
    match result {
        Ok(mut report) => {
            report.session_id = session;
            report
        }
        Err(e) => empty(session, &format!("Descriptor scan failed: {e}")),
    }
}
#[cfg(test)]
mod tests {
    //! Descriptor filtering, ambiguous selection and bounded failure-text regressions.
    use super::*;
    #[test]
    /// Long multibyte error text is bounded safely while preserving an explicit incomplete report.
    fn failure_reason_accepts_long_unicode_without_panicking() {
        for value in ["é", "界", "😀"] {
            let report = empty(
                SessionId::new("0".repeat(32)).unwrap(),
                &format!("Descriptor scan failed: {}", value.repeat(350)),
            );
            assert!(!report.complete);
            assert!(report.reason.as_str().len() <= 512);
            assert!(report.reason.as_str().starts_with("Descriptor scan failed: "));
            assert!(report.reason.as_str().ends_with("..."));
            report.validate().unwrap();
        }
    }
    /// Returns an unqualified BOOTSEL fixture with filtered optional `serial` and fresh selection identity.
    fn device(serial: Option<&str>) -> DeviceSummary {
        DeviceSummary {
            device_id: id().unwrap(),
            descriptor_sha256: Sha256::new("1".repeat(64)).unwrap(),
            vid: 0x2E8A,
            pid: 0xF,
            serial: text(serial),
            product: None,
            status: DeviceStatus::DescriptorOnly,
            reason: Text::new("descriptor").unwrap(),
            capacity: DeviceCapacity {
                state: DeviceCapacityState::Unverified,
                bytes: None,
                provenance: None,
            },
            flash_ready: false,
        }
    }
    #[test]
    /// Duplicate and missing serials remain unselectable with no invented capacity or readiness.
    fn missing_and_duplicate_serials_remain_unqualified() {
        let mut devices = vec![device(Some("same")), device(Some("same")), device(None)];
        qualify(&mut devices);
        assert_eq!(devices[0].status, DeviceStatus::Ambiguous);
        assert_eq!(devices[1].status, DeviceStatus::Ambiguous);
        assert_eq!(devices[2].status, DeviceStatus::Unsupported);
        assert!(devices.iter().all(|d| !d.flash_ready && d.capacity.bytes.is_none()));
    }
    #[test]
    /// Control-bearing and oversized descriptor strings are omitted instead of rendered or truncated into new
    /// identities.
    fn descriptor_controls_and_oversize_are_not_displayed() {
        assert!(text::<126>(Some("bad\nserial")).is_none());
        assert!(text::<126>(Some(&"a".repeat(127))).is_none());
    }
}
