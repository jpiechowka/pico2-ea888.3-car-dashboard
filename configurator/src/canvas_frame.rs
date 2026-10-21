//! Identity of pixels successfully written to the browser canvas.
use dashboard_config::Geometry;

use crate::editor::{AcceptedConfigKey, Editor, EditorStamp};
use crate::json_draft::JsonProjection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Rendering lane whose pixels currently occupy the shared canvas.
pub enum PreviewMode {
    /// Startup scene; dashboard pointer interactions are disabled.
    Startup,
    /// Editor appearance preview, eligible for matching pointer interactions.
    Appearance,
    /// Time-driven control simulation; editor placement is disabled.
    Simulation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Immutable rendering identity checked before accepting canvas interactions.
pub struct CanvasFrame {
    /// Owner/revision/selection stamp used to retire old callbacks.
    stamp: EditorStamp,
    /// Accepted configuration identity, independent of selection-only changes.
    config: AcceptedConfigKey,
    /// Selected view ID, not its ordinal position.
    view: u8,
    /// Scene lane actually rendered.
    mode: PreviewMode,
    /// Pixel dimensions and grid shape used by the render.
    geometry: Geometry,
    /// Pending JSON projection, which forbids visual editing even when rendered.
    json: Option<JsonProjection>,
}
impl CanvasFrame {
    /// Returns captured `editor` identity for the supplied rendered `mode` and `geometry`.
    /// This records metadata only; it does not prove that pixels were written.
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
/// Last successful pixel publication plus a sticky failure gate for interactions.
pub struct PublishedCanvas {
    /// Last successfully written frame; absent before the first success.
    frame: Option<CanvasFrame>,
    /// A later failed draw makes even retained frame metadata ineligible.
    failed: bool,
}
impl PublishedCanvas {
    /// Commit `frame` only when the actual writer's `result` is `Ok(())`.
    /// Return the original result; an error retains old metadata but disables
    /// interactions until a successful publication clears the failure.
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
    /// Replace `before` with `after` only when `before` is eligible and all fields
    /// except its stamp match. Otherwise leave publication unchanged.
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

    /// Disable interactions after an allocation/context/draw failure, retaining metadata.
    pub fn fail(&mut self) { self.failed = true; }

    /// Return whether `requested` exactly identifies published, successful appearance
    /// pixels with no pending JSON projection. Does not change publication state.
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
