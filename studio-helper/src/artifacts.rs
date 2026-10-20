//! Publication follows completed capture and independent fixed-tool validation.
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashboard_helper_protocol::*;

use crate::build_input::{AcceptedInput, FrozenInput, digest};
#[derive(Clone)]
pub(crate) struct Artifact {
    pub summary: ArtifactSummary,
    pub uf2: Arc<[u8]>,
    pub lease: Option<Arc<ArtifactLease>>,
}
/// Shared by cache and active operation; eviction cannot release an in-use artifact.
pub(crate) struct ArtifactLease {
    pub frozen: Arc<FrozenInput>,
    pub elf: PathBuf,
    pub converter: Converter,
    pub verification_ranges: Vec<(u32, u32)>,
    _holds: Vec<File>,
}
impl Artifact {
    pub fn verify_lease(&self) -> Result<&ArtifactLease, String> {
        let lease = self.lease.as_deref().ok_or("Artifact has no retained native lease")?;
        lease.frozen.verify()?;
        if digest(&bytes(&mut held(&lease.elf)?, 32 * 1024 * 1024)?) != self.summary.elf_sha256.as_str()
            || digest(&self.uf2) != self.summary.uf2_sha256.as_str()
            || lease.converter.hash != self.summary.converter_sha256.as_str()
            || lease.verification_ranges.is_empty()
        {
            return Err("Retained artifact identity mismatch".into());
        }
        Ok(lease)
    }
}
pub(crate) struct Converter {
    pub path: PathBuf,
    _file: File,
    _parents: Vec<File>,
    hash: String,
}
fn held(path: &Path) -> Result<File, String> { crate::file_locks::read_file(path) }
fn bytes(
    file: &mut File,
    max: usize,
) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    file.take((max + 1) as u64)
        .read_to_end(&mut result)
        .map_err(|e| e.to_string())?;
    if result.len() > max {
        return Err("Artifact byte bound".into());
    }
    Ok(result)
}
/// Validation stdout may be a machine report; stderr is always observable.
fn run_validation_tool(
    process: &mut crate::process::ProcessContext,
    tool: &Path,
    args: &[&str],
    cwd: &Path,
    machine_stdout: bool,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<crate::process::Output, String> {
    process.capture(tool, args, cwd, machine_stdout, &mut |stderr, text| {
        if stderr || !machine_stdout {
            sink(stderr, text);
        }
    })
}

pub(crate) fn converter(
    process: &mut crate::process::ProcessContext,
    repo: &Path,
) -> Result<Converter, String> {
    let path = repo
        .join("target/configurator-tools/bin/picotool.exe")
        .canonicalize()
        .map_err(|_| "Run Studio.ps1 -Setup to install picotool2.3.1")?;
    let parents = crate::file_locks::ancestors(std::slice::from_ref(&path))?;
    let mut file = held(&path)?;
    let hash = digest(&bytes(&mut file, 64 * 1024 * 1024)?);
    let output = process.run(&path, &["version", "-s"], repo)?;
    if output.code != Some(0) || output.stdout.trim() != "2.3.1" {
        return Err("Exact picotool2.3.1 converter required; run Studio.ps1 -Setup".into());
    }
    let help = process.run(&path, &["help", "uf2", "convert"], repo)?;
    if help.code != Some(0) || !help.stdout.contains("--family") || !help.stdout.contains("--platform") {
        return Err("Pinned converter capabilities unavailable".into());
    }
    Ok(Converter {
        path,
        _file: file,
        _parents: parents,
        hash,
    })
}
pub(crate) fn admit(
    process: &mut crate::process::ProcessContext,
    frozen: Arc<FrozenInput>,
    input: &AcceptedInput,
    session: SessionId,
    converter: Converter,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Artifact, String> {
    frozen.verify()?;
    let capture = frozen.source_root().join("firmware/target/bench-captures/native");
    let mut holds = crate::file_locks::ancestors(&[capture.join("pico2.elf")])?;
    for name in [
        "pico2.elf",
        "manifest.json",
        "dashboard.eav",
        "startup_profile.bin",
        "startup_asset.bin",
        "build-options.json",
        "source-context.json",
        "memory-layout.json",
    ] {
        holds.push(held(&capture.join(name))?);
    }
    let elf = capture.join("pico2.elf");
    let uf2 = frozen.attempt_root().join("program.uf2");
    if uf2.exists() {
        return Err("Converter output already exists".into());
    }
    let args = [
        "uf2",
        "convert",
        elf.to_str().ok_or("Invalid ELF path")?,
        uf2.to_str().ok_or("Invalid UF2 path")?,
        "--family",
        "rp2350-arm-s",
        "--platform",
        "rp2350",
    ];
    sink(true, "Running pinned UF2 converter");
    let converted = run_validation_tool(process, &converter.path, &args, frozen.source_root(), false, sink)?;
    if converted.code != Some(0) {
        return Err("Pinned UF2 conversion failed".into());
    }
    let mut uf2_file = held(&uf2)?;
    let uf2_bytes = bytes(&mut uf2_file, MAX_UF2_BYTES)?;
    let python = crate::prerequisites::python_executable().ok_or("Python executable unavailable")?;
    let script = frozen
        .source_root()
        .join("firmware/scripts/validate_capture_artifact.py");
    let inputs = frozen.attempt_root().join("input");
    let args = [
        "-I",
        script.to_str().ok_or("Invalid validator path")?,
        capture.to_str().ok_or("Invalid capture path")?,
        inputs.to_str().ok_or("Invalid inputs path")?,
        uf2.to_str().ok_or("Invalid UF2 path")?,
    ];
    // The script uses only its frozen sibling authorities, never current-checkout fallback.
    sink(true, "Running frozen artifact validator");
    let output = run_validation_tool(process, &python, &args, frozen.source_root(), true, sink)?;
    if output.code != Some(0) {
        return Err("Artifact validation failed; no artifact published".into());
    }
    let report: serde_json::Value =
        serde_json::from_slice(&output.raw_stdout).map_err(|_| "Invalid artifact validation result")?;
    validate_qualification(&report)?;
    for (key, expected) in [
        ("uf2_sha256", digest(&uf2_bytes)),
        ("config_sha256", digest(&input.eav)),
        ("startup_profile_sha256", digest(&input.profile)),
        ("startup_asset_sha256", digest(&input.asset)),
    ] {
        if report[key].as_str() != Some(&expected) {
            return Err("Artifact input digest mismatch".into());
        }
    }
    if report["uf2_bytes"].as_u64() != Some(uf2_bytes.len() as u64)
        || report["options"] != serde_json::to_value(&input.options).unwrap()
    {
        return Err("Artifact options or length mismatch".into());
    }
    frozen.verify()?;
    if crate::process::cancelled() {
        return Err("Artifact publication cancelled".into());
    }
    let identity = serde_json::json!({"schema":1,"project_sha256":digest(&input.project),"source_sha256":frozen.source_digest(),"converter_sha256":converter.hash,"validation":report});
    let summary = ArtifactSummary {
        artifact_id: ArtifactId::new(digest(&serde_json::to_vec(&identity).unwrap())).unwrap(),
        session_id: session,
        editor_revision: input.revision,
        project_sha256: Sha256::new(digest(&input.project)).unwrap(),
        config_sha256: Sha256::new(digest(&input.eav)).unwrap(),
        startup_profile_sha256: Sha256::new(digest(&input.profile)).unwrap(),
        startup_asset_sha256: Sha256::new(digest(&input.asset)).unwrap(),
        source_sha256: Sha256::new(frozen.source_digest()).unwrap(),
        elf_sha256: Sha256::new(report["elf_sha256"].as_str().ok_or("Missing ELF identity")?)
            .map_err(|e| e.to_string())?,
        uf2_sha256: Sha256::new(digest(&uf2_bytes)).unwrap(),
        converter_sha256: Sha256::new(converter.hash.clone()).unwrap(),
        options: input.options.clone(),
        uf2_bytes: uf2_bytes.len() as u32,
        hardware_qualified: false,
        flash_ready: false,
    };
    summary.validate().map_err(|e| e.to_string())?;
    let verification_ranges: Vec<(u32, u32)> =
        serde_json::from_value(report["program_image"]["verification_ranges"].clone())
            .map_err(|_| "Missing qualified verification ranges")?;
    if verification_ranges.is_empty()
        || verification_ranges.len() > 1024
        || verification_ranges
            .iter()
            .any(|(a, b)| *a < 0x10000000 || a >= b || *b > 0x101FE000)
        || verification_ranges.windows(2).any(|p| p[0].1 >= p[1].0)
    {
        return Err("Invalid verification ranges".into());
    }
    holds.push(uf2_file);

    let artifact = Artifact {
        summary,
        uf2: uf2_bytes.into(),
        lease: Some(Arc::new(ArtifactLease {
            frozen,
            elf,
            converter,
            verification_ranges,
            _holds: holds,
        })),
    };
    artifact.verify_lease()?;
    Ok(artifact)
}

/// The frozen Python authority revalidates every image field; this boundary rejects
/// legacy or experimental receipts before publishing a native artifact.
fn validate_qualification(report: &serde_json::Value) -> Result<(), String> {
    if report["schema_version"] != 2
        || report["scope"] != "practical_software_artifact"
        || report["image_gate"] != "practical"
        || report["complete_instruction_proof"] != false
        || report["hardware_qualified"] != false
        || report["flash_ready"] != false
    {
        return Err("Unsupported artifact qualification".into());
    }
    Ok(())
}

#[cfg(test)]
mod qualification_tests {
    use serde_json::json;

    use super::validate_qualification;

    #[test]
    fn only_explicit_practical_receipts_are_admitted() {
        let valid = json!({"schema_version":2,"scope":"practical_software_artifact",
            "image_gate":"practical","complete_instruction_proof":false,
            "hardware_qualified":false,"flash_ready":false});
        assert!(validate_qualification(&valid).is_ok());
        for key in [
            "schema_version",
            "scope",
            "image_gate",
            "complete_instruction_proof",
            "hardware_qualified",
            "flash_ready",
        ] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(validate_qualification(&missing).is_err(), "missing {key}");
        }
        for (key, value) in [
            ("schema_version", json!(1)),
            ("scope", json!("conditional_software_artifact")),
            ("image_gate", json!("experimental-full")),
            ("complete_instruction_proof", json!(true)),
            ("hardware_qualified", json!(true)),
            ("flash_ready", json!(true)),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(validate_qualification(&invalid).is_err(), "invalid {key}");
        }
    }
}

#[cfg(all(test, windows))]
mod diagnostic_tests {
    #[test]
    fn failed_validator_streams_stderr_but_preserves_machine_stdout_privately() {
        let cmd = crate::prerequisites::python_executable().expect("Python on an absolute PATH");
        let cwd = std::env::current_dir().unwrap();
        let mut observed = Vec::new();
        let output = super::run_validation_tool(
            &mut crate::process::ProcessContext::default(),
            &cmd,
            &[
                "-I",
                "-c",
                "import sys; print('machine-report'); print('validator-failure',file=sys.stderr); sys.exit(9)",
            ],
            &cwd,
            true,
            &mut |stderr, text| observed.push((stderr, text.to_owned())),
        )
        .unwrap();
        assert_eq!(output.code, Some(9));
        assert!(String::from_utf8_lossy(&output.raw_stdout).contains("machine-report"));
        assert!(
            observed
                .iter()
                .any(|(stderr, text)| *stderr && text.contains("validator-failure"))
        );
        assert!(!observed.iter().any(|(_, text)| text.contains("machine-report")));
    }
}
