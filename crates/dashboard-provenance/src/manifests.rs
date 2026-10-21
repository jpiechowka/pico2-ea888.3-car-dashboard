//! Repository-pinned HAL/radio identities and reviewed overlay admission.
//! Trust anchors live in source; manifest metadata cannot select a different release.
use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::{Hashes, Result, parse, read_manifest, tree, verify_archive};
/// Exact crates.io embassy-rp release admitted by this verifier.
const HAL_VERSION: &str = "0.10.0";
/// SHA256 of the immutable upstream HAL crate archive, independent of local overlays.
const HAL_HASH: &str = "5d98f472894c1ecffac5596c657af5305c57282d29e8746d7fec033951931bc8";
/// Embassy source revision associated with the pinned HAL and original radio firmware inventory.
const COMMIT: &str = "84444a19eb57d978e3c09fcdc8e60cdd7278eb03";
/// Exact reviewed HAL replacement/addition paths; update coherently with overlay provenance.
const HAL_PATCHES: [&str; 74] = [
    "Cargo.lock",
    "Cargo.toml",
    "Cargo.toml.orig",
    "LOCAL_PATCHES.md",
    "build.rs",
    "src/adc.rs",
    "src/aon_timer/mod.rs",
    "src/block.rs",
    "src/bootsel.rs",
    "src/clocks.rs",
    "src/critical_section_impl.rs",
    "src/datetime/datetime_chrono.rs",
    "src/datetime/datetime_no_deps.rs",
    "src/datetime/epoch.rs",
    "src/datetime/mod.rs",
    "src/dma.rs",
    "src/executor.rs",
    "src/executor_start.rs",
    "src/flash.rs",
    "src/flash_probe.rs",
    "src/flash_read.rs",
    "src/float/add_sub.rs",
    "src/float/cmp.rs",
    "src/float/conv.rs",
    "src/float/div.rs",
    "src/float/functions.rs",
    "src/float/mod.rs",
    "src/float/mul.rs",
    "src/fmt.rs",
    "src/gpio.rs",
    "src/i2c.rs",
    "src/i2c_slave.rs",
    "src/intrinsics.rs",
    "src/lib.rs",
    "src/multicore.rs",
    "src/multicore_stack.rs",
    "src/otp.rs",
    "src/pio/batch.rs",
    "src/pio/instr.rs",
    "src/pio/mod.rs",
    "src/pio_programs/clk.rs",
    "src/pio_programs/clock_divider.rs",
    "src/pio_programs/hd44780.rs",
    "src/pio_programs/i2s.rs",
    "src/pio_programs/mod.rs",
    "src/pio_programs/onewire.rs",
    "src/pio_programs/pwm.rs",
    "src/pio_programs/rotary_encoder.rs",
    "src/pio_programs/spi.rs",
    "src/pio_programs/stepper.rs",
    "src/pio_programs/uart.rs",
    "src/pio_programs/ws2812.rs",
    "src/psram.rs",
    "src/pwm.rs",
    "src/qmi_cs1.rs",
    "src/relocate.rs",
    "src/reset.rs",
    "src/rom_data/mod.rs",
    "src/rom_data/rp2040.rs",
    "src/rom_data/rp235x.rs",
    "src/rtc/conversions.rs",
    "src/rtc/filter.rs",
    "src/rtc/mod.rs",
    "src/spi.rs",
    "src/spi_divider.rs",
    "src/spinlock.rs",
    "src/spinlock_mutex.rs",
    "src/time_driver.rs",
    "src/trng.rs",
    "src/uart/buffered.rs",
    "src/uart/buffered_io.rs",
    "src/uart/mod.rs",
    "src/usb.rs",
    "src/watchdog.rs",
];
/// Traceable compatibility-backport description; syntax is checked, upstream content is not fetched.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Backport {
    /// Forty lowercase hexadecimal commit characters.
    upstream_commit: String,
    /// Embassy commit URL that must match upstream_commit exactly.
    upstream_url: String,
    /// Required nonempty description of the copied/backported behavior.
    scope: String,
    /// Optional lowercase SHA256 of a documented fragment; validated syntactically here.
    fragment_sha256: Option<String>,
}
/// HAL-specific strict JSON manifest with immutable release facts and reviewed source overlays.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hal {
    /// Must equal embassy-rp; JSON key is crate.
    #[serde(rename = "crate")]
    name: String,
    /// Must equal HAL_VERSION.
    version: String,
    /// Exact static.crates.io archive URL for the pinned name/version.
    url: String,
    /// Must equal the compiled-in immutable HAL archive digest.
    archive_sha256: String,
    /// Must equal the pinned Embassy revision.
    upstream_commit: String,
    /// Complete original upstream regular-file inventory, never rewritten by overlays.
    original_files_sha256: Hashes,
    /// Reviewed replacement/addition hashes with keys constrained by HAL_PATCHES.
    patched_files_sha256: Hashes,
    /// Named backport explanations; each value's commit URL/hash syntax is checked.
    compatibility_backports: BTreeMap<String, Backport>,
}
/// Strict manifest for one pinned Rust radio crate and its current local overlay.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Radio {
    /// cyw43 or trouble-host according to the current check iteration; JSON key is crate.
    #[serde(rename = "crate")]
    name: String,
    /// Exact version compiled into check_radio.
    version: String,
    /// Exact static.crates.io archive URL for that name/version.
    source: String,
    /// Expected pinned compressed archive digest; local-tree checks do not download the archive.
    archive_sha256: String,
    /// Complete original regular-file map.
    original: Hashes,
    /// Reviewed replacement/addition map overriding the original only for local-tree hashing.
    modified: Hashes,
}
/// Strict radio binary/NVRAM/license provenance and a separately pinned Bluetooth replacement.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Firmware {
    /// Exact upstream Embassy GitHub repository URL.
    repository: String,
    /// Original Embassy commit, pinned by COMMIT.
    revision: String,
    /// Must identify the cyw43-firmware directory.
    directory: String,
    /// Exact original five-file firmware/NVRAM/readme/license inventory.
    files: Hashes,
    /// Only the Bluetooth binary and README may replace original entries.
    modified: Hashes,
    /// Exact upstream array extraction identity for the current Bluetooth binary.
    bluetooth_update: BluetoothUpdate,
}
/// Source header/array and extracted-byte identities for the reviewed Bluetooth firmware update.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BluetoothUpdate {
    /// Pinned georgerobotics/cyw43-driver repository URL.
    repository: String,
    /// Exact reviewed upstream commit; not a floating branch/tag.
    revision: String,
    /// Repository-relative source header path.
    path: String,
    /// C array identifier extracted into the binary.
    array: String,
    /// SHA256 of the reviewed upstream header.
    source_sha256: String,
    /// SHA256 of the extracted Bluetooth binary, also required in the overlay map.
    blob_sha256: String,
    /// Exact expected binary length, 6970 bytes; origin metadata is checked here.
    bytes: u32,
}
/// Return success only when `firmware` matches pinned origins, extraction metadata and exact inventories.
/// This validates the manifest, not file bytes; check_radio subsequently hashes the actual tree.
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
/// Return the static.crates.io archive URL for trusted package `name` and `version` literals.
/// Values are interpolated unchanged; this is not a general URL encoder or network operation.
fn source_url(
    name: &str,
    version: &str,
) -> String {
    format!("https://static.crates.io/crates/{name}/{name}-{version}.crate")
}
/// Verify the HAL manifest/tree below vendor `root`; optionally verify original crate `archive` too.
/// Return inventory counts and whether the archive was supplied. Reject drift in pinned release,
/// overlay paths, metadata syntax, source bytes or filesystem/resource contracts. Backport prose
/// is not fetched or mechanically proven; original and patched bytes remain distinct evidence.
/// No network, extraction or writes occur; input files must remain unchanged during verification.
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
/// Verify pinned cyw43/trouble-host and firmware/NVRAM/license trees under vendor `root`.
/// Return a combined success summary only after all checks pass; errors discard partial summaries.
/// Archive digests are manifest trust anchors, not downloads; tree hashes prove current bytes only.
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
/// Pinning regressions for source-header extraction identity and permitted firmware overlays.
mod tests {
    use super::*;

    #[test]
    /// Exact baseline metadata passes; altered origins, digest/length or inventory entries fail.
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
