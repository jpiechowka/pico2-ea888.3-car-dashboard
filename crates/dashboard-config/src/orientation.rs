//! Logical panel geometry shared by validation, rendering and editor hit tests.
use serde::{Deserialize, Serialize};

/// Deserialize `deserializer` into an explicit orientation or inheritance `None`.
/// Used with `deserialize_with` so an absent JSON property is an error while
/// explicit `null` is accepted; malformed enum values return the deserializer error.
pub(crate) fn required_override<'de, D: serde::Deserializer<'de>>(
    deserializer: D
) -> Result<Option<Orientation>, D::Error> {
    Option::<Orientation>::deserialize(deserializer)
}

/// Mounting direction. Portrait assumes a clockwise turn from landscape;
/// inverted variants turn the corresponding mounting direction by 180 degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    /// Native 320×240, four-column/two-row mounting.
    Landscape,
    /// Clockwise 240×320, two-column/four-row mounting.
    Portrait,
    /// Landscape with a 180-degree mounting reversal; logical geometry is unchanged.
    LandscapeInverted,
    /// Portrait with a 180-degree mounting reversal; logical geometry is unchanged.
    PortraitInverted,
}

impl Orientation {
    /// Stable editor enumeration, with each logical shape followed later by its reversal.
    pub const ALL: [Self; 4] = [
        Self::Landscape,
        Self::Portrait,
        Self::LandscapeInverted,
        Self::PortraitInverted,
    ];

    /// Return the logical shape, discarding only the mounting-reversal flag.
    pub const fn geometry(self) -> Geometry {
        match self {
            Self::Landscape | Self::LandscapeInverted => Geometry::Landscape,
            Self::Portrait | Self::PortraitInverted => Geometry::Portrait,
        }
    }

    /// Return whether the physical mounting reverses both panel axes.
    pub const fn inverted(self) -> bool { matches!(self, Self::LandscapeInverted | Self::PortraitInverted) }
}

/// Only the two native panel shapes are admitted. Both contain 76,800 pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Geometry {
    #[default]
    /// 320×240 pixels with four columns and two dashboard rows.
    Landscape,
    /// 240×320 pixels with two columns and four dashboard rows.
    Portrait,
}

impl Geometry {
    /// Both supported shapes; neither changes the framebuffer pixel count.
    pub const ALL: [Self; 2] = [Self::Landscape, Self::Portrait];
    /// Header height in pixels, excluded from dashboard placement and hit tests.
    pub const HEADER_HEIGHT: u16 = 26;
    /// Fixed framebuffer pixel count for either shape; RGB565 needs twice these bytes.
    pub const PIXELS: usize = 320 * 240;

    /// Return logical horizontal pixels, before physical mounting reversal.
    pub const fn width(self) -> u16 {
        match self {
            Self::Landscape => 320,
            Self::Portrait => 240,
        }
    }

    /// Return logical vertical pixels, including the header.
    pub const fn height(self) -> u16 {
        match self {
            Self::Landscape => 240,
            Self::Portrait => 320,
        }
    }

    /// Return dashboard grid columns; the grid always contains eight cells.
    pub const fn columns(self) -> u8 {
        match self {
            Self::Landscape => 4,
            Self::Portrait => 2,
        }
    }

    /// Return dashboard rows below the fixed header.
    pub const fn rows(self) -> u8 {
        match self {
            Self::Landscape => 2,
            Self::Portrait => 4,
        }
    }

    /// Return the pixel boundary for `row` in `0..=rows()`, including the final edge.
    /// Larger indices return `None`; integer division distributes leftover pixels.
    pub const fn row_edge(
        self,
        row: u8,
    ) -> Option<u16> {
        if row > self.rows() {
            return None;
        }
        Some(Self::HEADER_HEIGHT + row as u16 * (self.height() - Self::HEADER_HEIGHT) / self.rows() as u16)
    }

    /// Return the pixel boundary for `column` in `0..=columns()`, else `None`.
    /// The final boundary is the exclusive right edge at `width()`.
    pub const fn column_edge(
        self,
        column: u8,
    ) -> Option<u16> {
        if column > self.columns() {
            return None;
        }
        Some(column as u16 * self.width() / self.columns() as u16)
    }

    /// Return `(column, row)` containing logical pixel coordinates `x`, `y`.
    /// Uses placement's half-open integer edges, including uneven rows. Header,
    /// negative and outside coordinates return `None`; mounting is resolved by callers.
    pub fn cell_at(
        self,
        x: i32,
        y: i32,
    ) -> Option<(u8, u8)> {
        if x < 0 || y < i32::from(Self::HEADER_HEIGHT) || x >= i32::from(self.width()) || y >= i32::from(self.height())
        {
            return None;
        }
        let column = (x as u16 / (self.width() / u16::from(self.columns()))) as u8;
        let row = (0..self.rows()).find(|&row| y < i32::from(self.row_edge(row + 1).unwrap_or(self.height())))?;
        Some((column, row))
    }
}
