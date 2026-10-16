//! Offline documentary references. These strings never construct requests or decoders.
use crate::config::{DefinitionId, OBD_REFERENCE, definition};

pub const MAX_QUERY_BYTES: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Vehicle,
    Signals,
    Units,
    Sources,
    Validation,
    Questions,
}
impl Section {
    pub const ALL: [Self; 6] = [
        Self::Vehicle,
        Self::Signals,
        Self::Units,
        Self::Sources,
        Self::Validation,
        Self::Questions,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Vehicle => "Vehicle",
            Self::Signals => "Signals",
            Self::Units => "Units & limits",
            Self::Sources => "Sources",
            Self::Validation => "Validation",
            Self::Questions => "Open questions",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Reference,
    Implemented,
    Candidate,
    Conflict,
}
impl Status {
    pub const ALL: [Self; 4] = [Self::Reference, Self::Implemented, Self::Candidate, Self::Conflict];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Reference => "Reference only",
            Self::Implemented => "Generic decoder implemented",
            Self::Candidate => "Unqualified candidate",
            Self::Conflict => "Unresolved conflict",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub id: &'static str,
    pub title: &'static str,
    pub section: Section,
    pub status: Status,
    pub scope: &'static str,
    pub detail: &'static str,
    pub units: &'static str,
    pub formula: &'static str,
    pub references: &'static [&'static str],
    pub definition: Option<DefinitionId>,
}
/// Text projection of the authoritative runtime catalogue, never a request constructor.
pub struct BindingDetail {
    pub request: String,
    pub decoder: String,
    pub interpretation: &'static str,
    pub availability: &'static str,
}
impl Entry {
    pub fn binding(&self) -> Option<BindingDetail> {
        use crate::config::{Access, UnavailableReason};
        let d = definition(self.definition?);
        let did = |value: u16| format!("22 {:02X} {:02X}", value >> 8, value & 255);
        let request = match d.access {
            Access::None => "No request".into(),
            Access::Obd01 { pid } => format!("01 {pid:02X}"),
            Access::ReadDid22 { did: value } => did(value),
            Access::MapMinusAmbient { map_pid, ambient_pid } => format!("01 {map_pid:02X} + 01 {ambient_pid:02X}"),
            Access::DidMapMinusAmbient { map_did, ambient_did } => format!("{} + {}", did(map_did), did(ambient_did)),
            Access::Unresolved => "No qualified request mapping".into(),
        };
        let availability = if d.id == DefinitionId::Disabled {
            "Disabled; no acquisition"
        } else {
            match d.unavailable {
                None => "Generic decoder implemented; responder capability and deployment qualification still required",
                Some(UnavailableReason::CandidateDecoder) => "Unavailable: family candidate decoder is not enabled",
                Some(UnavailableReason::ScalingUnresolved) => "Unavailable: scaling prerequisites are unresolved",
                Some(UnavailableReason::SensorUnresolved) => "Unavailable: sensor identity or semantics are unresolved",
                Some(UnavailableReason::MissingRoute) => "Unavailable: a qualified route is missing",
                Some(UnavailableReason::RouteNotAvailable) => "Unavailable: route is not supported by this deployment",
            }
        };
        Some(BindingDetail {
            request,
            decoder: format!("{:?}", d.decoder),
            interpretation: d.interpretation,
            availability,
        })
    }
}
#[derive(Clone, Debug, Default)]
pub struct ReferenceState {
    query: String,
    pub truncated: bool,
    pub section: Option<Section>,
    pub status: Option<Status>,
    selected: usize,
}
impl ReferenceState {
    pub fn query(&self) -> &str { &self.query }

    pub fn set_query(
        &mut self,
        text: &str,
    ) {
        let mut end = text.len().min(MAX_QUERY_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.query = text[..end].to_owned();
        self.truncated = end < text.len();
    }

    pub fn selected(&self) -> &'static Entry { &ENTRIES[self.selected] }

    pub fn select(
        &mut self,
        id: &str,
    ) -> bool {
        if let Some(index) = ENTRIES.iter().position(|e| e.id == id) {
            self.selected = index;
            true
        } else {
            false
        }
    }

    pub fn open_definition(
        &mut self,
        id: DefinitionId,
    ) {
        if let Some(entry) = ENTRIES.iter().find(|e| e.definition == Some(id)) {
            self.select(entry.id);
        }
        self.query.clear();
        self.section = None;
        self.status = None;
        self.truncated = false;
    }

    pub fn results(&self) -> Vec<&'static Entry> {
        let query = self.query.to_lowercase();
        ENTRIES
            .iter()
            .filter(|entry| {
                if self.section.is_some_and(|s| s != entry.section) || self.status.is_some_and(|s| s != entry.status) {
                    return false;
                }
                let runtime = entry.definition.map(definition);
                let text = format!(
                    "{} {} {} {} {} {} {} {}",
                    entry.id,
                    entry.title,
                    entry.detail,
                    entry.scope,
                    entry.units,
                    entry.formula,
                    entry.status.label(),
                    runtime.map(|d| d.interpretation).unwrap_or("")
                )
                .to_lowercase();
                query.split_whitespace().all(|term| text.contains(term))
            })
            .collect()
    }
}

use crate::config::{
    DSG_REFERENCE,
    OBD_COMMANDS_REFERENCE,
    OBD_SCALING_REFERENCE,
    SIMOS_DEFINITIONS_REFERENCE,
    SIMOS_REFERENCE,
};
const TOOLS: &str = "https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PID/PIDList22-USA.csv";
const HISTORICAL: &str = "https://www.autonerdz.com/yabbfiles/Attachments/1979v3.pdf";
const ANDRO: &str = "https://github.com/fr3ts0n/AndrOBD/blob/2d4974f3095ee2594833b59372b39d175ea24d71/library/src/main/java/com/fr3ts0n/ecu/prot/obd/res/conversions.csv";
const KERNEL: &str = "https://www.kernel.org/doc/html/latest/networking/iso15765-2.html";
const ROSS: &str = "https://www.ross-tech.com/vcds/tour/controller-channels-map.php";
const FAMILY: &str = "Family evidence; exact current ECU/TCU unqualified";
const GENERIC: &str = "Generic OBD; selected responder capability and deployment qualification required";
macro_rules! card {
    (
        $id:literal,
        $title:literal,
        $section:ident,
        $status:ident,
        $scope:expr,
        $detail:literal,
        $units:literal,
        $formula:literal,
        $refs:expr,
        $definition:expr
    ) => {
        Entry {
            id: $id,
            title: $title,
            section: Section::$section,
            status: Status::$status,
            scope: $scope,
            detail: $detail,
            units: $units,
            formula: $formula,
            references: $refs,
            definition: $definition,
        }
    };
}
use DefinitionId::*;
/// Fixed reviewed dataset: no runtime import, remote fetch or executable expressions.
pub static ENTRIES: &[Entry] = &[
    card!("identity", "Historical ECU identity", Vehicle, Reference, "Historical scan, February 2026; owner statements separately identified", "Observed software 8V0906259R / 0002; hardware 06K907425J / H13; ODX EV_ECM20TFS0208V0906259R / 001002; CAN: UDS. Owner identifies a 2019 EU FWD SEAT Leon Cupra 290, DNUC EA888 Gen3 / IS38 / DQ381, Shell 100 RON E5. Simos 18.10 is owner/vendor stated and independently corroborated by a public exact-box-code record. Current post-tune binary, TCU identity, routes and hardware remain unverified. The public record's 300 PS label does not override the owner's 290 PS identity.", "Technical identity only", "No live identity or measurement claim", &["https://files.vehical.net/Project/Seat_Leon_5F_2.0T_8V0906259R_0002_SCG00A1000000/167"], None),
    card!("disabled", "Disabled channel", Signals, Reference, "Configuration", "A disabled source makes no measurement request. Opening this reference does not change the configuration.", "None", "No decoder", &[], Some(Disabled)),
    card!("coolant", "OBD coolant · 01 05", Signals, Implemented, GENERIC, "Coolant temperature. Implemented generic decoding does not establish exact-car support.", "°C", "One unsigned byte A: A - 40", &[OBD_COMMANDS_REFERENCE, OBD_REFERENCE, OBD_SCALING_REFERENCE], Some(ObdCoolant05)),
    card!("intake", "OBD intake temperature · 01 0F", Signals, Implemented, GENERIC, "Intake air temperature; retain the source location.", "°C", "One unsigned byte A: A - 40", &[OBD_COMMANDS_REFERENCE, OBD_REFERENCE, OBD_SCALING_REFERENCE], Some(ObdIntake0f)),
    card!("oil", "OBD engine oil · 01 5C", Signals, Implemented, GENERIC, "Engine oil temperature. Display span and warning thresholds are separate settings.", "°C", "One unsigned byte A: A - 40", &[OBD_COMMANDS_REFERENCE, OBD_REFERENCE, OBD_SCALING_REFERENCE], Some(ObdOil5c)),
    card!("voltage", "OBD ECU voltage · 01 42", Signals, Implemented, GENERIC, "ECU module voltage is distinct from battery-post or adapter supply voltage.", "V", "Two unsigned big-endian bytes AB: AB / 1000", &[OBD_COMMANDS_REFERENCE, OBD_REFERENCE, OBD_SCALING_REFERENCE], Some(ObdVoltage42)),
    card!("obd-boost", "MAP / BARO and derived boost · 01 0B / 33", Signals, Conflict, GENERIC, "MAP is absolute pressure. Historical PID 4F field d changes MAP scaling, never BARO 33. A top wire code alone proves neither clipping nor a safety threshold. Gauge boost needs separately qualified ambient pressure and bounded sample age/skew; no numeric bound is established here.", "kPa absolute; bar gauge only after qualification", "Default MAP A kPa; nonzero 4F d: A*10*d/255 kPa. Derived (MAP-ambient)/100 bar", &[HISTORICAL, ANDRO], Some(ObdMapMinusAmbient)),
    card!("lambda24", "Measured wideband ratio / voltage · 01 24", Signals, Conflict, GENERIC, "Scaling unresolved. Keep first-pair ratio separate from second-pair sensor voltage. Bank/sensor and PID 4F state need qualification. Historical table 2/65535 and zero-default prose 1.999/65535 disagree; implementations also use 1/32768. Do not choose from an idle display near one.", "Lambda dimensionless; sensor V separately", "x=256*A+B, y=256*C+D. Historical x*2/65535; y*8/65535 V. Competing defaults: x/32768; x*1.999/65535; y*7.999/65535", &[HISTORICAL, ANDRO, OBD_SCALING_REFERENCE], Some(ObdLambda24)),
    card!("lambda34", "Measured wideband ratio / current · 01 34", Signals, Conflict, GENERIC, "Scaling unresolved for ratio as on PID 24. Current uses offset-binary input, not signed-int16 reinterpretation. PID 4F ratio and current maxima are independent; location remains unqualified.", "Lambda dimensionless; sensor mA separately", "Historical ratio x*2/65535 vs x/32768 and x*1.999/65535. Current (y-32768)*128/32768 mA; nonzero 4F c replaces 128", &[HISTORICAL, ANDRO], Some(ObdLambda34)),
    card!("simos-coolant", "Simos coolant · 22 11CD", Signals, Candidate, FAMILY, "Pinned logger entry; not enabled as a dashboard decoder.", "°C", "u8 x - 40", &[SIMOS_REFERENCE], Some(SimosCoolant11cd)),
    card!("simos-oil", "Simos oil · 22 202F", Signals, Candidate, FAMILY, "Preserve the source's 2731.4 constant. Sensor and software parity remain unverified.", "°C", "unsigned BE16 (x-2731.4)/10", &[SIMOS_REFERENCE], Some(SimosOil202f)),
    card!("simos-intake", "Simos intake · 22 1001", Signals, Candidate, FAMILY, "Intake location and calibration need qualification.", "°C", "u8 0.75*x-48", &[SIMOS_REFERENCE], Some(SimosIntake1001)),
    card!("simos-voltage", "Simos module voltage · 22 14A6", Signals, Candidate, FAMILY, "ECU module voltage, not battery-post measurement.", "V", "u8 0.1015625*x (13*x/128)", &[SIMOS_REFERENCE], Some(SimosVoltage14a6)),
    card!("simos-boost", "Simos manifold / ambient · 39C0 / 13CA", Signals, Candidate, FAMILY, "MAP 39C0 and requested 39C1 are manifold absolute pressure. Upstream-throttle 202A / 2029 is a different location. Derived gauge boost requires matching provenance and bounded input age/skew; no numeric bound is supplied.", "kPa absolute; derived bar gauge", "unsigned BE16 MAP x/10; ambient x/120.60176665439; (MAP-ambient)/100 bar", &[SIMOS_REFERENCE], Some(SimosMapMinusAmbient)),
    card!("simos-lambda", "Simos lambda actual / target · 10C0 / F444", Signals, Candidate, FAMILY, "Dimensionless actual ratio and target are separate. Another family list rounds actual to 0.000977*x and uses target DID 1456; labels alone cannot prove equivalence. AFR needs an explicit fuel assumption.", "Lambda dimensionless", "unsigned BE16 actual x/1024; target x/32768", &[SIMOS_REFERENCE, "https://github.com/bri3d/MQBSimosLogVariables/blob/747b50e1af650c39cd9c5f6b4d58905db6debb45/exportedPIDs.csv"], Some(SimosLambda10c0)),
    card!("dsg-oil", "DSG temperature · 22 2104", Signals, Candidate, FAMILY, "DQ381 is owner identified; exact TCU identity, route, temperature location and scale are unqualified. Historical SimosTools DSG path is little-endian, unlike its engine path. Do not merge oil, clutch and mechatronic temperatures.", "Source labels °C; exact meaning unresolved", "Historical signed 16-bit identity scale; not an admitted decoder", &[SIMOS_DEFINITIONS_REFERENCE, DSG_REFERENCE], Some(DsgOil2104)),
    card!("egt", "Exhaust temperature / GPF", Signals, Candidate, FAMILY, "No exact manifold EGT or GPF DID/scaling established. Catalyst temperature is not interchangeable with manifold EGT or filter temperature. Measured and modelled values must remain distinct.", "Location-specific °C once qualified", "No qualified definition", &[ROSS], Some(UnresolvedEgt)),
    card!("gear-conflict", "Gear · 210F offset conflict", Questions, Conflict, FAMILY, "VW_Flash and SimosTools SI use x; SimosTools USA uses x+1. Selected versus engaged gear semantics remain open.", "Gear enum", "Competing x and x+1", &[SIMOS_REFERENCE, SIMOS_DEFINITIONS_REFERENCE, TOOLS], None),
    card!("turbo-conflict", "Turbo speed · 1040 unit conflict", Questions, Conflict, FAMILY, "The same numeric scale is labelled rpm in VW_Flash and krpm in SimosTools USA. No measured turbo speed or warning threshold is established.", "rpm versus krpm unresolved", "x/163.84 with conflicting units", &[SIMOS_REFERENCE, TOOLS], None),
    card!("valve-conflict", "Valve lift · 167C versus 3D6C", Questions, Conflict, FAMILY, "Two pinned family lists assign different DIDs. Do not silently merge them.", "Definition unresolved", "VW_Flash 167C; SimosTools USA 3D6C", &[SIMOS_REFERENCE, TOOLS], None),
    card!("dsg-pressure-conflict", "DSG clutch pressure · 38EA / 38EB", Questions, Conflict, FAMILY, "Historical unsigned fields were corrected to signed in an issue attachment. The discussion concerns DQ500 and mentions DQ381; it does not qualify this TCU. The pinned DSG parser is little-endian. Vendor tuning pressure claims are not safe alarms.", "bar (family claim)", "x/100; unsigned versus signed 16-bit conflict", &[SIMOS_DEFINITIONS_REFERENCE, DSG_REFERENCE, "https://github.com/Switchleg1/SimosTools/issues/7"], None),
    card!("scaling4f", "PID 4F · independent scaling prerequisites", Units, Conflict, "Historical SAE tables; current applicable annex unresolved", "Four fields a/b/c/d are independent maxima for ratio/voltage/current/MAP. Zero selects that field's original definition. Established unsupported differs from timeout or unqueried. Keep all four bytes, responder, session and timestamps; requalify after identity/session changes. No decoder state machine is introduced here.", "ratio; V; mA; 10*d kPa", "Nonzero a: ratio x*a/65535; b: voltage y*b/65535; c: current (y-32768)*c/32768; d: MAP A*10*d/255", &[HISTORICAL, ANDRO], None),
    card!("commanded-lambda", "Commanded ratio · 01 44", Signals, Conflict, GENERIC, "Commanded ratio is distinct from measured O2 ratio. Historical printed defaults disagree with their zero-default prose and implementations. A rounded display cannot settle denominator choice. Applicable annex/erratum or exact-controller evidence is still needed.", "Lambda dimensionless", "Historical x*2/65535; prose x*1.999/65535; implementations x/32768 or rounded 0.0000305*x", &[HISTORICAL, ANDRO, OBD_SCALING_REFERENCE], None),
    card!("fuel", "Fuel, lambda and AFR · 100 RON / E5", Units, Reference, "Owner fuel label; composition unmeasured", "RON is not ethanol percentage. E5 is not an assay of exactly 5% ethanol. Lambda avoids an invented fuel-specific AFR. AFR = lambda times the configured stoichiometric ratio; composition and density assumptions must be stated. Editing fuel presentation does not qualify a sensor.", "Lambda; AFR mass ratio", "No measured ethanol signal", &["https://www.seat.co.uk/datamanual-manual/leon/my20_w28/en-uk/LEON_07_19_EN.pdf"], None),
    card!("limits", "Display ranges, freshness and warnings", Units, Reference, "Model guidance and host policies", "SEAT's model-family oil 80–120 °C normal-driving context is not a 120 °C critical alarm. Gauge maximum is a display span. Freshness is an acquisition policy. A tuning claim, family design rating or wire maximum cannot set a safe vehicle limit.", "Canonical metric values; presentation conversions", "No warning-default changes", &["https://www.seat.co.uk/datamanual-manual/leon/my20_w28/en-uk/LEON_07_19_EN.pdf"], None),
    card!("transport", "CAN / ISO-TP / UDS read-only transport", Sources, Reference, "Protocol documentation; exact route unverified", "Read-only diagnostics transmit requests and flow control. CAN: UDS identity does not prove addresses, bitrate, padding or payload schema. Mode 01 and service 22 are distinct. Reassembly must check source, epoch, lengths and sequence. NRC 78 is pending, not a sample; timeout is not proof of unsupported. No automatic session/security unlock.", "Request bytes and bounded timing", "01 PID; 22 DID_hi DID_lo; positive 41 PID or 62 DID echo", &[KERNEL, "https://udsoncan.readthedocs.io/en/latest/udsoncan/client.html"], None),
    card!("identity-discovery", "Read-only identity and capability discovery", Validation, Reference, "Future capture checklist; no vehicle connection made", "Capture current engine and TCU identities separately after tuning. Family identity DID candidates F19E/F1A2/F187/F189/F191/F1A3/F197/F1AD require exact lengths and semantics. Mode 01 support pages apply only to the selected responder/current epoch. Do not retrieve or publish VIN merely for this reference.", "Identity / support evidence", "A scan label is not a DID map", &["https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/constants.py", ROSS], None),
    card!("qualification", "Qualify a value before enabling it", Validation, Reference, "Four independent evidence concepts", "Evidence describes the source; applicability describes which controller it concerns; implementation describes decoder availability; live qualification requires current physical evidence. Record current identity/session, responder, raw request/reply, width, byte order, signedness, units, location, sentinel/NRC behavior, timestamps and independent comparisons. Measure latency/staleness and preserve sanitized replay fixtures. No card can grant live qualification.", "Evidence, applicability, implementation, live qualification", "Family/tool compatibility is not exact-car support", &[ROSS, "https://support.obdeleven.com/en/articles/11009883-what-is-live-data"], None),
    card!("trx", "CANchecked U-DNU.TRX · opaque vendor profile", Sources, Reference, "DNU-family lead only", "Supplied TRX has an ASCII header followed by opaque binary. Its contents are not a decoded DID/scale map. Filename and catalogue presence do not qualify DNUC sensors. CANchecked product transport settings and DSG accessory broadcasts cannot be copied as native DQ381 definitions.", "Unknown until decoded and qualified", "No recovered raw DID/scaling table", &["https://www.canchecked.de/updater/MFD2832-Gen2-manual.pdf", "https://www.canchecked.de/downloads/uds-einrichtung/"], None),
    card!("comparisons", "EA888 / Simos / DQ381 family comparisons", Vehicle, Reference, FAMILY, "Golf GTI TCR FWD/GPF/DSG is a useful 290 PS analogue. Audi S3/TTS, shared 06K907425J hardware and Simos family labels do not prove exact software parity. Tuning-tool support differs by box code and product. Advanced RAM logging can require patches and is outside this dashboard's read-only constructors.", "Family context only", "No shared-hardware decoder inference", &["https://www.volkswagen-newsroom.com/en/golf-gti-tcr-the-driving-presentation-4573/technical-data-the-golf-gti-tcr-in-numbers-4577", "https://docs.vehical.net/logger_basicadvanced"], None),
    card!("fuel-pressure", "Fuel pressure actual / requested", Signals, Candidate, FAMILY, "Direct fuel pressure 2027/293B and low-side 2025/2932 are different sources. Compare qualified synchronized inputs; do not borrow thresholds.", "bar", "unsigned BE16 direct x/10; low-side x/1000", &[SIMOS_REFERENCE], None),
    card!("ignition", "Ignition, misfire, torque and airflow", Signals, Candidate, FAMILY, "2004 average ignition and 200A–200D corrections are distinct. 2904 misfire count has an unresolved counting window. 437C/4380 torque is ECU-reported, not dyno wheel torque. 2032 air mass and 1070 pedal need independent semantics.", "degrees; count; Nm; kg/h; %", "signed BE16 ignition x/100, torque x/10; unsigned BE16 misfires x, airflow x, pedal x/10.24", &[SIMOS_REFERENCE], None),
    card!("gpf-questions", "GPF and DSG measurement gaps", Questions, Candidate, FAMILY, "Investigate explicitly labelled GPF differential pressure, soot versus ash estimates, regeneration state and temperature locations. No exact DIDs are established here; do not reuse diesel DPF definitions. DSG slip, clutch thermal state, pump state and selected/engaged gear need current TCU evidence.", "Unknown until individually qualified", "No invented GPF/TCU definitions", &[ROSS], None),
];
