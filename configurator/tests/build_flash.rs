use dashboard_build_profiles::{ClockSelection, CustomClock, ProfileId, Voltage, catalogue, resolve};
use dashboard_configurator::build_flash::*;
use dashboard_configurator::editor::{ActionBlock, Editor, PointerTarget};
use dashboard_helper_protocol::{Mode, Optimization};

#[test]
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

// Executes the exported recipe against a boundary fixture, without Cargo/USB.
// The fixture records the real script's bytes/options; it does not emulate its logic.
#[cfg(windows)]
#[test]
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
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(dir.clone());
    let toolchain = format!("[toolchain]\nchannel = \"{}\"\n", repository_toolchain().unwrap());
    fs::write(dir.join("rust-toolchain.toml"), &toolchain).unwrap();
    fs::write(dir.join("firmware/rust-toolchain.toml"), &toolchain).unwrap();
    fs::write(dir.join("firmware/scripts/Build-Dashboard.ps1"), r#"
param([string]$Name,[string]$Config,[string]$Mode,[string]$Profile,[string]$CpuHz,[string]$SpiHz,[string]$VselMv,[string]$Optimization,[bool]$FrameTrace,[bool]$TeTrace,[switch]$SerialDisplay,[switch]$FullOutline,[switch]$FullFrame,[switch]$Expert)
$data = @{ Name=$Name; Hex=[Convert]::ToHexString([IO.File]::ReadAllBytes($Config)); Mode=$Mode; Profile=$Profile; CpuHz=$CpuHz; SpiHz=$SpiHz; VselMv=$VselMv; Optimization=$Optimization; FrameTrace=$FrameTrace; TeTrace=$TeTrace; SerialDisplay=$SerialDisplay.IsPresent; FullOutline=$FullOutline.IsPresent; FullFrame=$FullFrame.IsPresent; Expert=$Expert.IsPresent; Toolchain=$env:RUSTUP_TOOLCHAIN }
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
    let recipe = selection.prepare(&Editor::default()).unwrap();
    let script = dir.join("recipe.ps1");
    fs::write(&script, recipe.powershell()).unwrap();
    let run = |name: &str| {
        Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-File"])
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
    fs::write(
        dir.join("firmware/scripts/Build-Dashboard.ps1"),
        "throw 'fixture build failed'\n",
    )
    .unwrap();
    let failed = run("failed");
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Build wrapper completed"));
}
