//! Canonical, bounded selected-peer evidence; all time values are boot-relative.
use super::capture_store::{PAYLOAD_MAX, crc32};
use super::names::{NameSource, ObservedName};
use super::*;
use crate::health::reset::{BootEvidence, ResetRecord, ResetStage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    Stock,
    Expert280,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildIdentity {
    pub source_digest: [u8; 32],
    pub cpu_hz: u32,
    pub spi_hz: u32,
    pub profile: BuildProfile,
    pub source_identity_available: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    BufferTooSmall,
    TooLarge,
    NoPeer,
    InvalidState,
    Truncated,
    Magic,
    Version,
    Integrity,
    InvalidData,
    Relationships,
    Trailing,
}

#[derive(Clone, Copy, Debug)]
pub struct PacketEvidence {
    bytes: [u8; 31],
    len: u8,
    pub truncated: bool,
    pub observation: Option<PacketObservation>,
}
impl PacketEvidence {
    pub fn bytes(&self) -> &[u8] { &self.bytes[..usize::from(self.len)] }
}
#[derive(Clone, Copy, Debug)]
pub struct ValueRef<'a> {
    pub service_start: u16,
    pub handle: u16,
    pub uuid: u16,
    pub result: ReadResult,
    pub failure: Option<FailureReason>,
    pub elapsed_ms: u32,
    pub bytes: &'a [u8],
    pub truncated: bool,
}
impl ValueRef<'_> {
    pub fn length_valid(&self) -> bool { info_length_valid(self.uuid, self.bytes.len()) }
}
#[derive(Clone, Copy, Debug)]
pub enum Record<'a> {
    Service(Service),
    Characteristic(Characteristic),
    Descriptor(Descriptor),
    Value(ValueRef<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureMetadata {
    pub schema_version: u16,
    /// Absent in legacy v1/v2 captures, never inferred from current firmware.
    pub fault_latch: Option<FaultLatch>,
    pub radio_bus_hz: Option<u32>,
    /// Absent in v1/v2/v3; never filled from the current Pico when loading.
    pub cached_name: Option<ObservedName>,
    pub boot: Option<BootEvidence>,
    pub build: BuildIdentity,
    pub captured_at_ms: u64,
    pub revision: u32,
    pub gatt_requested: bool,
    pub phase: RadioPhase,
    pub error: ProbeError,
    pub peer: Peer,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    pub reports: u32,
    pub rssi: Option<i8>,
    pub advertisement: PacketEvidence,
    pub response: PacketEvidence,
    pub link: Option<LinkInfo>,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub disconnect_reason: Option<u8>,
    pub cleanup_complete: bool,
    pub stages: [bool; 4],
    pub omitted: u32,
    pub malformed: u32,
    pub complete: bool,
    pub failure: Option<InspectionFailure>,
    pub failure_context: ConnectionFailureContext,
    pub dropped_reports: u32,
    pub malformed_reports: u32,
    pub catalog_overflow_reports: u32,
    pub scan_diagnostics: ScanDiagnostics,
}

impl CaptureMetadata {
    // Private parsing destination. Never published until all fields and record
    // relationships have been decoded and validated successfully.
    const EMPTY: Self = Self {
        schema_version: 0,
        fault_latch: None,
        radio_bus_hz: None,
        cached_name: None,
        boot: None,
        build: BuildIdentity {
            source_digest: [0; 32],
            cpu_hz: 0,
            spi_hz: 0,
            profile: BuildProfile::Stock,
            source_identity_available: false,
        },
        captured_at_ms: 0,
        revision: 0,
        gatt_requested: false,
        phase: RadioPhase::Starting,
        error: ProbeError::None,
        peer: Peer {
            address: [0; 6],
            random: false,
        },
        first_seen_ms: 0,
        last_seen_ms: 0,
        reports: 0,
        rssi: None,
        advertisement: PacketEvidence {
            bytes: [0; 31],
            len: 0,
            truncated: false,
            observation: None,
        },
        response: PacketEvidence {
            bytes: [0; 31],
            len: 0,
            truncated: false,
            observation: None,
        },
        link: None,
        started_at_ms: None,
        ended_at_ms: None,
        disconnect_reason: None,
        cleanup_complete: false,
        stages: [false; 4],
        omitted: 0,
        malformed: 0,
        complete: false,
        failure: None,
        failure_context: ConnectionFailureContext {
            local_error: None,
            disconnect_reason: None,
        },
        dropped_reports: 0,
        malformed_reports: 0,
        catalog_overflow_reports: 0,
        scan_diagnostics: ScanDiagnostics::new(),
    };
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureView<'a> {
    metadata: &'a CaptureMetadata,
    records: &'a [u8],
    counts: [u16; 4],
}
impl<'a> CaptureView<'a> {
    pub fn metadata(&self) -> &CaptureMetadata { self.metadata }

    pub fn counts(&self) -> [u16; 4] { self.counts }

    pub fn records(&self) -> Records<'a> { Records { bytes: self.records } }
}
/// Reusable parsing storage. Contains no references into a capture buffer.
/// A returned view borrows this decoder, preventing another decode while in use.
pub struct Decoder {
    metadata: CaptureMetadata,
}
impl Default for Decoder {
    fn default() -> Self { Self::new() }
}
impl Decoder {
    pub const fn new() -> Self {
        Self {
            metadata: CaptureMetadata::EMPTY,
        }
    }

    pub fn decode<'a>(
        &'a mut self,
        bytes: &'a [u8],
    ) -> Result<CaptureView<'a>, CaptureError> {
        decode_into(&mut self.metadata, bytes)
    }

    pub fn encode(
        &mut self,
        probe: &Probe,
        build: BuildIdentity,
        captured_at_ms: u64,
        output: &mut [u8],
    ) -> Result<usize, CaptureError> {
        let n = encode_bytes(probe, build, captured_at_ms, output)?;
        self.decode(&output[..n])?;
        Ok(n)
    }
}

/// Convenience owner for host callers. Firmware uses a reusable Decoder instead.
#[derive(Debug)]
pub struct DecodedCapture<'a> {
    metadata: CaptureMetadata,
    records: &'a [u8],
    counts: [u16; 4],
}
impl<'a> DecodedCapture<'a> {
    pub fn view(&self) -> CaptureView<'_> {
        CaptureView {
            metadata: &self.metadata,
            records: self.records,
            counts: self.counts,
        }
    }

    pub fn metadata(&self) -> &CaptureMetadata { &self.metadata }

    pub fn records(&self) -> Records<'a> { Records { bytes: self.records } }

    pub fn counts(&self) -> [u16; 4] { self.counts }
}

pub struct Records<'a> {
    bytes: &'a [u8],
}
impl<'a> Iterator for Records<'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let mut r = Reader { bytes: self.bytes };
        let record = read_record(&mut r).ok()?;
        self.bytes = r.bytes;
        Some(record)
    }
}

const MAGIC: &[u8; 8] = b"BLECAP01";
const HEADER: usize = 24;
/// Prefix size needed to identify the format and validate its current envelope.
pub const HEADER_BYTES: usize = HEADER;

struct Writer<'a> {
    bytes: &'a mut [u8],
    pos: usize,
}
impl Writer<'_> {
    fn bytes(
        &mut self,
        data: &[u8],
    ) -> Result<(), CaptureError> {
        let end = self.pos.checked_add(data.len()).ok_or(CaptureError::TooLarge)?;
        if end > PAYLOAD_MAX {
            return Err(CaptureError::TooLarge);
        }
        self.bytes
            .get_mut(self.pos..end)
            .ok_or(CaptureError::BufferTooSmall)?
            .copy_from_slice(data);
        self.pos = end;
        Ok(())
    }

    fn u8(
        &mut self,
        v: u8,
    ) -> Result<(), CaptureError> {
        self.bytes(&[v])
    }

    fn bool(
        &mut self,
        v: bool,
    ) -> Result<(), CaptureError> {
        self.u8(u8::from(v))
    }

    fn u16(
        &mut self,
        v: u16,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    fn u32(
        &mut self,
        v: u32,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    fn u64(
        &mut self,
        v: u64,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    fn opt_u8(
        &mut self,
        v: Option<u8>,
    ) -> Result<(), CaptureError> {
        self.bool(v.is_some())?;
        if let Some(v) = v {
            self.u8(v)?;
        }
        Ok(())
    }

    fn opt_u32(
        &mut self,
        v: Option<u32>,
    ) -> Result<(), CaptureError> {
        self.bool(v.is_some())?;
        if let Some(v) = v {
            self.u32(v)?;
        }
        Ok(())
    }

    fn opt_u64(
        &mut self,
        v: Option<u64>,
    ) -> Result<(), CaptureError> {
        self.bool(v.is_some())?;
        if let Some(v) = v {
            self.u64(v)?;
        }
        Ok(())
    }

    fn uuid(
        &mut self,
        v: Uuid,
    ) -> Result<(), CaptureError> {
        if !matches!(v.len, 2 | 4 | 16) {
            return Err(CaptureError::InvalidData);
        }
        self.u8(v.len)?;
        self.bytes(&v.bytes[..usize::from(v.len)])
    }

    fn record(
        &mut self,
        tag: u8,
        write: impl FnOnce(&mut Self) -> Result<(), CaptureError>,
    ) -> Result<(), CaptureError> {
        self.u8(tag)?;
        let length = self.pos;
        self.u16(0)?;
        let start = self.pos;
        write(self)?;
        self.bytes[length..length + 2].copy_from_slice(&((self.pos - start) as u16).to_le_bytes());
        Ok(())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(
        &mut self,
        n: usize,
    ) -> Result<&'a [u8], CaptureError> {
        let head = self.bytes.get(..n).ok_or(CaptureError::Truncated)?;
        self.bytes = &self.bytes[n..];
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, CaptureError> { Ok(self.take(1)?[0]) }

    fn bool(&mut self) -> Result<bool, CaptureError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CaptureError::InvalidData),
        }
    }

    fn u16(&mut self) -> Result<u16, CaptureError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, CaptureError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, CaptureError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn opt_u8(&mut self) -> Result<Option<u8>, CaptureError> {
        if self.bool()? { Ok(Some(self.u8()?)) } else { Ok(None) }
    }

    fn opt_u32(&mut self) -> Result<Option<u32>, CaptureError> {
        if self.bool()? { Ok(Some(self.u32()?)) } else { Ok(None) }
    }

    fn opt_u64(&mut self) -> Result<Option<u64>, CaptureError> {
        if self.bool()? { Ok(Some(self.u64()?)) } else { Ok(None) }
    }

    fn uuid(&mut self) -> Result<Uuid, CaptureError> {
        let len = usize::from(self.u8()?);
        Uuid::from_wire(self.take(len)?).ok_or(CaptureError::InvalidData)
    }
}

macro_rules! codes {
    ($encode:ident,$decode:ident,$typ:ty,{$($variant:path=>$code:literal),+$(,)?})=>{
        fn $encode(value:$typ)->u8{match value{$($variant=>$code),+}}
        fn $decode(value:u8)->Result<$typ,CaptureError>{match value{$($code=>Ok($variant)),+,_=>Err(CaptureError::InvalidData)}}
    }
}
codes!(phase_code,read_phase,RadioPhase,{RadioPhase::Starting=>0,RadioPhase::Scanning=>1,RadioPhase::Paused=>2,RadioPhase::Stopping=>3,RadioPhase::Connecting=>4,RadioPhase::Discovering=>5,RadioPhase::Disconnecting=>6,RadioPhase::Fault=>7});
codes!(error_code,read_error,ProbeError,{ProbeError::None=>0,ProbeError::Scan=>1,ProbeError::Connect=>2,ProbeError::Timeout=>3,ProbeError::Discovery=>4,ProbeError::Capacity=>5,ProbeError::Disconnected=>6,ProbeError::Cleanup=>7,ProbeError::Transport=>8,ProbeError::InvalidPeer=>9});
codes!(stage_code,read_stage,FailureStage,{FailureStage::Connect=>0,FailureStage::Mtu=>1,FailureStage::Services=>2,FailureStage::Characteristics=>3,FailureStage::Descriptors=>4,FailureStage::DeviceInfo=>5,FailureStage::ConnectionEvents=>6,FailureStage::ClientTask=>7,FailureStage::Inspection=>8});
codes!(kind_code,read_kind,ReportKind,{ReportKind::Connectable=>0,ReportKind::Directed=>1,ReportKind::ScanResponse=>2,ReportKind::Other=>3});
codes!(result_code,read_result,ReadResult,{ReadResult::Ok=>0,ReadResult::Protected=>1,ReadResult::Unavailable=>2,ReadResult::Error=>3});

fn write_reason(
    w: &mut Writer<'_>,
    v: Option<FailureReason>,
) -> Result<(), CaptureError> {
    let (tag, code) = match v {
        None => (0, 0),
        Some(FailureReason::Controller) => (1, 0),
        Some(FailureReason::Hardware(c)) => (2, c),
        Some(FailureReason::Timeout) => (3, 0),
        Some(FailureReason::Disconnected) => (4, 0),
        Some(FailureReason::DisconnectCode(c)) => (5, c),
        Some(FailureReason::Capacity) => (6, 0),
        Some(FailureReason::Att(c)) => (7, c),
        Some(FailureReason::Hci(c)) => (8, c),
        Some(FailureReason::HciDecode) => (9, 0),
        Some(FailureReason::UnexpectedResponse) => (10, 0),
        Some(FailureReason::InvalidValue) => (11, 0),
        Some(FailureReason::Codec) => (12, 0),
        Some(FailureReason::InvalidState) => (13, 0),
        Some(FailureReason::NotFound) => (14, 0),
        Some(FailureReason::Unsupported) => (15, 0),
        Some(FailureReason::Busy) => (16, 0),
        Some(FailureReason::Other) => (17, 0),
        Some(FailureReason::DeclarationLength) => (18, 0),
        Some(FailureReason::DeclarationOrder) => (19, 0),
        Some(FailureReason::ValueHandle) => (20, 0),
        Some(FailureReason::PeerMismatch) => (21, 0),
    };
    w.u8(tag)?;
    w.u8(code)
}
fn read_reason(r: &mut Reader<'_>) -> Result<Option<FailureReason>, CaptureError> {
    let tag = r.u8()?;
    let code = r.u8()?;
    if !matches!(tag, 2 | 5 | 7 | 8) && code != 0 {
        return Err(CaptureError::InvalidData);
    }
    Ok(match tag {
        0 => None,
        1 => Some(FailureReason::Controller),
        2 => Some(FailureReason::Hardware(code)),
        3 => Some(FailureReason::Timeout),
        4 => Some(FailureReason::Disconnected),
        5 => Some(FailureReason::DisconnectCode(code)),
        6 => Some(FailureReason::Capacity),
        7 => Some(FailureReason::Att(code)),
        8 => Some(FailureReason::Hci(code)),
        9 => Some(FailureReason::HciDecode),
        10 => Some(FailureReason::UnexpectedResponse),
        11 => Some(FailureReason::InvalidValue),
        12 => Some(FailureReason::Codec),
        13 => Some(FailureReason::InvalidState),
        14 => Some(FailureReason::NotFound),
        15 => Some(FailureReason::Unsupported),
        16 => Some(FailureReason::Busy),
        17 => Some(FailureReason::Other),
        18 => Some(FailureReason::DeclarationLength),
        19 => Some(FailureReason::DeclarationOrder),
        20 => Some(FailureReason::ValueHandle),
        21 => Some(FailureReason::PeerMismatch),
        _ => return Err(CaptureError::InvalidData),
    })
}
fn write_pair(
    w: &mut Writer<'_>,
    v: Option<(u16, u16)>,
) -> Result<(), CaptureError> {
    w.bool(v.is_some())?;
    if let Some((a, b)) = v {
        w.u16(a)?;
        w.u16(b)?;
    }
    Ok(())
}
fn read_pair(r: &mut Reader<'_>) -> Result<Option<(u16, u16)>, CaptureError> {
    if r.bool()? {
        Ok(Some((r.u16()?, r.u16()?)))
    } else {
        Ok(None)
    }
}
fn write_packet(
    w: &mut Writer<'_>,
    p: &Payload<31>,
    meta: Option<PacketObservation>,
) -> Result<(), CaptureError> {
    if usize::from(p.len) > 31 {
        return Err(CaptureError::InvalidData);
    }
    w.u8(p.len)?;
    w.bytes(p.as_slice())?;
    w.bool(p.truncated)?;
    w.bool(meta.is_some())?;
    if let Some(meta) = meta {
        w.u64(meta.at_ms)?;
        w.opt_u8(meta.rssi.map(|v| v as u8))?;
        w.u8(kind_code(meta.kind))?;
    }
    Ok(())
}
fn read_packet(
    r: &mut Reader<'_>,
    packet: &mut PacketEvidence,
) -> Result<(), CaptureError> {
    let n = usize::from(r.u8()?);
    if n > 31 {
        return Err(CaptureError::InvalidData);
    }
    packet.bytes.fill(0);
    packet.bytes[..n].copy_from_slice(r.take(n)?);
    packet.len = n as u8;
    packet.truncated = r.bool()?;
    packet.observation = if r.bool()? {
        Some(PacketObservation {
            at_ms: r.u64()?,
            rssi: read_rssi(r)?,
            kind: read_kind(r.u8()?)?,
        })
    } else {
        None
    };
    if packet.observation.is_none() && (packet.len != 0 || packet.truncated) {
        return Err(CaptureError::InvalidData);
    }
    Ok(())
}

fn read_rssi(r: &mut Reader<'_>) -> Result<Option<i8>, CaptureError> {
    let value = r.opt_u8()?.map(|v| v as i8);
    if value.is_some_and(|v| !(-127..=20).contains(&v)) {
        return Err(CaptureError::InvalidData);
    }
    Ok(value)
}
fn write_scan(
    w: &mut Writer<'_>,
    scan: &ScanDiagnostics,
) -> Result<(), CaptureError> {
    w.u32(scan.total)?;
    w.u32(scan.invalid_size)?;
    w.u32(scan.invalid_value)?;
    w.bool(scan.last.is_some())?;
    if let Some(v) = &scan.last {
        w.u8(match v.kind {
            ScanErrorKind::InvalidSize => 0,
            ScanErrorKind::InvalidValue => 1,
        })?;
        w.u32(v.revision)?;
        w.u8(v.index)?;
        w.u8(v.total_reports)?;
        w.u16(v.report_bytes)?;
        w.u16(v.remaining_bytes)?;
        w.opt_u8(v.declared_data_len)?;
        w.u64(v.at_ms)?;
    }
    Ok(())
}
fn read_scan(
    r: &mut Reader<'_>,
    scan: &mut ScanDiagnostics,
) -> Result<(), CaptureError> {
    scan.total = r.u32()?;
    scan.invalid_size = r.u32()?;
    scan.invalid_value = r.u32()?;
    scan.last = if r.bool()? {
        Some(ScanFailure {
            kind: match r.u8()? {
                0 => ScanErrorKind::InvalidSize,
                1 => ScanErrorKind::InvalidValue,
                _ => return Err(CaptureError::InvalidData),
            },
            revision: r.u32()?,
            index: r.u8()?,
            total_reports: r.u8()?,
            report_bytes: r.u16()?,
            remaining_bytes: r.u16()?,
            declared_data_len: r.opt_u8()?,
            at_ms: r.u64()?,
        })
    } else {
        None
    };
    Ok(())
}

fn write_metadata(
    w: &mut Writer<'_>,
    p: &Probe,
    b: BuildIdentity,
    at: u64,
) -> Result<(), CaptureError> {
    let d = p.selected_device().ok_or(CaptureError::NoPeer)?;
    let i = &p.inspection;
    let requested = matches!(p.request.action,Action::Inspect(peer)if peer==d.peer);
    if matches!(p.request.action,Action::Inspect(peer)if peer!=d.peer) {
        return Err(CaptureError::InvalidState);
    }
    w.bytes(&b.source_digest)?;
    w.u32(b.cpu_hz)?;
    w.u32(b.spi_hz)?;
    w.u8(match b.profile {
        BuildProfile::Stock => 0,
        BuildProfile::Expert280 => 1,
    })?;
    w.bool(b.source_identity_available)?;
    w.u64(at)?;
    w.u32(p.request.revision)?;
    w.bool(requested)?;
    w.u8(phase_code(p.phase))?;
    w.u8(error_code(p.error))?;
    w.bytes(&d.peer.address)?;
    w.bool(d.peer.random)?;
    w.u64(d.first_seen_ms)?;
    w.u64(d.last_seen_ms)?;
    w.u32(d.reports)?;
    w.opt_u8(d.rssi.map(|v| v as u8))?;
    write_packet(w, &d.advertisement, d.advertisement_meta)?;
    write_packet(w, &d.response, d.response_meta)?;
    w.bool(i.link.is_some())?;
    if let Some(v) = i.link {
        w.u64(v.connected_at_ms)?;
        w.u32(v.connect_elapsed_ms)?;
        w.u32(v.interval_us)?;
        w.u16(v.latency)?;
        w.u32(v.supervision_timeout_us)?;
        w.bool(v.att_mtu.is_some())?;
        if let Some(mtu) = v.att_mtu {
            w.u16(mtu)?;
        }
    }
    w.opt_u64(i.started_at_ms)?;
    w.opt_u64(i.ended_at_ms)?;
    w.opt_u8(i.disconnect_reason)?;
    w.bool(i.cleanup_complete)?;
    for s in i.stages {
        w.bool(s)?;
    }
    w.u32(i.omitted)?;
    w.u32(i.malformed)?;
    w.bool(i.complete)?;
    w.bool(p.failure.is_some())?;
    if let Some(f) = p.failure {
        w.u8(stage_code(f.stage))?;
        write_reason(w, Some(f.reason))?;
        write_pair(w, f.range)?;
        write_pair(w, f.last_accepted)?;
        w.opt_u32(f.elapsed_ms)?;
        w.bool(f.rejected.is_some())?;
        if let Some(rejected) = f.rejected {
            w.u16(rejected.handle)?;
            w.u16(rejected.length())?;
            w.bytes(rejected.bytes())?;
        }
    }
    write_reason(w, p.failure_context.local_error)?;
    w.opt_u8(p.failure_context.disconnect_reason)?;
    w.u32(p.dropped_reports)?;
    w.u32(p.malformed_reports)?;
    w.u32(p.catalog.overflow_reports)?;
    write_scan(w, &p.scan_diagnostics)?;
    w.u16(p.fault_latch.bits())?;
    w.u8(p.fault_latch.first().map_or(0, |f| f as u8))?;
    if p.radio_bus_hz == Some(0) {
        return Err(CaptureError::InvalidData);
    }
    w.u32(p.radio_bus_hz.unwrap_or(0))?;
    w.bool(d.cached_name.is_some())?;
    if let Some(name) = d.cached_name {
        if usize::from(name.data.len) > 64 {
            return Err(CaptureError::InvalidData);
        }
        w.u8(name.source.code())?;
        w.bool(name.complete)?;
        w.u64(name.at_ms)?;
        w.u8(name.data.len)?;
        w.bytes(name.data.as_slice())?;
        w.bool(name.data.truncated)?;
    }
    w.bool(p.boot.is_some())?;
    if let Some(boot) = p.boot {
        w.u32(boot.raw_reason)?;
        w.opt_u8(boot.boot_type)?;
        w.bool(boot.previous.is_some())?;
        if let Some(previous) = boot.previous {
            w.u8(previous.stage.code())?;
            w.u32(previous.elapsed_ms)?;
        }
    }
    Ok(())
}
fn read_metadata<'a>(
    r: &mut Reader<'a>,
    m: &mut CaptureMetadata,
    version: u16,
) -> Result<(), CaptureError> {
    m.schema_version = version;
    m.build.source_digest.copy_from_slice(r.take(32)?);
    m.build.cpu_hz = r.u32()?;
    m.build.spi_hz = r.u32()?;
    m.build.profile = match r.u8()? {
        0 => BuildProfile::Stock,
        1 => BuildProfile::Expert280,
        _ => return Err(CaptureError::InvalidData),
    };
    m.build.source_identity_available = r.bool()?;
    if m.build.cpu_hz == 0
        || m.build.spi_hz == 0
        || (m.build.source_identity_available && m.build.source_digest == [0; 32])
    {
        return Err(CaptureError::InvalidData);
    }
    m.captured_at_ms = r.u64()?;
    m.revision = r.u32()?;
    m.gatt_requested = r.bool()?;
    m.phase = read_phase(r.u8()?)?;
    m.error = read_error(r.u8()?)?;
    m.peer.address.copy_from_slice(r.take(6)?);
    m.peer.random = r.bool()?;
    m.first_seen_ms = r.u64()?;
    m.last_seen_ms = r.u64()?;
    m.reports = r.u32()?;
    m.rssi = read_rssi(r)?;
    read_packet(r, &mut m.advertisement)?;
    read_packet(r, &mut m.response)?;
    if m.advertisement
        .observation
        .is_some_and(|v| !matches!(v.kind, ReportKind::Connectable | ReportKind::Directed))
        || m.response
            .observation
            .is_some_and(|v| v.kind != ReportKind::ScanResponse)
    {
        return Err(CaptureError::InvalidData);
    }
    m.link = if r.bool()? {
        let v = LinkInfo {
            connected_at_ms: r.u64()?,
            connect_elapsed_ms: r.u32()?,
            interval_us: r.u32()?,
            latency: r.u16()?,
            supervision_timeout_us: r.u32()?,
            att_mtu: if r.bool()? { Some(r.u16()?) } else { None },
        };
        if v.att_mtu.is_some_and(|n| !(23..=517).contains(&n)) {
            return Err(CaptureError::InvalidData);
        }
        Some(v)
    } else {
        None
    };
    m.started_at_ms = r.opt_u64()?;
    m.ended_at_ms = r.opt_u64()?;
    m.disconnect_reason = r.opt_u8()?;
    m.cleanup_complete = r.bool()?;
    m.stages = [r.bool()?, r.bool()?, r.bool()?, r.bool()?];
    m.omitted = r.u32()?;
    m.malformed = r.u32()?;
    m.complete = r.bool()?;
    m.failure = if r.bool()? {
        Some(InspectionFailure {
            stage: read_stage(r.u8()?)?,
            reason: read_reason(r)?.ok_or(CaptureError::InvalidData)?,
            range: read_pair(r)?,
            last_accepted: read_pair(r)?,
            elapsed_ms: r.opt_u32()?,
            rejected: if version >= 2 && r.bool()? {
                let handle = r.u16()?;
                let length = r.u16()?;
                Some(
                    RejectedDeclaration::from_prefix(handle, length, r.take(usize::from(length).min(19))?)
                        .ok_or(CaptureError::InvalidData)?,
                )
            } else {
                None
            },
        })
    } else {
        None
    };
    m.failure_context = ConnectionFailureContext {
        local_error: read_reason(r)?,
        disconnect_reason: r.opt_u8()?,
    };
    m.dropped_reports = r.u32()?;
    m.malformed_reports = r.u32()?;
    m.catalog_overflow_reports = r.u32()?;
    read_scan(r, &mut m.scan_diagnostics)?;
    // Always overwrite these when reusing a decoder across schema versions.
    m.fault_latch = None;
    m.radio_bus_hz = None;
    m.cached_name = None;
    m.boot = None;
    if version >= 3 {
        m.fault_latch = Some(FaultLatch::from_parts(r.u16()?, r.u8()?).ok_or(CaptureError::InvalidData)?);
        let hz = r.u32()?;
        // Admit evidence independently of this build's conservative ceiling.
        if hz > 50_000_000 || hz > m.build.cpu_hz / 2 {
            return Err(CaptureError::InvalidData);
        }
        m.radio_bus_hz = (hz != 0).then_some(hz);
    }
    if version >= 4 {
        if r.bool()? {
            let source = NameSource::from_code(r.u8()?).ok_or(CaptureError::InvalidData)?;
            let complete = r.bool()?;
            let at_ms = r.u64()?;
            let len = usize::from(r.u8()?);
            if len == 0 || len > 64 {
                return Err(CaptureError::InvalidData);
            }
            // Fill unpublished metadata in place. The decoder returns no view
            // until the entire record and its relationships have passed.
            let name = m.cached_name.insert(ObservedName {
                data: Payload::empty(),
                source: NameSource::Advertisement,
                complete: false,
                at_ms: 0,
            });
            name.source = source;
            name.complete = complete;
            name.at_ms = at_ms;
            name.data.bytes[..len].copy_from_slice(r.take(len)?);
            name.data.len = len as u8;
            name.data.truncated = r.bool()?;
        }
        if r.bool()? {
            let raw_reason = r.u32()?;
            let boot_type = r.opt_u8()?;
            let previous = if r.bool()? {
                Some(ResetRecord {
                    stage: ResetStage::from_code(r.u8()?).ok_or(CaptureError::InvalidData)?,
                    elapsed_ms: r.u32()?,
                })
            } else {
                None
            };
            m.boot = Some(BootEvidence {
                raw_reason,
                boot_type,
                previous,
            });
        }
    }
    Ok(())
}

fn read_record<'a>(r: &mut Reader<'a>) -> Result<Record<'a>, CaptureError> {
    let tag = r.u8()?;
    let n = usize::from(r.u16()?);
    let mut row = Reader { bytes: r.take(n)? };
    let record = match tag {
        1 => {
            Record::Service(Service {
                start: row.u16()?,
                end: row.u16()?,
                uuid: row.uuid()?,
            })
        }
        2 => {
            Record::Characteristic(Characteristic {
                service_start: row.u16()?,
                declaration: row.u16()?,
                handle: row.u16()?,
                properties: row.u8()?,
                uuid: row.uuid()?,
            })
        }
        3 => {
            Record::Descriptor(Descriptor {
                service_start: row.u16()?,
                characteristic_handle: row.u16()?,
                handle: row.u16()?,
                uuid: row.uuid()?,
            })
        }
        4 => {
            let service_start = row.u16()?;
            let handle = row.u16()?;
            let uuid = row.u16()?;
            let result = read_result(row.u8()?)?;
            let failure = read_reason(&mut row)?;
            let elapsed_ms = row.u32()?;
            let truncated = row.bool()?;
            let len = usize::from(row.u8()?);
            if len > 64 || (result == ReadResult::Ok) != failure.is_none() {
                return Err(CaptureError::InvalidData);
            }
            Record::Value(ValueRef {
                service_start,
                handle,
                uuid,
                result,
                failure,
                elapsed_ms,
                truncated,
                bytes: row.take(len)?,
            })
        }
        _ => return Err(CaptureError::InvalidData),
    };
    if !row.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    Ok(record)
}

/// Output is untrusted on error. No allocation or complete-model clone occurs.
fn encode_bytes(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
    output: &mut [u8],
) -> Result<usize, CaptureError> {
    let mut w = Writer { bytes: output, pos: 0 };
    w.bytes(&[0; HEADER])?;
    write_metadata(&mut w, probe, build, captured_at_ms)?;
    let metadata_len = w.pos - HEADER;
    for v in &probe.inspection.services {
        w.record(1, |w| {
            w.u16(v.start)?;
            w.u16(v.end)?;
            w.uuid(v.uuid)
        })?;
    }
    for v in &probe.inspection.characteristics {
        w.record(2, |w| {
            w.u16(v.service_start)?;
            w.u16(v.declaration)?;
            w.u16(v.handle)?;
            w.u8(v.properties)?;
            w.uuid(v.uuid)
        })?;
    }
    for v in &probe.inspection.descriptors {
        w.record(3, |w| {
            w.u16(v.service_start)?;
            w.u16(v.characteristic_handle)?;
            w.u16(v.handle)?;
            w.uuid(v.uuid)
        })?;
    }
    for v in &probe.inspection.values {
        w.record(4, |w| {
            if usize::from(v.value.len) > 64 {
                return Err(CaptureError::InvalidData);
            }
            w.u16(v.service_start)?;
            w.u16(v.handle)?;
            w.u16(v.uuid)?;
            w.u8(result_code(v.result))?;
            write_reason(w, v.failure)?;
            w.u32(v.elapsed_ms)?;
            w.bool(v.value.truncated)?;
            w.u8(v.value.len)?;
            w.bytes(v.value.as_slice())
        })?;
    }
    let total = w.pos;
    let count = probe.inspection.services.len()
        + probe.inspection.characteristics.len()
        + probe.inspection.descriptors.len()
        + probe.inspection.values.len();
    w.bytes[..8].copy_from_slice(MAGIC);
    w.bytes[8..10].copy_from_slice(&4u16.to_le_bytes());
    w.bytes[10..12].copy_from_slice(&(total as u16).to_le_bytes());
    w.bytes[12..14].copy_from_slice(&(count as u16).to_le_bytes());
    w.bytes[14..16].copy_from_slice(&(metadata_len as u16).to_le_bytes());
    let body_crc = crc32(&w.bytes[HEADER..total]);
    w.bytes[16..20].copy_from_slice(&body_crc.to_le_bytes());
    let header_crc = crc32(&w.bytes[..20]);
    w.bytes[20..24].copy_from_slice(&header_crc.to_le_bytes());
    Ok(total)
}

fn find_service(
    view: &CaptureView<'_>,
    start: u16,
) -> Option<Service> {
    view.records().find_map(|r| {
        match r {
            Record::Service(s) if s.start == start => Some(s),
            _ => None,
        }
    })
}
fn find_characteristic(
    view: &CaptureView<'_>,
    handle: u16,
) -> Option<Characteristic> {
    view.records().find_map(|r| {
        match r {
            Record::Characteristic(c) if c.handle == handle => Some(c),
            _ => None,
        }
    })
}
fn relationships(view: &CaptureView<'_>) -> Result<(), CaptureError> {
    metadata_valid(view.metadata)?;
    let mut services = 0u16;
    let mut chars = 0u16;
    let mut descriptors = 0u16;
    let mut values = 0u16;
    for record in view.records() {
        let valid = match record {
            Record::Service(s) => {
                let prior = services;
                services += 1;
                s.start != 0
                    && s.end >= s.start
                    && view
                        .records()
                        .filter_map(|r| if let Record::Service(v) = r { Some(v) } else { None })
                        .take(usize::from(prior))
                        .all(|v| s.start > v.end || s.end < v.start)
            }
            Record::Characteristic(c) => {
                let prior = chars;
                chars += 1;
                find_service(view, c.service_start)
                    .is_some_and(|s| c.declaration > s.start && c.handle > c.declaration && c.handle <= s.end)
                    && view
                        .records()
                        .filter_map(|r| {
                            if let Record::Characteristic(v) = r {
                                Some(v)
                            } else {
                                None
                            }
                        })
                        .take(usize::from(prior))
                        .all(|v| {
                            c.declaration != v.declaration
                                && c.handle != v.handle
                                && c.handle != v.declaration
                                && c.declaration != v.handle
                                && (c.service_start != v.service_start
                                    || c.declaration > v.handle
                                    || v.declaration > c.handle)
                        })
            }
            Record::Descriptor(d) => {
                let prior = descriptors;
                descriptors += 1;
                find_characteristic(view, d.characteristic_handle).is_some_and(|c| {
                    c.service_start == d.service_start
                        && d.handle > c.handle
                        && find_service(view, d.service_start).is_some_and(|s| d.handle <= s.end)
                        && !view.records().any(|r| {
                            matches!(r, Record::Characteristic(v)
                                if v.service_start == d.service_start
                                    && v.declaration > c.handle
                                    && v.declaration <= d.handle)
                        })
                }) && view
                    .records()
                    .filter_map(|r| if let Record::Descriptor(v) = r { Some(v) } else { None })
                    .take(usize::from(prior))
                    .all(|v| v.handle != d.handle)
            }
            Record::Value(v) => {
                let prior = values;
                values += 1;
                find_characteristic(view, v.handle).is_some_and(|c| {
                    c.service_start == v.service_start
                        && c.uuid.short() == Some(v.uuid)
                        && find_service(view, c.service_start).is_some_and(|s| may_read(s.uuid, c.uuid, c.properties))
                }) && view
                    .records()
                    .filter_map(|r| if let Record::Value(v) = r { Some(v) } else { None })
                    .take(usize::from(prior))
                    .all(|p| p.handle != v.handle)
            }
        };
        if !valid {
            return Err(CaptureError::Relationships);
        }
    }
    if !view.metadata.gatt_requested
        && (view.counts != [0; 4]
            || view.metadata.link.is_some()
            || view.metadata.started_at_ms.is_some()
            || view.metadata.ended_at_ms.is_some()
            || view.metadata.complete
            || view.metadata.stages.iter().any(|&v| v)
            || view.metadata.failure.is_some()
            || view.metadata.cleanup_complete
            || view.metadata.disconnect_reason.is_some()
            || view.metadata.omitted != 0
            || view.metadata.malformed != 0
            || view.metadata.failure_context.local_error.is_some()
            || view.metadata.failure_context.disconnect_reason.is_some())
    {
        return Err(CaptureError::InvalidState);
    }
    Ok(())
}

/// Check producer bookkeeping separately from raw peer values. Invalid standard
/// value lengths and controller observations remain inspectable evidence.
fn metadata_valid(m: &CaptureMetadata) -> Result<(), CaptureError> {
    let packet_time_valid = |packet: &PacketEvidence| {
        packet
            .observation
            .is_none_or(|o| (m.first_seen_ms..=m.last_seen_ms).contains(&o.at_ms))
    };
    if m.revision == 0
        || m.cached_name.is_some_and(|name| {
            name.at_ms < m.first_seen_ms
                || name.at_ms > m.captured_at_ms
                || match name.source {
                    NameSource::Gap => !name.complete || name.data.truncated,
                    NameSource::Advertisement | NameSource::ScanResponse => {
                        name.data.len > 29 || name.at_ms > m.last_seen_ms
                    }
                }
        })
        || m.boot
            .is_some_and(|boot| boot.previous.is_some() && !boot.watchdog_fault())
        || m.reports == 0
        || m.first_seen_ms > m.last_seen_ms
        || m.last_seen_ms > m.captured_at_ms
        || m.scan_diagnostics
            .last
            .is_some_and(|last| last.at_ms > m.captured_at_ms)
        || m.advertisement.observation.is_none()
        || !packet_time_valid(&m.advertisement)
        || !packet_time_valid(&m.response)
        || m.started_at_ms.is_some_and(|start| start > m.captured_at_ms)
        || m.ended_at_ms
            .is_some_and(|end| end > m.captured_at_ms || m.started_at_ms.is_none_or(|start| end < start))
        || m.link.is_some_and(|link| {
            link.connected_at_ms > m.captured_at_ms
                || m.started_at_ms.is_none_or(|start| link.connected_at_ms < start)
                || m.ended_at_ms.is_some_and(|end| link.connected_at_ms > end)
        })
        || (m.cleanup_complete || m.disconnect_reason.is_some()) && m.ended_at_ms.is_none()
        || m.stages[1..].iter().any(|&complete| complete) && !m.stages[0]
        || (m.stages[2] || m.stages[3]) && !m.stages[1]
        || m.failure.is_some_and(|f| {
            f.rejected.is_some()
                && (f.stage != FailureStage::Characteristics
                    || f.range.is_none()
                    || !matches!(
                        f.reason,
                        FailureReason::DeclarationLength | FailureReason::DeclarationOrder | FailureReason::ValueHandle
                    ))
        })
        || m.complete
            && (!m.stages.iter().all(|&complete| complete)
                || m.error != ProbeError::None
                || m.failure.is_some()
                || m.omitted != 0
                || m.malformed != 0)
    {
        return Err(CaptureError::InvalidState);
    }
    Ok(())
}

/// Identify a bounded prefix and check the current envelope header against the
/// journal payload length. This does not validate body bytes or relationships.
pub fn check_header(
    prefix: &[u8],
    payload_length: usize,
) -> Result<(), CaptureError> {
    if payload_length > PAYLOAD_MAX {
        return Err(CaptureError::TooLarge);
    }
    let mut r = Reader { bytes: prefix };
    if r.take(8)? != MAGIC {
        return Err(CaptureError::Magic);
    }
    if !matches!(r.u16()?, 1..=4) {
        return Err(CaptureError::Version);
    }
    let total = usize::from(r.u16()?);
    if total != payload_length || total < HEADER {
        return Err(CaptureError::Truncated);
    }
    r.take(8)?;
    let header_crc = r.u32()?;
    if crc32(&prefix[..20]) != header_crc {
        return Err(CaptureError::Integrity);
    }
    Ok(())
}

/// Validate the full body and its relationships; header admission alone does not.
fn decode_into<'a>(
    metadata: &'a mut CaptureMetadata,
    bytes: &'a [u8],
) -> Result<CaptureView<'a>, CaptureError> {
    check_header(bytes, bytes.len())?;
    let mut r = Reader { bytes: &bytes[12..] };
    let count = r.u16()?;
    let metadata_len = usize::from(r.u16()?);
    let body_crc = r.u32()?;
    let header_crc = r.u32()?;
    if crc32(&bytes[..20]) != header_crc || crc32(&bytes[HEADER..]) != body_crc {
        return Err(CaptureError::Integrity);
    }
    let mut meta = Reader {
        bytes: r.take(metadata_len)?,
    };
    read_metadata(&mut meta, metadata, u16::from_le_bytes([bytes[8], bytes[9]]))?;
    let mut view = CaptureView {
        metadata,
        records: r.bytes,
        counts: [0; 4],
    };
    if !meta.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    let mut previous = 0;
    for _ in 0..count {
        let tag = *r.bytes.first().ok_or(CaptureError::Truncated)?;
        if !(1..=4).contains(&tag) || tag < previous {
            return Err(CaptureError::InvalidData);
        }
        previous = tag;
        let index = usize::from(tag - 1);
        view.counts[index] += 1;
        if view.counts[index] > [24, 64, 128, 16][index] {
            return Err(CaptureError::InvalidData);
        }
        read_record(&mut r)?;
    }
    if !r.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    relationships(&view)?;
    Ok(view)
}

/// Convenience decoding for host callers that need an independently owned view.
/// Firmware should retain a Decoder and borrow its small view instead.
pub fn decode(bytes: &[u8]) -> Result<DecodedCapture<'_>, CaptureError> {
    let mut decoder = Decoder::new();
    let view = decoder.decode(bytes)?;
    let records_offset = bytes.len() - view.records.len();
    let counts = view.counts;
    Ok(DecodedCapture {
        metadata: decoder.metadata,
        records: &bytes[records_offset..],
        counts,
    })
}
/// Convenience encoder; firmware should reuse Decoder::encode.
pub fn encode(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
    output: &mut [u8],
) -> Result<usize, CaptureError> {
    Decoder::new().encode(probe, build, captured_at_ms, output)
}
