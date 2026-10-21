//! Bounded reset evidence shared by firmware, saved captures and host tools.
use super::Fault;

/// First supervisor recovery observation, retained without asserting the reset's underlying cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryCause {
    /// Monotonic observation time regressed.
    Clock,
    /// Supervisor itself missed its bounded observation interval.
    Supervisor,
    /// Acquisition owner stopped making useful progress.
    Acquisition,
    /// Rendering owner stopped making useful progress.
    Rendering,
    /// Input owner stopped making useful progress.
    Input,
    /// Display progress or maintenance display-quiescence contract failed.
    Display,
    /// Flash-maintenance lifetime/identity/resumption contract failed.
    Maintenance,
    /// Radio owner reported an unrecoverable error.
    RadioFatal,
    /// Completed radio-boundary count stopped advancing.
    RadioStalled,
    /// Radio startup failed before normal supervision.
    RadioStartup,
    /// Display owner reported an unrecoverable transport/ownership error.
    DisplayFatal,
    /// Explicit recovery required reboot rather than ordinary in-place control recovery.
    RecoveryRequested,
}
impl RecoveryCause {
    /// Stable codes used by retained scratch and capture formats.
    /// Returns `self`'s one-based code in 1..=12, independent of Rust discriminant layout.
    pub const fn code(self) -> u8 {
        match self {
            Self::Clock => 1,
            Self::Supervisor => 2,
            Self::Acquisition => 3,
            Self::Rendering => 4,
            Self::Input => 5,
            Self::Display => 6,
            Self::Maintenance => 7,
            Self::RadioFatal => 8,
            Self::RadioStalled => 9,
            Self::RadioStartup => 10,
            Self::DisplayFatal => 11,
            Self::RecoveryRequested => 12,
        }
    }

    /// Returns recovery observation decoded from stable `code` 1..=12, or `None` for
    /// zero/reserved codes. Unknown evidence is never mapped to a healthy/default cause.
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::Clock,
            2 => Self::Supervisor,
            3 => Self::Acquisition,
            4 => Self::Rendering,
            5 => Self::Input,
            6 => Self::Display,
            7 => Self::Maintenance,
            8 => Self::RadioFatal,
            9 => Self::RadioStalled,
            10 => Self::RadioStartup,
            11 => Self::DisplayFatal,
            12 => Self::RecoveryRequested,
            _ => return None,
        })
    }

    /// Returns `self`'s compact observation label; it is not an electrical root-cause diagnosis.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::Supervisor => "supervisor",
            Self::Acquisition => "acquisition",
            Self::Rendering => "rendering",
            Self::Input => "input",
            Self::Display => "display",
            Self::Maintenance => "maintenance",
            Self::RadioFatal => "radio fatal",
            Self::RadioStalled => "radio stalled",
            Self::RadioStartup => "radio startup",
            Self::DisplayFatal => "display fatal",
            Self::RecoveryRequested => "recovery requested",
        }
    }
}
impl From<Fault> for RecoveryCause {
    /// Returns the matching retained recovery observation for supervisor `fault`.
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::Clock => Self::Clock,
            Fault::Supervisor => Self::Supervisor,
            Fault::Acquisition => Self::Acquisition,
            Fault::Rendering => Self::Rendering,
            Fault::Input => Self::Input,
            Fault::Display => Self::Display,
            Fault::Maintenance => Self::Maintenance,
        }
    }
}
/// Last retained boot stage or first explicitly recorded failure observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetStage {
    /// Startup began but no supervised feed was recorded yet.
    Booting,
    /// Most recent permitted watchdog feed observed useful supervised progress.
    Supervised,
    /// First failure observation; subsequent updates cannot overwrite it.
    Failure(
        /// Recovery condition observed before withholding feeds.
        RecoveryCause,
    ),
}
impl ResetStage {
    /// Returns `self`'s stable stage code: Booting 1, Supervised 2, or failure 17..=28.
    pub const fn code(self) -> u8 {
        match self {
            Self::Booting => 1,
            Self::Supervised => 2,
            Self::Failure(cause) => 16 + cause.code(),
        }
    }

    /// Returns a stage from stable `code` 1, 2 or 17..=28; other codes return `None`.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Booting),
            2 => Some(Self::Supervised),
            17..=28 => {
                match RecoveryCause::from_code(code - 16) {
                    Some(cause) => Some(Self::Failure(cause)),
                    None => None,
                }
            }
            _ => None,
        }
    }

    /// Returns `self`'s boot/feed label or underlying first recovery-observation label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Booting => "booting",
            Self::Supervised => "supervised",
            Self::Failure(cause) => cause.label(),
        }
    }
}
/// Last observed stage, not the reset's underlying electrical/software cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetRecord {
    /// Last observed boot/feed state or first failure, never a proven reset cause.
    pub stage: ResetStage,
    /// Boot-relative time at the observation; saturates at u32::MAX milliseconds.
    pub elapsed_ms: u32,
}
/// Retained scratch commit discriminator, written last after invalidation and payload.
const MAGIC: u32 = 0x5744_5231;
/// Retained record version encoded in the payload word's high byte.
const VERSION: u32 = 1;
impl ResetRecord {
    /// Returns the initial Booting observation at zero boot-relative milliseconds.
    pub const fn booting() -> Self {
        Self {
            stage: ResetStage::Booting,
            elapsed_ms: 0,
        }
    }

    /// Updates `self` to Supervised at boot-relative `elapsed_ms`, saturating time to
    /// u32 milliseconds; returns unit and preserves any already recorded first failure.
    pub fn supervised(
        &mut self,
        elapsed_ms: u64,
    ) {
        self.update(ResetStage::Supervised, elapsed_ms);
    }

    /// Records first `cause` in `self` at boot-relative `elapsed_ms`, saturating to u32
    /// milliseconds; returns unit. An existing failure and its timestamp remain unchanged.
    pub fn fail(
        &mut self,
        cause: RecoveryCause,
        elapsed_ms: u64,
    ) {
        self.update(ResetStage::Failure(cause), elapsed_ms);
    }

    /// Replaces `self` with `stage` and saturated boot-relative `elapsed_ms` unless a
    /// failure is already latched; returns unit. Does not validate monotonic progression.
    fn update(
        &mut self,
        stage: ResetStage,
        elapsed_ms: u64,
    ) {
        if !matches!(self.stage, ResetStage::Failure(_)) {
            self.stage = stage;
            self.elapsed_ms = elapsed_ms.min(u64::from(u32::MAX)) as u32;
        }
    }

    /// Returns `self` as magic, version/stage, milliseconds and CRC words for scratch 0..3.
    /// Word layout is explicit; CRC covers little-endian bytes and is not authentication.
    pub fn encode(self) -> [u32; 4] {
        let payload = (VERSION << 24) | u32::from(self.stage.code());
        [MAGIC, payload, self.elapsed_ms, checksum(payload, self.elapsed_ms)]
    }

    /// Returns retained evidence from `words` only if magic/version/reserved bits,
    /// checksum and stage code agree; otherwise `None`. Does not establish which image
    /// wrote the record, its boot duration beyond saturation, or that power retained scratch.
    pub fn decode(words: [u32; 4]) -> Option<Self> {
        if words[0] != MAGIC
            || words[1] >> 24 != VERSION
            || words[1] & 0x00FF_FF00 != 0
            || words[3] != checksum(words[1], words[2])
        {
            return None;
        }
        Some(Self {
            stage: ResetStage::from_code(words[1] as u8)?,
            elapsed_ms: words[2],
        })
    }

    /// Invalidate first, then publish the commit marker last. Hardware writers
    /// must apply these volatile register writes in order, with a barrier before commit.
    /// Returns five `(scratch_index, value)` writes for `self`, restricted to indices 0..3.
    /// This pure function performs no I/O; the sole owner must also exclude ROM reboot
    /// use of scratch 2/3 while applying the sequence.
    pub fn scratch_writes(self) -> [(usize, u32); 5] {
        let words = self.encode();
        [(0, 0), (1, words[1]), (2, words[2]), (3, words[3]), (0, words[0])]
    }
}
/// Returns CRC-32 of magic, encoded version/stage `payload`, and saturated `elapsed_ms`,
/// each as little-endian u32. This validates record coherence rather than authenticity.
fn checksum(
    payload: u32,
    elapsed_ms: u32,
) -> u32 {
    let mut bytes = [0; 12];
    bytes[..4].copy_from_slice(&MAGIC.to_le_bytes());
    bytes[4..8].copy_from_slice(&payload.to_le_bytes());
    bytes[8..].copy_from_slice(&elapsed_ms.to_le_bytes());
    crate::settings::crc32(&bytes)
}
/// Immutable pre-start watchdog/ROM evidence, interpreted conservatively for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootEvidence {
    /// Unmodified WATCHDOG_REASON, including any unexpected reserved bits.
    pub raw_reason: u32,
    /// Raw ROM byte including its chained flag; None means unavailable/invalid reply.
    pub boot_type: Option<u8>,
    /// Only admitted for a watchdog fault with a valid retained record.
    pub previous: Option<ResetRecord>,
}
impl BootEvidence {
    /// Returns raw `raw_reason`/`boot_type` plus decoded `scratch` only when the conservative
    /// watchdog classifier signals a fault. Unknown/torn scratch becomes absent; original
    /// hardware reason and ROM byte remain unchanged. Performs no register access or reboot.
    pub fn capture(
        raw_reason: u32,
        boot_type: Option<u8>,
        scratch: [u32; 4],
    ) -> Self {
        let mut evidence = Self {
            raw_reason,
            boot_type,
            previous: None,
        };
        if evidence.watchdog_fault() {
            evidence.previous = ResetRecord::decode(scratch);
        }
        evidence
    }

    /// Unknown ROM metadata never suppresses a hardware watchdog indication.
    /// SDK normalizes the chained flag; only its documented nonnormal types
    /// count as intentional here. Reserved hardware reason bits stay visible.
    /// A deliberate RP2350 NORMAL ROM reboot also uses the watchdog timer and
    /// overwrites retained scratch. Its metadata cannot prove a healthy reboot;
    /// keep it visible without interpreting TIMER as proof of a firmware stall.
    /// Returns whether `self` requires watchdog-fault visibility: any reserved reason bits
    /// do; zero reason does not; known ROM types 2/3/4/13 (optionally chained) suppress
    /// ordinary reason bits. NORMAL/type0 and missing/unknown metadata remain visible.
    /// See [SDK boot types](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2_common/boot_bootrom_headers/include/boot/bootrom_constants.h).
    pub const fn watchdog_fault(self) -> bool {
        if self.raw_reason == 0 {
            return false;
        }
        if self.raw_reason & !3 != 0 {
            return true;
        }
        !matches!(self.boot_type, Some(2 | 3 | 4 | 13 | 0x82 | 0x83 | 0x84 | 0x8D))
    }

    /// Returns `self`'s conservative diagnostic label: OTHER for zero raw reason,
    /// ROM REBOOT for recognized intentional types, otherwise raw timer/forced/both/unknown.
    /// WD TIMEOUT names the hardware bit; it does not prove a missed application feed.
    pub const fn label(self) -> &'static str {
        if self.raw_reason == 0 {
            return "OTHER";
        }
        if !self.watchdog_fault() {
            return "ROM REBOOT";
        }
        match self.raw_reason {
            1 => "WD TIMEOUT",
            2 => "WD FORCED",
            3 => "WD BOTH",
            _ => "WD UNKNOWN",
        }
    }
}
/// Validate the ROM SYS_INFO_BOOT_INFO result before extracting the raw byte.
/// Returns bits 8..15 of `words[1]` only when ROM `status` reports exactly five output
/// words and `words[0]` equals requested supported flag 0x40; otherwise `None`.
/// `words` is the complete five-word ROM output buffer.
/// Preserves the chained flag and unknown boot-type values for conservative classification.
pub const fn boot_type_from_sys_info(
    status: i32,
    words: [u32; 5],
) -> Option<u8> {
    if status == 5 && words[0] == 0x40 {
        Some((words[1] >> 8) as u8)
    } else {
        None
    }
}
