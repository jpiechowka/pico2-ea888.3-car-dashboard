//! Bounded Editor-owned JSON transactions. These candidates are not proof.
use dashboard_config::{Config, ConfigError, MAX_JSON_BYTES};

use crate::diagnostic::{ValidationReport, collect_candidate};
use crate::editor::{EditorError, EditorStamp, Revision};

/// Only Editor can issue handles. Read-only visual inspection never promotes
/// an old handle to an editable transaction after repair.
///
/// ```compile_fail
/// let handle = dashboard_configurator::json_draft::JsonDraftHandle { epoch: 1 };
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

impl JsonDraft {
    pub fn handle(&self) -> JsonDraftHandle { self.handle }

    pub fn text(&self) -> &str { &self.text }

    pub fn state(&self) -> &JsonState { &self.state }

    pub fn is_read_only(&self) -> bool { self.read_only }

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
