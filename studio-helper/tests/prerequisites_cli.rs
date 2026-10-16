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
    let output = Command::new(env!("CARGO_BIN_EXE_studio-helper"))
        .args(["check", "--json", "--repo", "Z:/not-a-studio-checkout"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ready"], false);
    assert_eq!(report["schema_version"], 1);
}
