//! Build-option normalization, frozen recipes and Windows script boundary regressions.

use dashboard_build_profiles::{ClockSelection, CustomClock, ProfileId, Voltage, catalogue, resolve};
use dashboard_configurator::build_flash::*;
use dashboard_configurator::editor::{ActionBlock, Editor, PointerTarget};
#[cfg(windows)]
use dashboard_helper_protocol::{Mode, Optimization};

#[test]
/// Every named profile uses shared actual divisors and Expert gating; unknown names preserve the previous selection.
fn defaults_and_all_named_profiles_use_the_shared_resolver() {
    let mut selection = BuildSelection::default();
    let stock = selection.normalized().unwrap();
    assert_eq!(stock.clock.actual_cpu_hz, 150_000_000);
    assert_eq!(stock.clock.actual_spi_hz, 37_500_000);
    assert_eq!(stock.clock.cpsdvsr, 2);
    assert_eq!(stock.clock.scr, 1);
    for profile in catalogue() {
        selection.options.expert = false;
        selection.select(profile.feature).unwrap();
        assert_eq!(selection.normalized().is_err(), profile.requires_expert);
        selection.options.expert = true;
        let normalized = selection.normalized().unwrap();
        assert_eq!(normalized.requested.clock, ClockSelection::Named(profile.id));
        assert_eq!(normalized.clock, resolve(ClockSelection::Named(profile.id)).unwrap());
    }
    let previous = selection.selected_key();
    assert_eq!(selection.select("$(bad)"), Err(BuildIssue::UnknownProfile));
    assert_eq!(selection.selected_key(), previous);
}

#[test]
/// Expert clock profiles retain actual PLL/SPI values including legacy rounding; this is not electrical qualification.
fn expert_gating_and_legacy_failure_settings_are_preserved() {
    let mut selection = BuildSelection::default();
    selection.select("cpu280-spi70-1v30").unwrap();
    assert_eq!(selection.normalized(), Err(BuildIssue::ExpertRequired));
    selection.options.expert = true;
    let working = selection.normalized().unwrap();
    assert_eq!(working.clock.actual_cpu_hz, 280_000_000);
    assert_eq!(working.clock.actual_spi_hz, 70_000_000);
    assert_eq!(working.clock.voltage.millivolts(), 1300);
    selection.select("cpu290-spi72-1v30").unwrap();
    let failed = selection.normalized().unwrap();
    assert_eq!(failed.clock.actual_cpu_hz, 290_400_000);
    assert_eq!(failed.clock.actual_spi_hz, 48_400_000);
    assert!(failed.clock.legacy_rounding);
    selection.options.expert = false;
    assert_eq!(selection.normalized(), Err(BuildIssue::ExpertRequired));
}

#[test]
/// Strict decimal Hz parsing rejects signs/whitespace/overflow; shared PLL/divisor admission determines achievable SPI
/// rates.
fn custom_clocks_reject_invalid_text_overflow_and_unrepresentable_requests() {
    let mut selection = BuildSelection::default();
    selection.select("custom").unwrap();
    selection.options.expert = true;
    for text in [
        "",
        "-1",
        "+150000000",
        " 150000000",
        "150000000 ",
        "1e8",
        "150.0",
        "１２",
        "4294967296",
        "1\n",
    ] {
        selection.cpu.set(text);
        assert!(selection.normalized().is_err(), "accepted {text:?}");
    }
    selection.cpu.set("1500000000000000000000000000000000");
    assert_eq!(selection.cpu.text().len(), CLOCK_TEXT_BYTES);
    assert_eq!(selection.normalized(), Err(BuildIssue::TooLong("CPU frequency")));
    selection.cpu.set("150000001");
    assert!(matches!(selection.normalized(), Err(BuildIssue::Clock(_))));
    selection.cpu.set("12000000");
    selection.spi.set("75000000");
    assert!(matches!(selection.normalized(), Err(BuildIssue::Clock(_))));
    selection.cpu.set("280000000");
    selection.spi.set("69000000");
    let rounded = selection.normalized().unwrap();
    assert_eq!(rounded.clock.actual_spi_hz, 46_666_666);
    selection.spi.set("70000000");
    assert_eq!(selection.normalized().unwrap().clock.actual_spi_hz, 70_000_000);
}

#[test]
/// Oversized custom clock input remains invalid across named-profile switching until explicitly replaced.
fn overlong_browser_input_stays_invalid_until_replaced() {
    let mut selection = BuildSelection::default();
    selection.select("custom").unwrap();
    selection.options.expert = true;
    selection.cpu.reject_oversize();
    assert!(matches!(selection.normalized(), Err(BuildIssue::TooLong(_))));
    selection.select("cpu150-spi15-1v10").unwrap();
    assert!(selection.normalized().is_ok());
    selection.select("custom").unwrap();
    assert!(selection.normalized().is_err());
    selection.cpu.set("150000000");
    assert!(selection.normalized().is_ok());
    selection.cpu.set(&"é".repeat(1000));
    assert_eq!(selection.cpu.text().len(), 32);
    assert!(matches!(selection.normalized(), Err(BuildIssue::TooLong(_))));
}

#[test]
/// Payload-only transfer estimates use exact divisors and reject malformed clocks; no scanout/FPS evidence is produced.
fn full_frame_estimate_uses_exact_dividers_not_truncated_spi_hz() {
    let clock = resolve(ClockSelection::Named(ProfileId::Cpu280Spi70)).unwrap();
    assert_eq!(
        transfer_estimate(&clock),
        Some(TransferEstimate {
            divisor: 4,
            payload_us: 17_555,
            payloads_per_second_milli: 56_966,
        })
    );
    let clock = resolve(ClockSelection::Custom(CustomClock {
        cpu_hz: 280_000_000,
        spi_max_hz: 69_000_000,
        voltage: Voltage::V1_10,
    }))
    .unwrap();
    assert_eq!(transfer_estimate(&clock).unwrap().payload_us, 26_332);
    let mut malformed = clock;
    malformed.actual_cpu_hz = 0;
    assert!(transfer_estimate(&malformed).is_none());
    malformed.actual_cpu_hz = 280_000_000;
    malformed.cpsdvsr = 3;
    assert!(transfer_estimate(&malformed).is_none());
}

#[test]
/// Recipe capture matches accepted binary bytes and denies an active pointer until cancellation.
fn recipe_captures_only_a_ready_accepted_configuration() {
    let selection = BuildSelection::default();
    let mut editor = Editor::default();
    let recipe = selection.prepare(&editor).unwrap();
    assert_eq!(recipe.config_bytes(), editor.export_binary().unwrap());
    let pointer = editor.begin_pointer(1, PointerTarget::Widget(1)).unwrap();
    assert!(matches!(
        selection.prepare(&editor),
        Err(BuildIssue::Editor(ActionBlock::PointerActive))
    ));
    editor.cancel_pointer(pointer).unwrap();
    assert!(selection.prepare(&editor).is_ok());
}

#[test]
/// Startup profile bytes are frozen after shared validation; durations over ten seconds fail.
fn recipe_carries_validated_startup_pixels_settings() {
    use dashboard_config::startup::{StartupProfile, StartupTheme};
    let profile = StartupProfile {
        theme: StartupTheme::Turbo,
        duration_ms: 10_000,
        ..StartupProfile::default()
    };
    let recipe = BuildSelection::default()
        .prepare(&Editor::default())
        .unwrap()
        .with_startup(profile)
        .unwrap();
    assert_eq!(recipe.startup_bytes(), &profile.encode().unwrap());
    let invalid = StartupProfile {
        duration_ms: 10_001,
        ..profile
    };
    assert!(matches!(
        BuildSelection::default()
            .prepare(&Editor::default())
            .unwrap()
            .with_startup(invalid),
        Err(BuildIssue::Startup)
    ));
}

// Executes the exported recipe against a boundary fixture, without Cargo/USB.
// The fixture records the real script's bytes/options; it does not emulate its logic.
#[cfg(windows)]
#[test]
/// Runs exported PowerShell against a temporary recording build-script boundary, checking
/// exact bytes/options, environment restoration, duplicate-name/toolchain rejection and
/// failure propagation. Requires pwsh; fixture never invokes Cargo, USB or flashing.
fn powershell_recipe_preserves_configuration_flags_and_failure_boundaries() {
    use std::fs;
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!(
        "studio-recipe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("firmware/scripts")).unwrap();
    /// Removes only the unique temporary recipe fixture directory on scope exit.
    struct Cleanup(
        /// Owned temporary repository root created by this test.
        std::path::PathBuf,
    );
    impl Drop for Cleanup {
        /// Best-effort removal of this fixture tree; cleanup errors do not mask assertions.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(dir.clone());
    let toolchain = format!("[toolchain]\nchannel = \"{}\"\n", repository_toolchain().unwrap());
    fs::write(dir.join("rust-toolchain.toml"), &toolchain).unwrap();
    fs::write(dir.join("firmware/rust-toolchain.toml"), &toolchain).unwrap();
    fs::write(dir.join("firmware/scripts/Build-Dashboard.ps1"), r#"
param([string]$Name,[string]$Config,[string]$Mode,[string]$Profile,[string]$CpuHz,[string]$SpiHz,[string]$VselMv,[string]$Optimization,[bool]$FrameTrace,[bool]$TeTrace,[switch]$SerialDisplay,[switch]$FullOutline,[switch]$FullFrame,[switch]$Expert)
$data = @{ Name=$Name; Hex=[Convert]::ToHexString([IO.File]::ReadAllBytes($Config)); Mode=$Mode; Profile=$Profile; CpuHz=$CpuHz; SpiHz=$SpiHz; VselMv=$VselMv; Optimization=$Optimization; FrameTrace=$FrameTrace; TeTrace=$TeTrace; SerialDisplay=$SerialDisplay.IsPresent; FullOutline=$FullOutline.IsPresent; FullFrame=$FullFrame.IsPresent; Expert=$Expert.IsPresent; Toolchain=$env:RUSTUP_TOOLCHAIN; StartupHex=[Convert]::ToHexString([IO.File]::ReadAllBytes($env:DASHBOARD_STARTUP_PROFILE)); AssetHex=if ($env:DASHBOARD_STARTUP_ASSET) {[Convert]::ToHexString([IO.File]::ReadAllBytes($env:DASHBOARD_STARTUP_ASSET))} else { $null } }
$data | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $PSScriptRoot '../../observed.json')
"#).unwrap();
    let mut selection = BuildSelection::default();
    selection.options.mode = Mode::Vehicle;
    selection.options.optimization = Optimization::SizeS;
    selection.options.simple_outline = false;
    selection.options.serial_display = true;
    selection.options.full_frame = true;
    selection.options.frame_trace = false;
    selection.options.te_trace = false;
    selection.options.expert = true;
    selection.select("custom").unwrap();
    selection.cpu.set("280000000");
    selection.spi.set("70000000");
    selection.voltage = Voltage::V1_30;
    let mut editor = Editor::default();
    let art = artwork();
    editor.set_artwork_if_current(editor.stamp(), art.clone()).unwrap();
    let recipe = selection.prepare(&editor).unwrap();
    let script = dir.join("recipe.ps1");
    fs::write(&script, recipe.powershell()).unwrap();
    let harness = dir.join("harness.ps1");
    fs::write(&harness,r#"
param([string]$Recipe,[string]$Repository,[string]$Name)
$ErrorActionPreference='Stop'
$env:RUSTUP_TOOLCHAIN='prior-toolchain'
$env:DASHBOARD_STARTUP_PROFILE='prior-profile'
$env:DASHBOARD_STARTUP_ASSET='prior-asset'
try { & $Recipe -Repository $Repository -Name $Name }
finally {
    @{ Toolchain=$env:RUSTUP_TOOLCHAIN; Startup=$env:DASHBOARD_STARTUP_PROFILE; Asset=$env:DASHBOARD_STARTUP_ASSET } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Repository 'restored.json')
}
"#).unwrap();
    let run = |name: &str| {
        Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(&harness)
            .arg("-Recipe")
            .arg(&script)
            .arg("-Repository")
            .arg(&dir)
            .arg("-Name")
            .arg(name)
            .output()
            .unwrap()
    };
    let result = run("test-capture");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let observed: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("observed.json")).unwrap()).unwrap();
    assert_eq!(
        fs::read(dir.join("target/studio-exports/test-capture.easa")).unwrap(),
        art.converted()
    );
    assert_eq!(
        observed["StartupHex"].as_str().unwrap(),
        recipe
            .startup_bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>()
    );
    assert_eq!(
        observed["AssetHex"].as_str().unwrap(),
        art.converted().iter().map(|b| format!("{b:02X}")).collect::<String>()
    );
    let restored: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("restored.json")).unwrap()).unwrap();
    assert_eq!(restored["Toolchain"], "prior-toolchain");
    assert_eq!(restored["Startup"], "prior-profile");
    assert_eq!(restored["Asset"], "prior-asset");
    assert_eq!(observed["Mode"], "Vehicle");
    assert_eq!(observed["Profile"], "");
    assert_eq!(observed["CpuHz"], "280000000");
    assert_eq!(observed["SpiHz"], "70000000");
    assert_eq!(observed["VselMv"], "1300");
    assert_eq!(observed["Optimization"], "s");
    assert_eq!(observed["FrameTrace"], false);
    assert_eq!(observed["TeTrace"], false);
    assert_eq!(observed["SerialDisplay"], true);
    assert_eq!(observed["FullOutline"], true);
    assert_eq!(observed["FullFrame"], true);
    assert_eq!(observed["Expert"], true);
    assert_eq!(observed["Toolchain"], repository_toolchain().unwrap());
    assert_eq!(
        fs::read(dir.join("target/studio-exports/test-capture.easp")).unwrap(),
        recipe.startup_bytes()
    );
    assert_eq!(
        fs::read(dir.join("target/studio-exports/test-capture.eav")).unwrap(),
        recipe.config_bytes()
    );
    assert!(
        !run("test-capture").status.success(),
        "duplicate name overwrote a prior configuration"
    );
    fs::write(dir.join("rust-toolchain.toml"), "channel = \"changed-toolchain\"\n").unwrap();
    assert!(!run("changed").status.success());
    assert!(!dir.join("target/studio-exports/changed.eav").exists());
    fs::write(dir.join("rust-toolchain.toml"), &toolchain).unwrap();
    let procedural = selection.prepare(&Editor::default()).unwrap();
    fs::write(&script, procedural.powershell()).unwrap();
    assert!(run("procedural").status.success());
    let procedural_observed: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("observed.json")).unwrap()).unwrap();
    assert!(procedural_observed["AssetHex"].is_null());
    assert!(!dir.join("target/studio-exports/procedural.easa").exists());
    fs::write(
        dir.join("firmware/scripts/Build-Dashboard.ps1"),
        "throw 'fixture build failed'\n",
    )
    .unwrap();
    let failed = run("failed");
    assert!(!failed.status.success());
    let restored: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("restored.json")).unwrap()).unwrap();
    assert_eq!(restored["Toolchain"], "prior-toolchain");
    assert_eq!(restored["Startup"], "prior-profile");
    assert_eq!(restored["Asset"], "prior-asset");
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Build wrapper completed"));
}

#[test]
/// A prepared recipe retains accepted startup settings even after editor undo.
fn recipe_snapshots_accepted_startup_instead_of_defaults() {
    use dashboard_config::startup::{StartupProfile, StartupTheme};
    let mut editor = Editor::default();
    let profile = StartupProfile {
        theme: StartupTheme::Turbo,
        duration_ms: 1234,
        ..StartupProfile::default()
    };
    editor.set_startup_profile(profile).unwrap();
    let recipe = BuildSelection::default().prepare(&editor).unwrap();
    assert_eq!(recipe.startup_bytes(), &profile.encode().unwrap());
    editor.undo().unwrap();
    assert_eq!(recipe.startup_bytes(), &profile.encode().unwrap());
}

use std::sync::Arc;

use dashboard_config_tools::project::*;
/// Returns a valid red landscape RLE asset with synthetic PNG-signature original identity;
/// source is not a decodable PNG. Fixture CRC/recipe admission failures panic.
fn artwork() -> Arc<Artwork> {
    use dashboard_config::startup_asset::{MAGIC, asset_crc32, crc32};
    let mut asset = vec![0; 52];
    asset[..8].copy_from_slice(MAGIC);
    asset[8..10].copy_from_slice(&dashboard_config::startup_asset::VERSION.to_le_bytes());
    asset[10..12].copy_from_slice(&32u16.to_le_bytes());
    asset[16..18].copy_from_slice(&320u16.to_le_bytes());
    asset[18..20].copy_from_slice(&240u16.to_le_bytes());
    asset[20..22].copy_from_slice(&1u16.to_le_bytes());
    asset[24..28].copy_from_slice(&100u32.to_le_bytes());
    asset[32..36].copy_from_slice(&100u32.to_le_bytes());
    asset[36..40].copy_from_slice(&52u32.to_le_bytes());
    let mut payload = Vec::new();
    for _ in 0..240 {
        payload.extend_from_slice(&0x813Fu16.to_le_bytes());
        payload.extend_from_slice(&[0xF8, 0]);
    }
    asset[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    asset[44..48].copy_from_slice(&crc32(&payload).to_le_bytes());
    asset[48] = 1;
    asset.extend(payload);
    let len = asset.len() as u32;
    asset[12..16].copy_from_slice(&len.to_le_bytes());
    let crc = asset_crc32(&asset).unwrap();
    asset[28..32].copy_from_slice(&crc.to_le_bytes());
    let source: Arc<[u8]> = Arc::from(&b"\x89PNG\r\n\x1a\nsynthetic-original"[..]);
    let recipe = Recipe {
        geometry: dashboard_config::Geometry::Landscape,
        version: 2,
        converter: CONVERTER.into(),
        label: "Example".into(),
        source_kind: SourceKind::Png,
        source_sha256: digest(&source),
        source_length: source.len() as u32,
        original_included: true,
        asset_sha256: digest(&asset),
        fit: Fit::Contain,
        matte_rgb: [0; 3],
        sample_fps: 20,
        trim_start_ms: 0,
        trim_end_ms: 100,
        sheet: None,
        gif_background: GifBackground::Logical,
    };
    Arc::new(Artwork::new(asset.into(), recipe, Some(source)).unwrap())
}

#[test]
/// Build recipes freeze converted pixels and all digests while excluding original bytes, even after editor undo.
fn recipe_freezes_converted_asset_and_never_embeds_original() {
    let mut editor = Editor::default();
    let art = artwork();
    editor.set_artwork_if_current(editor.stamp(), art.clone()).unwrap();
    let recipe = BuildSelection::default().prepare(&editor).unwrap();
    assert_eq!(recipe.artwork_bytes(), Some(art.converted()));
    assert_eq!(recipe.artwork_sha256(), Some(digest(art.converted())));
    assert_eq!(recipe.startup_sha256(), digest(recipe.startup_bytes()));
    assert_eq!(recipe.config_sha256(), digest(recipe.config_bytes()));
    let original_hex = art
        .original()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert!(!recipe.powershell().contains(&original_hex));
    editor.undo().unwrap();
    assert_eq!(recipe.artwork_bytes(), Some(art.converted()));
    assert!(
        BuildSelection::default()
            .prepare(&editor)
            .unwrap()
            .artwork_bytes()
            .is_none()
    );
}
