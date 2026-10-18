//! Explicit BOOTSEL information reads; no persistent programming or reboot.
use std::collections::BTreeMap;
use std::path::Path;

use dashboard_helper_protocol::*;
fn lines(raw: &[u8]) -> Result<Vec<&str>, String> {
    if raw.len() > 65536 {
        return Err("Information output bound".into());
    }
    let text = std::str::from_utf8(raw).map_err(|_| "Invalid information UTF-8")?;
    if text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r')) {
        return Err("Information control character".into());
    }
    let lines = text.lines().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    if lines.len() > 128 || lines.iter().any(|s| s.len() > 1024) {
        return Err("Information line bound".into());
    }
    Ok(lines)
}
fn hex(
    value: &str,
    n: usize,
) -> bool {
    value.len() == n && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn parse(
    info: &[u8],
    partition: &[u8],
) -> Result<BootselFacts, String> {
    let mut fields = BTreeMap::new();
    let mut header = false;
    for line in lines(info)? {
        if line == "Device Information" {
            if header {
                return Err("Multiple device groups".into());
            }
            header = true;
            continue;
        }
        if !header {
            return Err("Unexpected information group or warning".into());
        }
        let (key, value) = line
            .trim_matches(' ')
            .split_once(':')
            .ok_or("Malformed information field")?;
        let key = key.trim_end_matches(' ');
        let value = value.trim_matches(' ');
        if !matches!(
            key,
            "type"
                | "revision"
                | "package"
                | "chipid"
                | "flash devinfo"
                | "current cpu"
                | "available cpus"
                | "default cpu"
                | "secure boot"
                | "debug enable"
                | "secure debug enable"
                | "boot_random"
                | "boot type"
                | "last booted partition"
                | "explicit buy pending"
                | "other slot/partition erased"
                | "OTP version applied"
                | "diagnostic source"
                | "last boot diagnostics"
                | "reboot param 0"
                | "reboot param 1"
                | "rom gitrev"
                | "flash size"
        ) {
            return Err("Unsupported information field or warning".into());
        }
        if value.is_empty() || fields.insert(key, value).is_some() || fields.len() > 32 {
            return Err("Duplicate or empty information field".into());
        }
    }
    let field = |key| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| format!("Missing device field: {key}"))
    };
    if field("type")? != "RP2350" || !matches!(field("revision")?, "A2" | "A3" | "A4") {
        return Err("Unsupported chip model or revision".into());
    }
    let chip = field("chipid")?;
    if !chip.starts_with("0x") || !hex(&chip[2..], 16) {
        return Err("Invalid chip identity".into());
    }
    let random = field("boot_random")?;
    if random.len() != 35 || random.split(':').count() != 4 || !random.split(':').all(|p| hex(p, 8)) {
        return Err("Invalid boot observation".into());
    }
    if !matches!(field("available cpus")?, "ARM" | "ARM, RISC-V")
        || field("secure boot")? != "0"
        || field("debug enable")? != "1"
        || field("secure debug enable")? != "1"
    {
        return Err("Unsupported security or CPU policy".into());
    }
    let devinfo = field("flash devinfo")?;
    if !devinfo.starts_with("0x") || devinfo.len() != 6 || !hex(&devinfo[2..], 4) {
        return Err("Invalid flash configuration observation".into());
    }
    let partition = lines(partition)?;
    if partition.len() != 2 || partition[0] != "there is no partition table" {
        return Err("Partition state is present or unavailable".into());
    }
    let families = partition[1]
        .strip_prefix("un-partitioned_space :  S(rw) NSBOOT(rw) NS(rw), uf2 { ")
        .and_then(|s| s.strip_suffix(" }"))
        .ok_or("Unsupported unpartitioned permissions")?;
    let allowed = [
        "absolute",
        "rp2040",
        "rp2350-arm-s",
        "rp2350-arm-ns",
        "rp2350-riscv",
        "data",
    ];
    let mut previous = None;
    let mut secure = false;
    for family in families.split(", ") {
        let index = allowed.iter().position(|f| *f == family).ok_or("Unknown UF2 family")?;
        if previous.is_some_and(|p| p >= index) {
            return Err("Duplicate or reordered UF2 families".into());
        }
        previous = Some(index);
        secure |= family == "rp2350-arm-s";
    }
    if !secure {
        return Err("Secure ARM UF2 family unavailable".into());
    }
    let estimate = match fields.get("flash size") {
        None => None,
        Some(value) => {
            let number = value.strip_suffix('K').ok_or("Flash size estimate unavailable")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Invalid flash estimate".into());
            }
            let size = number
                .parse::<u64>()
                .map_err(|_| "Flash estimate overflow")?
                .checked_mul(1024)
                .ok_or("Flash estimate overflow")?;
            if size == 0 || size > 16 * 1024 * 1024 {
                return Err("Flash estimate unsupported".into());
            }
            Some(size)
        }
    };
    Ok(BootselFacts {
        revision: Text::new(field("revision")?).unwrap(),
        chip_id: Text::new(chip).unwrap(),
        boot_random: Text::new(random).unwrap(),
        available_cpus: Text::new(field("available cpus")?).unwrap(),
        secure_boot: false,
        debug_enabled: true,
        secure_debug_enabled: true,
        unpartitioned_families: Text::new(families).map_err(|e| e.to_string())?,
        flash_size_estimate: estimate,
    })
}
fn same_descriptor(
    repo: &Path,
    session: &SessionId,
    selected: &DeviceSummary,
) -> Result<(), String> {
    let scan = crate::devices::scan(repo, session.clone());
    if !scan.complete || scan.devices.iter().any(|d| d.status != DeviceStatus::DescriptorOnly) {
        return Err("Descriptor snapshot incomplete or ambiguous".into());
    }
    let matches = scan
        .devices
        .iter()
        .filter(|d| d.descriptor_sha256 == selected.descriptor_sha256 && d.serial == selected.serial)
        .count();
    if matches != 1 {
        return Err("Selected descriptor changed or disappeared".into());
    }
    Ok(())
}
pub(crate) fn inspect(
    repo: &Path,
    session: SessionId,
    selected: DeviceSummary,
) -> DeviceInspection {
    let mut report = DeviceInspection {
        session_id: session.clone(),
        device_id: selected.device_id.clone(),
        descriptor_sha256: selected.descriptor_sha256.clone(),
        complete: false,
        facts: None,
        information_sha256: None,
        reason: Text::new("Device inspection incomplete").unwrap(),
        capacity: DeviceCapacity {
            state: DeviceCapacityState::Unverified,
            bytes: None,
            provenance: None,
        },
        flash_ready: false,
    };
    let result = crate::process::with_probe_deadline(
        std::time::Duration::from_secs(30),
        || -> Result<BootselFacts, String> {
            if selected.status != DeviceStatus::DescriptorOnly {
                return Err("Descriptor is not uniquely selectable".into());
            }
            let serial = selected.serial.as_ref().ok_or("Missing device serial")?.as_str();
            if !serial.is_ascii() || serial.starts_with('-') || serial.chars().any(char::is_control) {
                return Err("Unsupported serial syntax".into());
            }
            same_descriptor(repo, &session, &selected)?;
            let tool = repo.join("target/configurator-tools/bin/picotool.exe");
            let _tool = crate::artifacts::converter(repo)?;
            let information = crate::process::run(
                &tool,
                &[
                    "info", "-d", "--debug", "--vid", "0x2e8a", "--pid", "0x000f", "--ser", serial,
                ],
                repo,
            )?;
            if information.code != Some(0) || !information.raw_stderr.is_empty() {
                return Err("Descriptor visible but picotool information unavailable".into());
            }
            let partitions = crate::process::run(
                &tool,
                &[
                    "partition",
                    "info",
                    "--vid",
                    "0x2e8a",
                    "--pid",
                    "0x000f",
                    "--ser",
                    serial,
                ],
                repo,
            )?;
            if partitions.code != Some(0) || !partitions.raw_stderr.is_empty() {
                return Err("Partition information unavailable".into());
            }
            let facts = parse(&information.raw_stdout, &partitions.raw_stdout)?;
            same_descriptor(repo, &session, &selected)?;
            Ok(facts)
        },
    );
    match result {
        Ok(facts) => {
            report.information_sha256 =
                Some(Sha256::new(crate::build_input::digest(&serde_json::to_vec(&facts).unwrap())).unwrap());
            report.facts = Some(facts);
            report.complete = true;
            report.reason = Text::new(
                "Information snapshot matched before/after descriptors; capacity unverified. No continuous attachment \
                 or hardware qualification claimed",
            )
            .unwrap();
        }
        Err(reason) => report.reason = crate::diagnostics::summary(&reason),
    }
    report
}
#[cfg(test)]
mod tests {
    use super::*;
    const INFO: &str = "Device Information\n type: RP2350\n revision: A3\n chipid: 0x0123456789abcdef\n flash \
                        devinfo: 0x1234\n available cpus: ARM, RISC-V\n secure boot: 0\n debug enable: 1\n secure \
                        debug enable: 1\n boot_random: 00000001:00000002:00000003:00000004\n flash size: 4096K\n";
    const PART: &str = "there is no partition table\nun-partitioned_space :  S(rw) NSBOOT(rw) NS(rw), uf2 { \
                        rp2350-arm-s, rp2350-arm-ns }\n";
    #[test]
    fn exact_separate_identity_and_capacity_observation() {
        let facts = parse(INFO.as_bytes(), PART.as_bytes()).unwrap();
        assert_eq!(facts.chip_id.as_str(), "0x0123456789abcdef");
        assert_eq!(facts.flash_size_estimate, Some(4194304));
    }
    #[test]
    fn duplicates_missing_security_and_partitions_fail() {
        for bad in [
            INFO.replace(" revision: A3\n", ""),
            INFO.replace("secure boot: 0", "secure boot: 1"),
            format!("{INFO} chipid: 0x0123456789abcdef\n"),
            INFO.replace("RP2350", "RP2350(64k)"),
        ] {
            assert!(parse(bad.as_bytes(), PART.as_bytes()).is_err());
        }
        assert!(
            parse(
                INFO.as_bytes(),
                PART.replace("there is no partition table", "the partition table is empty")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(parse(INFO.as_bytes(), PART.replace("NS(rw)", "NS(r)").as_bytes()).is_err());
    }
}
