//! Closed engine read catalogue and bounded raw replies; no I/O or vehicle qualification.
//!
//! The owner confirmed the twelve live-data channel labels in OBDeleven. Candidate
//! widths/scaling follow the pinned [VW_Flash table](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv)
//! and its [big-endian parser](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/simos_hsl.py).
//! [Identity definitions](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/constants.py)
//! establish names, not exact payload widths. No candidate qualifies this ECU/software.
//! [UDS22](https://udsoncan.readthedocs.io/en/latest/_modules/udsoncan/services/ReadDataByIdentifier.html)
//! supplies the service/DID echo contract, not manufacturer data semantics.
//!
//! Callers must independently enforce fixed engine routing, complete ISO-TP reception,
//! epoch/freshness, one outstanding transaction and bounded pending responses. This
//! module cannot verify adapter firmware behavior or guarantee the ECU's session state.
//! No session change, security unlock, TesterPresent, write or arbitrary DID API exists.

/// Maximum retained positive data bytes, excluding the service and DID echo.
pub const MAX_DATA_BYTES: usize = 64;

/// Explicit one-shot engine reads; no conversion from arbitrary identifiers is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadId {
    /// Current diagnostic session; a reply alone does not establish a default session.
    ActiveSession,
    /// ASAM/ODX file identifier.
    OdxIdentifier,
    /// ASAM/ODX file version.
    OdxVersion,
    /// VW spare-part number.
    PartNumber,
    /// Application software version; not a binary identity.
    SoftwareVersion,
    /// ECU hardware number.
    HardwareNumber,
    /// ECU hardware revision.
    HardwareVersion,
    /// ECU system name or engine type.
    SystemName,
    /// Engine code letters.
    EngineCode,
    /// Manifold absolute pressure, distinct from upstream-throttle pressure.
    ManifoldPressure,
    /// Ambient absolute pressure, not an assumed fixed atmosphere.
    AmbientPressure,
    /// Engine oil temperature, distinct from oil level.
    OilTemperature,
    /// Engine coolant temperature.
    CoolantTemperature,
    /// Simos intake temperature family candidate.
    IntakeTemperature,
    /// ECU module voltage, distinct from adapter supply or battery-post voltage.
    ModuleVoltage,
    /// Actual lambda family candidate, distinct from a commanded ratio.
    Lambda,
    /// Owner-confirmed oil fill level; raw schema unresolved.
    OilLevel,
    /// Signed timing correction for cylinder one.
    IgnitionRetard1,
    /// Signed timing correction for cylinder two.
    IgnitionRetard2,
    /// Signed timing correction for cylinder three.
    IgnitionRetard3,
    /// Signed timing correction for cylinder four.
    IgnitionRetard4,
    /// Requested upstream-throttle absolute pressure.
    ChargePressureSpecified,
    /// Actual upstream-throttle absolute pressure.
    ChargePressureActual,
    /// Owner-confirmed charge-pressure actuator acknowledgment; not actuator control.
    ChargeActuatorAcknowledgment,
    /// Owner-confirmed upper-threshold adaptation observation; no adaptation write.
    ChargeAdaptationHigh,
    /// Owner-confirmed lower-threshold adaptation observation; no adaptation write.
    ChargeAdaptationLow,
    /// Owner-confirmed unconditioned intake-sensor voltage; scale unresolved.
    IntakeSensorVoltage,
    /// Owner-confirmed F40F intake temperature; exact OBD-mirror schema unresolved.
    ObdIntakeTemperature,
}

/// Documentary payload width assessment; none of these outcomes qualifies a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaMatch {
    /// No independently reviewed width is available.
    Unknown,
    /// Width matches the family candidate, without proving its applicability.
    Exact,
    /// Raw bytes are retained, but candidate interpretation is forbidden.
    Mismatch {
        /// Candidate data width in bytes, excluding the three-byte echo.
        expected: usize,
        /// Actual admitted data width in bytes, from one through 64.
        actual: usize,
    },
}

/// Borrowed, correlated application response; transport source and freshness remain caller-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// Exact service/DID echo with one through 64 raw data bytes.
    Positive {
        /// Borrowed data after the three-byte echo, without trimming or substitution.
        data: &'a [u8],
        /// Candidate width match, never a qualification or numeric validity claim.
        schema: SchemaMatch,
    },
    /// Exact three-byte negative response other than pending.
    Negative(
        /// Original NRC, including unknown values.
        u8,
    ),
    /// NRC78; the caller must bound waiting without automatically resending.
    Pending,
}

/// Invalid application response; the caller may retain its bytes as failed evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyError {
    /// Missing prefix/data or negative reply not exactly three bytes.
    Length,
    /// Positive or negative service echo is not the expected ReadDataByIdentifier service.
    Service,
    /// Positive DID echo belongs to another request.
    Identifier,
    /// Positive data exceeds 64 bytes; no shortened successful response is returned.
    Oversized,
}

/// Unqualified family interpretation for evidence review, never an acquisition sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CandidateValue {
    /// Signed cylinder correction in degrees; no absolute-value normalization.
    Degrees(
        /// Finite family conversion in -327.68..=327.67 degrees; not a safety range.
        f32,
    ),
    /// Absolute pressure in kPa; never gauge boost or a pressure difference.
    AbsoluteKpa(
        /// Finite family conversion in kPa, without ambient subtraction.
        f32,
    ),
    /// Temperature in degrees Celsius at the source-specific location.
    Celsius(
        /// Finite family conversion in degrees Celsius; no sensor qualification.
        f32,
    ),
    /// ECU module voltage in volts.
    Volts(
        /// Finite family conversion in volts; not a battery-post measurement.
        f32,
    ),
    /// Dimensionless actual lambda; no implicit AFR/fuel conversion.
    Lambda(
        /// Finite raw ratio conversion; zero/sentinel validity remains unqualified.
        f32,
    ),
}

impl ReadId {
    /// Every admitted read exactly once, with session observation before identity and measurements.
    pub const ALL: [Self; 28] = [
        Self::ActiveSession,
        Self::OdxIdentifier,
        Self::OdxVersion,
        Self::PartNumber,
        Self::SoftwareVersion,
        Self::HardwareNumber,
        Self::HardwareVersion,
        Self::SystemName,
        Self::EngineCode,
        Self::ManifoldPressure,
        Self::AmbientPressure,
        Self::OilTemperature,
        Self::CoolantTemperature,
        Self::IntakeTemperature,
        Self::ModuleVoltage,
        Self::Lambda,
        Self::OilLevel,
        Self::IgnitionRetard1,
        Self::IgnitionRetard2,
        Self::IgnitionRetard3,
        Self::IgnitionRetard4,
        Self::ChargePressureSpecified,
        Self::ChargePressureActual,
        Self::ChargeActuatorAcknowledgment,
        Self::ChargeAdaptationHigh,
        Self::ChargeAdaptationLow,
        Self::IntakeSensorVoltage,
        Self::ObdIntakeTemperature,
    ];

    /// Returns the stable zero-based position in [`Self::ALL`], in 0..28.
    pub const fn index(self) -> usize { self as usize }

    /// Returns a fixed display label, never text obtained from an ECU or adapter.
    pub const fn label(self) -> &'static str { self.metadata().1 }

    /// Returns this closed operation's two-byte DID; does not authorize a route or send it.
    pub const fn did(self) -> u16 { self.metadata().0 }

    /// Returns exactly one UDS22 application request; excludes ISO-TP PCI and adapter framing.
    pub const fn request(self) -> [u8; 3] {
        let [hi, lo] = self.did().to_be_bytes();
        [0x22, hi, lo]
    }

    /// Returns the reviewed family data width, or None for unknown schemas including identities.
    pub const fn expected_data_len(self) -> Option<usize> { self.metadata().2 }

    /// Returns this operation's fixed DID, label and optional candidate width in data bytes.
    /// Unknown identities and owner-only schemas have no invented width or scale.
    const fn metadata(self) -> (u16, &'static str, Option<usize>) {
        match self {
            Self::ActiveSession => (0xF186, "Active diagnostic session", None),
            Self::OdxIdentifier => (0xF19E, "ASAM/ODX identifier", None),
            Self::OdxVersion => (0xF1A2, "ASAM/ODX version", None),
            Self::PartNumber => (0xF187, "VW spare-part number", None),
            Self::SoftwareVersion => (0xF189, "Application software version", None),
            Self::HardwareNumber => (0xF191, "ECU hardware number", None),
            Self::HardwareVersion => (0xF1A3, "ECU hardware version", None),
            Self::SystemName => (0xF197, "System name or engine type", None),
            Self::EngineCode => (0xF1AD, "Engine code letters", None),
            Self::ManifoldPressure => (0x39C0, "Manifold absolute pressure", Some(2)),
            Self::AmbientPressure => (0x13CA, "Ambient absolute pressure", Some(2)),
            Self::OilTemperature => (0x202F, "Engine oil temperature", Some(2)),
            Self::CoolantTemperature => (0x11CD, "Coolant temperature", Some(1)),
            Self::IntakeTemperature => (0x1001, "Simos intake temperature", Some(1)),
            Self::ModuleVoltage => (0x14A6, "ECU module voltage", Some(1)),
            Self::Lambda => (0x10C0, "Actual lambda", Some(2)),
            Self::OilLevel => (0x2006, "Oil fill level", None),
            Self::IgnitionRetard1 => (0x200A, "Timing retardation cylinder 1", Some(2)),
            Self::IgnitionRetard2 => (0x200B, "Timing retardation cylinder 2", Some(2)),
            Self::IgnitionRetard3 => (0x200C, "Timing retardation cylinder 3", Some(2)),
            Self::IgnitionRetard4 => (0x200D, "Timing retardation cylinder 4", Some(2)),
            Self::ChargePressureSpecified => (0x2029, "Charge air pressure specified", Some(2)),
            Self::ChargePressureActual => (0x202A, "Charge air pressure actual", Some(2)),
            Self::ChargeActuatorAcknowledgment => (0x2065, "Charge actuator acknowledgment", None),
            Self::ChargeAdaptationHigh => (0x293E, "Charge adaptation above maximum", None),
            Self::ChargeAdaptationLow => (0x293F, "Charge adaptation below minimum", None),
            Self::IntakeSensorVoltage => (0x29BF, "Intake sensor unconditioned voltage", None),
            Self::ObdIntakeTemperature => (0xF40F, "Intake air temperature", None),
        }
    }

    /// Classifies complete reassembled application `bytes`, borrowing positive data without changes.
    /// Rejects bad echoes/lengths/overflow; width mismatch remains raw evidence, never a numeric value.
    /// Returns a positive borrowed payload with schema status, a negative code or pending reply;
    /// malformed service, DID or payload bounds return the corresponding ReplyError.
    pub fn classify(
        self,
        bytes: &[u8],
    ) -> Result<Reply<'_>, ReplyError> {
        let Some(&service) = bytes.first() else {
            return Err(ReplyError::Length);
        };
        if service == 0x7F {
            let [_, echo, nrc] = bytes else {
                return Err(ReplyError::Length);
            };
            if *echo != 0x22 {
                return Err(ReplyError::Service);
            }
            return Ok(if *nrc == 0x78 {
                Reply::Pending
            } else {
                Reply::Negative(*nrc)
            });
        }
        if service != 0x62 {
            return Err(ReplyError::Service);
        }
        let [_, hi, lo, data @ ..] = bytes else {
            return Err(ReplyError::Length);
        };
        if u16::from_be_bytes([*hi, *lo]) != self.did() {
            return Err(ReplyError::Identifier);
        }
        if data.is_empty() {
            return Err(ReplyError::Length);
        }
        if data.len() > MAX_DATA_BYTES {
            return Err(ReplyError::Oversized);
        }
        let schema = match self.expected_data_len() {
            None => SchemaMatch::Unknown,
            Some(expected) if expected == data.len() => SchemaMatch::Exact,
            Some(expected) => {
                SchemaMatch::Mismatch {
                    expected,
                    actual: data.len(),
                }
            }
        };
        Ok(Reply::Positive { data, schema })
    }

    /// Interprets complete reply `bytes` only when its echo and known width match this operation.
    /// Returns unqualified family units, or None for unknown/mismatched/negative/malformed replies.
    pub fn candidate(
        self,
        bytes: &[u8],
    ) -> Option<CandidateValue> {
        let Reply::Positive {
            data,
            schema: SchemaMatch::Exact,
        } = self.classify(bytes).ok()?
        else {
            return None;
        };
        if let [a] = data {
            let value = f32::from(*a);
            return match self {
                Self::CoolantTemperature => Some(CandidateValue::Celsius(value - 40.0)),
                Self::IntakeTemperature => Some(CandidateValue::Celsius(value * 0.75 - 48.0)),
                Self::ModuleVoltage => Some(CandidateValue::Volts(value * 13.0 / 128.0)),
                _ => None,
            };
        }
        let [hi, lo] = data else {
            return None;
        };
        let value = f32::from(u16::from_be_bytes([*hi, *lo]));
        match self {
            Self::IgnitionRetard1 | Self::IgnitionRetard2 | Self::IgnitionRetard3 | Self::IgnitionRetard4 => {
                Some(CandidateValue::Degrees(
                    f32::from(i16::from_be_bytes([*hi, *lo])) / 100.0,
                ))
            }
            Self::ManifoldPressure | Self::ChargePressureActual | Self::ChargePressureSpecified => {
                Some(CandidateValue::AbsoluteKpa(value / 10.0))
            }
            Self::AmbientPressure => {
                // Preserve the source denominator, rounded once to this f32 evidence representation.
                Some(CandidateValue::AbsoluteKpa(value / (120.601_766_654_39_f64 as f32)))
            }
            Self::OilTemperature => Some(CandidateValue::Celsius((value - 2731.4) / 10.0)),
            Self::Lambda => Some(CandidateValue::Lambda(value / 1024.0)),
            _ => None,
        }
    }
}

#[cfg(test)]
/// Egress allowlist, raw retention and independent numeric boundary regressions; no device I/O.
mod tests {
    use super::*;

    #[test]
    /// Catches substituted services/DIDs, duplicates or ordering that bypasses session observation.
    fn only_the_twenty_eight_reviewed_single_did_requests_can_be_built() {
        let expected = [
            [0x22, 0xF1, 0x86],
            [0x22, 0xF1, 0x9E],
            [0x22, 0xF1, 0xA2],
            [0x22, 0xF1, 0x87],
            [0x22, 0xF1, 0x89],
            [0x22, 0xF1, 0x91],
            [0x22, 0xF1, 0xA3],
            [0x22, 0xF1, 0x97],
            [0x22, 0xF1, 0xAD],
            [0x22, 0x39, 0xC0],
            [0x22, 0x13, 0xCA],
            [0x22, 0x20, 0x2F],
            [0x22, 0x11, 0xCD],
            [0x22, 0x10, 0x01],
            [0x22, 0x14, 0xA6],
            [0x22, 0x10, 0xC0],
            [0x22, 0x20, 0x06],
            [0x22, 0x20, 0x0A],
            [0x22, 0x20, 0x0B],
            [0x22, 0x20, 0x0C],
            [0x22, 0x20, 0x0D],
            [0x22, 0x20, 0x29],
            [0x22, 0x20, 0x2A],
            [0x22, 0x20, 0x65],
            [0x22, 0x29, 0x3E],
            [0x22, 0x29, 0x3F],
            [0x22, 0x29, 0xBF],
            [0x22, 0xF4, 0x0F],
        ];
        for (index, (id, request)) in ReadId::ALL.into_iter().zip(expected).enumerate() {
            assert_eq!(id.index(), index);
            assert_eq!(id.request(), request);
            assert_eq!(id.did(), u16::from_be_bytes([request[1], request[2]]));
            assert!(!id.label().is_empty());
            for other in ReadId::ALL.into_iter().skip(index + 1) {
                assert_ne!(id.did(), other.did());
            }
        }
    }

    #[test]
    /// Catches loss/reinterpretation of any NRC and accidental pending-as-success admission.
    fn every_negative_code_is_retained_and_pending_is_distinct() {
        for id in ReadId::ALL {
            for nrc in 0..=u8::MAX {
                let bytes = [0x7F, 0x22, nrc];
                let expected = if nrc == 0x78 {
                    Reply::Pending
                } else {
                    Reply::Negative(nrc)
                };
                assert_eq!(id.classify(&bytes), Ok(expected));
                assert_eq!(id.candidate(&bytes), None);
            }
            assert_eq!(id.classify(&[0x7F, 0x22]), Err(ReplyError::Length));
            assert_eq!(id.classify(&[0x7F, 0x22, 0x31, 0]), Err(ReplyError::Length));
            assert_eq!(id.classify(&[0x7F, 0x2E, 0x31]), Err(ReplyError::Service));
        }
    }

    #[test]
    /// Catches accepting foreign echoes, empty positives or zero-trimming oversized data.
    fn echo_and_size_failures_never_become_positive_evidence() {
        let id = ReadId::OilLevel;
        for bytes in [&[][..], &[0x62], &[0x62, 0x20], &[0x62, 0x20, 0x06]] {
            assert_eq!(id.classify(bytes), Err(ReplyError::Length));
        }
        assert_eq!(id.classify(&[0x6E, 0x20, 0x06, 1]), Err(ReplyError::Service));
        assert_eq!(id.classify(&[0x62, 0x20, 0x07, 1]), Err(ReplyError::Identifier));
        assert_eq!(id.classify(&[0x62, 0x21, 0x06, 1]), Err(ReplyError::Identifier));
        let mut bytes = [0; 68];
        bytes[..3].copy_from_slice(&[0x62, 0x20, 0x06]);
        assert_eq!(id.classify(&bytes), Err(ReplyError::Oversized));
        assert_eq!(id.candidate(&bytes), None);
    }

    #[test]
    /// Catches guessed schemas and lost binary/NUL bytes for owner channels or identity reads.
    fn unknown_schemas_preserve_each_admitted_width_without_numeric_interpretation() {
        for id in [
            ReadId::ActiveSession,
            ReadId::OdxIdentifier,
            ReadId::OdxVersion,
            ReadId::PartNumber,
            ReadId::SoftwareVersion,
            ReadId::HardwareNumber,
            ReadId::HardwareVersion,
            ReadId::SystemName,
            ReadId::EngineCode,
            ReadId::OilLevel,
            ReadId::ChargeActuatorAcknowledgment,
            ReadId::ChargeAdaptationHigh,
            ReadId::ChargeAdaptationLow,
            ReadId::IntakeSensorVoltage,
            ReadId::ObdIntakeTemperature,
        ] {
            let [_, hi, lo] = id.request();
            let mut bytes = [0xFF; 67];
            bytes[..3].copy_from_slice(&[0x62, hi, lo]);
            bytes[3] = 0;
            for end in 4..=bytes.len() {
                assert_eq!(
                    id.classify(&bytes[..end]),
                    Ok(Reply::Positive {
                        data: &bytes[3..end],
                        schema: SchemaMatch::Unknown,
                    })
                );
                assert_eq!(id.candidate(&bytes[..end]), None);
            }
        }
    }

    #[test]
    /// Catches converting schema-mismatched replies instead of preserving their failure to match.
    fn known_width_mismatch_retains_bytes_but_blocks_candidate_numbers() {
        for (id, width) in [(ReadId::CoolantTemperature, 1), (ReadId::IgnitionRetard1, 2)] {
            let [_, hi, lo] = id.request();
            let mut bytes = [0; 67];
            bytes[..3].copy_from_slice(&[0x62, hi, lo]);
            for len in 1..=64 {
                let schema = if len == width {
                    SchemaMatch::Exact
                } else {
                    SchemaMatch::Mismatch {
                        expected: width,
                        actual: len,
                    }
                };
                assert_eq!(
                    id.classify(&bytes[..3 + len]),
                    Ok(Reply::Positive {
                        data: &bytes[3..3 + len],
                        schema,
                    })
                );
                assert_eq!(id.candidate(&bytes[..3 + len]).is_some(), len == width);
            }
        }
    }

    #[test]
    /// Catches unsigned, little-endian or absolute-value decoding of signed cylinder corrections.
    fn signed_retardation_boundaries_keep_their_direction_and_cylinder_echo() {
        for id in [
            ReadId::IgnitionRetard1,
            ReadId::IgnitionRetard2,
            ReadId::IgnitionRetard3,
            ReadId::IgnitionRetard4,
        ] {
            let [_, hi, lo] = id.request();
            for (data, expected) in [
                ([0, 0], 0.0),
                ([0, 1], 0.01),
                ([0x7F, 0xFF], 327.67),
                ([0x80, 0], -327.68),
                ([0xFF, 0xFF], -0.01),
                ([0xFE, 0x89], -3.75),
            ] {
                assert_eq!(
                    id.candidate(&[0x62, hi, lo, data[0], data[1]]),
                    Some(CandidateValue::Degrees(expected))
                );
            }
            assert_eq!(id.candidate(&[0x62, hi, lo + 1, 0, 1]), None);
        }
    }

    #[test]
    /// Catches gauge subtraction, wrong endianness and mixed actual/requested pressure units.
    fn pressure_candidates_remain_absolute_and_unsigned() {
        for id in [
            ReadId::ManifoldPressure,
            ReadId::ChargePressureActual,
            ReadId::ChargePressureSpecified,
        ] {
            let [_, hi, lo] = id.request();
            assert_eq!(
                id.candidate(&[0x62, hi, lo, 0x09, 0xC4]),
                Some(CandidateValue::AbsoluteKpa(250.0))
            );
            assert_eq!(
                id.candidate(&[0x62, hi, lo, 0xFF, 0xFF]),
                Some(CandidateValue::AbsoluteKpa(6553.5))
            );
        }
        let Some(CandidateValue::AbsoluteKpa(value)) =
            ReadId::AmbientPressure.candidate(&[0x62, 0x13, 0xCA, 0x2F, 0x1C])
        else {
            panic!("missing candidate")
        };
        assert!((value - 100.0).abs() < 0.002);
    }

    #[test]
    /// Catches offset/scaling swaps and accidental AFR conversion in other family candidates.
    fn temperature_voltage_and_lambda_use_only_their_own_documented_units() {
        assert_eq!(
            ReadId::CoolantTemperature.candidate(&[0x62, 0x11, 0xCD, 0]),
            Some(CandidateValue::Celsius(-40.0))
        );
        assert_eq!(
            ReadId::CoolantTemperature.candidate(&[0x62, 0x11, 0xCD, 255]),
            Some(CandidateValue::Celsius(215.0))
        );
        assert_eq!(
            ReadId::IntakeTemperature.candidate(&[0x62, 0x10, 1, 100]),
            Some(CandidateValue::Celsius(27.0))
        );
        assert_eq!(
            ReadId::ModuleVoltage.candidate(&[0x62, 0x14, 0xA6, 128]),
            Some(CandidateValue::Volts(13.0))
        );
        assert_eq!(
            ReadId::Lambda.candidate(&[0x62, 0x10, 0xC0, 4, 0]),
            Some(CandidateValue::Lambda(1.0))
        );
        let Some(CandidateValue::Celsius(value)) = ReadId::OilTemperature.candidate(&[0x62, 0x20, 0x2F, 0x0E, 0x93])
        else {
            panic!("missing candidate")
        };
        assert!((value - 99.96).abs() < 0.001);
    }
}
