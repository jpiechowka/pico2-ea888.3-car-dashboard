//! Build selection and an explicit local fallback. No browser process or USB access.
use std::fmt::Write;

use dashboard_build_profiles::{ClockError, ClockSelection, CustomClock, ProfileId, ResolvedClock, Voltage};
use dashboard_helper_protocol::{BuildOptions, Mode, NormalizedBuildOptions, normalize};

use crate::editor::{ActionBlock, Editor};

pub const CLOCK_TEXT_BYTES: usize = 32;
pub const FULL_FRAME_BYTES: u64 = 320 * 240 * 2;

/// Preserve rejected text without retaining an unbounded paste.
#[derive(Clone, Debug)]
pub struct ClockText {
    text: heapless::String<CLOCK_TEXT_BYTES>,
    too_long: bool,
}
impl ClockText {
    pub fn new(text: &str) -> Self {
        let mut result = Self {
            text: heapless::String::new(),
            too_long: false,
        };
        result.set(text);
        result
    }

    pub fn set(
        &mut self,
        text: &str,
    ) {
        self.text.clear();
        self.too_long = text.len() > CLOCK_TEXT_BYTES;
        for ch in text.chars() {
            if self.text.push(ch).is_err() {
                break;
            }
        }
    }

    pub fn reject_oversize(&mut self) { self.too_long = true; }

    pub fn text(&self) -> &str { &self.text }

    fn number(
        &self,
        field: &'static str,
    ) -> Result<u32, BuildIssue> {
        if self.too_long {
            return Err(BuildIssue::TooLong(field));
        }
        if self.text.is_empty() || !self.text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BuildIssue::Integer(field));
        }
        self.text.parse().map_err(|_| BuildIssue::Integer(field))
    }
}

#[derive(Clone, Debug)]
pub struct BuildSelection {
    pub options: BuildOptions,
    pub custom: bool,
    pub cpu: ClockText,
    pub spi: ClockText,
    pub voltage: Voltage,
}
impl Default for BuildSelection {
    fn default() -> Self {
        Self {
            options: BuildOptions::default(),
            custom: false,
            cpu: ClockText::new("150000000"),
            spi: ClockText::new("37500000"),
            voltage: Voltage::V1_10,
        }
    }
}
impl BuildSelection {
    pub fn select(
        &mut self,
        key: &str,
    ) -> Result<(), BuildIssue> {
        if key == "custom" {
            self.custom = true;
        } else {
            let profile = dashboard_build_profiles::catalogue()
                .iter()
                .find(|p| p.feature == key)
                .ok_or(BuildIssue::UnknownProfile)?;
            self.options.clock = ClockSelection::Named(profile.id);
            self.custom = false;
        }
        Ok(())
    }

    pub fn selected_key(&self) -> &'static str {
        if self.custom {
            return "custom";
        }
        dashboard_build_profiles::catalogue()
            .iter()
            .find(|p| self.options.clock == ClockSelection::Named(p.id))
            .map_or("", |p| p.feature)
    }

    pub fn normalized(&self) -> Result<NormalizedBuildOptions, BuildIssue> {
        let mut options = self.options.clone();
        if self.custom {
            options.clock = ClockSelection::Custom(CustomClock {
                cpu_hz: self.cpu.number("CPU frequency")?,
                spi_max_hz: self.spi.number("SPI frequency")?,
                voltage: self.voltage,
            });
        }
        normalize(&options).map_err(|error| {
            match error {
                dashboard_helper_protocol::Error::ExpertRequired => BuildIssue::ExpertRequired,
                dashboard_helper_protocol::Error::Clock => {
                    dashboard_build_profiles::resolve(options.clock)
                        .err()
                        .map_or(BuildIssue::Options, BuildIssue::Clock)
                }
                _ => BuildIssue::Options,
            }
        })
    }

    /// Capture the current accepted configuration synchronously at export time.
    pub fn prepare(
        &self,
        editor: &Editor,
    ) -> Result<BuildRecipe, BuildIssue> {
        if let Some(block) = editor.action_readiness().block {
            return Err(BuildIssue::Editor(block));
        }
        let normalized = self.normalized()?;
        let eav = editor.export_binary().map_err(|_| BuildIssue::Configuration)?;
        Ok(BuildRecipe {
            normalized,
            eav,
            startup: dashboard_config::startup::StartupProfile::default()
                .encode()
                .ok_or(BuildIssue::Startup)?,
            toolchain: repository_toolchain().ok_or(BuildIssue::Toolchain)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildIssue {
    TooLong(&'static str),
    Integer(&'static str),
    ExpertRequired,
    UnknownProfile,
    Clock(ClockError),
    Options,
    Editor(ActionBlock),
    Configuration,
    Startup,
    Toolchain,
}
impl BuildIssue {
    pub fn message(self) -> String {
        match self {
            Self::TooLong(field) => format!("{field} exceeds 32 bytes. Replace it with an integer in Hz."),
            Self::Integer(field) => format!("{field} must be a whole number in Hz, using digits only."),
            Self::ExpertRequired => "Enable Expert mode to use this profile or a custom clock.".into(),
            Self::UnknownProfile => "Select a profile from the catalogue.".into(),
            Self::Clock(error) => {
                match error {
                    ClockError::CpuOutOfRange => "CPU frequency must be between 12,000,000 and 300,000,000 Hz.",
                    ClockError::SpiOutOfRange => "SPI request must be between 1,000,000 and 75,000,000 Hz.",
                    ClockError::SpiAbovePeripheralLimit => "SPI request cannot exceed half the resolved CPU clock.",
                    ClockError::NoExactPll => {
                        "This CPU frequency cannot be generated exactly by the supported PLL settings."
                    }
                    _ => "The clock divisors cannot represent this request.",
                }
                .into()
            }
            Self::Options => "These build options are not supported.".into(),
            Self::Editor(block) => {
                match block {
                    ActionBlock::InvalidDraft => {
                        "Correct or discard invalid configuration edits before exporting a build recipe."
                    }
                    ActionBlock::JsonPending => "Apply or discard the JSON edit before exporting a build recipe.",
                    ActionBlock::PointerActive => "Finish the drag before exporting a build recipe.",
                    ActionBlock::ImportPending => "Finish or cancel the import before exporting a build recipe.",
                    ActionBlock::IdentityExhausted => "Reopen Studio before exporting another build recipe.",
                }
                .into()
            }
            Self::Configuration => "The accepted configuration could not be exported.".into(),
            Self::Startup => "The startup profile is invalid. Correct it before exporting a recipe.".into(),
            Self::Toolchain => "The repository toolchain declaration is not supported.".into(),
        }
    }
}

/// Repository expectation, never presented as a detected local installation.
pub fn repository_toolchain() -> Option<&'static str> {
    let mut channels = include_str!("../../rust-toolchain.toml").lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "channel").then_some(value.trim())
    });
    let value = channels.next()?.strip_prefix('"')?.strip_suffix('"')?;
    (channels.next().is_none()
        && !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    .then_some(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferEstimate {
    pub divisor: u32,
    pub payload_us: u64,
    pub payloads_per_second_milli: u64,
}
/// Ideal full RGB565 payload only. Uses the rational divider, not rounded Hz.
pub fn transfer_estimate(clock: &ResolvedClock) -> Option<TransferEstimate> {
    if clock.actual_cpu_hz == 0 || !(2..=254).contains(&clock.cpsdvsr) || !clock.cpsdvsr.is_multiple_of(2) {
        return None;
    }
    let divisor = u32::from(clock.cpsdvsr) * (u32::from(clock.scr) + 1);
    let cycles = FULL_FRAME_BYTES.checked_mul(8)?.checked_mul(u64::from(divisor))?;
    Some(TransferEstimate {
        divisor,
        payload_us: cycles.checked_mul(1_000_000)?.div_ceil(u64::from(clock.actual_cpu_hz)),
        payloads_per_second_milli: u64::from(clock.actual_cpu_hz).checked_mul(1000)? / cycles,
    })
}

#[derive(Debug)]
pub struct BuildRecipe {
    pub normalized: NormalizedBuildOptions,
    eav: Vec<u8>,
    startup: [u8; 16],
    toolchain: &'static str,
}
impl BuildRecipe {
    pub fn config_bytes(&self) -> &[u8] { &self.eav }

    pub fn startup_bytes(&self) -> &[u8; 16] { &self.startup }

    pub fn with_startup(
        mut self,
        profile: dashboard_config::startup::StartupProfile,
    ) -> Result<Self, BuildIssue> {
        self.startup = profile.encode().ok_or(BuildIssue::Startup)?;
        Ok(self)
    }

    pub fn powershell(&self) -> String {
        let mut hex = String::with_capacity(self.eav.len() * 2);
        for byte in &self.eav {
            write!(hex, "{byte:02x}").expect("writing to String cannot fail");
        }
        let mut startup_hex = String::with_capacity(32);
        for byte in &self.startup {
            write!(startup_hex, "{byte:02x}").expect("writing to String cannot fail");
        }
        let opts = &self.normalized.requested;
        let mut arguments = format!(
            "    Mode = '{}'\n    Optimization = '{}'\n    FrameTrace = ${}\n    TeTrace = ${}\n    SerialDisplay = \
             ${}\n    FullOutline = ${}\n    FullFrame = ${}\n    Expert = ${}\n",
            match opts.mode {
                Mode::Demo => "Demo",
                Mode::Vehicle => "Vehicle",
            },
            opts.optimization.cargo_value(),
            opts.frame_trace,
            opts.te_trace,
            opts.serial_display,
            !opts.simple_outline,
            opts.full_frame,
            opts.expert,
        );
        match opts.clock {
            ClockSelection::Named(id) => {
                let alias = dashboard_build_profiles::catalogue()
                    .iter()
                    .find(|p| p.id == id)
                    .expect("normalized profile exists")
                    .aliases[0];
                writeln!(arguments, "    Profile = '{alias}'").unwrap();
            }
            ClockSelection::Custom(clock) => {
                writeln!(
                    arguments,
                    "    CpuHz = '{}'\n    SpiHz = '{}'\n    VselMv = '{}'",
                    clock.cpu_hz,
                    clock.spi_max_hz,
                    clock.voltage.millivolts()
                )
                .unwrap();
            }
        }
        include_str!("build_flash/recipe.ps1.in")
            .replace("@@TOOLCHAIN@@", self.toolchain)
            .replace("@@EAV@@", &hex)
            .replace("@@STARTUP@@", &startup_hex)
            .replace("@@OPTIONS@@", &arguments)
    }
}

pub fn selected_profile(selection: &BuildSelection) -> Option<&'static dashboard_build_profiles::Profile> {
    if selection.custom {
        return None;
    }
    let ClockSelection::Named(id) = selection.options.clock else {
        return None;
    };
    dashboard_build_profiles::catalogue().iter().find(|p| p.id == id)
}

pub fn profile_by_id(id: ProfileId) -> &'static dashboard_build_profiles::Profile {
    dashboard_build_profiles::catalogue()
        .iter()
        .find(|p| p.id == id)
        .expect("closed profile catalogue")
}
