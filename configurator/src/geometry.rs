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
    /// Left content edge in client CSS pixels, excluding borders/letterboxing.
    pub left: f64,
    /// Top content edge in client CSS pixels.
    pub top: f64,
    /// Positive content width in CSS pixels; checked before mapping.
    pub width: f64,
    /// Positive content height in CSS pixels; checked before mapping.
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Two-dimensional point; mapping results use panel pixels, drag origins use CSS pixels.
pub struct PanelPoint {
    /// Horizontal coordinate in the producing operation's coordinate space.
    pub x: f64,
    /// Vertical coordinate in the producing operation's coordinate space.
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Zero-based dashboard grid location below the header.
pub struct Cell {
    /// Horizontal cell index within the effective geometry.
    pub column: u8,
    /// Vertical cell index within the effective geometry.
    pub row: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Pointer distance from the grabbed widget's top-left corner in native panel pixels.
pub struct GrabOffset {
    /// Horizontal distance; must remain inside the widget width.
    x: f64,
    /// Vertical distance; validated against supported row heights when dropped.
    y: f64,
}

impl GrabOffset {
    /// Anchor at the widget's top-left corner, with zero panel-pixel displacement.
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };

    /// Returns the center of `span` placed in the first landscape row.
    pub fn center(span: Span) -> Self { Self::center_in(Geometry::Landscape, span) }

    /// Returns the center of `span` at row/column zero in `geometry`, in panel pixels.
    /// All current span/geometry combinations fit there; adding a nonfitting span
    /// without updating this invariant would panic.
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
/// Mapping/admission rejection; callers retain the last accepted placement.
pub enum GeometryError {
    /// Nonfinite, nonpositive or unrepresentable CSS content bounds.
    InvalidRect,
    /// A supplied pointer coordinate is NaN or infinite.
    NonFinitePointer,
    /// Pointer lies outside half-open canvas bounds, including rounded panel edges.
    OutsideCanvas,
    /// Pointer lands in the noninteractive header.
    Header,
    /// Pointer does not lie inside the widget being grabbed.
    OutsideWidget,
    /// Grab offset is nonfinite, negative or outside the widget's possible extent.
    InvalidOffset,
    /// The snapped widget cannot fit within the effective grid.
    OutsideGrid,
    /// Requested existing widget ID does not occur in the candidate configuration.
    MissingWidget,
    /// Requested view ID does not occur in the candidate configuration.
    MissingView,
    /// Adding a library widget would exceed the bounded widget collection.
    Capacity,
    /// Shared full-configuration validation rejected the placement candidate.
    Validation(
        /// Exact full-configuration validation rejection.
        ValidationError,
    ),
}

/// A physical client-space movement is required before layout mapping. Canvas
/// reflow, scroll and resize alone never turn a click into a placement commit.
pub const DRAG_THRESHOLD_CSS_PX: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
/// Pointer-down origin and sticky movement threshold for one drag gesture.
pub struct DragGesture {
    /// Original pointer location in client CSS pixels, independent of canvas reflow.
    origin: PanelPoint,
    /// Once true, remains true even if the pointer returns to its origin.
    dragging: bool,
}
impl DragGesture {
    /// Returns a stationary gesture at `client_x`/`client_y` in CSS pixels, or
    /// `NonFinitePointer` if either coordinate is not finite.
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

    /// Returns whether this gesture has ever crossed the CSS movement threshold.
    pub fn is_dragging(self) -> bool { self.dragging }

    /// The threshold is Euclidean distance in CSS pixels, independent of canvas
    /// scale. Once crossed, a gesture remains a drag even if it returns to origin.
    /// Maps `client_x`/`client_y` inside `rect` using landscape geometry and the
    /// supplied `target`, `span` and panel-pixel `offset`. Returns `Ok(None)` before
    /// the threshold, otherwise the placement or the mapping rejection.
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

    /// Update movement from CSS `client` and map into `geometry` using `rect`.
    /// Returns `Ok(None)` before the threshold; afterward library `target`s create
    /// 1×1 placements, while existing widgets preserve `span` and panel `offset`.
    /// Mapping errors do not clear an already-crossed drag threshold.
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
/// Returns landscape panel pixels for CSS `client_x`/`client_y` inside `rect`,
/// or the same validation errors as [`panel_point_in`].
pub fn panel_point(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<PanelPoint, GeometryError> {
    panel_point_in(Geometry::Landscape, rect, client_x, client_y)
}

/// Returns native `geometry` panel coordinates for CSS `client_x`/`client_y` in `rect`.
/// Rejects malformed rectangle bounds, nonfinite pointers, exterior points and
/// scaling that rounds onto the excluded right/bottom panel edge. Never clamps.
/// Coordinates match [`PointerEvent.clientX`](https://www.w3.org/TR/pointerevents3/#pointerevent-interface)
/// and the content box supplied by the browser adapter.
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

/// Returns the landscape cell under CSS `client_x`/`client_y` in `rect`.
/// Propagates rectangle/pointer errors and rejects header or exterior grid positions.
pub fn cell_at(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
) -> Result<Cell, GeometryError> {
    cell_at_in(Geometry::Landscape, rect, client_x, client_y)
}

/// Returns the `geometry` cell under CSS `client_x`/`client_y` in `rect`.
/// Panel coordinates are floored before grid lookup; the header has no cell.
/// Returns mapping errors or `OutsideGrid` when no cell contains the mapped point.
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
/// Returns the landscape offset for CSS `client_x`/`client_y` in `rect` relative
/// to `placement`, or the same mapping/containment errors as [`grab_offset_in`].
pub fn grab_offset(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    placement: Placement,
) -> Result<GrabOffset, GeometryError> {
    grab_offset_in(Geometry::Landscape, rect, client_x, client_y, placement)
}

/// Returns the panel-pixel offset of CSS `client_x`/`client_y` in `rect` from
/// `placement` in `geometry`. Rejects invalid mapping/placement and pointers outside
/// the widget's half-open rectangle; never changes the placement.
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
/// Returns a landscape placement for CSS `client_x`/`client_y` in `rect`, preserving
/// `span` and panel-pixel `offset`, or the same errors as [`drop_placement_in`].
pub fn drop_placement(
    rect: ContentRect,
    client_x: f64,
    client_y: f64,
    span: Span,
    offset: GrabOffset,
) -> Result<Placement, GeometryError> {
    drop_placement_in(Geometry::Landscape, rect, client_x, client_y, span, offset)
}

/// Returns a snapped `span` placement in `geometry` from CSS `client_x`/`client_y`
/// inside `rect`, subtracting the original panel-pixel `offset` first.
/// Rejects mapping/header errors, invalid offsets and out-of-grid results. Portrait
/// offsets allow either row height; nearest-edge midpoint ties advance right/down.
/// Internal edge lookups rely on every supported span fitting the supported grids.
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
/// Tests placing `target` at `placement` in view ID `view` of `config`; returns
/// `Ok(())` only for a fully valid cloned candidate, otherwise the admission error.
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
/// Clones `config`, moves/adds `target` at `placement` in view ID `view`, and returns
/// `Ok(())` only after full validation. Missing IDs, capacity and validation errors
/// leave the supplied configuration untouched. New widget IDs are chosen unused.
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
/// Returns the first free location for `span` in view ID `view` of `config`, ignoring
/// widget ID `except` when present. Returns `None` for a missing view, no fit or an
/// invalid occupied rectangle; this placement search does not run full validation.
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
