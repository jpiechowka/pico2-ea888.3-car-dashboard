//! Shared widget capabilities, canonical gauge limits and quantized RGB565 appearance contracts.
//! Visibility and validation are separate: hidden options/colors remain stored and admissible.
/// Shared selector/style/span truth. Readout compatibility is independently validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetCapabilities {
    /// Layout can display secondary units/readout decoration.
    pub secondary: bool,
    /// Layout supports a history trace; currently numeric widgets only.
    pub history: bool,
    /// Gauge layout supports peak/reference markers.
    pub markers: bool,
    /// Gauge layout supports bounded colored bands.
    pub bands: bool,
    /// Large gauge layout supports short labels beside bands.
    pub band_labels: bool,
    /// Layout supports accumulated average/extrema presentation.
    pub statistics: bool,
}
use serde::{Deserialize, Serialize};

use crate::Channel;

impl crate::Widget {
    /// Return selector/style/span capabilities; group widgets expose no scalar decorations.
    /// Does not mutate or discard dormant option values.
    pub fn capabilities(&self) -> WidgetCapabilities {
        if self.channel.scalar().is_none() {
            return WidgetCapabilities {
                secondary: false,
                history: false,
                markers: false,
                bands: false,
                band_labels: false,
                statistics: false,
            };
        }
        let numeric = self.gauge.kind.is_numeric();
        WidgetCapabilities {
            secondary: numeric || self.placement.span != crate::Span::OneByOne,
            history: numeric,
            markers: !numeric,
            bands: !numeric,
            band_labels: !numeric && self.placement.span == crate::Span::TwoByTwo,
            statistics: numeric || self.placement.span == crate::Span::TwoByTwo,
        }
    }

    /// Return whether the style/span has room for statistics, regardless of enabled options.
    pub fn supports_statistics(&self) -> bool { self.capabilities().statistics }

    /// Return whether graph is enabled and the current layout supports history.
    pub fn shows_graph(&self) -> bool { self.capabilities().history && self.display.graph }

    /// Return whether trend is enabled for a scalar numeric layout.
    pub fn shows_trend(&self) -> bool { self.capabilities().history && self.display.trend }

    /// Return whether average is enabled in a statistics-capable layout.
    pub fn shows_average(&self) -> bool { self.supports_statistics() && self.display.average }

    /// Return whether extrema are enabled in a statistics-capable layout.
    pub fn shows_extrema(&self) -> bool { self.supports_statistics() && self.display.extrema }
}

/// Renderer presentation family; numerical values and raw warnings keep their original semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GaugeKind {
    #[default]
    /// Numeric value with optional history and statistics.
    Numeric,
    /// Circular arc indicator.
    Circular,
    /// Needle dial indicator.
    Dial,
    /// Continuous horizontal bar.
    Horizontal,
    /// Continuous vertical bar.
    Vertical,
    /// Discrete horizontal segment indicator.
    Segmented,
}
impl GaugeKind {
    /// Stable complete style picker order.
    pub const ALL: [Self; 6] = [
        Self::Numeric,
        Self::Circular,
        Self::Dial,
        Self::Horizontal,
        Self::Vertical,
        Self::Segmented,
    ];

    /// Return the fixed human-readable style label.
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

    /// Return whether the style is the numeric/history renderer rather than a gauge.
    pub const fn is_numeric(self) -> bool { matches!(self, Self::Numeric) }
}

/// Candidate gauge bounds in canonical units; display spans are not operating-limit advice.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gauge {
    /// Presentation family, independent of channel selection.
    pub kind: GaugeKind,
    /// Initial lower finite channel-domain limit.
    pub min: f32,
    /// Initial upper finite channel-domain limit, at least 0.001 above `min`.
    pub max: f32,
    /// Expand from initial limits using producer extrema; statistics reset restores the baseline.
    pub expand: bool,
}
impl Gauge {
    /// Return numeric, nonexpanding display defaults for `channel`; no warning threshold is implied.
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

    /// Return whether endpoints are finite, within `channel`'s domain and separated by at least 0.001.
    /// Shape selection and automatic expansion do not weaken initial endpoint admission.
    pub fn valid(
        self,
        channel: Channel,
    ) -> bool {
        self.valid_domain(channel.domain())
    }

    /// Return whether endpoints fit the finite inclusive `domain` and are at least 0.001 apart.
    /// Used for dormant group bounds without introducing a scalar identity.
    pub fn valid_domain(
        self,
        domain: (f32, f32),
    ) -> bool {
        let (low, high) = domain;
        self.min.is_finite()
            && self.max.is_finite()
            && self.min >= low
            && self.max <= high
            && self.max - self.min >= 0.001
    }

    /// Return configured bounds, expanded by `minimum`/`maximum` only when enabled.
    /// Missing, nonfinite or absolute values above 1,000,000 are ignored. This method
    /// assumes initial bounds were admitted and retains no history; producer statistics
    /// define expansion/reset lifetime. A reversed extrema pair cannot shrink the baseline.
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

/// Native panel colour, two bytes in RAM. Human-readable files use `#RRGGBB`;
/// RGB888 input is quantized once and exports the actual represented colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(
    /// Packed red:5/green:6/blue:5 bits; byte transmission order belongs to the display driver.
    pub u16,
);
impl Color {
    /// Return RGB565 by truncating low bits of eight-bit `r`, `g`, `b` channels.
    /// All inputs are accepted; this performs quantization, not gamma conversion.
    pub const fn rgb(
        r: u8,
        g: u8,
        b: u8,
    ) -> Self {
        Self(((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3))
    }

    /// Return represented `[red, green, blue]` bytes using high-bit replication.
    /// Original RGB888 input low bits cannot be recovered after quantization.
    pub const fn rgb888(self) -> [u8; 3] {
        let r = ((self.0 >> 11) & 31) as u8;
        let g = ((self.0 >> 5) & 63) as u8;
        let b = (self.0 & 31) as u8;
        [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
    }

    /// Parse exact `#RRGGBB` from `text`, case-insensitive hex, and return quantized color.
    /// Missing prefix, wrong length or nonhex characters return `None`; no whitespace is trimmed.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let raw = u32::from_str_radix(digits, 16).ok()?;
        Some(Self::rgb((raw >> 16) as u8, (raw >> 8) as u8, raw as u8))
    }

    /// Return uppercase `#RRGGBB` for the represented color, not the pre-quantization input.
    /// Seven-byte output fits its fixed-capacity string without allocation.
    pub fn hex(self) -> heapless::String<7> {
        use core::fmt::Write;
        let [r, g, b] = self.rgb888();
        let mut text = heapless::String::new();
        write!(text, "#{r:02X}{g:02X}{b:02X}").ok();
        text
    }
}
impl Serialize for Color {
    /// Write represented hex text or packed binary `u16` to `serializer` according to its mode.
    /// Returns serializer output/error; byte endianness is delegated to the binary format.
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
    /// Read color from `deserializer`; returns bounded hex parsing errors in human mode.
    /// Binary mode accepts every `u16` bit pattern as RGB565.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let text = heapless::String::<7>::deserialize(deserializer)?;
            Self::parse(&text).ok_or_else(|| serde::de::Error::custom("expected #RRGGBB"))
        } else {
            u16::deserialize(deserializer).map(Self)
        }
    }
}

/// Popup text/fill colors; admission requires foreground contrast against background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningColors {
    /// Popup text color.
    pub foreground: Color,
    /// Popup fill color.
    pub background: Color,
}
impl Default for WarningColors {
    /// Return white text on RGB565 red.
    fn default() -> Self {
        Self {
            foreground: Color(0xFFFF),
            background: Color(0xF800),
        }
    }
}

/// Complete retained per-widget palette, including currently hidden decorations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetColors {
    /// Widget fill against which stored foreground roles are checked.
    pub background: Color,
    /// Decorative border; no foreground contrast floor is required.
    pub border: Color,
    /// Channel/custom label text.
    pub label: Color,
    /// Primary numeric value.
    pub value: Color,
    /// Secondary units/AFR decoration.
    pub secondary: Color,
    /// Historical trace.
    pub graph: Color,
    /// Gauge indicator, checked against both background and track.
    pub gauge: Color,
    /// Gauge track, permitted to be subtle against background.
    pub track: Color,
    /// Scale labels and ticks.
    pub ticks: Color,
    /// Average statistic/marker.
    pub average: Color,
    /// Minimum statistic/marker.
    pub minimum: Color,
    /// Maximum statistic/marker.
    pub maximum: Color,
    /// Trend arrow.
    pub trend: Color,
    /// Peak indicator.
    pub peak: Color,
    /// Per-widget warning text/fill pair.
    pub warning: WarningColors,
}
impl Default for WidgetColors {
    /// Return the fixed dark palette with white text, red gauge, orange average and yellow peak.
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
    /// Widget fill.
    Background,
    /// Decorative outline.
    Border,
    /// Widget label text.
    Label,
    /// Primary number.
    Value,
    /// Secondary units/AFR text.
    Secondary,
    /// Historical trace.
    Graph,
    /// Gauge indicator.
    Gauge,
    /// Gauge track.
    Track,
    /// Gauge scale labels/ticks.
    Ticks,
    /// Average statistic/marker.
    Average,
    /// Minimum statistic/marker.
    Minimum,
    /// Maximum statistic/marker.
    Maximum,
    /// Trend arrow.
    Trend,
    /// Peak indication.
    Peak,
    /// Per-widget warning text.
    WarningForeground,
    /// Per-widget warning fill.
    WarningBackground,
}
impl ColorRole {
    /// Stable full palette-editor enumeration, including hidden roles.
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

    /// Return the fixed human-readable palette field label.
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

    /// Return this role's retained color from `colors`, independent of current visibility.
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

    /// Set this role in `colors` to `color`; does not validate contrast or alter other roles.
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
    /// Return the first role failing contrast, or `None` if all required pairs pass.
    /// All stored foregrounds remain legible so switching styles/options cannot
    /// turn a valid document into an unrepairable typed edit. Decorative border
    /// and track may be subtle; the indicator must contrast with track and fill.
    pub fn invalid_role(self) -> Option<ColorRole> {
        let mut first = None;
        let _ = self.visit_invalid_roles(|role| {
            first = Some(role);
            core::ops::ControlFlow::Break(())
        });
        first
    }

    /// Call `emit` for every failing stored foreground pair, stopping on visitor break.
    /// Returns `Continue(())` after traversal. Gauge may be reported twice when it fails
    /// both background and track comparisons; decorative border/track have no own floor.
    pub(crate) fn visit_invalid_roles(
        self,
        mut emit: impl FnMut(ColorRole) -> core::ops::ControlFlow<()>,
    ) -> core::ops::ControlFlow<()> {
        use ColorRole::*;
        for role in [
            Label, Value, Secondary, Graph, Gauge, Ticks, Average, Minimum, Maximum, Trend, Peak,
        ] {
            if !role.get(&self).contrasts(self.background) {
                emit(role)?;
            }
        }
        if !self.gauge.contrasts(self.track) {
            emit(Gauge)?;
        }
        if !self.warning.foreground.contrasts(self.warning.background) {
            emit(WarningForeground)?;
        }
        core::ops::ControlFlow::Continue(())
    }
}

impl Color {
    /// Return whether this color and `other` pass the conservative product contrast floor.
    /// Product visibility guard, not a WCAG conformance claim. sRGB luminance
    /// of the actual quantized RGB565 colour, with a conservative 3:1 boundary.
    /// Q16 tables use the W3C/IEC sRGB transfer function, rounded to nearest.
    /// No powf, allocation or frame-time conversion is required on the Pico.
    /// Transfer definition: <https://www.w3.org/TR/css-color-4/#color-conversion-code>.
    pub fn contrasts(
        self,
        other: Self,
    ) -> bool {
        let a = self.luminance();
        let b = other.luminance();
        // One Q16 unit on each side bounds table and arithmetic rounding.
        a.max(b) + 3276 >= 3 * (a.min(b) + 3278)
    }

    /// Return Q16 relative luminance from represented sRGB channels and fixed integer weights.
    /// Output lies in 0..=65535; intermediate sums fit `u32` for every RGB565 pattern.
    fn luminance(self) -> u32 {
        (2126 * u32::from(LINEAR_5[usize::from(self.0 >> 11)])
            + 7152 * u32::from(LINEAR_6[usize::from((self.0 >> 5) & 63)])
            + 722 * u32::from(LINEAR_5[usize::from(self.0 & 31)]))
            / 10000
    }
}

/// Rounded Q16 linear-light values for each replicated five-bit sRGB red/blue level.
const LINEAR_5: [u16; 32] = [
    0, 159, 340, 599, 997, 1453, 2013, 2681, 3570, 4488, 5530, 6700, 8177, 9635, 11235, 12980, 15122, 17187, 19407,
    21787, 24658, 27386, 30282, 33350, 37008, 40449, 44069, 47871, 52369, 56567, 60955, 65535,
];

/// Rounded Q16 linear-light values for each replicated six-bit sRGB green level.
const LINEAR_6: [u16; 64] = [
    0, 80, 159, 241, 340, 458, 599, 761, 947, 1156, 1391, 1651, 1937, 2250, 2592, 2961, 3464, 3900, 4366, 4864, 5392,
    5953, 6547, 7174, 7834, 8528, 9258, 10022, 10822, 11658, 12530, 13440, 14629, 15623, 16656, 17727, 18837, 19987,
    21177, 22407, 23678, 24990, 26344, 27739, 29176, 30656, 32179, 33745, 35764, 37429, 39138, 40891, 42690, 44534,
    46423, 48359, 50341, 52369, 54445, 56567, 58737, 60955, 63221, 65535,
];
