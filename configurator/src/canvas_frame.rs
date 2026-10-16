//! Identity of pixels successfully written to the browser canvas.
use dashboard_config::Geometry;

use crate::editor::{AcceptedConfigKey, Editor, EditorStamp};
use crate::json_draft::JsonProjection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewMode {
    Startup,
    Appearance,
    Simulation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasFrame {
    stamp: EditorStamp,
    config: AcceptedConfigKey,
    view: u8,
    mode: PreviewMode,
    geometry: Geometry,
    json: Option<JsonProjection>,
}
impl CanvasFrame {
    pub fn capture(
        editor: &Editor,
        mode: PreviewMode,
        geometry: Geometry,
    ) -> Self {
        Self {
            stamp: editor.stamp(),
            config: editor.accepted_config_key(),
            view: editor.selection().view,
            mode,
            geometry,
            json: editor
                .json_draft()
                .filter(|_| editor.has_pending_json())
                .map(|d| d.projection()),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct PublishedCanvas {
    frame: Option<CanvasFrame>,
    failed: bool,
}
impl PublishedCanvas {
    /// Commit metadata only after the actual canvas writer reports success.
    pub fn publish<E>(
        &mut self,
        frame: CanvasFrame,
        result: Result<(), E>,
    ) -> Result<(), E> {
        match result {
            Ok(()) => {
                self.frame = Some(frame);
                self.failed = false;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }

    /// Only the adapter's selection-only operation may advance this stamp:
    /// selecting a widget in the same view does not repaint its pixels.
    pub fn selected(
        &mut self,
        before: CanvasFrame,
        after: CanvasFrame,
    ) {
        if self.permits(before)
            && (CanvasFrame {
                stamp: before.stamp,
                ..after
            }) == before
        {
            self.frame = Some(after);
        }
    }

    pub fn fail(&mut self) { self.failed = true; }

    pub fn permits(
        &self,
        requested: CanvasFrame,
    ) -> bool {
        !self.failed
            && requested.mode == PreviewMode::Appearance
            && requested.json.is_none()
            && self.frame == Some(requested)
    }
}
