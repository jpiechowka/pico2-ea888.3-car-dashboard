//! Logical panel geometry shared by validation, rendering and editor hit tests.
use serde::{Deserialize, Serialize};

/// Mounting direction. Portrait assumes a clockwise turn from landscape;
/// inverted variants turn the corresponding mounting direction by 180 degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Landscape,
    Portrait,
    LandscapeInverted,
    PortraitInverted,
}

impl Orientation {
    pub const ALL: [Self; 4] = [
        Self::Landscape,
        Self::Portrait,
        Self::LandscapeInverted,
        Self::PortraitInverted,
    ];

    pub const fn geometry(self) -> Geometry {
        match self {
            Self::Landscape | Self::LandscapeInverted => Geometry::Landscape,
            Self::Portrait | Self::PortraitInverted => Geometry::Portrait,
        }
    }

    pub const fn inverted(self) -> bool { matches!(self, Self::LandscapeInverted | Self::PortraitInverted) }
}

/// Only the two native panel shapes are admitted. Both contain 76,800 pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Geometry {
    #[default]
    Landscape,
    Portrait,
}

impl Geometry {
    pub const ALL: [Self; 2] = [Self::Landscape, Self::Portrait];
    pub const HEADER_HEIGHT: u16 = 26;
    pub const PIXELS: usize = 320 * 240;

    pub const fn width(self) -> u16 {
        match self {
            Self::Landscape => 320,
            Self::Portrait => 240,
        }
    }

    pub const fn height(self) -> u16 {
        match self {
            Self::Landscape => 240,
            Self::Portrait => 320,
        }
    }

    pub const fn columns(self) -> u8 {
        match self {
            Self::Landscape => 4,
            Self::Portrait => 2,
        }
    }

    pub const fn rows(self) -> u8 {
        match self {
            Self::Landscape => 2,
            Self::Portrait => 4,
        }
    }

    /// Includes the final edge; integer division distributes leftover pixels.
    pub const fn row_edge(
        self,
        row: u8,
    ) -> Option<u16> {
        if row > self.rows() {
            return None;
        }
        Some(Self::HEADER_HEIGHT + row as u16 * (self.height() - Self::HEADER_HEIGHT) / self.rows() as u16)
    }

    pub const fn column_edge(
        self,
        column: u8,
    ) -> Option<u16> {
        if column > self.columns() {
            return None;
        }
        Some(column as u16 * self.width() / self.columns() as u16)
    }

    /// Uses the same half-open integer edges as placement, including odd rows.
    /// Header, negative and outside coordinates never select a dashboard cell.
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
