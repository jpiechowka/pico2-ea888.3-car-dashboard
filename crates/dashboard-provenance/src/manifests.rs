use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::{Hashes, Result, parse, read_manifest, tree, verify_archive};
const HAL_VERSION: &str = "0.10.0";
const HAL_HASH: &str = "5d98f472894c1ecffac5596c657af5305c57282d29e8746d7fec033951931bc8";
const COMMIT: &str = "84444a19eb57d978e3c09fcdc8e60cdd7278eb03";
const HAL_PATCHES: [&str; 34] = [
    "Cargo.lock",
    "Cargo.toml",
    "Cargo.toml.orig",
    "LOCAL_PATCHES.md",
    "src/aon_timer/mod.rs",
    "src/block.rs",
    "src/clocks.rs",
    "src/datetime/datetime_chrono.rs",
    "src/datetime/datetime_no_deps.rs",
    "src/datetime/epoch.rs",
    "src/flash.rs",
    "src/flash_probe.rs",
    "src/gpio.rs",
    "src/i2c_slave.rs",
    "src/lib.rs",
    "src/multicore.rs",
    "src/pio/mod.rs",
    "src/pio_programs/clk.rs",
    "src/pio_programs/clock_divider.rs",
    "src/pio_programs/i2s.rs",
    "src/pio_programs/mod.rs",
    "src/pio_programs/onewire.rs",
    "src/pio_programs/pwm.rs",
    "src/pio_programs/spi.rs",
    "src/pio_programs/stepper.rs",
    "src/pio_programs/uart.rs",
    "src/pio_programs/ws2812.rs",
    "src/psram.rs",
    "src/pwm.rs",
    "src/rom_data/rp235x.rs",
    "src/trng.rs",
    "src/uart/buffered.rs",
    "src/uart/buffered_io.rs",
    "src/uart/mod.rs",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Backport {
    upstream_commit: String,
    upstream_url: String,
    scope: String,
    fragment_sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hal {
    #[serde(rename = "crate")]
    name: String,
    version: String,
    url: String,
    archive_sha256: String,
    upstream_commit: String,
    original_files_sha256: Hashes,
    patched_files_sha256: Hashes,
    compatibility_backports: BTreeMap<String, Backport>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Radio {
    #[serde(rename = "crate")]
    name: String,
    version: String,
    source: String,
    archive_sha256: String,
    original: Hashes,
    modified: Hashes,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Firmware {
    repository: String,
    revision: String,
    directory: String,
    files: Hashes,
    modified: Hashes,
    bluetooth_update: BluetoothUpdate,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BluetoothUpdate {
    repository: String,
    revision: String,
    path: String,
    array: String,
    source_sha256: String,
    blob_sha256: String,
    bytes: u32,
}
fn validate_firmware_manifest(firmware: &Firmware) -> Result<()> {
    let update = &firmware.bluetooth_update;
    if firmware.repository != "https://github.com/embassy-rs/embassy"
        || firmware.revision != COMMIT
        || firmware.directory != "cyw43-firmware"
    {
        return Err("unexpected radio firmware revision".into());
    }
    let names = [
        "43439A0.bin",
        "43439A0_btfw.bin",
        "nvram_rp2040.bin",
        "README.md",
        "LICENSE-permissive-binary-license-1.0.txt",
    ];
    if firmware.files.0.len() != names.len() || names.iter().any(|name| !firmware.files.0.contains_key(*name)) {
        return Err("radio firmware inventory".into());
    }
    if update.repository != "https://github.com/georgerobotics/cyw43-driver"
        || update.revision != "bf3fa8ad5f61a30c1b4e7ee5ce86f7dbd8b8d2e4"
        || update.path != "firmware/cyw43_btfw_43439.h"
        || update.array != "cyw43_btfw_43439"
        || update.source_sha256 != "d05a8dca7e232ada470d8ed723edaa1ea78000098870f9b979d490d83ad35b22"
        || update.blob_sha256 != "d974384d0aa8124d30084f30d13519a3ed3b9f759f7ddd6a34ef3dcf1441bcf7"
        || update.bytes != 6970
        || firmware.modified.0.len() != 2
        || !firmware.modified.0.contains_key("README.md")
        || firmware.modified.0.get("43439A0_btfw.bin") != Some(&update.blob_sha256)
        || firmware.files.0.get("43439A0_btfw.bin").map(String::as_str)
            != Some("ce1992c1a6a16ae51bc012439486e9fb212623eca92d9e82a8090c2acf7ef1df")
    {
        return Err("unexpected Bluetooth firmware update".into());
    }
    Ok(())
}
fn source_url(
    name: &str,
    version: &str,
) -> String {
    format!("https://static.crates.io/crates/{name}/{name}-{version}.crate")
}
pub fn check_hal(
    root: &Path,
    archive: Option<&Path>,
) -> Result<String> {
    let manifest: Hal = parse(&read_manifest(&root.join("embassy-rp-provenance.json"))?)?;
    if manifest.name != "embassy-rp"
        || manifest.version != HAL_VERSION
        || manifest.archive_sha256 != HAL_HASH
        || manifest.url != source_url("embassy-rp", HAL_VERSION)
        || manifest.upstream_commit != COMMIT
    {
        return Err("unexpected HAL source release".into());
    }
    if manifest.patched_files_sha256.0.len() != HAL_PATCHES.len()
        || HAL_PATCHES
            .iter()
            .any(|path| !manifest.patched_files_sha256.0.contains_key(*path))
    {
        return Err("unexpected HAL patch inventory".into());
    }
    for backport in manifest.compatibility_backports.values() {
        if backport.upstream_commit.len() != 40
            || !backport
                .upstream_commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || backport.upstream_url
                != format!(
                    "https://github.com/embassy-rs/embassy/commit/{}",
                    backport.upstream_commit
                )
            || backport.scope.is_empty()
        {
            return Err("invalid backport metadata".into());
        }
        if let Some(hash) = &backport.fragment_sha256 {
            crate::valid_hash(hash)?;
        }
    }
    tree(
        &root.join("embassy-rp"),
        &manifest.original_files_sha256,
        &manifest.patched_files_sha256,
        // Native HAL regression builds have the same top-level Cargo output
        // boundary as the radio crates. Links are rejected; nested targets
        // remain inventoried source, not ignored build output.
        true,
    )?;
    if let Some(archive) = archive {
        let maps=serde_json::to_vec(&serde_json::json!({"original":manifest.original_files_sha256.0,"modified":manifest.patched_files_sha256.0})).map_err(|e| e.to_string())?;
        verify_archive(archive, &format!("embassy-rp-{HAL_VERSION}"), HAL_HASH, &maps)?;
    }
    let modified = manifest
        .patched_files_sha256
        .0
        .keys()
        .filter(|key| manifest.original_files_sha256.0.contains_key(*key))
        .count();
    Ok(format!(
        "Verified {} original files, {modified} modified files and {} added helpers; archive={}",
        manifest.original_files_sha256.0.len(),
        manifest.patched_files_sha256.0.len() - modified,
        if archive.is_some() { "checked" } else { "not supplied" }
    ))
}
pub fn check_radio(root: &Path) -> Result<String> {
    let mut output = String::new();
    for (name, version, hash) in [
        (
            "cyw43",
            "0.7.0",
            "033e856bf6f38c96fee3e3f398cceff0ca2c2fae5db255bcd23495216c7047da",
        ),
        (
            "trouble-host",
            "0.8.0",
            "19a566cc37ceaff5cc2a6ff8cc02debd1ee6b95d59ced8830d005da8b080cbe9",
        ),
    ] {
        let manifest: Radio = parse(&read_manifest(&root.join(format!("{name}-provenance.json")))?)?;
        if manifest.name != name
            || manifest.version != version
            || manifest.archive_sha256 != hash
            || manifest.source != source_url(name, version)
        {
            return Err("unexpected radio source release".into());
        }
        tree(&root.join(name), &manifest.original, &manifest.modified, true)?;
        output.push_str(&format!(
            "{name} {version}: complete upstream tree and reviewed overlay verified\n"
        ));
    }
    let firmware: Firmware = parse(&read_manifest(&root.join("cyw43-firmware-provenance.json"))?)?;
    validate_firmware_manifest(&firmware)?;
    tree(&root.join("cyw43-firmware"), &firmware.files, &firmware.modified, true)?;
    output.push_str("CYW firmware, NVRAM and license bytes verified");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_update_binds_exact_origin_extraction_and_overlay() {
        let baseline: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../firmware/vendor/cyw43-firmware-provenance.json"
        )))
        .unwrap();
        validate_firmware_manifest(&serde_json::from_value(baseline.clone()).unwrap()).unwrap();
        for field in [
            "repository",
            "revision",
            "path",
            "array",
            "source_sha256",
            "blob_sha256",
        ] {
            let mut changed = baseline.clone();
            changed["bluetooth_update"][field] = "other".into();
            assert!(
                validate_firmware_manifest(&serde_json::from_value(changed).unwrap()).is_err(),
                "{field}"
            );
        }
        for field in ["repository", "revision", "directory"] {
            let mut changed = baseline.clone();
            changed[field] = "other".into();
            assert!(
                validate_firmware_manifest(&serde_json::from_value(changed).unwrap()).is_err(),
                "{field}"
            );
        }
        for fault in 0..6 {
            let mut changed = baseline.clone();
            match fault {
                0 => changed["modified"]["43439A0_btfw.bin"] = "0".repeat(64).into(),
                1 => {
                    changed["modified"].as_object_mut().unwrap().remove("README.md");
                }
                2 => changed["modified"]["nvram_rp2040.bin"] = "0".repeat(64).into(),
                3 => changed["bluetooth_update"]["bytes"] = 6164.into(),
                4 => {
                    changed["files"].as_object_mut().unwrap().remove("43439A0.bin");
                }
                _ => changed["files"]["43439A0_btfw.bin"] = "0".repeat(64).into(),
            }
            assert!(
                validate_firmware_manifest(&serde_json::from_value(changed).unwrap()).is_err(),
                "{fault}"
            );
        }
    }
}
