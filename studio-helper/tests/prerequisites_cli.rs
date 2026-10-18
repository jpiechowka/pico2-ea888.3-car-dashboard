use std::process::Command;
#[test]
fn help_and_usage_are_distinct() {
    let exe = env!("CARGO_BIN_EXE_studio-helper");
    let help = Command::new(exe).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("check"));
    assert_eq!(
        Command::new(exe).args(["check", "--install"]).status().unwrap().code(),
        Some(2)
    );
}
#[test]
fn invalid_repository_is_not_ready() {
    let missing = std::env::temp_dir().join(format!("studio-missing-repository-{}", std::process::id()));
    assert!(!missing.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_studio-helper"))
        .args(["check", "--json", "--repo"])
        .arg(missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ready"], false);
    assert_eq!(report["schema_version"], 1);
}
#[test]
fn relative_repository_is_usage_error_on_every_host() {
    let output = Command::new(env!("CARGO_BIN_EXE_studio-helper"))
        .args(["check", "--json", "--repo", "relative-repository"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
#[cfg(not(windows))]
#[test]
fn unsupported_host_does_not_claim_native_readiness() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_studio-helper"))
        .args(["check", "--json", "--repo"])
        .arg(repo)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ready"], false);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "host" && row["status"] == "unsupported")
    );
}
