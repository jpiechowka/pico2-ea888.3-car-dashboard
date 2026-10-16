//! Checked mapping from an actual canvas content rectangle to the Pico grid.

use dashboard_config::{
    Config,
    Geometry,
    MAX_WIDGETS,
    Placement,
    Span,
    ValidatedConfig,
    ValidationError,
    default_widget,
    validate,
};
use dashboard_core::config::HEADER_HEIGHT;

use crate::editor::PointerTarget;

#[derive(Clone, Copy, Debug, PartialEq)]
/// The displayed canvas content, excluding borders and letterboxing. Values are
/// CSS pixels from the same coordinate space as PointerEvent.clientX/clientY.
pub struct ContentRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub column: u8,
    pub row: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrabOffset {
    x: f64,
    y: f64,
}

impl GrabOffset {
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };

    pub fn center(span: Span) -> Self { Self::center_in(Geometry::Landscape, span) }

    pub fn center_in(
        geometry: Geometry,
        span: Span,
    ) -> Self {
        let area = Placement {
            column: 0,
            row: 0,
            span,
        }
        .rect_in(geometry)
        .expect("supported span");
        Self {
            x: f64::from(area.width) / 2.,
            y: f64::from(area.height) / 2.,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    InvalidRect,
    NonFinitePointer,
    OutsideCanvas,
    Header,
    OutsideWidget,
    InvalidOffset,
    OutsideGrid,
    MissingWidget,
    MissingView,
    Capacity,
    Validation(ValidationError),
}

/// A physical client-space movement is required before layout mapping. Canvas
/// reflow, scroll and resize alone never turn a click into a placement commit.
pub const DRAG_THRESHOLD_CSS_PX: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragGesture {
    origin: PanelPoint,
    dragging: bool,
}
impl DragGesture {
    pub fn new(
        client_x: f64,
        client_y: f64,
    ) -> Result<Self, GeometryError> {
        if !client_x.is_finite() || !client_y.is_finite() {
            return Err(GeometryError::NonFinitePointer);
        }
        Ok(Self {
            origin: PanelPoint {
                x: client_x,
                y: client_y,
            },
            dragging: false,
        })
    }

    pub fn is_dragging(self) -> bool { self.dragging }

    /// The threshold is Euclidean distance in CSS pixels, independent of canvas
    /// scale. Once crossed, a gesture remains a drag even if it returns to origin.
    pub fn placement(
        &mut self,
        rect: ContentRect,
        client_x: f64,
        client_y: f64,
        target: PointerTarget,
        span: Span,
        offset: GrabOffset,
    ) -> Result<Option<Placement>, GeometryError> {
        self.placement_in(Geometry::Landscape, rect, (client_x, client_y), target, span, offset)
    }

    pub fn placement_in(
        &mut self,
        geometry: Geometry,
        rect: ContentRect,
        client: (f64, f64),
        target: PointerTarget,
        span: Span,
        offset: GrabOffset,
    ) -> Result<Option<Placement>, GeometryError> {
        let (client_x, client_y) = client;
        if !client_x.is_finite() || !client_y.is_finite() {
            return Err(GeometryError::NonFinitePointer);
        }
        self.dragging |= (client_x - self.origin.x).hypot(client_y - self.origin.y) >= DRAG_THRESHOLD_CSS_PX;
        if !self.dragging {
            return Ok(None);
        }
        // Library tiles belong to the pointed cell; existing widgets retain
        // their original panel-space grab anchor through canvas resizing.
        if matches!(target, PointerTarget::Library(_)) {
            let cell = cell_at_in(geometry, rect, client_x, client_y)?;
            Ok(Some(Placement {
                column: cell.column,
                row: cell.row,
                span: Span::OneByOne,
            }))
        } else {
            drop_placement_in(geometry, rect, client_x, client_y, span, offset).map(Some)
        }
    }
}

/// Half-open bounds keep right/bottom edges from becoming nonexistent cells.
pub fn panel_point(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<PanelPoint, GeometryError> {
    panel_point_in(Geometry::Landscape, rect, client_x, client_y)
}

pub fn panel_point_in(
    geometry: Geometry,
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<PanelPoint, GeometryError> {
    let right = rect.left + rect.width;
    let bottom = rect.top + rect.height;
    if ![rect.left, rect.top, rect.width, rect.height, right, bottom]
        .into_iter()
        .all(f64::is_finite)
        || rect.width <= 0.0
        || rect.height <= 0.0
        || right <= rect.left
        || bottom <= rect.top
    {
        return Err(GeometryError::InvalidRect);
    }
    if !client_x.is_finite() || !client_y.is_finite() {
        return Err(GeometryError::NonFinitePointer);
    }
    if !(rect.left..right).contains(&client_x) || !(rect.top..bottom).contains(&client_y) {
        return Err(GeometryError::OutsideCanvas);
    }
    let point = PanelPoint {
        x: (client_x - rect.left) / rect.width * f64::from(geometry.width()),
        y: (client_y - rect.top) / rect.height * f64::from(geometry.height()),
    };
    // A representable CSS point can round to the outer panel edge. Reject it
    // instead of clamping it into a valid placement or relying on float casts.
    if !(0.0..f64::from(geometry.width())).contains(&point.x) || !(0.0..f64::from(geometry.height())).contains(&point.y)
    {
        return Err(GeometryError::OutsideCanvas);
    }
    Ok(point)
}

pub fn cell_at(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<Cell, GeometryError> {
    cell_at_in(Geometry::Landscape, rect, client_x, client_y)
}

pub fn cell_at_in(
    geometry: Geometry,
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<Cell, GeometryError> {
    let point = panel_point_in(geometry, rect, client_x, client_y)?;
    if point.y < f64::from(HEADER_HEIGHT) {
        return Err(GeometryError::Header);
    }
    let (column, row) = geometry
        .cell_at(point.x.floor() as i32, point.y.floor() as i32)
        .ok_or(GeometryError::OutsideGrid)?;
    Ok(Cell { column, row })
}

/// The grab offset is stored in panel pixels, so resizing the displayed canvas
/// during a drag does not move the widget's anchor to its corner.
pub fn grab_offset(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    placement: Placement,
) -> Result<GrabOffset, GeometryError> {
    grab_offset_in(Geometry::Landscape, rect, client_x, client_y, placement)
}

pub fn grab_offset_in(
    geometry: Geometry,
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    placement: Placement,
) -> Result<GrabOffset, GeometryError> {
    let point = panel_point_in(geometry, rect, client_x, client_y)?;
    let area = placement.rect_in(geometry).ok_or(GeometryError::OutsideGrid)?;
    let offset = GrabOffset {
        x: point.x - f64::from(area.x),
        y: point.y - f64::from(area.y),
    };
    if !(0.0..f64::from(area.width)).contains(&offset.x) || !(0.0..f64::from(area.height)).contains(&offset.y) {
        return Err(GeometryError::OutsideWidget);
    }
    Ok(offset)
}

/// Subtract the original grab position, then snap the widget origin to the
/// nearest cell. Half-cell ties advance right/down. No edge clamping occurs.
pub fn drop_placement(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    span: Span,
    offset: GrabOffset,
) -> Result<Placement, GeometryError> {
    drop_placement_in(Geometry::Landscape, rect, client_x, client_y, span, offset)
}

pub fn drop_placement_in(
    geometry: Geometry,
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    span: Span,
    offset: GrabOffset,
) -> Result<Placement, GeometryError> {
    let point = panel_point_in(geometry, rect, client_x, client_y)?;
    if point.y < f64::from(HEADER_HEIGHT) {
        return Err(GeometryError::Header);
    }
    let width = geometry.width() / u16::from(geometry.columns()) * u16::from(span.columns());
    // A stored anchor can originate in either of the unequal portrait rows.
    let max_height = (0..=geometry.rows() - span.rows())
        .map(|row| geometry.row_edge(row + span.rows()).unwrap() - geometry.row_edge(row).unwrap())
        .max()
        .unwrap();
    if !(0.0..f64::from(width)).contains(&offset.x) || !(0.0..f64::from(max_height)).contains(&offset.y) {
        return Err(GeometryError::InvalidOffset);
    }
    let x = point.x - offset.x;
    let y = point.y - offset.y;
    if x < 0.0 || y < f64::from(Geometry::HEADER_HEIGHT) {
        return Err(GeometryError::OutsideGrid);
    }
    // Compare the actual adjacent edge midpoints, including the final boundary.
    let nearest = |value: f64, count: u8, edge: &dyn Fn(u8) -> u16| {
        (0..count)
            .find(|&index| value < (f64::from(edge(index)) + f64::from(edge(index + 1))) / 2.)
            .unwrap_or(count)
    };
    let column = nearest(x, geometry.columns(), &|c| geometry.column_edge(c).unwrap());
    let row = nearest(y, geometry.rows(), &|r| geometry.row_edge(r).unwrap());
    let placement = Placement { column, row, span };
    placement.rect_in(geometry).ok_or(GeometryError::OutsideGrid)?;
    Ok(placement)
}

/// Guide validity uses the same validator as import, reducer commits and boot.
/// This is advisory only: the reducer still checks its pointer/revision token
/// and validates the committed transaction independently.
pub fn validate_drop(
    config: &ValidatedConfig,
    view: u8,
    target: PointerTarget,
    placement: Placement,
) -> Result<(), GeometryError> {
    validate_draft_drop(config.config(), view, target, placement)
}

/// Diagnose the layout actually visible in a rejected preview, including edits
/// already made during recovery. Only the shared validator can approve it.
pub fn validate_draft_drop(
    config: &Config,
    view: u8,
    target: PointerTarget,
    placement: Placement,
) -> Result<(), GeometryError> {
    if !config.views.iter().any(|candidate| candidate.id == view) {
        return Err(GeometryError::MissingView);
    }
    let mut candidate = config.clone();
    match target {
        PointerTarget::Widget(id) => {
            let widget = candidate
                .widgets
                .iter_mut()
                .find(|widget| widget.id == id)
                .ok_or(GeometryError::MissingWidget)?;
            widget.view = view;
            widget.placement = placement;
        }
        PointerTarget::Library(channel) => {
            if candidate.widgets.len() == MAX_WIDGETS {
                return Err(GeometryError::Capacity);
            }
            let id = (1..=MAX_WIDGETS as u16 + 1)
                .find(|id| candidate.widgets.iter().all(|widget| widget.id != *id))
                .expect("at most 63 occupied IDs before adding a widget");
            candidate
                .widgets
                .push(default_widget(channel, id, view, placement))
                .map_err(|_| GeometryError::Capacity)?;
        }
    }
    validate(candidate).map(|_| ()).map_err(GeometryError::Validation)
}

/// Row-major placement shared by library additions and duplicate commands.
/// An invalid occupied rectangle blocks placement instead of inventing free space.
pub fn first_free_placement(
    config: &Config,
    view: u8,
    span: Span,
    except: Option<u16>,
) -> Option<Placement> {
    let geometry = config.view_orientation(view)?.geometry();
    let occupied = config
        .widgets
        .iter()
        .filter(|w| w.view == view && Some(w.id) != except)
        .fold(0u8, |mask, w| {
            mask | w.placement.occupied_in(geometry).unwrap_or(u8::MAX)
        });
    for row in 0..geometry.rows() {
        for column in 0..geometry.columns() {
            let placement = Placement { column, row, span };
            if placement.occupied_in(geometry).is_some_and(|mask| mask & occupied == 0) {
                return Some(placement);
            }
        }
    }
    None
}
