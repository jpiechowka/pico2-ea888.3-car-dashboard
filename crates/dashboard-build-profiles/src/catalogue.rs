//! Named profile identity, CLI compatibility and explicit observation provenance.

use crate::Voltage;

/// Stable named selection; aliases/features are supplied by [`catalogue`].
///
/// Serde uses snake-case variant names; frequency labels describe requests, not
/// a substitute for [`crate::resolve()`]'s actual rates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProfileId {
    /// Default 150 MHz CPU, 62.5 MHz SPI request; SPI resolves to 37.5 MHz.
    Stock,
    /// Expert-gated candidate: 132 MHz CPU and 66 MHz SPI.
    Cpu132Spi66,
    /// Expert-gated candidate: 140 MHz CPU and 70 MHz SPI.
    Cpu140Spi70,
    /// 150 MHz CPU and 15 MHz SPI timing reference, without Expert admission.
    Timing150Spi15,
    /// Experimental 250 MHz CPU and 62.5 MHz SPI at 1.10 V.
    Cpu250Spi62,
    /// Owner-reported working 280/70 MHz at 1.30 V; no safety certification.
    Cpu280Spi70,
    /// Failed legacy 290/72.5 MHz request; actual clocks are 290.4/48.4 MHz.
    Legacy290Spi72,
    /// Experimental 300 MHz CPU and 75 MHz SPI at 1.30 V.
    Cpu300Spi75,
}

/// UI evidence category; Expert gating is separately encoded in [`Profile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProfileKind {
    /// Default stock CPU profile, retaining its historical SPI request.
    Default,
    /// Candidate configuration without an owner success/failure observation.
    Candidate,
    /// Conservative SPI timing reference for comparison measurements.
    TimingReference,
    /// Unqualified overclock experiment.
    Experimental,
    /// Explicit Expert selection with device-specific working evidence.
    Expert,
    /// Compatibility-only selection carrying a recorded device failure.
    LegacyFailed,
}

/// Owner-reported observation of one setup, not population or margin evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum UserObservation {
    /// The owner reported operation at this named setting.
    Working,
    /// The owner reported failure at this named setting.
    Failed,
}

/// Immutable shared metadata for one named clock selection.
///
/// Build/UI adapters must call [`crate::resolve()`] before reporting actual clocks;
/// this metadata neither programs registers nor authorizes an Expert selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Profile {
    /// Typed selection identity used by recipes and serialization.
    pub id: ProfileId,
    /// Human-readable request/qualification summary.
    pub label: &'static str,
    /// Cargo feature spelling used by firmware build adapters.
    pub feature: &'static str,
    /// Accepted legacy and current command aliases; unique across the catalogue.
    pub aliases: &'static [&'static str],
    /// Requested core frequency, Hz; legacy 290 MHz deliberately rounds.
    pub requested_cpu_hz: u32,
    /// Requested SPI upper bound, Hz; actual divider rate may be lower.
    pub requested_spi_hz: u32,
    /// Regulator setpoint selection; no measured-voltage claim.
    pub voltage: Voltage,
    /// Host admission must require explicit Expert mode when true.
    pub requires_expert: bool,
    /// Display/evidence category, independent of the gating boolean.
    pub kind: ProfileKind,
    /// Device-specific owner result, or `None` when no result is recorded.
    pub observation: Option<UserObservation>,
    /// Display timing warning required; false only for the timing reference.
    pub panel_timing_warning: bool,
}

/// Canonical ordered profiles shared by host recipes, UI and build scripts.
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

/// Return all eight profiles in stable UI order, without allocating or resolving.
pub fn catalogue() -> &'static [Profile] { &PROFILES }

/// Return the canonical entry for `id`; the exhaustive mapping admits every ID.
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
