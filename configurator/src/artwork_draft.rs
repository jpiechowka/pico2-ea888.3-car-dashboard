//! Bounded importer numbers. Invalid text never silently reuses older options.
use dashboard_startup_tools::{ConvertError, Options};

#[derive(Clone, Copy, Debug)]
pub enum Number {
    Fps,
    Start,
    End,
    Columns,
    Rows,
    Frame,
}
impl Number {
    pub const ALL: [Self; 6] = [
        Self::Fps,
        Self::Start,
        Self::End,
        Self::Columns,
        Self::Rows,
        Self::Frame,
    ];

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
pub struct Numbers([String; 6]);
impl Numbers {
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

    pub fn text(
        &self,
        field: Number,
    ) -> &str {
        &self.0[field as usize]
    }

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
    owner: crate::editor::AcceptedConfigKey,
    revision: crate::editor::Revision,
    text: String,
}
impl ProjectFeedback {
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
