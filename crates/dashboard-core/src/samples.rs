//! Canonical quantities and source/quality metadata. Numeric validity is separate
//! from vehicle support: a correctly decoded OBD reply is not ECU validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataMode {
    Vehicle,
    Demo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum SensorId {
    Boost,
    Oil,
    Water,
    Dsg,
    Iat,
    Egt,
    Battery,
    Lambda,
}

impl SensorId {
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
    pub const fn stale_after_ms(self) -> u64 { self.channel().freshness_cap() as u64 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Unknown,
    Valid,
    Stale,
    Unsupported,
    TransportError,
    InvalidPayload,
}

impl Quality {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Unknown,
    Demo,
    Obd {
        ecu: u16,
        pid: u8,
    },
    /// Gauge pressure derived from same-ECU MAP and ambient pressure, with
    /// freshness/skew and MAP saturation checked by the decoder.
    ObdMapDifference {
        ecu: u16,
    },
}

impl Source {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Quantity {
    BoostBar(f32),
    EngineOilC(f32),
    CoolantC(f32),
    TransmissionOilC(f32),
    IntakeC(f32),
    ExhaustC(f32),
    ModuleVolts(f32),
    Lambda(f32),
}

impl Quantity {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub value: Quantity,
    pub received_at_ms: u64,
    pub sequence: u64,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    Future,
    OutOfOrder,
    Source,
    InvalidValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleMeta {
    pub received_at_ms: Option<u64>,
    pub status_at_ms: u64,
    pub sequence: u64,
    pub source: Source,
    pub quality: Quality,
}

impl SampleMeta {
    pub const UNKNOWN: Self = Self {
        received_at_ms: None,
        status_at_ms: 0,
        sequence: 0,
        source: Source::Unknown,
        quality: Quality::Unknown,
    };

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

    pub fn quality_at(
        self,
        sensor: SensorId,
        now_ms: u64,
    ) -> Quality {
        self.quality_with_freshness(sensor, now_ms, sensor.stale_after_ms())
    }

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
