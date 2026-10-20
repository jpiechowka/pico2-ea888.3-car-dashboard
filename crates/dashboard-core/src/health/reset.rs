//! Bounded reset evidence shared by firmware, saved captures and host tools.
use super::Fault;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryCause {
    Clock,
    Supervisor,
    Acquisition,
    Rendering,
    Input,
    Display,
    Maintenance,
    RadioFatal,
    RadioStalled,
    RadioStartup,
    DisplayFatal,
    RecoveryRequested,
}
impl RecoveryCause {
    /// Stable codes used by retained scratch and capture formats.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetStage {
    Booting,
    Supervised,
    Failure(RecoveryCause),
}
impl ResetStage {
    pub const fn code(self) -> u8 {
        match self {
            Self::Booting => 1,
            Self::Supervised => 2,
            Self::Failure(cause) => 16 + cause.code(),
        }
    }

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
    pub stage: ResetStage,
    /// Boot-relative time at the observation; saturates at u32::MAX milliseconds.
    pub elapsed_ms: u32,
}
const MAGIC: u32 = 0x5744_5231;
const VERSION: u32 = 1;
impl ResetRecord {
    pub const fn booting() -> Self {
        Self {
            stage: ResetStage::Booting,
            elapsed_ms: 0,
        }
    }

    pub fn supervised(
        &mut self,
        elapsed_ms: u64,
    ) {
        self.update(ResetStage::Supervised, elapsed_ms);
    }

    pub fn fail(
        &mut self,
        cause: RecoveryCause,
        elapsed_ms: u64,
    ) {
        self.update(ResetStage::Failure(cause), elapsed_ms);
    }

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

    pub fn encode(self) -> [u32; 4] {
        let payload = (VERSION << 24) | u32::from(self.stage.code());
        [MAGIC, payload, self.elapsed_ms, checksum(payload, self.elapsed_ms)]
    }

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
    pub fn scratch_writes(self) -> [(usize, u32); 5] {
        let words = self.encode();
        [(0, 0), (1, words[1]), (2, words[2]), (3, words[3]), (0, words[0])]
    }
}
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
    pub const fn watchdog_fault(self) -> bool {
        if self.raw_reason == 0 {
            return false;
        }
        if self.raw_reason & !3 != 0 {
            return true;
        }
        !matches!(self.boot_type, Some(2 | 3 | 4 | 13 | 0x82 | 0x83 | 0x84 | 0x8D))
    }

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
