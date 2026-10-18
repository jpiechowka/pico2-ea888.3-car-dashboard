use std::path::Path;
use std::process::Command;
#[test]
fn cli_checks_real_sources_and_rejects_unknown_or_missing_options() {
    let exe = env!("CARGO_BIN_EXE_dashboard-provenance");
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/vendor");
    for kind in ["hal", "radio"] {
        let output = Command::new(exe)
            .arg(kind)
            .arg("--vendor-root")
            .arg(&vendor)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("verified")
                || String::from_utf8_lossy(&output.stdout).contains("Verified")
        );
    }
    for args in [
        vec![],
        vec!["other"],
        vec!["hal", "--vendor-root"],
        vec!["radio", "--archive", "x"],
    ] {
        let output = Command::new(exe).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }
}
