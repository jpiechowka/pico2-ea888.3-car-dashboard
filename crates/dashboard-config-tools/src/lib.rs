//! Host-only configuration snapshot helper; never a device dependency.
pub mod build_options;
pub mod clocks;
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

/// Host snapshot/import failure; invalid input never yields a validated configuration.
#[derive(Debug)]
pub enum Error {
    /// Explicitly empty environment selection; absence has different default semantics.
    EmptySelection,
    /// Non-file input or a path spelling unsafe for Cargo line directives.
    Path,
    /// Filesystem/open/read/write failure with the original operating-system cause.
    Io(
        /// Underlying I/O error.
        std::io::Error,
    ),
    /// Read consumed the permitted byte limit plus its overflow sentinel.
    TooLarge,
    /// Configuration decode, migration, validation or encode rejection.
    Config(
        /// Shared format/validation failure.
        ConfigError,
    ),
}

impl From<std::io::Error> for Error {
    /// Return `value` wrapped without dropping its original I/O category or message.
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}

impl core::fmt::Display for Error {
    /// Write a concise diagnostic to `f`; return only the formatter's write failure.
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

/// Return `bytes` as lowercase two-digit hexadecimal, with no separators or prefix.
///
/// Empty input returns an empty string; output owns exactly two characters per byte.
pub fn hex_bytes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing a String cannot fail");
    }
    text
}

/// Return the SHA-256 of `bytes` as 64 lowercase hex characters, including empty input.
pub fn sha256_hex(bytes: &[u8]) -> String { hex_bytes(&Sha256::new().update(bytes).finalize()) }

/// Read regular file `path` with at most `limit` bytes; return owned bytes or an error.
///
/// Reads one extra byte to reject oversized input, saturating at the largest read
/// limit. Zero accepts only an empty file. Checks path and opened-handle metadata;
/// symlinks to regular files are allowed, and this is not a path-confinement API.
/// Callers must select a practical allocation bound. I/O failure returns no bytes.
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
    file.take((limit as u64).saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

/// Encode validated `config`; return only its current-schema bytes, without buffer padding.
///
/// Allocates at most the shared format capacity and propagates encoder errors.
pub fn compile(config: &ValidatedConfig) -> Result<Vec<u8>, Error> {
    let mut bytes = vec![0; MAX_FILE_BYTES];
    let len = encode(config, &mut bytes).map_err(Error::Config)?.len();
    bytes.truncate(len);
    Ok(bytes)
}

/// Import `path`; return a decoded, migrated and validated configuration or an error.
///
/// A case-insensitive `.json` extension selects bounded JSON; every other extension
/// selects the bounded binary format. No content guessing or fallback occurs.
pub fn import(path: &Path) -> Result<ValidatedConfig, Error> {
    let json = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let bytes = read_bounded(path, if json { MAX_JSON_BYTES } else { MAX_FILE_BYTES })?;
    if json { from_json_bytes(&bytes) } else { decode(&bytes) }.map_err(Error::Config)
}

/// Owns the validated configuration re-encoded in the current binary schema.
/// Legacy input bytes may differ. Publication never reopens the input path,
/// preventing validation/copy disagreement.
pub struct Prepared {
    /// Requested path joined to the manifest directory; absent for built-in defaults.
    source: Option<PathBuf>,
    /// Canonical input path captured before reading; not reopened during publication.
    canonical_source: Option<PathBuf>,
    /// Validated configuration re-encoded into the current binary schema.
    bytes: Vec<u8>,
    /// Lowercase SHA-256 of exactly `bytes`, not the original legacy input file.
    fingerprint: String,
}

impl Prepared {
    /// Return the requested source path, or `None` for built-in defaults.
    pub fn source(&self) -> Option<&Path> { self.source.as_deref() }

    /// Return the canonical path observed during preparation; it may later change on disk.
    pub fn canonical_source(&self) -> Option<&Path> { self.canonical_source.as_deref() }

    /// Return the immutable current-schema snapshot; no filesystem read is performed.
    pub fn bytes(&self) -> &[u8] { &self.bytes }

    /// Return the stored 64-character fingerprint of the prepared binary snapshot.
    pub fn sha256_hex(&self) -> &str { &self.fingerprint }

    /// Write snapshot and fingerprint into existing directory `out`; return I/O failures.
    ///
    /// Writes `dashboard.eav` then `dashboard.sha256` with a trailing newline. This
    /// build-output pair is not transactional: a second-write failure can leave the
    /// binary replaced and an older fingerprint. Callers must fail the build on error.
    pub fn write_to(
        &self,
        out: &Path,
    ) -> Result<(), Error> {
        std::fs::write(out.join("dashboard.eav"), &self.bytes)?;
        std::fs::write(out.join("dashboard.sha256"), format!("{}\n", self.fingerprint))?;
        Ok(())
    }
}

/// Freeze binary `selection` relative to `manifest_dir`; return a current-schema snapshot.
///
/// `None` selects built-in defaults; an empty explicit selection is an error. Absolute
/// selections are allowed. Reject non-Unicode/control-containing Cargo path spellings,
/// then canonicalize, read once, decode and re-encode supported legacy formats.
/// Errors never fall back to defaults. Returned bytes and hash stay coherent even if
/// the source changes later. A broken compiled-in default is an internal panic.
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
pub mod project;
