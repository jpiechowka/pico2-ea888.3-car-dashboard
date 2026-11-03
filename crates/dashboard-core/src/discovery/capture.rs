//! Canonical, bounded selected-peer evidence; all time values are boot-relative.
use super::capture_store::{PAYLOAD_MAX, crc32};
use super::names::{NameSource, ObservedName};
use super::*;
use crate::health::reset::{BootEvidence, ResetRecord, ResetStage};

/// Borrowed capture-v6 collection rows and compact summary, separate from GATT counts.
mod collection_codec;
/// Stable bounded identity metadata extension; legacy schemas omit it.
mod identity_codec;
pub use collection_codec::TerminalRef;

/// Capture producer's supported clock-profile label; rates are retained separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    /// Stock build profile.
    Stock,
    /// Explicit experimental 280 MHz profile; not electrical qualification.
    Expert280,
}
/// Producer-supplied build provenance; neither a runtime self-check nor measured clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildIdentity {
    /// Full source-inventory digest; must be nonzero when declared available.
    pub source_digest: [u8; 32],
    /// Configured CPU frequency in hertz, nonzero for admitted captures.
    pub cpu_hz: u32,
    /// Configured LCD SPI frequency in hertz, nonzero for admitted captures.
    pub spi_hz: u32,
    /// Producer profile classification, independent of the exact frequency fields.
    pub profile: BuildProfile,
    /// Source digest was supplied by the checked build route; false preserves unavailable provenance.
    pub source_identity_available: bool,
}
/// Capture framing, schema or semantic-admission failure; no partially decoded view escapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// Caller output buffer cannot hold the next bounded write.
    BufferTooSmall,
    /// Encoding/declared payload exceeds 8192 bytes or length arithmetic overflowed.
    TooLarge,
    /// Probe has no selected peer to capture.
    NoPeer,
    /// Producer bookkeeping/timestamps/completion or inspection identity is inconsistent.
    InvalidState,
    /// Required input bytes are absent or total length disagrees with the envelope.
    Truncated,
    /// Capture discriminator does not match the supported format family.
    Magic,
    /// Capture schema is outside supported versions 1..=6.
    Version,
    /// Header or body CRC disagrees with the input bytes.
    Integrity,
    /// Field code, boolean, width, count or representable metadata violates the schema.
    InvalidData,
    /// GATT ownership, range, uniqueness or standard-read relationship fails.
    Relationships,
    /// Metadata/record/body contains bytes beyond its declared structure.
    Trailing,
}

impl CaptureError {
    /// Returns this unit error's exact Debug name without allocating or formatting.
    pub const fn label(self) -> &'static str {
        match self {
            Self::BufferTooSmall => "BufferTooSmall",
            Self::TooLarge => "TooLarge",
            Self::NoPeer => "NoPeer",
            Self::InvalidState => "InvalidState",
            Self::Truncated => "Truncated",
            Self::Magic => "Magic",
            Self::Version => "Version",
            Self::Integrity => "Integrity",
            Self::InvalidData => "InvalidData",
            Self::Relationships => "Relationships",
            Self::Trailing => "Trailing",
        }
    }
}

/// Frozen legacy packet prefix with its own optional observation metadata.
#[derive(Clone, Copy, Debug)]
pub struct PacketEvidence {
    /// Zero-padded owned raw packet storage; only `len` bytes are meaningful.
    bytes: [u8; 31],
    /// Admitted byte count in 0..=31.
    len: u8,
    /// Producer omitted source packet bytes; independent of AD Complete Local Name type.
    pub truncated: bool,
    /// Packet-specific timestamp/RSSI/kind; absent only with empty, untruncated bytes.
    pub observation: Option<PacketObservation>,
}
impl PacketEvidence {
    /// Returns `self`'s admitted raw packet prefix, without sanitizing text or parsing AD fields.
    pub fn bytes(&self) -> &[u8] { &self.bytes[..usize::from(self.len)] }
}
/// Borrowed standard-read evidence with admitted declaration relationships and lossless bytes.
#[derive(Clone, Copy, Debug)]
pub struct ValueRef<'a> {
    /// Owning service declaration handle.
    pub service_start: u16,
    /// Allowlisted characteristic's value handle.
    pub handle: u16,
    /// Normalized 16-bit standard characteristic UUID.
    pub uuid: u16,
    /// Read outcome, coherent with optional failure evidence.
    pub result: ReadResult,
    /// Present exactly for failed/protected/unavailable reads.
    pub failure: Option<FailureReason>,
    /// Producer-measured read duration in milliseconds.
    pub elapsed_ms: u32,
    /// Original retained read prefix, at most 64 bytes; not necessarily UTF-8.
    pub bytes: &'a [u8],
    /// Read had more bytes than retained; does not make the retained prefix a complete value.
    pub truncated: bool,
}
impl ValueRef<'_> {
    /// Returns whether `self`'s retained byte count matches a fixed-width standard UUID.
    /// Variable-width UUIDs return true; this does not clear truncation or certify a value.
    pub fn length_valid(&self) -> bool { info_length_valid(self.uuid, self.bytes.len()) }
}
/// One admitted record, iterated in service/characteristic/descriptor/value group order.
#[derive(Clone, Copy, Debug)]
pub enum Record<'a> {
    /// Bounded raw engine-read evidence, never a qualified live measurement.
    Collection(
        /// Fixed-manifest identity, timing, result and borrowed raw prefix.
        collection::RowRef<'a>,
    ),
    /// Latest terminal transport prefix, not a full wire transcript.
    CollectionEnvelope(
        /// Borrowed raw terminal prefix with explicit observed length/provenance.
        TerminalRef<'a>,
    ),
    /// Nonoverlapping primary-service range.
    Service(
        /// Owned service declaration.
        Service,
    ),
    /// Characteristic linked to an admitted service.
    Characteristic(
        /// Owned characteristic declaration.
        Characteristic,
    ),
    /// Descriptor within its admitted characteristic range.
    Descriptor(
        /// Owned descriptor declaration.
        Descriptor,
    ),
    /// Standard-read outcome tied to its actual allowlisted declaration.
    Value(
        /// Borrowed raw bytes and owned read metadata.
        ValueRef<'a>,
    ),
}

/// Frozen producer and selected-peer context; optional legacy fields remain unavailable.
#[derive(Clone, Copy, Debug)]
pub struct CaptureMetadata {
    /// Small v6 collection context; raw rows are borrowed separately, absent in v1-v5.
    pub collection: Option<collection::Summary>,
    /// Collection row count, exactly28 when present, otherwise zero; excludes GATT counts.
    pub collection_row_count: u8,
    /// Actual admitted payload schema version in 1..=6, independent of journal/USB versions.
    pub schema_version: u16,
    /// Producer image selected identity mode; absent in legacy v1-v4, never inferred.
    pub identity_mode: Option<bool>,
    /// Explicit source-configured identity trial; absent in v1-v4 and ordinary probe captures.
    pub identity: Option<identity::Evidence>,
    /// Absent in legacy v1/v2 captures, never inferred from current firmware.
    pub fault_latch: Option<FaultLatch>,
    /// Configured radio bus hertz, absent before v3 or when encoded as unavailable zero.
    pub radio_bus_hz: Option<u32>,
    /// Absent in v1/v2/v3; never filled from the current Pico when loading.
    pub cached_name: Option<ObservedName>,
    /// Frozen boot/reset evidence added in v4; absent is never filled from the current device.
    pub boot: Option<BootEvidence>,
    /// Producer-supplied source identity, profile and configured clocks.
    pub build: BuildIdentity,
    /// Boot-relative time in milliseconds when this evidence was frozen.
    pub captured_at_ms: u64,
    /// Nonzero discovery request generation associated with this snapshot.
    pub revision: u32,
    /// An explicit inspection was requested for this exact captured peer.
    pub gatt_requested: bool,
    /// Radio-owner phase at capture, retained without pretending partial work completed.
    pub phase: RadioPhase,
    /// Coarse probe error at capture time.
    pub error: ProbeError,
    /// Observed address/type identity; not an authenticated permanent physical identity.
    pub peer: Peer,
    /// First admitted peer report time in boot-relative milliseconds.
    pub first_seen_ms: u64,
    /// Latest admitted peer report time in boot-relative milliseconds.
    pub last_seen_ms: u64,
    /// Nonzero saturating count of admitted reports for this peer.
    pub reports: u32,
    /// Latest peer-report RSSI in -127..=20 dBm, or unavailable.
    pub rssi: Option<i8>,
    /// Latest advertising packet and its independent original metadata.
    pub advertisement: PacketEvidence,
    /// Latest scan response and its independent metadata; may be absent/empty.
    pub response: PacketEvidence,
    /// Observed link parameters, absent when no connection was observed.
    pub link: Option<LinkInfo>,
    /// Boot-relative inspection start milliseconds; absent for advertisement-only capture.
    pub started_at_ms: Option<u64>,
    /// Boot-relative inspection end milliseconds; absent while unfinished/unrequested.
    pub ended_at_ms: Option<u64>,
    /// Raw HCI disconnect reason from inspection termination, if observed.
    pub disconnect_reason: Option<u8>,
    /// Radio owner acknowledged cleanup; does not itself certify clean termination.
    pub cleanup_complete: bool,
    /// Services/characteristics/descriptors/reads stage acknowledgments in enum order.
    pub stages: [bool; 4],
    /// Saturating count of bounded inventory omissions.
    pub omitted: u32,
    /// Saturating count of rejected inventory structures.
    pub malformed: u32,
    /// Producer's clean-inventory claim, checked against stages/errors/loss counters.
    pub complete: bool,
    /// First detailed inspection failure, including optional rejected tuple evidence.
    pub failure: Option<InspectionFailure>,
    /// Independent local receive failure and terminal-link reason evidence.
    pub failure_context: ConnectionFailureContext,
    /// Global producer report losses; not attributed to the selected peer.
    pub dropped_reports: u32,
    /// Global malformed-report count; not attributed to the selected peer.
    pub malformed_reports: u32,
    /// Global catalogue capacity rejections; not selected-peer failures.
    pub catalog_overflow_reports: u32,
    /// Global scan batch failures, possibly predating this peer's first report.
    pub scan_diagnostics: ScanDiagnostics,
}

impl CaptureMetadata {
    /// Private invalid parsing seed, never published before full field/relationship admission.
    const EMPTY: Self = Self {
        collection: None,
        collection_row_count: 0,
        schema_version: 0,
        identity_mode: None,
        identity: None,
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

/// Fully admitted capture borrowing metadata and immutable encoded record bytes.
#[derive(Clone, Copy, Debug)]
pub struct CaptureView<'a> {
    /// Validated metadata owned by a decoder or host convenience owner.
    metadata: &'a CaptureMetadata,
    /// Fully parsed record stream borrowing original capture bytes.
    records: &'a [u8],
    /// Record counts in service/characteristic/descriptor/value order.
    counts: [u16; 4],
}
impl<'a> CaptureView<'a> {
    /// Returns `self`'s admitted frozen metadata by reference.
    pub fn metadata(&self) -> &CaptureMetadata { self.metadata }

    /// Returns `self`'s service/characteristic/descriptor/value counts, bounded by 24/64/128/16.
    pub fn counts(&self) -> [u16; 4] { self.counts }

    /// Returns a fresh zero-allocation iterator over `self`'s already admitted record stream.
    pub fn records(&self) -> Records<'a> { Records { bytes: self.records } }
}
/// Reusable parsing storage. Contains no references into a capture buffer.
/// A returned view borrows this decoder, preventing another decode while in use.
pub struct Decoder {
    /// Reused parsing destination; partial failed parses cannot escape through a view.
    metadata: CaptureMetadata,
}
impl Default for Decoder {
    /// Returns reusable decoder storage without any admitted capture.
    fn default() -> Self { Self::new() }
}
impl Decoder {
    /// Returns reusable decoder storage initialized with private invalid metadata.
    pub const fn new() -> Self {
        Self {
            metadata: CaptureMetadata::EMPTY,
        }
    }

    /// Returns an admitted view borrowing `self` and immutable `bytes` after envelope,
    /// CRC, field, count, metadata and record-relationship checks. Supports schema 1..=6;
    /// legacy absent fields are cleared on every reuse. Errors leave unpublished partial
    /// metadata in `self`, so no previous view survives this mutable call.
    pub fn decode<'a>(
        &'a mut self,
        bytes: &'a [u8],
    ) -> Result<CaptureView<'a>, CaptureError> {
        decode_into(&mut self.metadata, bytes)
    }

    /// Encodes selected-peer `probe` with producer `build` at boot-relative `captured_at_ms`
    /// milliseconds into `output`, then validates by decoding with `self`'s reusable storage.
    /// Returns the admitted v5/v6 byte count, at most8192; output tail is unchanged.
    /// Errors leave a partial or fully encoded but untrusted prefix. Performs no allocation,
    /// radio or storage I/O and never turns incomplete evidence into complete evidence.
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
    /// Independently owned admitted metadata, avoiding a decoder lifetime on host callers.
    metadata: CaptureMetadata,
    /// Admitted encoded records borrowing the input payload.
    records: &'a [u8],
    /// Service/characteristic/descriptor/value counts.
    counts: [u16; 4],
}
impl<'a> DecodedCapture<'a> {
    /// Returns a lightweight admitted view borrowing `self`'s metadata and input records.
    pub fn view(&self) -> CaptureView<'_> {
        CaptureView {
            metadata: &self.metadata,
            records: self.records,
            counts: self.counts,
        }
    }

    /// Returns `self`'s owned frozen metadata by reference.
    pub fn metadata(&self) -> &CaptureMetadata { &self.metadata }

    /// Returns a fresh iterator over `self`'s admitted records, borrowing original input bytes.
    pub fn records(&self) -> Records<'a> { Records { bytes: self.records } }

    /// Returns `self`'s bounded service/characteristic/descriptor/value counts.
    pub fn counts(&self) -> [u16; 4] { self.counts }
}

/// Zero-allocation iterator constructed only over already-admitted immutable record bytes.
pub struct Records<'a> {
    /// Remaining encoded suffix; successful iteration consumes one complete record.
    bytes: &'a [u8],
}
impl<'a> Iterator for Records<'a> {
    /// Admitted declaration or borrowed standard-read evidence.
    type Item = Record<'a>;

    /// Consumes and returns `self`'s next admitted record, or `None` at stream end.
    /// Defensive parse failure also returns `None`; public constructors validate the
    /// entire immutable stream first, so no partial parse is exposed as evidence.
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

/// Capture-family discriminator shared by supported schema versions.
const MAGIC: &[u8; 8] = b"BLECAP01";
/// Fixed envelope bytes preceding variable metadata and records.
const HEADER: usize = 24;
/// Prefix size needed to identify the format and validate its current envelope.
pub const HEADER_BYTES: usize = HEADER;

/// Checked writer into caller-owned output, bounded by both slice size and payload maximum.
struct Writer<'a> {
    /// Mutable caller storage; errors may leave an already written prefix.
    bytes: &'a mut [u8],
    /// Next write offset, always no greater than admitted output capacity.
    pos: usize,
    /// Dry-run mode executes identical serializers and bounds without touching output storage.
    measuring: bool,
}
impl Writer<'_> {
    /// Appends `data` to `self`, returning unit; checked overflow/payload limit returns
    /// TooLarge and insufficient caller space returns BufferTooSmall, without advancing
    /// or partially applying this individual write.
    fn bytes(
        &mut self,
        data: &[u8],
    ) -> Result<(), CaptureError> {
        let end = self.pos.checked_add(data.len()).ok_or(CaptureError::TooLarge)?;
        if end > PAYLOAD_MAX {
            return Err(CaptureError::TooLarge);
        }
        if !self.measuring {
            self.bytes
                .get_mut(self.pos..end)
                .ok_or(CaptureError::BufferTooSmall)?
                .copy_from_slice(data);
        }
        self.pos = end;
        Ok(())
    }

    /// Appends raw octet `v` to `self`; returns unit or the bounded-write error without this byte.
    fn u8(
        &mut self,
        v: u8,
    ) -> Result<(), CaptureError> {
        self.bytes(&[v])
    }

    /// Appends `v` as canonical 0/1 to `self`; returns unit or the bounded-write error.
    fn bool(
        &mut self,
        v: bool,
    ) -> Result<(), CaptureError> {
        self.u8(u8::from(v))
    }

    /// Appends `v` as two little-endian bytes to `self`; returns unit or a bounded-write error.
    fn u16(
        &mut self,
        v: u16,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    /// Appends `v` as four little-endian bytes to `self`; returns unit or a bounded-write error.
    fn u32(
        &mut self,
        v: u32,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    /// Appends `v` as eight little-endian bytes to `self`; returns unit or a bounded-write error.
    fn u64(
        &mut self,
        v: u64,
    ) -> Result<(), CaptureError> {
        self.bytes(&v.to_le_bytes())
    }

    /// Appends presence plus optional octet `v` to `self`; returns unit or a write error.
    /// Failure after presence may leave an incomplete optional field.
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

    /// Appends presence plus optional little-endian `v` to `self`; returns unit or a
    /// write error that may leave the presence byte without its value.
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

    /// Appends presence plus optional little-endian `v` to `self`; returns unit or a
    /// write error that may leave the presence byte without its value.
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

    /// Appends width and wire-order UUID `v` to `self`, returning unit. Invalid widths
    /// fail before writing; bounded-write errors can leave only the width prefix.
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

    /// Appends `tag`, a reserved u16 length and the body produced by `write` into `self`.
    /// Returns unit after backfilling body length; callback/bounds errors leave an
    /// untrusted partial record. Global 8192-byte bounds make the u16 length cast exact.
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
        if !self.measuring {
            self.bytes[length..length + 2].copy_from_slice(&((self.pos - start) as u16).to_le_bytes());
        }
        Ok(())
    }
}
/// Checked forward reader over immutable capture bytes; failed fields may consume prefixes.
struct Reader<'a> {
    /// Unconsumed input suffix.
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    /// Returns and consumes exactly `n` bytes from `self`, or Truncated without advancing
    /// when insufficient bytes remain. Zero consumes nothing and returns an empty slice.
    fn take(
        &mut self,
        n: usize,
    ) -> Result<&'a [u8], CaptureError> {
        let head = self.bytes.get(..n).ok_or(CaptureError::Truncated)?;
        self.bytes = &self.bytes[n..];
        Ok(head)
    }

    /// Returns and consumes one octet from `self`, or Truncated without advancing.
    fn u8(&mut self) -> Result<u8, CaptureError> { Ok(self.take(1)?[0]) }

    /// Returns and consumes canonical boolean from `self`; bytes other than 0/1 are
    /// consumed then rejected as InvalidData. Missing byte returns Truncated.
    fn bool(&mut self) -> Result<bool, CaptureError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CaptureError::InvalidData),
        }
    }

    /// Returns and consumes a little-endian u16 from `self`, or Truncated without advancing.
    fn u16(&mut self) -> Result<u16, CaptureError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Returns and consumes a little-endian u32 from `self`, or Truncated without advancing.
    fn u32(&mut self) -> Result<u32, CaptureError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Returns and consumes a little-endian u64 from `self`, or Truncated without advancing.
    fn u64(&mut self) -> Result<u64, CaptureError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// Returns optional octet from `self` after canonical presence decoding; absence is
    /// `None`. Errors can consume presence while leaving a missing/invalid value untrusted.
    fn opt_u8(&mut self) -> Result<Option<u8>, CaptureError> {
        if self.bool()? { Ok(Some(self.u8()?)) } else { Ok(None) }
    }

    /// Returns optional little-endian u32 from `self`; absence is `None`. Invalid
    /// presence/truncation errors can leave the reader after the presence byte.
    fn opt_u32(&mut self) -> Result<Option<u32>, CaptureError> {
        if self.bool()? { Ok(Some(self.u32()?)) } else { Ok(None) }
    }

    /// Returns optional little-endian u64 from `self`; absence is `None`. Invalid
    /// presence/truncation errors can leave the reader after the presence byte.
    fn opt_u64(&mut self) -> Result<Option<u64>, CaptureError> {
        if self.bool()? { Ok(Some(self.u64()?)) } else { Ok(None) }
    }

    /// Returns width-prefixed wire-order UUID from `self`; rejects widths other than
    /// 2/4/16 or truncated bytes. Width/body prefixes may already be consumed on error.
    fn uuid(&mut self) -> Result<Uuid, CaptureError> {
        Uuid::from_wire(self.uuid_bytes()?).ok_or(CaptureError::InvalidData)
    }

    /// Borrows width-prefixed UUID bytes from `self`; preserves width/truncation error order.
    /// Only 2/4/16-byte bodies are returned, with no owned UUID array or normalization.
    fn uuid_bytes(&mut self) -> Result<&'a [u8], CaptureError> {
        let len = usize::from(self.u8()?);
        let bytes = self.take(len)?;
        if matches!(len, 2 | 4 | 16) {
            Ok(bytes)
        } else {
            Err(CaptureError::InvalidData)
        }
    }
}

/// Defines exhaustive enum-to-octet encoding and rejecting octet-to-enum decoding
/// from an explicit stable schema mapping; generated functions carry their own contracts.
macro_rules! codes {
    ($encode:ident,$decode:ident,$typ:ty,{$($variant:path=>$code:literal),+$(,)?})=>{
        #[doc = concat!("Returns the stable capture octet for `value` of type `", stringify!($typ), "`; mapping is exhaustive and performs no mutation.")]
        fn $encode(value:$typ)->u8{match value{$($variant=>$code),+}}
        #[doc = concat!("Returns `", stringify!($typ), "` decoded from stable capture octet `value`; unknown codes return CaptureError::InvalidData without a fallback variant.")]
        fn $decode(value:u8)->Result<$typ,CaptureError>{match value{$($code=>Ok($variant)),+,_=>Err(CaptureError::InvalidData)}}
    }
}
codes!(phase_code,read_phase,RadioPhase,{RadioPhase::Starting=>0,RadioPhase::Scanning=>1,RadioPhase::Paused=>2,RadioPhase::Stopping=>3,RadioPhase::Connecting=>4,RadioPhase::Discovering=>5,RadioPhase::Disconnecting=>6,RadioPhase::Fault=>7});
codes!(error_code,read_error,ProbeError,{ProbeError::None=>0,ProbeError::Scan=>1,ProbeError::Connect=>2,ProbeError::Timeout=>3,ProbeError::Discovery=>4,ProbeError::Capacity=>5,ProbeError::Disconnected=>6,ProbeError::Cleanup=>7,ProbeError::Transport=>8,ProbeError::InvalidPeer=>9});
codes!(stage_code,read_stage,FailureStage,{FailureStage::Connect=>0,FailureStage::Mtu=>1,FailureStage::Services=>2,FailureStage::Characteristics=>3,FailureStage::Descriptors=>4,FailureStage::DeviceInfo=>5,FailureStage::ConnectionEvents=>6,FailureStage::ClientTask=>7,FailureStage::Inspection=>8});
codes!(kind_code,read_kind,ReportKind,{ReportKind::Connectable=>0,ReportKind::Directed=>1,ReportKind::ScanResponse=>2,ReportKind::Other=>3});
codes!(result_code,read_result,ReadResult,{ReadResult::Ok=>0,ReadResult::Protected=>1,ReadResult::Unavailable=>2,ReadResult::Error=>3});

/// Appends optional failure `v` as stable tag plus raw-code octet to `w`, returning unit.
/// No-reason/local reasons use code zero. Bounded-write errors may leave the tag prefix.
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
/// Returns optional failure decoded from two consumed bytes in `r`; tag zero is None.
/// Unknown tags or nonzero unused code bytes return InvalidData; short input returns
/// Truncated after any consumed prefix. Raw-code categories preserve all octet values.
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
/// Appends optional little-endian handle pair `v` to `w`, returning unit or a write error.
/// Presence/first handle may remain on failure; handle ordering is not validated here.
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
/// Returns optional little-endian handle pair from `r`, without validating handle order.
/// Invalid presence/truncated members return errors after any consumed prefix.
fn read_pair(r: &mut Reader<'_>) -> Result<Option<(u16, u16)>, CaptureError> {
    if r.bool()? {
        Ok(Some((r.u16()?, r.u16()?)))
    } else {
        Ok(None)
    }
}
/// Writes packet prefix `p` and optional packet-specific `meta` into `w`, returning unit.
/// Rejects `p.len` above 31 before writing. Other coherence checks occur on decode;
/// bounded-write errors can leave a partial packet field.
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
/// Decodes one packet from `r` into unpublished `packet`, returning unit after byte-count,
/// canonical boolean, RSSI and report-kind checks. Metadata absence requires empty,
/// untruncated bytes. Errors may partially modify both reader position and destination.
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

/// Returns optional signed dBm RSSI from `r`, accepting -127..=20 and preserving absence.
/// Out-of-range values return InvalidData after consumption; prefix errors propagate.
fn read_rssi(r: &mut Reader<'_>) -> Result<Option<i8>, CaptureError> {
    let value = r.opt_u8()?.map(|v| v as i8);
    if value.is_some_and(|v| !(-127..=20).contains(&v)) {
        return Err(CaptureError::InvalidData);
    }
    Ok(value)
}
/// Writes global counters and optional latest batch failure from `scan` into `w`.
/// Returns unit or a bounded-write error with a possibly written prefix; timestamps
/// are validated against capture time by full decode, not by this serializer.
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
/// Decodes global scan counters/latest context from `r` into unpublished `scan`, returning
/// unit or framing/code errors. Mutations may be partial; global failure time is checked
/// against capture time later, not against selected-peer first-seen time.
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

/// Appends schema-v5 metadata from selected-peer probe `p`, producer `b` and boot-relative
/// capture time `at` milliseconds into `w`, returning unit. Rejects no peer, a requested
/// inspection for another peer, impossible payload widths and explicit zero radio clock.
/// Full semantic checks follow encode by decode; any error leaves output untrusted.
fn write_metadata(
    w: &mut Writer<'_>,
    p: &Probe,
    b: BuildIdentity,
    at: u64,
) -> Result<(), CaptureError> {
    let d = p.selected_device().ok_or(CaptureError::NoPeer)?;
    let i = &p.inspection;
    let requested = p.request.action.peer() == Some(d.peer);
    if p.request.action.peer().is_some_and(|peer| peer != d.peer)
        || (i.collection.is_some() && (!matches!(p.request.action, Action::Collect(_)) || !p.identity_mode))
    {
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
    w.bool(p.identity_mode)?;
    identity_codec::write(w, p.inspection.identity.as_ref())?;
    Ok(())
}
/// Decodes schema `version` metadata from `r` into unpublished reusable `m`, returning unit.
/// Caller first bounds version to1..=6. Validates primitive codes, RSSI, UUID-independent
/// lengths, ATT MTU 23..=517, positive build clocks and radio clock at most 50 MHz/CPU half.
/// Explicitly clears fields absent in legacy schemas. Errors leave partial `m` and reader
/// progress; complete temporal/relationship validation occurs before any view is returned.
#[inline(always)]
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
    m.identity = None;
    m.identity_mode = None;
    m.collection = None;
    m.collection_row_count = 0;
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
    if version >= 5 {
        m.identity_mode = Some(r.bool()?);
        identity_codec::read(r, &mut m.identity)?;
    }
    if version >= 6 {
        m.collection_row_count = collection_codec::read_summary(r, &mut m.collection)?;
    }
    Ok(())
}

/// Consumes the common tag/u16-length envelope from `r`, returning its tag and borrowed body.
/// Truncation can consume the header; body semantics and trailing bytes remain caller checks.
fn record_body<'a>(r: &mut Reader<'a>) -> Result<(u8, Reader<'a>), CaptureError> {
    let tag = r.u8()?;
    let n = usize::from(r.u16()?);
    Ok((tag, Reader { bytes: r.take(n)? }))
}

/// Returns the next tag/u16-length record consumed from `r`, validating exact body
/// consumption, UUID widths and read outcome/failure consistency. Value bytes borrow
/// the source and are at most 64. Errors may consume the envelope/body; cross-record
/// ownership and ordering require the subsequent full-stream checks.
#[inline(always)]
fn read_record<'a>(r: &mut Reader<'a>) -> Result<Record<'a>, CaptureError> {
    let (tag, mut row) = record_body(r)?;
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
        5 => Record::Collection(collection_codec::read_row(&mut row)?),
        6 => Record::CollectionEnvelope(collection_codec::read_terminal(&mut row)?),
        _ => return Err(CaptureError::InvalidData),
    };
    if !row.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    Ok(record)
}

/// Output is untrusted on error. No allocation or complete-model clone occurs.
/// Serializes `probe`, producer `build` and boot-relative `captured_at_ms` into `output`,
/// returning v5 (ordinary/identity) or v6 (collection) byte count with CRCs and grouped records. Returns
/// bounds/field errors with possible partial writes. The caller must run full decode
/// before accepting this prefix; raw encoding alone does not establish metadata consistency.
fn encode_bytes(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
    output: &mut [u8],
) -> Result<usize, CaptureError> {
    let mut w = Writer {
        bytes: output,
        pos: 0,
        measuring: false,
    };
    let metadata_len = write_body(&mut w, probe, build, captured_at_ms, false)?;
    let total = w.pos;
    let count = probe.inspection.services.len()
        + probe.inspection.characteristics.len()
        + probe.inspection.descriptors.len()
        + probe.inspection.values.len()
        + probe
            .inspection
            .collection
            .as_ref()
            .map_or(0, |e| 28 + usize::from(e.summary.terminal_kind.is_some()));
    w.bytes[..8].copy_from_slice(MAGIC);
    let version = if probe.inspection.collection.is_some() { 6u16 } else { 5 };
    w.bytes[8..10].copy_from_slice(&version.to_le_bytes());
    w.bytes[10..12].copy_from_slice(&(total as u16).to_le_bytes());
    w.bytes[12..14].copy_from_slice(&(count as u16).to_le_bytes());
    w.bytes[14..16].copy_from_slice(&(metadata_len as u16).to_le_bytes());
    let body_crc = crc32(&w.bytes[HEADER..total]);
    w.bytes[16..20].copy_from_slice(&body_crc.to_le_bytes());
    let header_crc = crc32(&w.bytes[..20]);
    w.bytes[20..24].copy_from_slice(&header_crc.to_le_bytes());
    Ok(total)
}

/// Serializes the exact body for `probe`/`build`/`captured_at_ms`; `reserve` replaces
/// collection fields by their maximal encoded budget. Returns metadata length in bytes.
/// Writer dry-run mode shares every ordinary field/record serializer and capacity error.
/// `w` receives the body or counts its bytes; failures may leave an untrusted written prefix.
// Share the serializer across writing, sizing and reservation even under the
// utility binary's aggressive inlining policy; preserve one bounded call frame.
#[inline(never)]
fn write_body(
    w: &mut Writer<'_>,
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
    reserve: bool,
) -> Result<usize, CaptureError> {
    w.bytes(&[0; HEADER])?;
    write_metadata(w, probe, build, captured_at_ms)?;
    if !reserve && let Some(e) = probe.inspection.collection.as_ref() {
        collection_codec::write_summary(w, &e.summary)?;
    }
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
    if reserve {
        collection_codec::reserve(w)?;
    } else if let Some(e) = probe.inspection.collection.as_ref() {
        collection_codec::write_records(w, e)?;
    }
    Ok(metadata_len)
}

/// Returns exact serialized size of current `probe` evidence at `captured_at_ms` with `build`.
/// Executes the production body serializers without an 8192-byte scratch allocation.
/// Field/capacity errors are preserved; this does not replace full semantic decode admission.
pub fn encoded_size(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
) -> Result<usize, CaptureError> {
    let mut w = Writer {
        bytes: &mut [],
        pos: 0,
        measuring: true,
    };
    write_body(&mut w, probe, build, captured_at_ms, false)?;
    Ok(w.pos)
}

/// Returns whether frozen GATT/INFO plus maximal collection and final metadata fit 8192 bytes.
/// Requires explicit Collect, full GATT stages, complete submitted INFO and a selected peer.
/// Caller must freeze GATT/vendor/raw INFO after admission and reserve before the voltage command.
/// The 512-byte finalization allowance exceeds upper bounds for remaining optional growth:
/// packet prefixes/observations84, peer RSSI1, link26, lifecycle18, failure48,
/// scan22, name77, boot14, identity channel16/fault16/end8 =430 bytes.
/// These conservatively include complete optional bodies/presence bytes; bounded UUIDs,
/// GATT values, vendor values and INFO bytes are frozen. Fixed counters/flags do not grow.
/// False permits no collection traffic; this function performs no I/O or model mutation.
/// `probe` supplies the live selected operation/evidence; `build` supplies its frozen build identity.
/// `captured_at_ms` is the boot-relative snapshot time used to admit existing INFO timestamps.
pub fn collection_fits(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
) -> bool {
    if !matches!(probe.request.action, Action::Collect(_))
        || !probe.identity_mode
        || !probe.collection_enabled
        || probe.phase != RadioPhase::Discovering
        || probe.inspection.started_at_ms.is_none()
        || probe.inspection.ended_at_ms.is_some()
        || probe.inspection.cleanup_complete
        || probe.inspection.omitted != 0
        || probe.inspection.malformed != 0
        || probe.error != ProbeError::None
        || probe.failure.is_some()
        || !probe.inspection.stages.iter().all(|&v| v)
        || probe.inspection.identity.as_ref().is_none_or(|e| {
            !e.valid(captured_at_ms)
                || e.channel.is_none()
                || e.fault.is_some()
                || e.ended_at_ms.is_some()
                || e.request_submitted_at_ms.is_none()
                || e.raw_incomplete()
                || crate::ble::canyonero::validate_frame(e.raw()).is_err()
        })
    {
        return false;
    }
    let mut w = Writer {
        bytes: &mut [],
        pos: 0,
        measuring: true,
    };
    write_body(&mut w, probe, build, captured_at_ms, true).is_ok()
        && w.pos.checked_add(512).is_some_and(|n| n <= PAYLOAD_MAX)
}

/// Checked envelopes of one `tag` in an immutable, fully primitive-validated record stream.
/// Nested relationship scans borrow only matching bodies instead of constructing every Record.
struct Bodies<'a> {
    /// Unconsumed envelopes; shared framing checks remain active even for skipped tags.
    reader: Reader<'a>,
    /// Desired GATT tag, supplied only by private relationship checks.
    tag: u8,
}
impl<'a> Iterator for Bodies<'a> {
    /// Matching body or a framing error; errors stop further iteration.
    type Item = Result<Reader<'a>, CaptureError>;

    /// Returns the next matching checked envelope, None at end, or one terminal framing error.
    /// Body semantics are established by decode_into's preceding complete read_record pass.
    fn next(&mut self) -> Option<Self::Item> {
        while !self.reader.bytes.is_empty() {
            match record_body(&mut self.reader) {
                Ok((tag, body)) if tag == self.tag => return Some(Ok(body)),
                Ok(_) => {}
                Err(error) => {
                    self.reader.bytes = &[];
                    return Some(Err(error));
                }
            }
        }
        None
    }
}

/// Returns checked borrowed envelopes of `tag` from primitive-admitted `view`; no allocation.
fn bodies<'a>(
    view: &CaptureView<'a>,
    tag: u8,
) -> Bodies<'a> {
    Bodies {
        reader: Reader { bytes: view.records },
        tag,
    }
}

/// Service fields needed by relationships; UUID bytes borrow the immutable capture.
struct ServiceRef<'a> {
    /// Service declaration handle.
    start: u16,
    /// Inclusive final service handle.
    end: u16,
    /// Checked 2/4/16-byte wire UUID, interpreted only for read allowlisting.
    uuid: &'a [u8],
}

/// Characteristic fields needed by relationships without an owned UUID array.
struct CharacteristicRef<'a> {
    /// Parent service declaration handle.
    service_start: u16,
    /// Characteristic declaration handle.
    declaration: u16,
    /// Characteristic value handle.
    handle: u16,
    /// Original property bits, including Read at bit1.
    properties: u8,
    /// Checked 2/4/16-byte wire UUID.
    uuid: &'a [u8],
}

/// Reads service `body` through shared primitive readers; requires exact consumption.
/// Returns borrowed fields or the same truncation, width or trailing-byte error.
fn service_ref(mut body: Reader<'_>) -> Result<ServiceRef<'_>, CaptureError> {
    let row = ServiceRef {
        start: body.u16()?,
        end: body.u16()?,
        uuid: body.uuid_bytes()?,
    };
    if !body.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    Ok(row)
}

/// Reads characteristic `body` through shared primitive readers; requires exact consumption.
/// Returns borrowed fields or the same truncation, width or trailing-byte error.
fn characteristic_ref(mut body: Reader<'_>) -> Result<CharacteristicRef<'_>, CaptureError> {
    let row = CharacteristicRef {
        service_start: body.u16()?,
        declaration: body.u16()?,
        handle: body.u16()?,
        properties: body.u8()?,
        uuid: body.uuid_bytes()?,
    };
    if !body.bytes.is_empty() {
        return Err(CaptureError::Trailing);
    }
    Ok(row)
}

/// Finds service declaration `start` in primitive-validated `view`, borrowing its UUID.
/// Returns None when absent; checked envelope/body errors propagate without mutation.
fn find_service<'a>(
    view: &CaptureView<'a>,
    start: u16,
) -> Result<Option<ServiceRef<'a>>, CaptureError> {
    for body in bodies(view, 1) {
        let row = service_ref(body?)?;
        if row.start == start {
            return Ok(Some(row));
        }
    }
    Ok(None)
}

/// Finds characteristic value `handle` in primitive-validated `view`, borrowing its UUID.
/// Returns None when absent; checked envelope/body errors propagate without mutation.
fn find_characteristic<'a>(
    view: &CaptureView<'a>,
    handle: u16,
) -> Result<Option<CharacteristicRef<'a>>, CaptureError> {
    for body in bodies(view, 2) {
        let row = characteristic_ref(body?)?;
        if row.handle == handle {
            return Ok(Some(row));
        }
    }
    Ok(None)
}
/// Returns unit if bounded, fully parsed `view` has coherent metadata and GATT relationships.
/// Checks nonoverlap/uniqueness, parent handles, descriptor boundaries and service-qualified
/// standard-read allowlisting; advertisement-only captures must carry no inspection state.
/// Fails with InvalidState/Relationships without mutation. Repeated scans are bounded by
/// previously admitted 24/64/128/16 record limits, avoiding another inventory allocation.
fn relationships(view: &CaptureView<'_>) -> Result<(), CaptureError> {
    metadata_valid(view.metadata)?;
    if let Some(identity) = &view.metadata.identity
        && identity.vendor_values.iter().any(Option::is_some)
    {
        if !view.metadata.stages.iter().all(|&done| done) {
            return Err(CaptureError::Relationships);
        }
        // Ordered records put every service before characteristics. Retain only
        // scope/count/match scalars, not copied service/characteristic payloads.
        let mut service_start = None;
        let mut declarations = [0u8; 4];
        let mut matched = 0u8;
        for record in view.records() {
            match record {
                Record::Service(s) if s.uuid.short() == Some(0xFFF1) => {
                    if service_start.replace(s.start).is_some() {
                        return Err(CaptureError::Relationships);
                    }
                }
                Record::Characteristic(c) if Some(c.service_start) == service_start => {
                    let short = c.uuid.short();
                    if let Some(index) = identity::VENDOR_UUIDS.iter().position(|&uuid| Some(uuid) == short) {
                        // Full-stream admission bounds all characteristics to64.
                        declarations[index] += 1;
                        if let Some(value) = identity.vendor_values[index].as_ref()
                            && value.service_start == c.service_start
                            && value.handle == c.handle
                            && c.properties & 2 != 0
                        {
                            matched |= 1 << index;
                        }
                    }
                }
                _ => {}
            }
        }
        for (index, value) in identity.vendor_values.iter().enumerate() {
            if let Some(value) = value
                && (Some(value.service_start) != service_start
                    || declarations[index] != 1
                    || matched & (1 << index) == 0)
            {
                return Err(CaptureError::Relationships);
            }
        }
    }
    let mut services = 0u16;
    let mut chars = 0u16;
    let mut descriptors = 0u16;
    let mut values = 0u16;
    let mut collection_rows = 0usize;
    let mut terminal_rows = 0usize;
    let mut previous_collection_complete = true;
    let mut previous_collection_time = 0;
    let mut default_session = false;
    for record in view.records() {
        let valid = match record {
            Record::Collection(row) => {
                let Some(summary) = view.metadata.collection.as_ref() else {
                    return Err(CaptureError::Relationships);
                };
                let valid = row.id.index() == collection_rows
                    && row.valid(summary)
                    && (row.attempted_at_ms().is_none()
                        || (previous_collection_complete
                            && (collection_rows == 0 || default_session)
                            && row.attempted_at_ms().is_some_and(|at| at >= previous_collection_time)));
                if collection_rows == 0 {
                    default_session = row.complete() && row.raw == [0x62, 0xF1, 0x86, 1];
                }
                previous_collection_complete = row.complete();
                previous_collection_time = row.completed_at_ms().unwrap_or(previous_collection_time);
                collection_rows += 1;
                valid
            }
            Record::CollectionEnvelope(raw) => {
                terminal_rows += 1;
                view.metadata
                    .collection
                    .as_ref()
                    .is_some_and(|s| s.terminal_kind == Some(raw.kind) && s.terminal_len == raw.declared_len)
                    && terminal_rows == 1
            }
            Record::Service(s) => {
                let prior = services;
                services += 1;
                let mut valid = s.start != 0 && s.end >= s.start;
                for body in bodies(view, 1).take(usize::from(prior)) {
                    let previous = service_ref(body?)?;
                    valid &= s.start > previous.end || s.end < previous.start;
                }
                valid
            }
            Record::Characteristic(c) => {
                let prior = chars;
                chars += 1;
                let mut valid = find_service(view, c.service_start)?
                    .is_some_and(|s| c.declaration > s.start && c.handle > c.declaration && c.handle <= s.end);
                for body in bodies(view, 2).take(usize::from(prior)) {
                    let v = characteristic_ref(body?)?;
                    valid &= c.declaration != v.declaration
                        && c.handle != v.handle
                        && c.handle != v.declaration
                        && c.declaration != v.handle
                        && (c.service_start != v.service_start || c.declaration > v.handle || v.declaration > c.handle);
                }
                valid
            }
            Record::Descriptor(d) => {
                let prior = descriptors;
                descriptors += 1;
                let mut valid = false;
                if let Some(c) = find_characteristic(view, d.characteristic_handle)? {
                    valid = c.service_start == d.service_start
                        && d.handle > c.handle
                        && find_service(view, d.service_start)?.is_some_and(|s| d.handle <= s.end);
                    for body in bodies(view, 2) {
                        let v = characteristic_ref(body?)?;
                        valid &= !(v.service_start == d.service_start
                            && v.declaration > c.handle
                            && v.declaration <= d.handle);
                    }
                }
                for body in bodies(view, 3).take(usize::from(prior)) {
                    let mut body = body?;
                    body.u16()?;
                    body.u16()?;
                    valid &= body.u16()? != d.handle;
                }
                valid
            }
            Record::Value(v) => {
                let prior = values;
                values += 1;
                let mut valid = false;
                if let Some(c) = find_characteristic(view, v.handle)? {
                    let characteristic = Uuid::from_wire(c.uuid).ok_or(CaptureError::InvalidData)?;
                    if c.service_start == v.service_start
                        && characteristic.short() == Some(v.uuid)
                        && let Some(s) = find_service(view, c.service_start)?
                    {
                        let service = Uuid::from_wire(s.uuid).ok_or(CaptureError::InvalidData)?;
                        valid = may_read(service, characteristic, c.properties);
                    }
                }
                for body in bodies(view, 4).take(usize::from(prior)) {
                    let mut body = body?;
                    body.u16()?;
                    valid &= body.u16()? != v.handle;
                }
                valid
            }
        };
        if !valid {
            return Err(CaptureError::Relationships);
        }
    }
    if collection_rows != usize::from(view.metadata.collection_row_count)
        || terminal_rows
            != usize::from(
                view.metadata
                    .collection
                    .as_ref()
                    .is_some_and(|s| s.terminal_kind.is_some()),
            )
    {
        return Err(CaptureError::Relationships);
    }
    if !view.metadata.gatt_requested
        && (view.counts != [0; 4]
            || view.metadata.collection.is_some()
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
/// Returns unit for coherent `m` timestamps, peer-report presence, cached-name/reset
/// provenance, stage order and completion claims; otherwise InvalidState. Global scan
/// failures may precede peer discovery but cannot postdate capture. Does not reinterpret
/// legacy termination claims or qualify raw link timing as a valid controller configuration.
fn metadata_valid(m: &CaptureMetadata) -> Result<(), CaptureError> {
    let packet_time_valid = |packet: &PacketEvidence| {
        packet
            .observation
            .is_none_or(|o| (m.first_seen_ms..=m.last_seen_ms).contains(&o.at_ms))
    };
    if m.revision == 0
        || m.collection.as_ref().is_some_and(|s| {
            m.schema_version != 6
                || m.identity_mode != Some(true)
                || !m.gatt_requested
                || m.started_at_ms.is_none_or(|start| s.started_at_ms < start)
                || !s.valid(m.captured_at_ms)
                || m.collection_row_count != 28
        })
        || (m.collection.is_none() && m.collection_row_count != 0)
        || m.identity.as_ref().is_some_and(|identity| {
            m.identity_mode != Some(true)
                || !m.gatt_requested
                || m.started_at_ms.is_none_or(|start| identity.started_at_ms < start)
                || !identity.valid(m.captured_at_ms)
        })
        || m.cached_name.as_ref().is_some_and(|name| {
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
            .as_ref()
            .is_some_and(|last| last.at_ms > m.captured_at_ms)
        || m.advertisement.observation.is_none()
        || !packet_time_valid(&m.advertisement)
        || !packet_time_valid(&m.response)
        || m.started_at_ms.is_some_and(|start| start > m.captured_at_ms)
        || m.ended_at_ms
            .is_some_and(|end| end > m.captured_at_ms || m.started_at_ms.is_none_or(|start| end < start))
        || m.link.as_ref().is_some_and(|link| {
            link.connected_at_ms > m.captured_at_ms
                || m.started_at_ms.is_none_or(|start| link.connected_at_ms < start)
                || m.ended_at_ms.is_some_and(|end| link.connected_at_ms > end)
        })
        || (m.cleanup_complete || m.disconnect_reason.is_some()) && m.ended_at_ms.is_none()
        || m.stages[1..].iter().any(|&complete| complete) && !m.stages[0]
        || (m.stages[2] || m.stages[3]) && !m.stages[1]
        || m.failure.as_ref().is_some_and(|f| {
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
/// Returns unit when `prefix` contains a supported 24-byte header whose total equals
/// `payload_length` (at most 8192) and whose CRC matches. Additional prefix bytes are
/// ignored. Returns TooLarge, Truncated, Magic, Version or Integrity on the first failure.
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
    if !matches!(r.u16()?, 1..=6) {
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
/// Parses immutable `bytes` into unpublished reusable `metadata`, returning a view
/// borrowing both only after all CRC, schema, ordered-record/count and relationship checks.
/// Errors may partially update metadata but expose no view; at most 8192 input bytes
/// and 24/64/128/16 GATT records plus 28 collection rows/one terminal prefix are admitted.
/// No heap allocation or hardware I/O occurs.
// Reuse the complete validator across UI and journal call sites without duplicating
// its record traversal under the utility binary's aggressive inlining policy.
#[inline(never)]
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
    let mut collection_count = 0;
    let mut terminal_count = 0;
    for _ in 0..count {
        let tag = *r.bytes.first().ok_or(CaptureError::Truncated)?;
        if !(1..=6).contains(&tag) || tag < previous || (tag >= 5 && metadata.schema_version < 6) {
            return Err(CaptureError::InvalidData);
        }
        previous = tag;
        if tag <= 4 {
            let index = usize::from(tag - 1);
            view.counts[index] += 1;
            if view.counts[index] > [24, 64, 128, 16][index] {
                return Err(CaptureError::InvalidData);
            }
        } else if tag == 5 {
            collection_count += 1;
            if collection_count > 28 {
                return Err(CaptureError::InvalidData);
            }
        } else {
            terminal_count += 1;
            if terminal_count > 1 {
                return Err(CaptureError::InvalidData);
            }
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
/// Returns admitted owned metadata with records borrowing immutable `bytes`, or the
/// same framing/CRC/schema/state/relationship errors as [`Decoder::decode`].
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
/// Returns admitted v5/v6 prefix length written to `output` from selected-peer `probe`,
/// producer `build` and boot-relative `captured_at_ms` milliseconds. Uses temporary decoder
/// storage to revalidate all encoded state; errors leave output untrusted. No I/O occurs.
pub fn encode(
    probe: &Probe,
    build: BuildIdentity,
    captured_at_ms: u64,
    output: &mut [u8],
) -> Result<usize, CaptureError> {
    Decoder::new().encode(probe, build, captured_at_ms, output)
}

#[cfg(test)]
/// Borrowed nested scans retain framing checks and complete capture relationship admission.
mod borrowed_lookup_tests {
    use super::*;

    /// Borrowed and owned projections reject every truncated declaration with the same error.
    #[test]
    fn borrowed_gatt_fields_match_full_parser_and_share_envelope_errors() {
        for bytes in [
            &[1, 7, 0, 1, 0, 20, 0, 2, 0x0A, 0x18][..],
            &[2, 10, 0, 1, 0, 2, 0, 3, 0, 2, 2, 0x29, 0x2A][..],
        ] {
            for length in 0..=bytes.len() {
                let input = &bytes[..length];
                let owned = read_record(&mut Reader { bytes: input });
                let borrowed = record_body(&mut Reader { bytes: input }).and_then(|(tag, body)| {
                    if tag == 1 {
                        service_ref(body).map(|r| (r.start, r.end, 0, r.uuid))
                    } else {
                        characteristic_ref(body).map(|r| (r.service_start, r.declaration, r.handle, r.uuid))
                    }
                });
                match (owned, borrowed) {
                    (Ok(Record::Service(a)), Ok((start, end, _, uuid))) => {
                        assert_eq!((a.start, a.end), (start, end));
                        assert_eq!(Some(a.uuid), Uuid::from_wire(uuid));
                    }
                    (Ok(Record::Characteristic(a)), Ok((service, declaration, handle, uuid))) => {
                        assert_eq!(
                            (a.service_start, a.declaration, a.handle),
                            (service, declaration, handle)
                        );
                        assert_eq!(Some(a.uuid), Uuid::from_wire(uuid));
                    }
                    (Err(a), Err(b)) => assert_eq!(a, b),
                    _ => panic!("projection changed parser admission at length {length}"),
                }
            }
        }
        // Skipped tags still require a complete envelope, then the iterator stops on error.
        let mut scan = Bodies {
            reader: Reader { bytes: &[2, 8, 0, 1] },
            tag: 1,
        };
        assert!(matches!(scan.next(), Some(Err(CaptureError::Truncated))));
        assert!(scan.next().is_none());
        // Invalid widths and trailing fields cannot be hidden by the borrowed projection.
        for bytes in [
            &[1, 8, 0, 1, 0, 20, 0, 3, 1, 2, 3][..],
            &[1, 8, 0, 1, 0, 20, 0, 2, 1, 2, 3][..],
        ] {
            let owned = read_record(&mut Reader { bytes }).err();
            let borrowed = record_body(&mut Reader { bytes })
                .and_then(|(_, body)| service_ref(body))
                .err();
            assert_eq!(owned, borrowed);
            assert!(owned.is_some());
        }
    }

    /// Returns a synthetic pending inspection with one DIS service and readable characteristic, at 11ms.
    fn probe() -> Probe {
        let mut p = Probe::new();
        p.catalog.observe(Advertisement::new(
            Peer {
                address: [1; 6],
                random: false,
            },
            ReportKind::Connectable,
            -50,
            &[],
            10,
        ));
        p.input(Input::Click, 11).unwrap();
        p.inspection
            .services
            .push(Service {
                start: 1,
                end: 20,
                uuid: Uuid::from_wire(&[0x0A, 0x18]).unwrap(),
            })
            .unwrap();
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: 1,
                declaration: 2,
                handle: 3,
                properties: 2,
                uuid: Uuid::from_wire(&[0x29, 0x2A]).unwrap(),
            })
            .unwrap();
        p
    }

    /// Encodes untrusted `p` with valid CRCs, then runs complete production decoding.
    /// Returns semantic/format errors without relying on the live model to reject invalid rows first.
    fn admitted(p: &Probe) -> Result<(), CaptureError> {
        let mut bytes = [0; PAYLOAD_MAX];
        let len = encode_bytes(
            p,
            BuildIdentity {
                source_digest: [1; 32],
                cpu_hz: 150_000_000,
                spi_hz: 75_000_000,
                profile: BuildProfile::Stock,
                source_identity_available: true,
            },
            12,
            &mut bytes,
        )?;
        decode(&bytes[..len]).map(|_| ())
    }

    /// Narrow repeated scans still reject duplicate, crossing and missing-parent relationships.
    #[test]
    fn borrowed_relationship_scans_keep_nonlocal_rejections() {
        let mut p = probe();
        assert_eq!(admitted(&p), Ok(()));
        p.inspection.services.push(p.inspection.services[0]).unwrap();
        assert_eq!(admitted(&p), Err(CaptureError::Relationships));
        let mut p = probe();
        p.inspection.characteristics[0].service_start = 99;
        assert_eq!(admitted(&p), Err(CaptureError::Relationships));
        let mut p = probe();
        p.inspection
            .characteristics
            .push(Characteristic {
                declaration: 4,
                handle: 5,
                ..p.inspection.characteristics[0]
            })
            .unwrap();
        p.inspection
            .descriptors
            .push(Descriptor {
                service_start: 1,
                characteristic_handle: 3,
                handle: 5,
                uuid: Uuid::from_wire(&[1, 0x29]).unwrap(),
            })
            .unwrap();
        assert_eq!(admitted(&p), Err(CaptureError::Relationships));
        let mut p = probe();
        let value = InfoValue {
            service_start: 1,
            handle: 3,
            uuid: 0x2A29,
            value: Payload::new(b"x"),
            result: ReadResult::Ok,
            failure: None,
            elapsed_ms: 0,
        };
        p.inspection.values.push(value).unwrap();
        assert_eq!(admitted(&p), Ok(()));
        p.inspection.values.push(value).unwrap();
        assert_eq!(admitted(&p), Err(CaptureError::Relationships));
    }
}
