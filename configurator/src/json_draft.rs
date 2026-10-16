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
    pub(crate) stamp: EditorStamp,
    pub(crate) accepted_revision: Revision,
    pub(crate) epoch: u64,
}

/// Debounce callbacks retain this exact token; they never obtain a fresh one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonParseToken {
    pub(crate) handle: JsonDraftHandle,
    pub(crate) text_epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonPhase {
    Clean,
    Pending,
    SyntaxError,
    SemanticInvalid,
    ValidUnapplied,
    ReadOnly,
}

impl JsonPhase {
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
    handle: JsonDraftHandle,
    text_epoch: u64,
    phase: JsonPhase,
}
impl JsonProjection {
    pub fn phase(self) -> JsonPhase { self.phase }
}

/// Preview navigation does not select or mutate the firmware view.
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
pub enum JsonState {
    Unparsed,
    SyntaxError {
        line: usize,
        column: usize,
        message: String,
    },
    Candidate {
        config: Box<Config>,
        report: ValidationReport,
    },
}

#[derive(Debug)]
pub struct JsonDraft {
    pub(crate) handle: JsonDraftHandle,
    pub(crate) text: String,
    pub(crate) text_epoch: u64,
    pub(crate) pending: bool,
    pub(crate) read_only: bool,
    pub(crate) state: JsonState,
}

/// Immutable validation proof sealed to one actual Editor-owned JSON projection.
/// Clones share the proof; they do not own editable text or acquisition state.
#[derive(Clone)]
pub struct JsonPreviewProof {
    projection: JsonProjection,
    config: Arc<ValidatedConfig>,
}
impl JsonPreviewProof {
    pub const fn projection(&self) -> JsonProjection { self.projection }

    pub fn config(&self) -> &ValidatedConfig { &self.config }
}

impl JsonDraft {
    pub fn preview_proof(&self) -> Result<JsonPreviewProof, EditorError> {
        let JsonState::Candidate { config, .. } = &self.state else {
            return Err(EditorError::StaleToken);
        };
        Ok(JsonPreviewProof {
            projection: self.projection(),
            config: Arc::new(validate(config.as_ref().clone()).map_err(EditorError::Validation)?),
        })
    }

    pub fn handle(&self) -> JsonDraftHandle { self.handle }

    pub fn text(&self) -> &str { &self.text }

    pub fn state(&self) -> &JsonState { &self.state }

    pub fn is_read_only(&self) -> bool { self.read_only }

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
