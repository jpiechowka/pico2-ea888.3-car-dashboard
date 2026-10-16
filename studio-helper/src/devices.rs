//! Descriptor-only BOOTSEL discovery. Never opens a PICOBOOT interface or runs picotool.
use std::path::Path;

use dashboard_helper_protocol::*;
fn text<const N: usize>(s: Option<&str>) -> Option<Text<N>> {
    s.filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .and_then(|s| Text::new(s).ok())
}
pub(crate) fn empty(
    session: SessionId,
    reason: &str,
) -> DeviceScanReport {
    DeviceScanReport {
        schema_version: 1,
        session_id: session,
        complete: false,
        reason: Text::new(reason).unwrap(),
        devices: BoundedVec::new(vec![]).unwrap(),
    }
}
fn id() -> Result<DeviceId, String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "OS entropy unavailable")?;
    DeviceId::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).map_err(|e| e.to_string())
}
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
            reason: Text::new("BOOTSEL VID/PID observed; descriptor identity only, not confirmed Pico 2 hardware")
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
pub(crate) fn scan(
    repo: &Path,
    session: SessionId,
) -> DeviceScanReport {
    let result = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|exe| crate::process::run(&exe, &["--internal-device-descriptors"], repo))
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
        Err(e) => {
            empty(
                session,
                &format!("Descriptor scan failed: {}", e.chars().take(350).collect::<String>()),
            )
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
    fn missing_and_duplicate_serials_remain_unqualified() {
        let mut devices = vec![device(Some("same")), device(Some("same")), device(None)];
        qualify(&mut devices);
        assert_eq!(devices[0].status, DeviceStatus::Ambiguous);
        assert_eq!(devices[1].status, DeviceStatus::Ambiguous);
        assert_eq!(devices[2].status, DeviceStatus::Unsupported);
        assert!(devices.iter().all(|d| !d.flash_ready && d.capacity.bytes.is_none()));
    }
    #[test]
    fn descriptor_controls_and_oversize_are_not_displayed() {
        assert!(text::<126>(Some("bad\nserial")).is_none());
        assert!(text::<126>(Some(&"a".repeat(127))).is_none());
    }
}
