use crate::Voltage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProfileId {
    Stock,
    Cpu132Spi66,
    Cpu140Spi70,
    Timing150Spi15,
    Cpu250Spi62,
    Cpu280Spi70,
    Legacy290Spi72,
    Cpu300Spi75,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProfileKind {
    Default,
    Candidate,
    TimingReference,
    Experimental,
    Expert,
    LegacyFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum UserObservation {
    Working,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Profile {
    pub id: ProfileId,
    pub label: &'static str,
    pub feature: &'static str,
    pub aliases: &'static [&'static str],
    pub requested_cpu_hz: u32,
    pub requested_spi_hz: u32,
    pub voltage: Voltage,
    pub requires_expert: bool,
    pub kind: ProfileKind,
    pub observation: Option<UserObservation>,
    pub panel_timing_warning: bool,
}

const PROFILES: [Profile; 8] = [
    Profile {
        id: ProfileId::Stock,
        label: "Stock 150 MHz / requested 62.5 MHz SPI (actual 37.5 MHz)",
        feature: "cpu150-spi62-1v10",
        aliases: &[
            "stock",
            "pico2",
            "pico2-run",
            "pico2-fast",
            "pico2-fast-run",
            "pico2-150-62-1v10",
            "pico2-150-62-1v10-run",
        ],
        requested_cpu_hz: 150_000_000,
        requested_spi_hz: 62_500_000,
        voltage: Voltage::V1_10,
        requires_expert: false,
        kind: ProfileKind::Default,
        observation: None,
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Cpu132Spi66,
        label: "132 MHz / 66 MHz SPI candidate",
        feature: "cpu132-spi66-1v10",
        aliases: &["132-66", "pico2-132-66-1v10", "pico2-132-66-1v10-run"],
        requested_cpu_hz: 132_000_000,
        requested_spi_hz: 66_000_000,
        voltage: Voltage::V1_10,
        requires_expert: true,
        kind: ProfileKind::Candidate,
        observation: None,
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Cpu140Spi70,
        label: "140 MHz / 70 MHz SPI candidate",
        feature: "cpu140-spi70-1v10",
        aliases: &["140-70", "pico2-140-70-1v10", "pico2-140-70-1v10-run"],
        requested_cpu_hz: 140_000_000,
        requested_spi_hz: 70_000_000,
        voltage: Voltage::V1_10,
        requires_expert: true,
        kind: ProfileKind::Candidate,
        observation: None,
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Timing150Spi15,
        label: "150 MHz / 15 MHz SPI timing reference",
        feature: "cpu150-spi15-1v10",
        aliases: &["150-15", "pico2-150-15-1v10", "pico2-150-15-1v10-run"],
        requested_cpu_hz: 150_000_000,
        requested_spi_hz: 15_000_000,
        voltage: Voltage::V1_10,
        requires_expert: false,
        kind: ProfileKind::TimingReference,
        observation: None,
        panel_timing_warning: false,
    },
    Profile {
        id: ProfileId::Cpu250Spi62,
        label: "250 MHz / 62.5 MHz SPI experimental",
        feature: "cpu250-spi62-1v10",
        aliases: &["250-62.5", "pico2-250-62-1v10", "pico2-250-62-1v10-run"],
        requested_cpu_hz: 250_000_000,
        requested_spi_hz: 62_500_000,
        voltage: Voltage::V1_10,
        requires_expert: true,
        kind: ProfileKind::Experimental,
        observation: None,
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Cpu280Spi70,
        label: "Expert 280 MHz / 70 MHz SPI, 1.30 V selected",
        feature: "cpu280-spi70-1v30",
        aliases: &["280-70", "pico2-280-70-1v30", "pico2-280-70-1v30-run"],
        requested_cpu_hz: 280_000_000,
        requested_spi_hz: 70_000_000,
        voltage: Voltage::V1_30,
        requires_expert: true,
        kind: ProfileKind::Expert,
        observation: Some(UserObservation::Working),
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Legacy290Spi72,
        label: "Legacy 290 MHz requested / 290.4 MHz actual, 72.5 MHz SPI requested / 48.4 MHz actual",
        feature: "cpu290-spi72-1v30",
        aliases: &["290-72.5", "pico2-290-72-1v30", "pico2-290-72-1v30-run"],
        requested_cpu_hz: 290_000_000,
        requested_spi_hz: 72_500_000,
        voltage: Voltage::V1_30,
        requires_expert: true,
        kind: ProfileKind::LegacyFailed,
        observation: Some(UserObservation::Failed),
        panel_timing_warning: true,
    },
    Profile {
        id: ProfileId::Cpu300Spi75,
        label: "300 MHz / 75 MHz SPI experimental",
        feature: "cpu300-spi75-1v30",
        aliases: &["300-75", "pico2-300-75-1v30", "pico2-300-75-1v30-run"],
        requested_cpu_hz: 300_000_000,
        requested_spi_hz: 75_000_000,
        voltage: Voltage::V1_30,
        requires_expert: true,
        kind: ProfileKind::Experimental,
        observation: None,
        panel_timing_warning: true,
    },
];

pub fn catalogue() -> &'static [Profile] { &PROFILES }

pub(crate) fn profile(id: ProfileId) -> &'static Profile {
    &PROFILES[match id {
        ProfileId::Stock => 0,
        ProfileId::Cpu132Spi66 => 1,
        ProfileId::Cpu140Spi70 => 2,
        ProfileId::Timing150Spi15 => 3,
        ProfileId::Cpu250Spi62 => 4,
        ProfileId::Cpu280Spi70 => 5,
        ProfileId::Legacy290Spi72 => 6,
        ProfileId::Cpu300Spi75 => 7,
    }]
}
