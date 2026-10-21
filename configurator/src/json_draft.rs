//! Bounded Editor-owned JSON transactions. These candidates are not proof.
use std::sync::Arc;

use dashboard_config::{Config, ConfigError, MAX_JSON_BYTES, ValidatedConfig, validate};

use crate::diagnostic::{ValidationReport, collect_candidate};
use crate::editor::{EditorError, EditorStamp, Revision};

/// Only Editor can issue handles. Read-only visual inspection never promotes
/// an old handle to an editable transaction after repair.
///
/// ```compile_fail
/// let mut editor = dashboard_configurator::editor::Editor::default();
/// let mut handle = editor.open_json().unwrap();
/// handle.epoch = 1;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonDraftHandle {
    /// Editor owner/action revision that opened this draft.
    pub(crate) stamp: EditorStamp,
    /// Accepted-project revision at opening, independently checked by the editor.
    pub(crate) accepted_revision: Revision,
    /// Checked draft generation; a reopened draft cannot inherit prior authority.
    pub(crate) epoch: u64,
}

/// Debounce callbacks retain this exact token; they never obtain a fresh one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonParseToken {
    /// Exact editor-owned draft capability captured when text changed.
    pub(crate) handle: JsonDraftHandle,
    /// Checked raw-text generation; old debounce callbacks cannot parse newer input.
    pub(crate) text_epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Read-only presentation phase; none grants editor mutation or firmware admission.
pub enum JsonPhase {
    /// No unapplied JSON text changes.
    Clean,
    /// Changed text awaits parsing; accepted pixels remain the fallback.
    Pending,
    /// Current text failed syntax/schema/version parsing.
    SyntaxError,
    /// Typed candidate has semantic issues or omitted diagnostic findings.
    SemanticInvalid,
    /// Candidate passed collected diagnostics but still needs explicit Apply.
    ValidUnapplied,
    /// Visual rejected-draft inspection; JSON controls cannot edit this projection.
    ReadOnly,
}

impl JsonPhase {
    /// Returns the UI phase label, explicitly marking candidate inspection/unapplied state.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clean => "JSON · no pending edits",
            Self::Pending => "JSON PENDING · parsing · accepted display",
            Self::SyntaxError => "INVALID JSON · accepted display",
            Self::SemanticInvalid | Self::ReadOnly => "INVALID PREVIEW · inspection only",
            Self::ValidUnapplied => "UNAPPLIED JSON PREVIEW",
        }
    }
}

/// Read-only equality projection. Not a new owner or callback authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonProjection {
    /// Owning draft's immutable identity.
    handle: JsonDraftHandle,
    /// Raw-text generation represented by the projection.
    text_epoch: u64,
    /// Rendering/inspection phase at capture time.
    phase: JsonPhase,
}
impl JsonProjection {
    /// Returns this projection's captured presentation phase.
    pub fn phase(self) -> JsonPhase { self.phase }
}

/// Preview navigation does not select or mutate the firmware view.
/// Returns a unique view ID in `config`, preferring `preferred`, then boot view,
/// then stored order. Missing/repeated IDs are skipped; `None` means no unique view.
pub fn inspection_view(
    config: &Config,
    preferred: u8,
) -> Option<u8> {
    let unique = |id| config.views.iter().filter(|view| view.id == id).count() == 1;
    [preferred, config.boot_view]
        .into_iter()
        .chain(config.views.iter().map(|view| view.id))
        .find(|id| unique(*id))
}

#[derive(Debug)]
/// Current raw-text parse result; a typed candidate can still be semantically invalid.
pub enum JsonState {
    /// No result for current text, either initially clean or awaiting debounce.
    Unparsed,
    /// Syntax/schema/version failure with an optional source location.
    SyntaxError {
        /// One-based parser line; zero when no source location is available.
        line: usize,
        /// Parser column; zero for version/general failures without location.
        column: usize,
        /// Bounded-by-input diagnostic generated from the shared parser result.
        message: String,
    },
    /// Strictly typed configuration and collected diagnostics, not an acceptance proof.
    Candidate {
        /// Owned unvalidated configuration parsed from the current raw text.
        config: Box<Config>,
        /// Collected semantic/raw issues, including an omitted-issue count.
        report: ValidationReport,
    },
}

#[derive(Debug)]
/// Editor-owned bounded JSON transaction, retaining text independently of accepted state.
pub struct JsonDraft {
    /// Authority sealed to editor owner/action/accepted revisions and draft generation.
    pub(crate) handle: JsonDraftHandle,
    /// Current JSON text, bounded by `MAX_JSON_BYTES` at construction/replacement.
    pub(crate) text: String,
    /// Nonwrapping text generation used by debounce tokens.
    pub(crate) text_epoch: u64,
    /// Text changes require Apply/Discard even after successful parsing.
    pub(crate) pending: bool,
    /// Draft is inspection-only and rejects raw text replacement.
    pub(crate) read_only: bool,
    /// Latest parse result for the current text generation.
    pub(crate) state: JsonState,
}

/// Immutable validation proof sealed to one actual Editor-owned JSON projection.
/// Clones share the proof; they do not own editable text or acquisition state.
#[derive(Clone)]
pub struct JsonPreviewProof {
    /// Exact text/phase identity for which full validation succeeded.
    projection: JsonProjection,
    /// Shared immutable fully validated configuration for preview rendering only.
    config: Arc<ValidatedConfig>,
}
impl JsonPreviewProof {
    /// Returns the projection identity sealed into this proof.
    pub const fn projection(&self) -> JsonProjection { self.projection }

    /// Returns borrowed validated preview configuration; does not publish accepted state.
    pub fn config(&self) -> &ValidatedConfig { &self.config }
}

impl JsonDraft {
    /// Returns a fresh full-validation proof for the current typed candidate and
    /// projection. Noncandidate state returns `StaleToken`; invalid configuration
    /// returns its shared validation error. This does not Apply or authorize editing.
    pub fn preview_proof(&self) -> Result<JsonPreviewProof, EditorError> {
        let JsonState::Candidate { config, .. } = &self.state else {
            return Err(EditorError::StaleToken);
        };
        Ok(JsonPreviewProof {
            projection: self.projection(),
            config: Arc::new(validate(config.as_ref().clone()).map_err(EditorError::Validation)?),
        })
    }

    /// Returns the existing editor-issued draft handle, without refreshing its authority.
    pub fn handle(&self) -> JsonDraftHandle { self.handle }

    /// Returns the exact retained JSON text, including syntax/semantic errors.
    pub fn text(&self) -> &str { &self.text }

    /// Returns the current parse result without parsing or validation work.
    pub fn state(&self) -> &JsonState { &self.state }

    /// Returns whether this draft permits inspection only and refuses text replacement.
    pub fn is_read_only(&self) -> bool { self.read_only }

    /// Returns equality-only text/phase identity, prioritizing read-only state,
    /// then parsing/diagnostic issues and unapplied text. Does not mint new authority.
    pub fn projection(&self) -> JsonProjection {
        let phase = if self.read_only {
            JsonPhase::ReadOnly
        } else {
            match &self.state {
                JsonState::Unparsed if self.pending => JsonPhase::Pending,
                JsonState::Unparsed => JsonPhase::Clean,
                JsonState::SyntaxError { .. } => JsonPhase::SyntaxError,
                JsonState::Candidate { report, .. }
                    if !report.issues.is_empty() || !report.raw_errors.is_empty() || report.omitted != 0 =>
                {
                    JsonPhase::SemanticInvalid
                }
                JsonState::Candidate { .. } if self.pending => JsonPhase::ValidUnapplied,
                JsonState::Candidate { .. } => JsonPhase::Clean,
            }
        };
        JsonProjection {
            handle: self.handle,
            text_epoch: self.text_epoch,
            phase,
        }
    }

    /// Replace with owned `text`, retire the prior parse result and return its new
    /// debounce token. Read-only, byte-bound and checked epoch-exhaustion errors
    /// preserve prior state. Successful replacement always becomes pending/unparsed.
    pub(crate) fn replace_text(
        &mut self,
        text: String,
    ) -> Result<JsonParseToken, EditorError> {
        if self.read_only {
            return Err(EditorError::InvalidDraft);
        }
        if text.len() > MAX_JSON_BYTES {
            return Err(EditorError::JsonTooLong);
        }
        let text_epoch = self.text_epoch.checked_add(1).ok_or(EditorError::CounterExhausted)?;
        self.text = text;
        self.text_epoch = text_epoch;
        self.pending = true;
        self.state = JsonState::Unparsed;
        Ok(JsonParseToken {
            handle: self.handle,
            text_epoch,
        })
    }

    /// Parse current owned text using the strict current-version schema and replace
    /// its result with candidate diagnostics or a syntax/schema/version error.
    /// Preserves pending/read-only flags; the editor checks token identity beforehand.
    pub(crate) fn parse(&mut self) {
        self.state = match dashboard_config::parse_current_json_candidate(self.text.as_bytes()) {
            Ok(config) => {
                let report = collect_candidate(&config);
                JsonState::Candidate {
                    config: Box::new(config),
                    report,
                }
            }
            Err(error) => {
                let (line, column, message) = match error {
                    ConfigError::Json { line, column } => {
                        (
                            line,
                            column,
                            format!("Invalid JSON syntax or schema at line {line}, column {column}"),
                        )
                    }
                    ConfigError::Version => {
                        (
                            0,
                            0,
                            "Inline JSON requires the current version; use Import for legacy files".into(),
                        )
                    }
                    _ => (0, 0, format!("JSON parse rejected: {error:?}")),
                };
                JsonState::SyntaxError { line, column, message }
            }
        };
    }
}
