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
        "firmware/vendor/embassy-rp",
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
pub fn freeze(
    repo: &Path,
    input: &AcceptedInput,
) -> Result<FrozenInput, String> {
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    let reservation = crate::build_storage::reserve(&repo)?;
    freeze_at(&repo, input, &reservation.path)
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
    frozen: &FrozenInput,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<crate::process::Output, String> {
    frozen.verify()?;
    let python = std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH unavailable")?)
        .map(|p| p.join("python.exe"))
        .find(|p| p.is_absolute() && p.is_file())
        .ok_or("Python executable unavailable")?;
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
    let output = crate::process::run_capture_stream(
        &python,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        &frozen.source,
        sink,
    )?;
    frozen.verify()?;
    Ok(output)
}
