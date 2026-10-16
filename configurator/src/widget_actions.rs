//! Revision-bound context actions shared by the browser and native tests.

use dashboard_config::{Geometry, Placement};

use crate::editor::{Edit, Editor, EditorError, FieldToken, Revision, Selection};
use crate::geometry::PanelPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// A nudge keeps the entire rectangle in the grid. Overlaps remain allowed
    /// as invalid draft edits so a crowded layout can be repaired in steps.
    pub fn placement(
        self,
        old: Placement,
    ) -> Option<Placement> {
        self.placement_in(Geometry::Landscape, old)
    }

    pub fn placement_in(
        self,
        geometry: Geometry,
        old: Placement,
    ) -> Option<Placement> {
        old.rect_in(geometry)?;
        let mut next = old;
        match self {
            Self::Left => next.column = next.column.checked_sub(1)?,
            Self::Right => next.column = next.column.checked_add(1)?,
            Self::Up => next.row = next.row.checked_sub(1)?,
            Self::Down => next.row = next.row.checked_add(1)?,
        }
        next.rect_in(geometry)?;
        Some(next)
    }
}

#[derive(Clone, Debug)]
pub struct WidgetMenu {
    token: FieldToken,
    target: u16,
    view: u8,
    widgets: Vec<(u16, String)>,
}

impl WidgetMenu {
    pub fn open(
        editor: &mut Editor,
        target: u16,
        point: Option<PanelPoint>,
    ) -> Result<Self, EditorError> {
        let view = editor
            .editing_config()
            .widgets
            .iter()
            .find(|w| w.id == target)
            .ok_or(EditorError::MissingWidget(target))?
            .view;
        editor.select(Selection {
            view,
            widget: Some(target),
        })?;
        let geometry = editor
            .editing_config()
            .view_orientation(view)
            .ok_or(EditorError::MissingView(view))?
            .geometry();
        let widgets = editor
            .editing_config()
            .widgets
            .iter()
            .rev()
            .filter(|w| {
                w.view == view
                    && (w.id == target
                        || point.is_some_and(|point| {
                            w.placement.rect_in(geometry).is_some_and(|r| {
                                (f64::from(r.x)..f64::from(r.x + r.width)).contains(&point.x)
                                    && (f64::from(r.y)..f64::from(r.y + r.height)).contains(&point.y)
                            })
                        }))
            })
            .map(|w| (w.id, w.label.to_string()))
            .collect();
        Ok(Self {
            token: editor.field_token(),
            target,
            view,
            widgets,
        })
    }

    pub fn target(&self) -> u16 { self.target }

    pub fn widgets(&self) -> &[(u16, String)] { &self.widgets }

    pub fn is_current(
        &self,
        editor: &Editor,
    ) -> bool {
        self.token == editor.field_token() && editor.can_edit_layout()
    }

    fn check(
        &self,
        editor: &Editor,
    ) -> Result<(), EditorError> {
        if self.is_current(editor) {
            Ok(())
        } else {
            Err(EditorError::StaleToken)
        }
    }

    pub fn select(
        &mut self,
        editor: &mut Editor,
        id: u16,
    ) -> Result<Revision, EditorError> {
        self.check(editor)?;
        if !self.widgets.iter().any(|w| w.0 == id) {
            return Err(EditorError::MissingWidget(id));
        }
        let revision = editor.select(Selection {
            view: self.view,
            widget: Some(id),
        })?;
        self.target = id;
        self.token = editor.field_token();
        Ok(revision)
    }

    pub fn can_move(
        &self,
        editor: &Editor,
        direction: Direction,
    ) -> bool {
        self.is_current(editor)
            && editor
                .editing_config()
                .widgets
                .iter()
                .find(|w| w.id == self.target)
                .is_some_and(|w| {
                    editor
                        .editing_config()
                        .view_orientation(w.view)
                        .and_then(|o| direction.placement_in(o.geometry(), w.placement))
                        .is_some()
                })
    }

    pub fn move_widget(
        &self,
        editor: &mut Editor,
        direction: Direction,
    ) -> Result<Revision, EditorError> {
        self.check(editor)?;
        let widget = editor
            .editing_config()
            .widgets
            .iter()
            .find(|w| w.id == self.target)
            .ok_or(EditorError::MissingWidget(self.target))?;
        let Some(placement) = direction.placement_in(
            editor
                .editing_config()
                .view_orientation(widget.view)
                .ok_or(EditorError::MissingView(widget.view))?
                .geometry(),
            widget.placement,
        ) else {
            return Ok(editor.revision());
        };
        editor.apply_if_current(
            self.token,
            Edit::PlaceWidget {
                id: self.target,
                placement,
            },
        )
    }

    pub fn delete(
        &self,
        editor: &mut Editor,
    ) -> Result<Revision, EditorError> {
        editor.apply_if_current(self.token, Edit::DeleteWidget(self.target))
    }
}

/// CSS constrains oversized menus to the available viewport. This also clamps
/// their origin, using actual measured dimensions, without NaN CSS or panics.
pub fn menu_position(
    anchor: (f64, f64),
    size: (f64, f64),
    viewport: (f64, f64),
) -> Option<(f64, f64)> {
    if ![anchor.0, anchor.1, size.0, size.1, viewport.0, viewport.1]
        .into_iter()
        .all(f64::is_finite)
        || size.0 <= 0.0
        || size.1 <= 0.0
        || viewport.0 <= 0.0
        || viewport.1 <= 0.0
    {
        return None;
    }
    fn axis(
        anchor: f64,
        size: f64,
        viewport: f64,
    ) -> f64 {
        let margin = if viewport > 16.0 { 8.0 } else { 0.0 };
        anchor.clamp(margin, (viewport - size - margin).max(margin))
    }
    Some((axis(anchor.0, size.0, viewport.0), axis(anchor.1, size.1, viewport.1)))
}
