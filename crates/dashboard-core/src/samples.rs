//! Canonical quantities and source/quality metadata. Numeric validity is separate
//! from vehicle support: a correctly decoded OBD reply is not ECU validation.
/// Explicit data provenance mode; transport loss never selects synthetic data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataMode {
    /// Source-checked vehicle samples; compatibility/qualification remains external.
    Vehicle,
    /// Synthetic samples used for preview/demo with separate source admission.
    Demo,
}

/// Stable canonical channel order used by eight-element acquisition arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum SensorId {
    /// Gauge pressure in bar relative to ambient, not absolute manifold pressure.
    Boost,
    /// Engine oil temperature in degrees Celsius.
    Oil,
    /// Engine coolant temperature in degrees Celsius.
    Water,
    /// Transmission oil temperature in degrees Celsius; no vehicle source inferred.
    Dsg,
    /// Intake air temperature in degrees Celsius.
    Iat,
    /// Exhaust temperature in degrees Celsius; no physical sensor location inferred.
    Egt,
    /// ECU/control-module supply volts, not a direct battery-terminal measurement.
    Battery,
    /// Dimensionless lambda ratio; AFR conversion requires a fuel assumption.
    Lambda,
}

impl SensorId {
    /// Complete array-index order; discriminants index this array and snapshot fields.
    pub const ALL: [Self; 8] = [
        Self::Boost,
        Self::Oil,
        Self::Water,
        Self::Dsg,
        Self::Iat,
        Self::Egt,
        Self::Battery,
        Self::Lambda,
    ];

    /// Returns the corresponding configuration channel without changing units.
    pub const fn channel(self) -> dashboard_config::Channel {
        use dashboard_config::Channel;
        match self {
            Self::Boost => Channel::Boost,
            Self::Oil => Channel::Oil,
            Self::Water => Channel::Water,
            Self::Dsg => Channel::Dsg,
            Self::Iat => Channel::Iat,
            Self::Egt => Channel::Egt,
            Self::Battery => Channel::Battery,
            Self::Lambda => Channel::Lambda,
        }
    }

    /// Returns the runtime channel corresponding exactly to configuration `channel`.
    pub const fn from_channel(channel: dashboard_config::Channel) -> Self {
        use dashboard_config::Channel;
        match channel {
            Channel::Boost => Self::Boost,
            Channel::Oil => Self::Oil,
            Channel::Water => Self::Water,
            Channel::Dsg => Self::Dsg,
            Channel::Iat => Self::Iat,
            Channel::Egt => Self::Egt,
            Channel::Battery => Self::Battery,
            Channel::Lambda => Self::Lambda,
        }
    }

    /// Returns the short static dashboard label, distinguishing relative boost and ECU volts.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Boost => "BOOST REL",
            Self::Oil => "OIL",
            Self::Water => "COOL",
            Self::Dsg => "DSG OIL",
            Self::Iat => "IAT",
            Self::Egt => "EGT",
            Self::Battery => "ECU V",
            Self::Lambda => "LAMBDA",
        }
    }

    /// Provisional presentation limits, not proven vehicle polling rates.
    /// Returns the maximum sample age in milliseconds for this channel.
    pub const fn stale_after_ms(self) -> u64 { self.channel().freshness_cap() as u64 }
}

/// Availability classification; a retained last-good value does not imply validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// No qualified sample or more specific failure has been observed.
    Unknown,
    /// Accepted value, subject to effective freshness/source checks at presentation.
    Valid,
    /// Previously accepted sample exceeded its effective age limit.
    Stale,
    /// Required capability/source is unavailable or not admitted.
    Unsupported,
    /// Link/session failure prevents using the retained sample.
    TransportError,
    /// Payload/value or timestamp consistency failed validation.
    InvalidPayload,
}

impl Quality {
    /// Returns the short diagnostic label; unavailable states are distinct from zero.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "NO DATA",
            Self::Valid => "VALID",
            Self::Stale => "STALE",
            Self::Unsupported => "N/A",
            Self::TransportError => "LINK ERR",
            Self::InvalidPayload => "INVALID",
        }
    }
}

/// Typed provenance admitted independently from numeric and freshness validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// No source known; never admits a live numeric value.
    Unknown,
    /// Shared synthetic generator/fixture, admitted only in Demo mode.
    Demo,
    /// Generic OBD parameter from an 11-bit diagnostic responder.
    Obd {
        /// Response CAN ID; source matching accepts only 0x7E8..=0x7EF.
        ecu: u16,
        /// Service-01 PID matched to the canonical quantity by the allowlist.
        pid: u8,
    },
    /// Gauge pressure derived from same-ECU MAP and ambient pressure, with
    /// freshness/skew and MAP saturation checked by the decoder.
    ObdMapDifference {
        /// Common responder for both pressure inputs, in 0x7E8..=0x7EF.
        ecu: u16,
    },
}

impl Source {
    /// Returns whether provenance is allowed for `sensor` in explicit `mode`.
    /// Vehicle supports the listed temperature/voltage/wideband PIDs and derived
    /// boost only; Demo accepts only Demo. Does not establish decoder scaling,
    /// configured-source selection, operating context or exact ECU compatibility.
    pub fn matches(
        self,
        sensor: SensorId,
        mode: DataMode,
    ) -> bool {
        match (mode, self, sensor) {
            (DataMode::Demo, Self::Demo, _) => true,
            (DataMode::Vehicle, Self::ObdMapDifference { ecu }, SensorId::Boost) => (0x7E8..=0x7EF).contains(&ecu),
            (DataMode::Vehicle, Self::Obd { ecu, pid }, sensor) if (0x7E8..=0x7EF).contains(&ecu) => {
                matches!(
                    (sensor, pid),
                    (SensorId::Water, 0x05)
                        | (SensorId::Iat, 0x0F)
                        | (SensorId::Oil, 0x5C)
                        | (SensorId::Battery, 0x42)
                        | (SensorId::Lambda, 0x24 | 0x34)
                )
            }
            _ => false,
        }
    }
}

/// Canonical unvalidated numeric payload; ingestion checks representability while
/// protocol decoders own physical/scaling bounds. NaN/infinity can be constructed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Quantity {
    /// Manifold pressure relative to ambient; negative vacuum is representable.
    BoostBar(
        /// Relative pressure in bar.
        f32,
    ),
    /// Engine oil temperature.
    EngineOilC(
        /// Degrees Celsius.
        f32,
    ),
    /// Engine coolant temperature.
    CoolantC(
        /// Degrees Celsius.
        f32,
    ),
    /// Transmission oil temperature; no source capability is implied.
    TransmissionOilC(
        /// Degrees Celsius.
        f32,
    ),
    /// Intake air temperature.
    IntakeC(
        /// Degrees Celsius.
        f32,
    ),
    /// Exhaust temperature, whose physical location is source-dependent.
    ExhaustC(
        /// Degrees Celsius.
        f32,
    ),
    /// Reported control-module supply potential.
    ModuleVolts(
        /// Volts.
        f32,
    ),
    /// Fuel-independent canonical mixture ratio, not a derived AFR.
    Lambda(
        /// Dimensionless lambda value.
        f32,
    ),
}

impl Quantity {
    /// Returns the canonical destination channel selected by this quantity variant.
    pub const fn sensor(self) -> SensorId {
        match self {
            Self::BoostBar(_) => SensorId::Boost,
            Self::EngineOilC(_) => SensorId::Oil,
            Self::CoolantC(_) => SensorId::Water,
            Self::TransmissionOilC(_) => SensorId::Dsg,
            Self::IntakeC(_) => SensorId::Iat,
            Self::ExhaustC(_) => SensorId::Egt,
            Self::ModuleVolts(_) => SensorId::Battery,
            Self::Lambda(_) => SensorId::Lambda,
        }
    }

    /// Returns the unchanged numeric payload in the variant's canonical units;
    /// no finite/range/source validation is performed.
    pub const fn value(self) -> f32 {
        match self {
            Self::BoostBar(v)
            | Self::EngineOilC(v)
            | Self::CoolantC(v)
            | Self::TransmissionOilC(v)
            | Self::IntakeC(v)
            | Self::ExhaustC(v)
            | Self::ModuleVolts(v)
            | Self::Lambda(v) => v,
        }
    }
}

/// Typed candidate observation; construction alone does not qualify its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Canonical quantity identifying its sensor and units.
    pub value: Quantity,
    /// Original receive/capture timestamp in monotonic milliseconds.
    pub received_at_ms: u64,
    /// Nonzero strictly increasing per-channel producer sequence.
    pub sequence: u64,
    /// Provenance bound to the decoder/session producing the sample.
    pub source: Source,
}

/// Typed admission failure; numeric failure may publish invalid quality while
/// future/order/source failures are rejected before acquisition mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    /// Sample receive time is later than the supplied ingestion clock.
    Future,
    /// Sequence/time is replayed, regressed or older than a known status change.
    OutOfOrder,
    /// Provenance mismatches channel/mode/configuration or changes within a channel.
    Source,
    /// Numeric payload is nonfinite or outside the shared statistics' representable range.
    InvalidValue,
}

/// Channel metadata published coherently with its retained statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleMeta {
    /// Timestamp of last accepted good sample, or `None` before one exists.
    pub received_at_ms: Option<u64>,
    /// Last sample/fault status timestamp in monotonic milliseconds.
    pub status_at_ms: u64,
    /// Last consumed typed sample sequence; faults need not advance this counter.
    pub sequence: u64,
    /// Last admitted sample provenance, retained through faults.
    pub source: Source,
    /// Stored quality; effective freshness is calculated at the reader's clock.
    pub quality: Quality,
}

impl SampleMeta {
    /// No good sample, source or sequence; zero timestamps are not validity evidence.
    pub const UNKNOWN: Self = Self {
        received_at_ms: None,
        status_at_ms: 0,
        sequence: 0,
        source: Source::Unknown,
        quality: Quality::Unknown,
    };

    /// Returns finite `value` for `sensor` in `mode` only when `sampled_at_ms`
    /// equals the metadata receive time and quality is fresh at `now_ms`, using
    /// the channel age cap. Times are monotonic milliseconds; otherwise returns `None`.
    pub fn value_at(
        self,
        sensor: SensorId,
        mode: DataMode,
        value: Option<f32>,
        sampled_at_ms: Option<u64>,
        now_ms: u64,
    ) -> Option<f32> {
        self.value_with_freshness(sensor, mode, value, sampled_at_ms, now_ms, sensor.stale_after_ms())
    }

    /// Returns finite `value` for `sensor` in `mode` at monotonic `now_ms` only
    /// when source matches, stored quality is Valid, and `sampled_at_ms` equals
    /// the last-good timestamp. `freshness_ms` may tighten but never extend the
    /// channel cap. Returns `None` on any failed check without altering metadata.
    pub fn value_with_freshness(
        self,
        sensor: SensorId,
        mode: DataMode,
        value: Option<f32>,
        sampled_at_ms: Option<u64>,
        now_ms: u64,
        freshness_ms: u64,
    ) -> Option<f32> {
        if self.quality_with_freshness(sensor, now_ms, freshness_ms) != Quality::Valid
            || !self.source.matches(sensor, mode)
            || sampled_at_ms != self.received_at_ms
        {
            return None;
        }
        value.filter(|value| value.is_finite())
    }

    /// Returns effective quality for `sensor` at monotonic `now_ms` milliseconds
    /// using its presentation cap; retained non-Valid quality is never promoted.
    pub fn quality_at(
        self,
        sensor: SensorId,
        now_ms: u64,
    ) -> Quality {
        self.quality_with_freshness(sensor, now_ms, sensor.stale_after_ms())
    }

    /// Returns effective quality for `sensor` at monotonic `now_ms` milliseconds,
    /// capped by both `freshness_ms` and channel policy. Valid age is inclusive;
    /// absent/future receive time becomes InvalidPayload, excess age becomes Stale.
    /// Stored non-Valid quality passes through unchanged, even when times are absent.
    pub fn quality_with_freshness(
        self,
        sensor: SensorId,
        now_ms: u64,
        freshness_ms: u64,
    ) -> Quality {
        if self.quality != Quality::Valid {
            return self.quality;
        }
        match self.received_at_ms.and_then(|at| now_ms.checked_sub(at)) {
            Some(age) if age <= sensor.stale_after_ms().min(freshness_ms) => Quality::Valid,
            Some(_) => Quality::Stale,
            None => Quality::InvalidPayload,
        }
    }
}
