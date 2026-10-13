//! Host-only configuration snapshot helper; never a device dependency.
use std::ffi::OsStr;
use std::fmt::Write;
use std::io::Read;
use std::path::{Path, PathBuf};

use dashboard_config::{
    ConfigError,
    MAX_FILE_BYTES,
    MAX_JSON_BYTES,
    ValidatedConfig,
    decode,
    default_config,
    encode,
    from_json_bytes,
    validate,
};
use sha2_const_stable::Sha256;

#[derive(Debug)]
pub enum Error {
    EmptySelection,
    Path,
    Io(std::io::Error),
    TooLarge,
    Config(ConfigError),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}

impl core::fmt::Display for Error {
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        match self {
            Self::EmptySelection => f.write_str("DASHBOARD_CONFIG is explicitly empty"),
            Self::Path => f.write_str("configuration path must name a regular file and contain no control characters"),
            Self::Io(error) => write!(f, "configuration file: {error}"),
            Self::TooLarge => f.write_str("configuration exceeds the format size limit"),
            Self::Config(error) => write!(f, "invalid configuration: {error:?}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::new().update(bytes).finalize();
    let mut text = String::with_capacity(64);
    for byte in digest {
        write!(text, "{byte:02x}").expect("writing a String cannot fail");
    }
    text
}

pub fn read_bounded(
    path: &Path,
    limit: usize,
) -> Result<Vec<u8>, Error> {
    // Reject ordinary non-files before open (a FIFO could otherwise block on
    // Unix). Recheck the opened handle as well in case the path was replaced.
    if !path.metadata()?.is_file() {
        return Err(Error::Path);
    }
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Path);
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

pub fn compile(config: &ValidatedConfig) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; MAX_FILE_BYTES];
    let len = encode(config, &mut bytes).map_err(Error::Config)?.len();
    bytes.truncate(len);
    Ok(bytes)
}

pub fn import(path: &Path) -> Result<ValidatedConfig, Error> {
    let json = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let bytes = read_bounded(path, if json { MAX_JSON_BYTES } else { MAX_FILE_BYTES })?;
    if json { from_json_bytes(&bytes) } else { decode(&bytes) }.map_err(Error::Config)
}

/// Owns exactly the bytes that passed decoding. Writing this object never
/// reopens the input path, preventing validation/copy disagreement.
pub struct Prepared {
    source: Option<PathBuf>,
    canonical_source: Option<PathBuf>,
    bytes: Vec<u8>,
    fingerprint: String,
}

impl Prepared {
    pub fn source(&self) -> Option<&Path> { self.source.as_deref() }

    pub fn canonical_source(&self) -> Option<&Path> { self.canonical_source.as_deref() }

    pub fn bytes(&self) -> &[u8] { &self.bytes }

    pub fn sha256_hex(&self) -> &str { &self.fingerprint }

    pub fn write_to(
        &self,
        out: &Path,
    ) -> Result<(), Error> {
        std::fs::write(out.join("dashboard.eav"), &self.bytes)?;
        std::fs::write(out.join("dashboard.sha256"), format!("{}\n", self.fingerprint))?;
        Ok(())
    }
}

pub fn prepare(
    selection: Option<&OsStr>,
    manifest_dir: &Path,
) -> Result<Prepared, Error> {
    let (source, canonical_source, bytes) = if let Some(selected) = selection {
        if selected.is_empty() {
            return Err(Error::EmptySelection);
        }
        // Paths are used in Cargo line directives. Reject controls before any
        // I/O and require an unambiguous Unicode spelling on every host.
        if selected.to_str().is_none_or(|text| text.chars().any(char::is_control)) {
            return Err(Error::Path);
        }
        let requested = manifest_dir.join(selected);
        if requested.to_str().is_none_or(|text| text.chars().any(char::is_control)) {
            return Err(Error::Path);
        }
        let canonical = requested.canonicalize()?;
        if canonical.to_str().is_none_or(|text| text.chars().any(char::is_control)) {
            return Err(Error::Path);
        }
        let bytes = read_bounded(&canonical, MAX_FILE_BYTES)?;
        let config = decode(&bytes).map_err(Error::Config)?;
        // Re-encode all supported legacy inputs into the canonical current format.
        // Never reopen the source between validation and compilation.
        (Some(requested), Some(canonical), compile(&config)?)
    } else {
        let default = validate(default_config()).expect("reviewed default configuration");
        (None, None, compile(&default)?)
    };
    let fingerprint = sha256_hex(&bytes);
    Ok(Prepared {
        source,
        canonical_source,
        bytes,
        fingerprint,
    })
}
