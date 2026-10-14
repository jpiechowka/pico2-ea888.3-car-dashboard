use std::process::Command;

fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dashboard-config"))
        .args(args)
        .output()
        .unwrap()
}
fn json(args: &[&str]) -> serde_json::Value {
    let output = invoke(args);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn profiles_export_requested_and_actual_legacy_identity() {
    let data = json(&["profiles"]);
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["profiles"].as_array().unwrap().len(), 8);
    let legacy = &data["profiles"][6];
    assert_eq!(legacy["profile"]["aliases"][0], "290-72.5");
    assert_eq!(legacy["profile"]["observation"], "failed");
    assert_eq!(legacy["clock"]["requested_cpu_hz"], 290000000);
    assert_eq!(legacy["clock"]["actual_cpu_hz"], 290400000);
    assert_eq!(legacy["clock"]["actual_spi_hz"], 48400000);
    assert!(data.to_string().len() < 65536);
}

#[test]
fn build_options_defaults_and_false_features_are_exact() {
    let data = json(&["build-options"]);
    assert_eq!(data["requested"]["clock"]["named"], "stock");
    assert_eq!(
        data["features"],
        serde_json::json!(["cpu150-spi62-1v10", "demo", "frame-trace", "simple-outline", "te-trace"])
    );
    let data = json(&[
        "build-options",
        "--mode",
        "vehicle",
        "--profile",
        "150-15",
        "--simple-outline",
        "false",
        "--frame-trace",
        "false",
        "--te-trace",
        "false",
        "--serial-display",
        "true",
        "--full-frame",
        "true",
        "--optimization",
        "s",
    ]);
    assert_eq!(
        data["features"],
        serde_json::json!(["cpu150-spi15-1v10", "full-frame", "longan-rx12", "serial-display"])
    );
    assert_eq!(data["requested"]["optimization"], "size_s");
}

#[test]
fn exact_custom_clock_is_not_legacy_rounding() {
    let data = json(&[
        "build-options",
        "--cpu-hz",
        "290000000",
        "--spi-hz",
        "72500000",
        "--vsel-mv",
        "1300",
        "--expert",
        "true",
    ]);
    assert_eq!(
        data["clock"]["pll"],
        serde_json::json!({"refdiv":2,"fbdiv":145,"postdiv1":3,"postdiv2":1})
    );
    assert_eq!(data["clock"]["actual_cpu_hz"], 290000000);
    assert_eq!(data["clock"]["actual_spi_hz"], 72500000);
    assert_eq!(data["clock"]["legacy_rounding"], false);
    assert_eq!(
        data["features"],
        serde_json::json!(["demo", "frame-trace", "simple-outline", "te-trace"])
    );
}

#[test]
fn bad_cli_requests_never_emit_success_json() {
    for args in [
        vec!["build-options", "--profile", "280-70"],
        vec!["build-options", "--profile", "legacy290-72.5", "--expert", "true"],
        vec![
            "build-options",
            "--profile",
            "stock",
            "--cpu-hz",
            "50000000",
            "--spi-hz",
            "25000000",
            "--vsel-mv",
            "1100",
            "--expert",
            "true",
        ],
        vec!["build-options", "--cpu-hz", "50000000", "--expert", "true"],
        vec!["build-options", "--mode", "demo", "--mode", "vehicle"],
        vec!["build-options", "--optimization", "1"],
        vec!["build-options", "--features", "target-bench"],
        vec!["build-options", "--env", "RUSTFLAGS=anything"],
    ] {
        let output = invoke(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    for bad in [
        "",
        "+50000000",
        "-50000000",
        "50.0",
        "5e7",
        "0x2faf080",
        "50_000_000",
        " 50000000",
        "50000000\n",
        "５０",
        "4294967296",
        "290000001",
    ] {
        let output = invoke(&[
            "build-options",
            "--cpu-hz",
            bad,
            "--spi-hz",
            "25000000",
            "--vsel-mv",
            "1100",
            "--expert",
            "true",
        ]);
        assert!(!output.status.success(), "{bad:?}");
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn capture_adapter_keeps_benchmark_separate_and_requires_expert() {
    let unavailable = json(&["capture-options", "--features", "frame-trace"]);
    assert_eq!(unavailable["legacy_unavailable_backend"], true);
    assert_eq!(
        unavailable["features"],
        serde_json::json!(["frame-trace", "simple-outline"])
    );
    let data = json(&[
        "capture-options",
        "--features",
        "target-bench,longan-rx12,frame-trace,te-trace",
        "--no-default-features",
        "false",
    ]);
    assert_eq!(data["benchmark"], true);
    assert_eq!(data["legacy_stock"], true);
    assert_eq!(data["normalized"]["requested"]["mode"], "vehicle");
    assert!(
        data["features"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("target-bench"))
    );
    assert!(
        !data["normalized"]["features"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("target-bench"))
    );
    assert!(
        !invoke(&["capture-options", "--features", "cpu280-spi70-1v30,demo"])
            .status
            .success()
    );
    let data = json(&[
        "capture-options",
        "--features",
        "cpu280-spi70-1v30,demo",
        "--expert",
        "true",
        "--optimization",
        "2",
    ]);
    assert_eq!(data["optimization"], "2");
    assert_eq!(data["normalized"]["requested"]["expert"], true);
}

#[test]
fn legacy_unavailable_capture_never_enables_vehicle_transport() {
    for features in ["", "target-bench"] {
        for no_default in ["false", "true"] {
            let result = json(&[
                "capture-options",
                "--features",
                features,
                "--no-default-features",
                no_default,
            ]);
            assert_eq!(result["legacy_unavailable_backend"], true);
            assert_eq!(result["normalized"]["requested"]["mode"], "vehicle");
            assert!(
                !result["features"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!("longan-rx12"))
            );
            assert!(
                !result["features"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!("demo"))
            );
        }
    }
    for features in ["demo", "longan-rx12"] {
        assert_eq!(
            json(&["capture-options", "--features", features])["legacy_unavailable_backend"],
            false
        );
    }
    let n = json(&["build-options", "--mode", "vehicle"]);
    let result = json(&["capture-options", "--normalized-json", &n.to_string()]);
    assert_eq!(result["legacy_unavailable_backend"], false);
    assert!(
        result["features"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("longan-rx12"))
    );
}

#[test]
fn every_boolean_combination_and_optimization_survives_normalization() {
    for mask in 0..32 {
        let enabled: Vec<_> = (0..5)
            .map(|i| if mask & (1 << i) == 0 { "false" } else { "true" })
            .collect();
        let n = json(&[
            "build-options",
            "--simple-outline",
            enabled[0],
            "--serial-display",
            enabled[1],
            "--full-frame",
            enabled[2],
            "--frame-trace",
            enabled[3],
            "--te-trace",
            enabled[4],
        ]);
        for (i, feature) in [
            "simple-outline",
            "serial-display",
            "full-frame",
            "frame-trace",
            "te-trace",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                n["features"].as_array().unwrap().contains(&serde_json::json!(feature)),
                mask & (1 << i) != 0
            );
        }
    }
    for (argument, wire) in [("3", "speed3"), ("2", "speed2"), ("s", "size_s"), ("z", "size_z")] {
        assert_eq!(
            json(&["build-options", "--optimization", argument])["requested"]["optimization"],
            wire
        );
    }
}

#[test]
fn capture_rejects_tampered_and_oversized_normalized_evidence() {
    let mut n = json(&["build-options"]);
    n["clock"]["actual_cpu_hz"] = serde_json::json!(140000000);
    assert!(
        !invoke(&["capture-options", "--normalized-json", &n.to_string()])
            .status
            .success()
    );
    let padded = format!("{}{}", json(&["build-options"]), " ".repeat(4096));
    assert!(
        !invoke(&["capture-options", "--normalized-json", &padded])
            .status
            .success()
    );
}
