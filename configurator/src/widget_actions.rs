//! Revision-bound context actions shared by the browser and native tests.

use dashboard_config::{Geometry, Placement};

use crate::editor::{Edit, Editor, EditorError, FieldToken, Revision, Selection};
use crate::geometry::PanelPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// One-cell movement in resolved logical grid coordinates, independent of mounting flip.
pub enum Direction {
    /// Decrease column by one.
    Left,
    /// Increase column by one.
    Right,
    /// Decrease row by one.
    Up,
    /// Increase row by one.
    Down,
}

impl Direction {
    /// Returns `old` moved one landscape-grid cell, or None if the original/result rectangle is outside the grid.
    /// Does not check other widgets; overlap can remain an inspectable invalid draft during layout repair.
    pub fn placement(
        self,
        old: Placement,
    ) -> Option<Placement> {
        self.placement_in(Geometry::Landscape, old)
    }

    /// Returns `old` moved one cell within `geometry`, or None for an invalid original, arithmetic boundary or
    /// out-of-grid result. Preserves span and does not perform overlap validation.
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
/// Revision/selection-bound context snapshot allowing repair actions on retained layout drafts.
pub struct WidgetMenu {
    /// Field authority captured after initial target selection, refreshed only by this menu's selection action.
    token: FieldToken,
    /// Widget currently addressed by move/delete actions.
    target: u16,
    /// Selected view captured when the menu opened.
    view: u8,
    /// Target and overlapping widgets in reverse paint order, with captured display labels.
    widgets: Vec<(u16, String)>,
}

impl WidgetMenu {
    /// Selects `target` in `editor` and returns a menu snapshot, including same-view widgets under optional panel-pixel
    /// `point`. None includes only the target. Returns missing-widget/view, selection or identity errors; selection
    /// may already change before a later missing-view error. Browser callers must first prove current canvas
    /// geometry and editable state.
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

    /// Returns the captured/current menu widget ID; not independent proof that it remains editable.
    pub fn target(&self) -> u16 { self.target }

    /// Returns captured ID/label pairs in reverse paint order for overlap selection; labels do not refresh
    /// automatically.
    pub fn widgets(&self) -> &[(u16, String)] { &self.widgets }

    /// Returns whether `editor` still matches this menu's field token and permits layout editing.
    pub fn is_current(
        &self,
        editor: &Editor,
    ) -> bool {
        self.token == editor.field_token() && editor.can_edit_layout()
    }

    /// Returns success for current editable `editor`, otherwise StaleToken; performs no mutation.
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

    /// Selects captured candidate `id` in `editor`; returns the action revision and refreshes this menu's target/token.
    /// Returns stale/missing/selection errors without refreshing menu authority; IDs outside the captured list are
    /// rejected.
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

    /// Returns whether the current target in `editor` can move one cell in `direction` within its resolved geometry.
    /// False covers stale authority, missing target/view and grid boundary; overlap with another widget is
    /// intentionally allowed.
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

    /// Applies a one-cell `direction` move to this target in `editor`; returns current revision unchanged at grid
    /// boundaries. Returns stale/missing/validation errors; shared editor validation retains a rejected overlapping
    /// layout for repair. A committed or retained-invalid edit retires this menu's old token instead of silently
    /// refreshing it.
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

    /// Deletes this captured target from `editor` through token-checked admission; returns new revision or editor
    /// rejection. Invalid retained layouts may be repaired by deletion; this method never refreshes stale menu
    /// authority.
    pub fn delete(
        &self,
        editor: &mut Editor,
    ) -> Result<Revision, EditorError> {
        editor.apply_if_current(self.token, Edit::DeleteWidget(self.target))
    }
}

/// Returns a viewport-relative CSS-pixel origin for `anchor` and measured menu `size` within `viewport`.
/// None rejects nonfinite values or nonpositive sizes; 8-pixel margins apply only on axes above 16 pixels.
/// Oversized menus pin to the leading margin; CSS must constrain their actual extent.
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
    /// Returns finite `anchor` clamped for positive `size`/`viewport` CSS pixels, pinning oversized menus to the
    /// leading margin.
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
