//! Bounded importer numbers. Invalid text never silently reuses older options.
use dashboard_startup_tools::{ConvertError, Options};

#[derive(Clone, Copy, Debug)]
/// Numeric importer control; discriminants index the six retained raw strings.
pub enum Number {
    /// Requested output sampling rate in frames per second.
    Fps,
    /// Inclusive source clip start in milliseconds.
    Start,
    /// Source clip end in milliseconds, interpreted by the converter options.
    End,
    /// Sprite-sheet horizontal frame count.
    Columns,
    /// Sprite-sheet vertical frame count.
    Rows,
    /// Duration of each sprite-sheet frame in milliseconds.
    Frame,
}
impl Number {
    /// Every numeric control in its storage and display order.
    pub const ALL: [Self; 6] = [
        Self::Fps,
        Self::Start,
        Self::End,
        Self::Columns,
        Self::Rows,
        Self::Frame,
    ];

    /// Return the static UI label, including units where applicable, for `self`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fps => "Output fps (1–20)",
            Self::Start => "Clip start (ms)",
            Self::End => "Clip end (ms)",
            Self::Columns => "Sheet columns",
            Self::Rows => "Sheet rows",
            Self::Frame => "Sheet frame duration (ms)",
        }
    }
}
#[derive(Clone)]
/// Raw numeric input retained independently of the last valid conversion options.
pub struct Numbers(
    /// Strings indexed by [`Number`]; malformed text remains visible.
    [String; 6],
);
impl Numbers {
    /// Returns editable decimal strings from `options`; absent sheet settings use 1×1/100 ms.
    pub fn new(options: &Options) -> Self {
        Self([
            options.sample_fps.to_string(),
            options.trim_start_ms.to_string(),
            options.trim_end_ms.to_string(),
            options.sheet.map_or(1, |s| s.columns).to_string(),
            options.sheet.map_or(1, |s| s.rows).to_string(),
            options.sheet.map_or(100, |s| s.frame_ms).to_string(),
        ])
    }

    /// Returns the exact retained string for `field`, including invalid input.
    pub fn text(
        &self,
        field: Number,
    ) -> &str {
        &self.0[field as usize]
    }

    /// Replace `field` with `text`; inputs over 32 UTF-8 bytes become an invalid sentinel.
    /// No parsing or accepted-option mutation occurs.
    pub fn set(
        &mut self,
        field: Number,
        text: &str,
    ) {
        self.0[field as usize] = if text.len() > 32 {
            "[input exceeds limit]".into()
        } else {
            text.into()
        };
    }

    /// Clone `base`, parse active numeric fields, and return fully validated options.
    /// Sheet fields are ignored when `base.sheet` is absent. Whitespace is trimmed;
    /// parse/narrowing failures return `ConvertError::Options`, and validation errors
    /// propagate without modifying `base` or these raw strings.
    pub fn options(
        &self,
        base: &Options,
    ) -> Result<Options, ConvertError> {
        let mut next = base.clone();
        let parse = |field| {
            self.text(field)
                .trim()
                .parse::<u32>()
                .map_err(|_| ConvertError::Options)
        };
        next.sample_fps = parse(Number::Fps)?.try_into().map_err(|_| ConvertError::Options)?;
        next.trim_start_ms = parse(Number::Start)?;
        next.trim_end_ms = parse(Number::End)?;
        if let Some(sheet) = &mut next.sheet {
            sheet.columns = parse(Number::Columns)?.try_into().map_err(|_| ConvertError::Options)?;
            sheet.rows = parse(Number::Rows)?.try_into().map_err(|_| ConvertError::Options)?;
            sheet.frame_ms = parse(Number::Frame)?.try_into().map_err(|_| ConvertError::Options)?;
        }
        next.validate()?;
        Ok(next)
    }
}

/// Operation feedback belongs to one accepted project, including its Editor owner.
#[derive(Clone)]
pub struct ProjectFeedback {
    /// Accepted configuration identity, including the owning editor instance.
    owner: crate::editor::AcceptedConfigKey,
    /// Complete accepted-project revision, including startup/artwork changes.
    revision: crate::editor::Revision,
    /// Feedback retained for this exact project snapshot.
    text: String,
}
impl ProjectFeedback {
    /// Returns owned feedback `text` bound to `editor`'s accepted identity/revision.
    pub fn new(
        editor: &crate::editor::Editor,
        text: String,
    ) -> Self {
        Self {
            owner: editor.accepted_config_key(),
            revision: editor.accepted_revision(),
            text,
        }
    }

    /// Return stored feedback only while `editor` matches both identities; otherwise `""`.
    pub fn text<'a>(
        &'a self,
        editor: &crate::editor::Editor,
    ) -> &'a str {
        if self.owner == editor.accepted_config_key() && self.revision == editor.accepted_revision() {
            &self.text
        } else {
            ""
        }
    }
}
