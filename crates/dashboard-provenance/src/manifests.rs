use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::{Hashes, Result, parse, read_manifest, tree, verify_archive};
const HAL_VERSION: &str = "0.10.0";
const HAL_HASH: &str = "5d98f472894c1ecffac5596c657af5305c57282d29e8746d7fec033951931bc8";
const COMMIT: &str = "84444a19eb57d978e3c09fcdc8e60cdd7278eb03";
const HAL_PATCHES: [&str; 10] = [
    "src/uart/buffered.rs",
    "src/uart/mod.rs",
    "src/uart/buffered_io.rs",
    "src/multicore.rs",
    "src/flash.rs",
    "src/flash_probe.rs",
    "src/pwm.rs",
    "Cargo.toml",
    "Cargo.toml.orig",
    "src/lib.rs",
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
        false,
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
    tree(&root.join("cyw43-firmware"), &firmware.files, &Hashes::default(), true)?;
    output.push_str("CYW firmware, NVRAM and license bytes verified");
    Ok(output)
}
