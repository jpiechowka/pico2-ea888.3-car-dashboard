//! Documentary source catalogue. A catalogue entry is not ECU qualification.
use crate::*;

/// Stable editor enumeration of disabled, generic, candidate and unresolved selections.
pub const DEFINITIONS: [DefinitionId; 16] = [
    DefinitionId::Disabled,
    DefinitionId::ObdCoolant05,
    DefinitionId::ObdIntake0f,
    DefinitionId::ObdOil5c,
    DefinitionId::ObdVoltage42,
    DefinitionId::ObdMapMinusAmbient,
    DefinitionId::ObdLambda24,
    DefinitionId::ObdLambda34,
    DefinitionId::SimosCoolant11cd,
    DefinitionId::SimosOil202f,
    DefinitionId::SimosIntake1001,
    DefinitionId::SimosVoltage14a6,
    DefinitionId::SimosMapMinusAmbient,
    DefinitionId::SimosLambda10c0,
    DefinitionId::DsgOil2104,
    DefinitionId::UnresolvedEgt,
];

/// Pinned decoder implementation; command mapping and scaling tables are separate evidence.
pub const OBD_REFERENCE: &str =
    "https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py";
/// Public secondary PID table used as a research lead, not normative or exact-ECU proof.
pub const OBD_TABLE_REFERENCE: &str = "https://www.csselectronics.com/pages/obd2-pid-table-on-board-diagnostics-j1979";
/// Pinned Simos documentary parameter table; installed ECU/software applicability is unverified.
pub const SIMOS_REFERENCE: &str =
    "https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv";
/// Pinned logging implementation used to inspect DSG byte handling and interpretation.
pub const DSG_REFERENCE: &str = "https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/UDSlog.kt";
/// Pinned SimosTools request/definition source; interpreted together with its logger.
pub const SIMOS_DEFINITIONS_REFERENCE: &str = "https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt";
/// Pinned PID-to-decoder mapping, distinct from the decoder function itself.
pub const OBD_COMMANDS_REFERENCE: &str =
    "https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/commands.py";
/// Pinned units/scaling implementation referenced by generic OBD command decoders.
pub const OBD_SCALING_REFERENCE: &str =
    "https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/UnitsAndScaling.py";

/// Reason a selected documentary source cannot resolve to the supported live request shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    /// Definition/formula remains an unqualified candidate or lacks an executable binding.
    CandidateDecoder,
    /// Required scaling/sensor prerequisites remain unresolved.
    ScalingUnresolved,
    /// Requested measured sensor has no established source.
    SensorUnresolved,
    /// No physical/deployment route is selected.
    MissingRoute,
    /// Explicit physical route does not match the supplied active route.
    RouteNotAvailable,
}

/// Immutable documentary record; interpretation and links retain uncertainty explicitly.
#[derive(Clone, Copy, Debug)]
pub struct Definition {
    /// Closed catalogue key.
    pub id: DefinitionId,
    /// Required owning channel; `None` is the disabled selection usable for any channel.
    pub channel: Option<Channel>,
    /// Human-readable selection name; not a peer- or ECU-provided value.
    pub name: &'static str,
    /// Fixed request shape constrained by configuration admission.
    pub access: Access,
    /// Fixed documentary conversion identity.
    pub decoder: Decoder,
    /// Source-evidence class, separate from deployment qualification.
    pub evidence: Evidence,
    /// Reply width, byte order, formula and limitations; data widths exclude service/identifier echoes.
    pub interpretation: &'static str,
    /// Primary display link; use [`Self::references`] to retain companion mapping/formula evidence.
    pub reference: &'static str,
    /// Documentary blocker; `None` still does not establish live ECU capability or authority.
    pub unavailable: Option<UnavailableReason>,
}

impl Definition {
    /// Return companion request, formula and byte-order links; disabled returns an empty slice.
    /// A decoder implementation alone does not establish which PID selects it.
    pub fn references(self) -> &'static [&'static str] {
        use DefinitionId::*;
        match self.id {
            Disabled => &[],
            ObdCoolant05 | ObdIntake0f | ObdOil5c | ObdVoltage42 => {
                &[OBD_COMMANDS_REFERENCE, OBD_REFERENCE, OBD_SCALING_REFERENCE]
            }
            DsgOil2104 => &[SIMOS_DEFINITIONS_REFERENCE, DSG_REFERENCE],
            SimosCoolant11cd | SimosOil202f | SimosIntake1001 | SimosVoltage14a6 | SimosMapMinusAmbient
            | SimosLambda10c0 => &[SIMOS_REFERENCE, SIMOS_DEFINITIONS_REFERENCE, DSG_REFERENCE],
            _ => &[OBD_TABLE_REFERENCE],
        }
    }

    /// Return editable source metadata with timing defaults derived from `channel`.
    /// Unavailable/disabled definitions default to polling off; DSG/EGT/disabled routes are unknown.
    /// Does not check that `channel` matches this definition; full validation must follow.
    pub fn source(
        self,
        channel: Channel,
    ) -> ChannelSource {
        ChannelSource {
            definition: self.id,
            access: self.access,
            decoder: self.decoder,
            evidence: self.evidence,
            route: if matches!(
                self.id,
                DefinitionId::DsgOil2104 | DefinitionId::UnresolvedEgt | DefinitionId::Disabled
            ) {
                Route::Unknown
            } else {
                Route::DeploymentProfile
            },
            poll: if self.unavailable.is_none() && self.id != DefinitionId::Disabled {
                Poll::EveryMs(minimum_period(channel))
            } else {
                Poll::Off
            },
            freshness_ms: channel.freshness_cap(),
        }
    }
}

/// Return the configured minimum poll interval for `channel` in milliseconds.
/// This scheduler policy is not an ECU response-time or bus-capacity guarantee.
pub const fn minimum_period(channel: Channel) -> u16 {
    match channel {
        Channel::Battery | Channel::Boost | Channel::Lambda => 500,
        Channel::Oil => 1_500,
        _ => 1_000,
    }
}

/// Return the complete fixed record for `id`; the closed enum makes lookup total.
/// Candidate formulas are documentary only; no request is transmitted or qualified here.
pub fn definition(id: DefinitionId) -> Definition {
    use DefinitionId::*;
    let (channel, name, access, decoder, evidence, interpretation, reference, unavailable) = match id {
        Disabled => {
            (
                None,
                "Disabled",
                Access::None,
                Decoder::None,
                Evidence::Disabled,
                "No request or decoder.",
                "",
                None,
            )
        }
        ObdCoolant05 | ObdIntake0f | ObdOil5c => {
            let (channel, name, pid) = match id {
                ObdCoolant05 => (Channel::Water, "OBD 01 05 coolant", 0x05),
                ObdIntake0f => (Channel::Iat, "OBD 01 0F intake", 0x0F),
                _ => (Channel::Oil, "OBD 01 5C oil", 0x5C),
            };
            (
                Some(channel),
                name,
                Access::Obd01 { pid },
                Decoder::TemperatureAminus40,
                Evidence::GenericObd,
                "41 PID echo, 1 unsigned data byte A; A - 40 degC. Capability and deployment qualification required.",
                OBD_REFERENCE,
                None,
            )
        }
        ObdVoltage42 => {
            (
                Some(Channel::Battery),
                "OBD 01 42 ECU voltage",
                Access::Obd01 { pid: 0x42 },
                Decoder::Be16Millivolts,
                Evidence::GenericObd,
                "41 42 echo, 2 unsigned big-endian data bytes AB; AB / 1000 V. ECU module voltage, not battery-post \
                 measurement.",
                OBD_REFERENCE,
                None,
            )
        }
        ObdMapMinusAmbient => {
            (
                Some(Channel::Boost),
                "OBD manifold minus ambient",
                Access::MapMinusAmbient {
                    map_pid: 0x0B,
                    ambient_pid: 0x33,
                },
                Decoder::MapDifferenceWithScalingPrerequisites,
                Evidence::DocumentedCandidate,
                "Each 41 PID echo has 1 unsigned byte kPa; (MAP - ambient) / 100 bar. PID4F scaling, saturation, same \
                 ECU/epoch and input age/skew unresolved. No executable binding.",
                OBD_TABLE_REFERENCE,
                Some(UnavailableReason::ScalingUnresolved),
            )
        }
        ObdLambda24 | ObdLambda34 => {
            (
                Some(Channel::Lambda),
                if id == ObdLambda24 {
                    "OBD 01 24 lambda candidate"
                } else {
                    "OBD 01 34 lambda candidate"
                },
                Access::Obd01 {
                    pid: if id == ObdLambda24 { 0x24 } else { 0x34 },
                },
                Decoder::WidebandLambdaWithScalingPrerequisites,
                Evidence::DocumentedCandidate,
                "41 PID echo, 4 data bytes; first unsigned BE16 / 32768 under reviewed default scaling. Remaining \
                 voltage/current is not lambda. Bank/sensor, PID4F and normative scaling unresolved.",
                OBD_TABLE_REFERENCE,
                Some(UnavailableReason::ScalingUnresolved),
            )
        }
        SimosCoolant11cd => {
            (
                Some(Channel::Water),
                "Simos 22 11CD coolant candidate",
                Access::ReadDid22 { did: 0x11CD },
                Decoder::TemperatureAminus40,
                Evidence::DocumentedCandidate,
                "62 11 CD echo, 1 unsigned data byte A; A - 40 degC. Not verified on this ECU/software.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        SimosOil202f => {
            (
                Some(Channel::Oil),
                "Simos 22 202F oil candidate",
                Access::ReadDid22 { did: 0x202F },
                Decoder::Be16KelvinTenths,
                Evidence::DocumentedCandidate,
                "62 20 2F echo, 2 unsigned big-endian data bytes AB; (AB - 2731.4) / 10 degC. Preserve documentary \
                 constant; ECU/software unverified.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        SimosIntake1001 => {
            (
                Some(Channel::Iat),
                "Simos 22 1001 intake candidate",
                Access::ReadDid22 { did: 0x1001 },
                Decoder::IntakeThreeQuartersMinus48,
                Evidence::DocumentedCandidate,
                "62 10 01 echo, 1 unsigned data byte A; 0.75 * A - 48 degC. Intake location and ECU/software \
                 unverified.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        SimosVoltage14a6 => {
            (
                Some(Channel::Battery),
                "Simos 22 14A6 voltage candidate",
                Access::ReadDid22 { did: 0x14A6 },
                Decoder::Voltage13Over128,
                Evidence::DocumentedCandidate,
                "62 14 A6 echo, 1 unsigned data byte A; A * 13 / 128 V. ECU module voltage. ECU/software unverified.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        SimosMapMinusAmbient => {
            (
                Some(Channel::Boost),
                "Simos manifold minus ambient",
                Access::DidMapMinusAmbient {
                    map_did: 0x39C0,
                    ambient_did: 0x13CA,
                },
                Decoder::SimosMapDifference,
                Evidence::DocumentedCandidate,
                "Each 62 DID echo: 2 unsigned BE data bytes. MAP39C0 = AB/10 kPa absolute; ambient13CA = \
                 AB/120.60176665439 kPa. Difference /100 bar. Same ECU/epoch and bounded age/skew required. Neither \
                 alone is gauge boost.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        SimosLambda10c0 => {
            (
                Some(Channel::Lambda),
                "Simos 22 10C0 lambda candidate",
                Access::ReadDid22 { did: 0x10C0 },
                Decoder::Be16LambdaOver1024,
                Evidence::DocumentedCandidate,
                "62 10 C0 echo, 2 unsigned big-endian data bytes AB; AB/1024 actual lambda (lean above 1). Not \
                 commanded lambda. ECU/software unverified.",
                SIMOS_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        DsgOil2104 => {
            (
                Some(Channel::Dsg),
                "DSG 22 2104 candidate",
                Access::ReadDid22 { did: 0x2104 },
                Decoder::Le16SignedCelsius,
                Evidence::DocumentedCandidate,
                "62 21 04 echo, 2 signed little-endian data bytes, identity degC. Historical generic DSG lead, not \
                 DQ381 proof. Route, TCU identity and exact sensor semantics unknown.",
                DSG_REFERENCE,
                Some(UnavailableReason::CandidateDecoder),
            )
        }
        UnresolvedEgt => {
            (
                Some(Channel::Egt),
                "Unresolved measured EGT",
                Access::Unresolved,
                Decoder::Unresolved,
                Evidence::Unresolved,
                "No request or decoder established for measured manifold EGT on this ECU. Catalyst PIDs and modeled \
                 temperatures are not substitutes.",
                OBD_TABLE_REFERENCE,
                Some(UnavailableReason::SensorUnresolved),
            )
        }
    };
    Definition {
        id,
        channel,
        name,
        access,
        decoder,
        evidence,
        interpretation,
        reference,
        unavailable,
    }
}

/// Bounded source-selection outcome; runtime must separately admit transport/deployment/sample quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceResolution {
    /// Polling is off or the definition is disabled.
    Disabled,
    /// Selected polling cannot resolve to a supported generic OBD request.
    Unavailable(
        /// First routing/documentary blocker encountered during resolution.
        UnavailableReason,
    ),
    /// Describes selection only. The live transport must additionally enforce
    /// deployment qualification, capability discovery and sample quality.
    Obd {
        /// Current-data service 01 PID from the validated catalogue record.
        pid: u8,
        /// Admitted requested polling period in milliseconds.
        period_ms: u16,
        /// Admitted sample lifetime in milliseconds.
        freshness_ms: u16,
    },
}

/// Resolve `channel` from validated `config` against active CAN IDs `request` and `response`.
/// Returns disabled before checking routes; unknown/mismatched explicit routes precede
/// documentary blockers. Only the four generic temperature/voltage definitions resolve to OBD.
/// Deployment-profile routes inherit the supplied route without qualifying it; callers must
/// validate actual transport authority/capability before sending any bytes.
pub fn resolve_source(
    config: &ValidatedConfig,
    channel: Channel,
    request: u16,
    response: u16,
) -> SourceResolution {
    let source = config.config().sources[channel.index()];
    let Poll::EveryMs(period_ms) = source.poll else {
        return SourceResolution::Disabled;
    };
    if source.definition == DefinitionId::Disabled {
        return SourceResolution::Disabled;
    }
    match source.route {
        Route::Unknown => return SourceResolution::Unavailable(UnavailableReason::MissingRoute),
        Route::Physical11 {
            request: req,
            response: resp,
        } if req != request || resp != response => {
            return SourceResolution::Unavailable(UnavailableReason::RouteNotAvailable);
        }
        _ => {}
    }
    let definition = definition(source.definition);
    if let Some(reason) = definition.unavailable {
        return SourceResolution::Unavailable(reason);
    }
    match source.definition {
        DefinitionId::ObdCoolant05
        | DefinitionId::ObdIntake0f
        | DefinitionId::ObdOil5c
        | DefinitionId::ObdVoltage42 => {
            if let Access::Obd01 { pid } = source.access {
                SourceResolution::Obd {
                    pid,
                    period_ms,
                    freshness_ms: source.freshness_ms,
                }
            } else {
                SourceResolution::Unavailable(UnavailableReason::CandidateDecoder)
            }
        }
        _ => SourceResolution::Unavailable(UnavailableReason::CandidateDecoder),
    }
}
