//! Compact presentation types. All ranges use the channel's canonical units.
use serde::{Deserialize, Serialize};

use crate::Channel;

impl crate::Widget {
    pub fn supports_statistics(&self) -> bool {
        self.gauge.kind.is_numeric() || self.placement.span == crate::Span::TwoByTwo
    }

    pub fn shows_graph(&self) -> bool { self.gauge.kind.is_numeric() && self.display.graph }

    pub fn shows_trend(&self) -> bool { self.gauge.kind.is_numeric() && self.display.trend }

    pub fn shows_average(&self) -> bool { self.supports_statistics() && self.display.average }

    pub fn shows_extrema(&self) -> bool { self.supports_statistics() && self.display.extrema }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GaugeKind {
    #[default]
    Numeric,
    Circular,
    Dial,
    Horizontal,
    Vertical,
    Segmented,
}
impl GaugeKind {
    pub const ALL: [Self; 6] = [
        Self::Numeric,
        Self::Circular,
        Self::Dial,
        Self::Horizontal,
        Self::Vertical,
        Self::Segmented,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Numeric => "Numeric & history",
            Self::Circular => "Circular arc",
            Self::Dial => "Needle dial",
            Self::Horizontal => "Horizontal bar",
            Self::Vertical => "Vertical bar",
            Self::Segmented => "Segmented bar",
        }
    }

    pub const fn is_numeric(self) -> bool { matches!(self, Self::Numeric) }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gauge {
    pub kind: GaugeKind,
    pub min: f32,
    pub max: f32,
    pub expand: bool,
}
impl Gauge {
    pub const fn for_channel(channel: Channel) -> Self {
        let (min, max) = match channel {
            Channel::Boost => (-1.0, 3.0),
            Channel::Oil => (0.0, 150.0),
            Channel::Water => (40.0, 130.0),
            Channel::Dsg => (0.0, 140.0),
            Channel::Iat => (-20.0, 80.0),
            Channel::Egt => (0.0, 1200.0),
            Channel::Battery => (8.0, 16.0),
            Channel::Lambda => (0.7, 1.3),
        };
        Self {
            kind: GaugeKind::Numeric,
            min,
            max,
            expand: false,
        }
    }

    pub fn valid(
        self,
        channel: Channel,
    ) -> bool {
        let (low, high) = channel.domain();
        self.min.is_finite()
            && self.max.is_finite()
            && self.min >= low
            && self.max <= high
            && self.max - self.min >= 0.001
    }

    /// No history of its own: producer statistics define expansion/reset lifetime.
    pub fn limits(
        self,
        minimum: Option<f32>,
        maximum: Option<f32>,
    ) -> (f32, f32) {
        let mut bounds = (self.min, self.max);
        if self.expand {
            if let Some(min) = minimum.filter(|v| v.is_finite() && v.abs() <= 1_000_000.0) {
                bounds.0 = bounds.0.min(min);
            }
            if let Some(max) = maximum.filter(|v| v.is_finite() && v.abs() <= 1_000_000.0) {
                bounds.1 = bounds.1.max(max);
            }
        }
        bounds
    }
}

/// Native panel colour, two bytes in RAM. Human-readable files use #RRGGBB;
/// RGB888 input is quantized once and exports the actual represented colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u16);
impl Color {
    pub const fn rgb(
        r: u8,
        g: u8,
        b: u8,
    ) -> Self {
        Self(((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3))
    }

    pub const fn rgb888(self) -> [u8; 3] {
        let r = ((self.0 >> 11) & 31) as u8;
        let g = ((self.0 >> 5) & 63) as u8;
        let b = (self.0 & 31) as u8;
        [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
    }

    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let raw = u32::from_str_radix(digits, 16).ok()?;
        Some(Self::rgb((raw >> 16) as u8, (raw >> 8) as u8, raw as u8))
    }

    pub fn hex(self) -> heapless::String<7> {
        use core::fmt::Write;
        let [r, g, b] = self.rgb888();
        let mut text = heapless::String::new();
        write!(text, "#{r:02X}{g:02X}{b:02X}").ok();
        text
    }
}
impl Serialize for Color {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.hex())
        } else {
            serializer.serialize_u16(self.0)
        }
    }
}
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let text = heapless::String::<7>::deserialize(deserializer)?;
            Self::parse(&text).ok_or_else(|| serde::de::Error::custom("expected #RRGGBB"))
        } else {
            u16::deserialize(deserializer).map(Self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningColors {
    pub foreground: Color,
    pub background: Color,
}
impl Default for WarningColors {
    fn default() -> Self {
        Self {
            foreground: Color(0xFFFF),
            background: Color(0xF800),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetColors {
    pub background: Color,
    pub border: Color,
    pub label: Color,
    pub value: Color,
    pub secondary: Color,
    pub graph: Color,
    pub gauge: Color,
    pub track: Color,
    pub ticks: Color,
    pub average: Color,
    pub minimum: Color,
    pub maximum: Color,
    pub trend: Color,
    pub peak: Color,
    pub warning: WarningColors,
}
impl Default for WidgetColors {
    fn default() -> Self {
        Self {
            background: Color(0),
            border: Color::rgb(60, 60, 60),
            label: Color(0xFFFF),
            value: Color(0xFFFF),
            secondary: Color(0xFFFF),
            graph: Color(0xFFFF),
            gauge: Color::rgb(255, 48, 72),
            track: Color::rgb(44, 48, 56),
            ticks: Color::rgb(156, 168, 184),
            average: Color::rgb(255, 165, 0),
            minimum: Color(0xFFFF),
            maximum: Color(0xFFFF),
            trend: Color(0xFFFF),
            peak: Color(0xFFE0),
            warning: WarningColors::default(),
        }
    }
}
