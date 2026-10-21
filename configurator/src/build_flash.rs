//! Build selection and an explicit local fallback. No browser process or USB access.
use std::fmt::Write;

use dashboard_build_profiles::{ClockError, ClockSelection, CustomClock, ProfileId, ResolvedClock, Voltage};
use dashboard_config_tools::hex_bytes;
use dashboard_helper_protocol::{BuildOptions, Mode, NormalizedBuildOptions, normalize};

use crate::editor::{ActionBlock, Editor};

/// Maximum retained UTF-8 bytes for each raw CPU/SPI frequency control.
pub const CLOCK_TEXT_BYTES: usize = 32;
/// Complete 320×240 RGB565 payload bytes; portrait has the same area.
pub const FULL_FRAME_BYTES: u64 = 320 * 240 * 2;

/// Preserve rejected text without retaining an unbounded paste.
#[derive(Clone, Debug)]
pub struct ClockText {
    /// Visible input prefix, never split inside a UTF-8 scalar.
    text: heapless::String<CLOCK_TEXT_BYTES>,
    /// Original input exceeded capacity; a retained numeric prefix is not admissible.
    too_long: bool,
}
impl ClockText {
    /// Returns bounded raw `text`, retaining an oversize rejection flag when truncated.
    pub fn new(text: &str) -> Self {
        let mut result = Self {
            text: heapless::String::new(),
            too_long: false,
        };
        result.set(text);
        result
    }

    /// Replace the visible prefix with `text` and recompute the oversize flag.
    /// Retains invalid syntax for repair; parsing is deferred to normalization.
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

    /// Latch browser-side oversize rejection without replacing the retained visible text.
    pub fn reject_oversize(&mut self) { self.too_long = true; }

    /// Returns the retained UTF-8 prefix; numeric-looking text may still be oversized.
    pub fn text(&self) -> &str { &self.text }

    /// Returns exact unsigned decimal Hz, using `field` in diagnostics. Oversize,
    /// empty/non-ASCII-digit/whitespace text or `u32` overflow is rejected.
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
/// Browser build settings with independent retained raw custom clock inputs.
pub struct BuildSelection {
    /// Requested mode/profile/compiler/diagnostic options before normalization.
    pub options: BuildOptions,
    /// Whether raw CPU/SPI/voltage controls override the options' clock selector.
    pub custom: bool,
    /// Requested CPU frequency text in Hz, requiring an exact supported PLL solution.
    pub cpu: ClockText,
    /// Requested maximum SPI frequency text in Hz; attainable rate can round downward.
    pub spi: ClockText,
    /// Discrete regulator setpoint for custom mode, not measured device voltage.
    pub voltage: Voltage,
}
impl Default for BuildSelection {
    /// Returns shared default options with custom mode off and 150 MHz/37.5 MHz/1.10 V text.
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
    /// Select `key` as `custom` or an exact catalogue feature name; returns `Ok(())`
    /// on selection. Unknown keys return `UnknownProfile` without changing state.
    /// Raw custom values survive named-profile selection for later repair/reuse.
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

    /// Returns `custom`, the selected named feature key, or `""` for unmatched options.
    pub fn selected_key(&self) -> &'static str {
        if self.custom {
            return "custom";
        }
        dashboard_build_profiles::catalogue()
            .iter()
            .find(|p| self.options.clock == ClockSelection::Named(p.id))
            .map_or("", |p| p.feature)
    }

    /// Returns shared resolved build options, parsing raw Hz only in custom mode.
    /// Reports text/integer, Expert, clock or option failures without mutation.
    /// Successful resolution proves representability, not electrical qualification.
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
    /// Returns a frozen recipe from ready `editor` and normalized settings, including
    /// accepted startup/artwork. Readiness, encoding, startup or embedded toolchain
    /// failures return `BuildIssue`; no process, file write or device operation occurs.
    pub fn prepare(
        &self,
        editor: &Editor,
    ) -> Result<BuildRecipe, BuildIssue> {
        if let Some(block) = editor.action_readiness().block {
            return Err(BuildIssue::Editor(block));
        }
        let normalized = self.normalized()?;
        let project = editor.accepted_project().map_err(|_| BuildIssue::Configuration)?;
        Ok(BuildRecipe {
            normalized,
            eav: project.dashboard_bytes().to_vec(),
            startup: project.profile().encode().ok_or(BuildIssue::Startup)?,
            artwork: project.artwork().map(|a| a.converted().to_vec()),
            toolchain: repository_toolchain().ok_or(BuildIssue::Toolchain)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Admission failure while normalizing settings or capturing an exportable recipe.
pub enum BuildIssue {
    /// Named frequency input exceeded its UTF-8 text bound.
    TooLong(
        /// UI field label identifying the oversized frequency input.
        &'static str,
    ),
    /// Named frequency input is not a complete decimal `u32` Hz value.
    Integer(
        /// UI field label identifying malformed or overflowing Hz text.
        &'static str,
    ),
    /// Requested profile/custom settings require explicit Expert selection.
    ExpertRequired,
    /// Profile key is absent from the shared catalogue.
    UnknownProfile,
    /// Shared clock resolver rejected the requested clock configuration.
    Clock(
        /// Exact shared clock-resolver rejection.
        ClockError,
    ),
    /// Shared build-option normalization rejected the combination.
    Options,
    /// Editor has a pending/invalid operation or exhausted identity.
    Editor(
        /// Current editor readiness blocker.
        ActionBlock,
    ),
    /// Accepted project could not be captured/encoded.
    Configuration,
    /// Startup profile is invalid or requires unavailable converted artwork.
    Startup,
    /// Embedded repository toolchain declaration is unsupported or ambiguous.
    Toolchain,
}
impl BuildIssue {
    /// Returns actionable UI text for this failure, preserving the failed field/clock cause.
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
                    ActionBlock::StartupPending => "Apply or discard startup timing before preparing a build.",
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
/// Returns the single quoted embedded `channel` value if nonempty, at most 64 bytes
/// and restricted to ASCII alphanumerics/hyphens; otherwise `None`. Does not read disk.
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
/// Ideal full-payload wire lower bound from resolved clocks, excluding all overhead.
pub struct TransferEstimate {
    /// Product of SPI prescaler and serial-clock divider.
    pub divisor: u32,
    /// Full RGB565 payload duration in microseconds, rounded up.
    pub payload_us: u64,
    /// Ideal complete payloads per second multiplied by 1000, rounded down.
    pub payloads_per_second_milli: u64,
}
/// Ideal full RGB565 payload only. Uses the rational divider, not rounded Hz.
/// Returns timing derived from `clock`'s actual CPU/peripheral rate and SPI divisors,
/// or `None` for zero clock, invalid prescaler or checked arithmetic overflow.
/// Assumes the resolved peripheral rate equals actual CPU rate; not physical FPS.
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
/// Immutable exported build inputs plus normalized options; never a device-write capability.
pub struct BuildRecipe {
    /// Shared requested/resolved build options captured at recipe creation.
    pub normalized: NormalizedBuildOptions,
    /// Bounded accepted dashboard binary.
    eav: Vec<u8>,
    /// Encoded accepted startup profile, exactly 16 bytes.
    startup: [u8; 16],
    /// Optional bounded converted artwork; original source files are excluded.
    artwork: Option<Vec<u8>>,
    /// Compiler channel embedded from this Studio build's repository pin.
    toolchain: &'static str,
}
impl BuildRecipe {
    /// Returns the frozen dashboard configuration bytes.
    pub fn config_bytes(&self) -> &[u8] { &self.eav }

    /// Returns the frozen 16-byte startup profile encoding.
    pub fn startup_bytes(&self) -> &[u8; 16] { &self.startup }

    /// Returns converted artwork bytes, or `None` when the recipe carries no asset.
    pub fn artwork_bytes(&self) -> Option<&[u8]> { self.artwork.as_deref() }

    /// Returns SHA256 of the exact frozen dashboard bytes.
    pub fn config_sha256(&self) -> [u8; 32] { dashboard_config_tools::project::digest(&self.eav) }

    /// Returns SHA256 of the exact frozen startup profile encoding.
    pub fn startup_sha256(&self) -> [u8; 32] { dashboard_config_tools::project::digest(&self.startup) }

    /// Returns the converted asset SHA256, or `None` when no artwork is included.
    pub fn artwork_sha256(&self) -> Option<[u8; 32]> {
        self.artwork_bytes().map(dashboard_config_tools::project::digest)
    }

    /// Consume this recipe and return it with encoded `profile`. Custom startup
    /// requires existing artwork; a procedural theme removes recipe artwork.
    /// Invalid/missing-custom startup returns `Startup`; the editor is unaffected.
    pub fn with_startup(
        mut self,
        profile: dashboard_config::startup::StartupProfile,
    ) -> Result<Self, BuildIssue> {
        if profile.theme == dashboard_config::startup::StartupTheme::Custom && self.artwork.is_none() {
            return Err(BuildIssue::Startup);
        }
        self.startup = profile.encode().ok_or(BuildIssue::Startup)?;
        if profile.theme != dashboard_config::startup::StartupTheme::Custom {
            self.artwork = None;
        }
        Ok(self)
    }

    /// Returns the standalone PowerShell build recipe with hex-encoded inputs,
    /// digests, pinned compiler and normalized arguments; does not execute it.
    /// Named profiles rely on the closed catalogue having at least one alias.
    /// The script uses CreateNew snapshots and restores environment in `finally`.
    pub fn powershell(&self) -> String {
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
            .replace("@@EAV@@", &hex_bytes(&self.eav))
            .replace("@@STARTUP@@", &hex_bytes(&self.startup))
            .replace("@@ASSET@@", &hex_bytes(self.artwork_bytes().unwrap_or_default()))
            .replace("@@EAV_SHA@@", &hex_bytes(&self.config_sha256()))
            .replace("@@STARTUP_SHA@@", &hex_bytes(&self.startup_sha256()))
            .replace(
                "@@ASSET_SHA@@",
                &self.artwork_sha256().map(|d| hex_bytes(&d)).unwrap_or_default(),
            )
            .replace("@@OPTIONS@@", &arguments)
    }
}

/// Returns `selection`'s named catalogue profile, or `None` for custom/unmatched settings.
pub fn selected_profile(selection: &BuildSelection) -> Option<&'static dashboard_build_profiles::Profile> {
    if selection.custom {
        return None;
    }
    let ClockSelection::Named(id) = selection.options.clock else {
        return None;
    };
    dashboard_build_profiles::catalogue().iter().find(|p| p.id == id)
}

/// Returns the catalogue profile for `id`; panics only if the closed ID/catalogue invariant drifts.
pub fn profile_by_id(id: ProfileId) -> &'static dashboard_build_profiles::Profile {
    dashboard_build_profiles::catalogue()
        .iter()
        .find(|p| p.id == id)
        .expect("closed profile catalogue")
}
