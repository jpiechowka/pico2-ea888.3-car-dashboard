//! Strict accepted input and frozen repository source. No compilation or artifact admission.
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use dashboard_helper_protocol::*;
use sha2::{Digest, Sha256 as Hasher};
const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_SOURCE_FILE: usize = 8 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 10000;
pub fn digest(bytes: &[u8]) -> String { Hasher::digest(bytes).iter().map(|b| format!("{b:02x}")).collect() }
#[derive(Clone)]
pub struct AcceptedInput {
    pub(crate) profile: Vec<u8>,
    pub(crate) asset: Vec<u8>,
    pub(crate) project: Vec<u8>,
    pub(crate) eav: Vec<u8>,
    pub(crate) options: NormalizedBuildOptions,
    pub(crate) revision: u64,
    config_digest: String,
}
impl AcceptedInput {
    pub fn from_request(request: &BuildRequest) -> Result<Self, String> {
        request.validate().map_err(|e| e.to_string())?;
        let eav = request
            .eav_hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let config_digest = digest(&eav);
        if config_digest != request.config_sha256.as_str() {
            return Err("Configuration digest mismatch".into());
        }
        let config = dashboard_config::decode(&eav).map_err(|e| format!("Configuration rejected: {e:?}"))?;
        let mut canonical = vec![0; dashboard_config::MAX_FILE_BYTES];
        let encoded = dashboard_config::encode(&config, &mut canonical)
            .map_err(|e| format!("Configuration encoding failed: {e:?}"))?;
        if encoded != eav {
            return Err("Configuration is not canonical current-version EAV".into());
        }
        Ok(Self {
            profile: dashboard_config::startup::StartupProfile::default()
                .encode()
                .unwrap()
                .to_vec(),
            asset: Vec::new(),
            project: Vec::new(),
            eav,
            options: normalize(&request.options).map_err(|e| e.to_string())?,
            revision: request.editor_revision,
            config_digest,
        })
    }
}
pub struct FrozenInput {
    _locks: Vec<std::fs::File>,
    attempt: PathBuf,
    source: PathBuf,
    files: BTreeMap<String, String>,
    inputs: BTreeMap<String, String>,
    source_digest: String,
}
impl FrozenInput {
    pub fn source_root(&self) -> &Path { &self.source }

    pub fn attempt_root(&self) -> &Path { &self.attempt }

    pub fn source_digest(&self) -> &str { &self.source_digest }

    pub fn verify(&self) -> Result<(), String> {
        let current = inventory(&self.source)?;
        if current.len() != self.files.len() {
            return Err("Frozen source inventory changed".into());
        }
        for (name, bytes) in current {
            if self.files.get(&name) != Some(&digest(&bytes)) {
                return Err(format!("Frozen source changed: {name}"));
            }
        }
        for (name, hash) in &self.inputs {
            let bytes = read_bounded(&self.attempt.join(name))?;
            if digest(&bytes) != *hash {
                return Err(format!("Frozen input changed: {name}"));
            }
        }
        Ok(())
    }
}
fn safe_metadata(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Source reparse point is not admitted".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("Source symbolic link is not admitted".into());
    }
    Ok(metadata)
}
fn add_file(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut usize,
) -> Result<(), String> {
    let metadata = safe_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_FILE as u64 {
        return Err("Source file type or size is unsupported".into());
    }
    let name = path
        .strip_prefix(root)
        .map_err(|_| "Source outside repository")?
        .to_str()
        .ok_or("Non-UTF8 source path")?
        .replace('\\', "/");
    if name.len() > 512 || files.len() >= MAX_SOURCE_FILES {
        return Err("Source inventory exceeds bounds".into());
    }
    let bytes = read_bounded(path)?;
    if bytes.len() > MAX_SOURCE_FILE || *total + bytes.len() > MAX_SOURCE_BYTES {
        return Err("Source snapshot exceeds byte bound".into());
    }
    *total += bytes.len();
    files.insert(name, bytes);
    Ok(())
}
fn tree(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut usize,
) -> Result<(), String> {
    if path.components().count().saturating_sub(root.components().count()) > 32 {
        return Err("Source depth exceeds bound".into());
    }
    if !safe_metadata(path)?.is_dir() {
        return Err("Expected source directory".into());
    }
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some("target" | ".git" | "__pycache__")) {
            continue;
        }
        let path = entry.path();
        let metadata = safe_metadata(&path)?;
        if metadata.is_dir() {
            tree(root, &path, files, total)?
        } else {
            add_file(root, &path, files, total)?
        }
    }
    Ok(())
}
fn inventory(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut total = 0;
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "firmware/Cargo.toml",
        "firmware/Cargo.lock",
        "firmware/rust-toolchain.toml",
        "firmware/.cargo/config.toml",
    ] {
        add_file(root, &root.join(relative), &mut files, &mut total)?;
    }
    for relative in [
        "crates",
        "firmware/pico2",
        // Cargo resolves every member manifest, including unselected images.
        "firmware/ble-probe",
        "firmware/vendor",
        "firmware/scripts",
        "host/scripts",
    ] {
        tree(root, &root.join(relative), &mut files, &mut total)?;
    }
    Ok(files)
}
fn write_new(
    path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn freeze_at(
    repo: &Path,
    input: &AcceptedInput,
    attempt: &Path,
) -> Result<FrozenInput, String> {
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    let bytes = inventory(&repo)?;
    if inventory(&repo)? != bytes {
        return Err("Repository sources changed during snapshot".into());
    }
    let attempt = attempt.to_path_buf();
    let source = attempt.join("source");
    fs::create_dir(&source).map_err(|e| e.to_string())?;
    let mut files = BTreeMap::new();
    for (name, data) in bytes {
        write_new(&source.join(&name), &data)?;
        files.insert(name, digest(&data));
    }
    let manifest_bytes = serde_json::to_vec(&files).map_err(|e| e.to_string())?;
    let source_digest = digest(&manifest_bytes);
    write_new(&attempt.join("source-manifest.json"), &manifest_bytes)?;
    let mut inputs = BTreeMap::new();
    for (name, bytes) in [
        ("input/dashboard.eav", input.eav.clone()),
        ("input/startup.easp", input.profile.clone()),
        ("input/startup.easa", input.asset.clone()),
        ("input/project.eavp", input.project.clone()),
        (
            "input/options.json",
            serde_json::to_vec(&input.options).map_err(|e| e.to_string())?,
        ),
    ] {
        write_new(&attempt.join(name), &bytes)?;
        inputs.insert(name.to_string(), digest(&bytes));
    }
    let admission = serde_json::json!({"schema_version":1,"editor_revision":input.revision,"config_sha256":input.config_digest,"source_manifest_sha256":source_digest,"scope":"frozen_build_input_only","artifact_admitted":false});
    write_new(
        &attempt.join("input/admission.json"),
        &serde_json::to_vec_pretty(&admission).unwrap(),
    )?;
    let paths = files
        .keys()
        .map(|name| source.join(name))
        .chain(inputs.keys().map(|name| attempt.join(name)))
        .collect::<Vec<_>>();
    let mut locks = crate::file_locks::ancestors(&paths)?;
    for path in paths {
        locks.push(crate::file_locks::read_file(&path)?);
    }
    let result = FrozenInput {
        _locks: locks,
        attempt,
        source,
        files,
        inputs,
        source_digest,
    };
    result.verify()?;
    Ok(result)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((MAX_SOURCE_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SOURCE_FILE {
        return Err("File exceeds bound".into());
    }
    Ok(bytes)
}
#[derive(Clone)]
pub struct UploadedProject {
    pub response: ProjectUploadResponse,
    bytes: Vec<u8>,
}
impl UploadedProject {
    pub fn admit(
        session: SessionId,
        bytes: Vec<u8>,
    ) -> Result<Self, String> {
        use dashboard_config_tools::project::{ExportMode, Project};
        if bytes.len() > PROJECT_UPLOAD_BYTES {
            return Err("Project exceeds reduced upload bound".into());
        }
        let project = Project::decode(&bytes).map_err(|e| format!("Project rejected: {e:?}"))?;
        if project.artwork().is_some_and(|a| a.original().is_some()) {
            return Err("Upload must omit original artwork".into());
        }
        if project
            .encode(ExportMode::ConvertedOnly)
            .map_err(|e| format!("{e:?}"))?
            != bytes
        {
            return Err("Project must be canonical converted-only EAVP".into());
        }
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).map_err(|_| "OS entropy unavailable")?;
        let response = ProjectUploadResponse {
            session_id: session,
            project_id: ProjectId::new(id.iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap(),
            project_sha256: Sha256::new(digest(&bytes)).unwrap(),
            config_sha256: Sha256::new(digest(project.dashboard_bytes())).unwrap(),
            startup_profile_sha256: Sha256::new(digest(&project.profile().encode().ok_or("Invalid profile")?)).unwrap(),
            startup_asset_sha256: Sha256::new(digest(project.artwork().map(|a| a.converted()).unwrap_or(&[]))).unwrap(),
        };
        Ok(Self { response, bytes })
    }

    pub fn accepted(
        &self,
        session: &SessionId,
        request: &ProjectBuildRequest,
    ) -> Result<AcceptedInput, String> {
        request.validate().map_err(|e| e.to_string())?;
        if *session != self.response.session_id
            || request.project_id != self.response.project_id
            || request.project_sha256 != self.response.project_sha256
        {
            return Err("Project token or digest does not match this session".into());
        }
        let project = dashboard_config_tools::project::Project::decode(&self.bytes).map_err(|e| format!("{e:?}"))?;
        Ok(AcceptedInput {
            eav: project.dashboard_bytes().to_vec(),
            profile: project.profile().encode().ok_or("Invalid profile")?.to_vec(),
            asset: project.artwork().map(|a| a.converted().to_vec()).unwrap_or_default(),
            project: self.bytes.clone(),
            options: normalize(&request.options).map_err(|e| e.to_string())?,
            revision: request.editor_revision,
            config_digest: self.response.config_sha256.as_str().to_string(),
        })
    }
}

pub(crate) fn capture(
    process: &mut crate::process::ProcessContext,
    frozen: &FrozenInput,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<crate::process::Output, String> {
    frozen.verify()?;
    let python = crate::prerequisites::python_executable().ok_or("Python executable unavailable")?;
    // A fixed sibling keeps MSVC outputs below MAX_PATH without a shared cache.
    let scratch = frozen.attempt.join("b");
    fs::create_dir(&scratch).map_err(|e| format!("Reserve native build scratch: {e}"))?;
    let _scratch_locks = crate::file_locks::ancestors(&[scratch.join("owned")])?;
    let input = frozen.attempt.join("input");
    let options =
        String::from_utf8(read_bounded(&input.join("options.json"))?).map_err(|_| "Invalid options encoding")?;
    let script = frozen.source.join("firmware/scripts/capture_target_bench.py");
    let config = input.join("dashboard.eav");
    let profile = input.join("startup.easp");
    let asset = input.join("startup.easa");
    let mut args = vec![
        "-I".to_string(),
        script.to_str().ok_or("Non-UTF8 source path")?.to_string(),
        "native".into(),
        "--firmware".into(),
        "--native-scratch".into(),
        "--options-json".into(),
        options,
        "--dashboard-config".into(),
        config.to_str().ok_or("Non-UTF8 input path")?.into(),
        "--startup-profile".into(),
        profile.to_str().ok_or("Non-UTF8 input path")?.into(),
    ];
    if fs::metadata(&asset).map_err(|e| e.to_string())?.len() > 0 {
        args.extend([
            "--startup-asset".into(),
            asset.to_str().ok_or("Non-UTF8 input path")?.into(),
        ]);
    }
    let output = process.capture(
        &python,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        &frozen.source,
        false,
        sink,
    )?;
    frozen.verify()?;
    Ok(output)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn request() -> BuildRequest {
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let mut buf = vec![0; dashboard_config::MAX_FILE_BYTES];
        let bytes = dashboard_config::encode(&config, &mut buf).unwrap();
        BuildRequest {
            editor_revision: 1,
            config_sha256: Sha256::new(digest(bytes)).unwrap(),
            eav_hex: Text::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap(),
            options: BuildOptions::default(),
            retry_of: None,
        }
    }
    #[test]
    fn snapshot_is_independent_and_detects_input_mutation() {
        use std::fs;
        let repo = std::env::temp_dir().join(format!(
            "studio-frozen-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&repo).unwrap();
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "firmware/Cargo.toml",
            "firmware/Cargo.lock",
            "firmware/rust-toolchain.toml",
            "firmware/.cargo/config.toml",
            "crates/example/lib.rs",
            "firmware/pico2/src/main.rs",
            "firmware/ble-probe/src/main.rs",
            "firmware/vendor/embassy-rp/src/lib.rs",
            "firmware/vendor/cyw43/src/lib.rs",
            "firmware/vendor/trouble-host/Cargo.toml",
            "firmware/vendor/cyw43-firmware/nested/radio.bin",
            "firmware/vendor/cyw43-provenance.json",
            "firmware/vendor/trouble-host-provenance.json",
            "firmware/scripts/capture.py",
            "host/scripts/environment.py",
        ] {
            let path = repo.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"source").unwrap();
        }
        // Cargo loads every workspace member, even when only pico2 is selected.
        fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers=[]\nresolver=\"3\"\n").unwrap();
        fs::write(
            repo.join("firmware/Cargo.toml"),
            "[workspace]\nmembers=[\"pico2\",\"ble-probe\"]\ndefault-members=[\"pico2\"]\nresolver=\"3\"\n",
        )
        .unwrap();
        for (directory, package) in [("pico2", "dashboard-pico2"), ("ble-probe", "dashboard-ble-probe")] {
            fs::write(
                repo.join("firmware").join(directory).join("Cargo.toml"),
                format!("[package]\nname=\"{package}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n"),
            )
            .unwrap();
        }
        for name in ["rust-toolchain.toml", "firmware/rust-toolchain.toml"] {
            fs::write(repo.join(name), include_str!("../../rust-toolchain.toml")).unwrap();
        }
        fs::write(repo.join("firmware/.cargo/config.toml"), "").unwrap();
        fs::write(repo.join("firmware/Cargo.lock"), "version = 4\n").unwrap();
        let mut reservation = crate::build_storage::reserve(&repo).unwrap();
        let frozen = freeze_at(
            &repo,
            &AcceptedInput::from_request(&request()).unwrap(),
            &reservation.path,
        )
        .unwrap();
        let metadata = std::process::Command::new("cargo")
            .args(["metadata", "--offline", "--no-deps", "--format-version", "1"])
            .current_dir(frozen.source_root().join("firmware"))
            .output()
            .unwrap();
        assert!(
            metadata.status.success(),
            "Frozen workspace must remain loadable: {}",
            String::from_utf8_lossy(&metadata.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
        let packages = metadata["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 2);
        let dashboard_id = packages.iter().find(|p| p["name"] == "dashboard-pico2").unwrap()["id"].clone();
        assert_eq!(metadata["workspace_default_members"], serde_json::json!([dashboard_id]));
        for name in ["firmware/ble-probe/Cargo.toml", "firmware/ble-probe/src/main.rs"] {
            assert!(frozen.files.contains_key(name), "{name}");
            assert!(fs::write(frozen.source_root().join(name), b"mutated member").is_err());
        }
        for name in [
            "firmware/vendor/cyw43/src/lib.rs",
            "firmware/vendor/trouble-host/Cargo.toml",
            "firmware/vendor/cyw43-firmware/nested/radio.bin",
            "firmware/vendor/cyw43-provenance.json",
            "firmware/vendor/trouble-host-provenance.json",
        ] {
            assert_eq!(frozen.files.get(name), Some(&digest(b"source")), "{name}");
            assert!(fs::write(frozen.source_root().join(name), b"mutated vendor input").is_err());
        }
        fs::write(repo.join("crates/example/lib.rs"), b"changed live source").unwrap();
        assert!(frozen.verify().is_ok());
        assert!(
            fs::write(
                frozen.attempt_root().join("input/dashboard.eav"),
                b"changed frozen input"
            )
            .is_err()
        );
        assert!(frozen.verify().is_ok());
        drop(frozen);
        reservation.close(false, Some("Snapshot fixture completed")).unwrap();
        let records = crate::build_storage::list(&repo, &Default::default()).unwrap();
        assert_eq!(records.attempts[0].state, BuildAttemptState::Failed);
        // Closure must release the active reservation for another explicit attempt.
        let mut next = crate::build_storage::reserve(&repo).unwrap();
        next.close(false, Some("Follow-up fixture completed")).unwrap();
        drop((next, reservation));
        // Fixture path was created above under the OS temporary directory.
        assert!(repo.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(repo).unwrap();
    }
}
