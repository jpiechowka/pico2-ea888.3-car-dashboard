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
            border: Color(0x4208),
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

/// Independent editable colour roles. Stored even when a display option is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    Background,
    Border,
    Label,
    Value,
    Secondary,
    Graph,
    Gauge,
    Track,
    Ticks,
    Average,
    Minimum,
    Maximum,
    Trend,
    Peak,
    WarningForeground,
    WarningBackground,
}
impl ColorRole {
    pub const ALL: [Self; 16] = [
        Self::Background,
        Self::Border,
        Self::Label,
        Self::Value,
        Self::Secondary,
        Self::Graph,
        Self::Gauge,
        Self::Track,
        Self::Ticks,
        Self::Average,
        Self::Minimum,
        Self::Maximum,
        Self::Trend,
        Self::Peak,
        Self::WarningForeground,
        Self::WarningBackground,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Background => "Background",
            Self::Border => "Border",
            Self::Label => "Label",
            Self::Value => "Primary value",
            Self::Secondary => "Units / AFR",
            Self::Graph => "History line",
            Self::Gauge => "Gauge indicator",
            Self::Track => "Gauge track",
            Self::Ticks => "Scale labels / ticks",
            Self::Average => "Average",
            Self::Minimum => "Minimum",
            Self::Maximum => "Maximum",
            Self::Trend => "Trend arrow",
            Self::Peak => "New peak",
            Self::WarningForeground => "Warning text",
            Self::WarningBackground => "Warning background",
        }
    }

    pub const fn get(
        self,
        colors: &WidgetColors,
    ) -> Color {
        match self {
            Self::Background => colors.background,
            Self::Border => colors.border,
            Self::Label => colors.label,
            Self::Value => colors.value,
            Self::Secondary => colors.secondary,
            Self::Graph => colors.graph,
            Self::Gauge => colors.gauge,
            Self::Track => colors.track,
            Self::Ticks => colors.ticks,
            Self::Average => colors.average,
            Self::Minimum => colors.minimum,
            Self::Maximum => colors.maximum,
            Self::Trend => colors.trend,
            Self::Peak => colors.peak,
            Self::WarningForeground => colors.warning.foreground,
            Self::WarningBackground => colors.warning.background,
        }
    }

    pub fn set(
        self,
        colors: &mut WidgetColors,
        color: Color,
    ) {
        match self {
            Self::Background => colors.background = color,
            Self::Border => colors.border = color,
            Self::Label => colors.label = color,
            Self::Value => colors.value = color,
            Self::Secondary => colors.secondary = color,
            Self::Graph => colors.graph = color,
            Self::Gauge => colors.gauge = color,
            Self::Track => colors.track = color,
            Self::Ticks => colors.ticks = color,
            Self::Average => colors.average = color,
            Self::Minimum => colors.minimum = color,
            Self::Maximum => colors.maximum = color,
            Self::Trend => colors.trend = color,
            Self::Peak => colors.peak = color,
            Self::WarningForeground => colors.warning.foreground = color,
            Self::WarningBackground => colors.warning.background = color,
        }
    }
}

impl WidgetColors {
    /// All stored foregrounds remain legible so switching styles/options cannot
    /// turn a valid document into an unrepairable typed edit. Decorative border
    /// and track may be subtle; the indicator must contrast with track and fill.
    pub fn invalid_role(self) -> Option<ColorRole> {
        use ColorRole::*;
        for role in [
            Label, Value, Secondary, Graph, Gauge, Ticks, Average, Minimum, Maximum, Trend, Peak,
        ] {
            if !role.get(&self).contrasts(self.background) {
                return Some(role);
            }
        }
        if !self.gauge.contrasts(self.track) {
            return Some(Gauge);
        }
        if !self.warning.foreground.contrasts(self.warning.background) {
            return Some(WarningForeground);
        }
        None
    }
}

impl Color {
    /// Product visibility guard, not a WCAG conformance claim. sRGB luminance
    /// of the actual quantized RGB565 colour, with a conservative 3:1 boundary.
    /// Q16 tables use the W3C/IEC sRGB transfer function, rounded to nearest.
    /// No powf, allocation or frame-time conversion is required on the Pico.
    pub fn contrasts(
        self,
        other: Self,
    ) -> bool {
        let a = self.luminance();
        let b = other.luminance();
        // One Q16 unit on each side bounds table and arithmetic rounding.
        a.max(b) + 3276 >= 3 * (a.min(b) + 3278)
    }

    fn luminance(self) -> u32 {
        (2126 * u32::from(LINEAR_5[usize::from(self.0 >> 11)])
            + 7152 * u32::from(LINEAR_6[usize::from((self.0 >> 5) & 63)])
            + 722 * u32::from(LINEAR_5[usize::from(self.0 & 31)]))
            / 10000
    }
}

const LINEAR_5: [u16; 32] = [
    0, 159, 340, 599, 997, 1453, 2013, 2681, 3570, 4488, 5530, 6700, 8177, 9635, 11235, 12980, 15122, 17187, 19407,
    21787, 24658, 27386, 30282, 33350, 37008, 40449, 44069, 47871, 52369, 56567, 60955, 65535,
];

const LINEAR_6: [u16; 64] = [
    0, 80, 159, 241, 340, 458, 599, 761, 947, 1156, 1391, 1651, 1937, 2250, 2592, 2961, 3464, 3900, 4366, 4864, 5392,
    5953, 6547, 7174, 7834, 8528, 9258, 10022, 10822, 11658, 12530, 13440, 14629, 15623, 16656, 17727, 18837, 19987,
    21177, 22407, 23678, 24990, 26344, 27739, 29176, 30656, 32179, 33745, 35764, 37429, 39138, 40891, 42690, 44534,
    46423, 48359, 50341, 52369, 54445, 56567, 58737, 60955, 63221, 65535,
];
