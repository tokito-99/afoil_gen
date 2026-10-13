use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("afoil-calibration-{}-{nonce}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_calibration_roundtrip_and_overwrite_guard() {
    let scratch = Scratch::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = scratch.0.join("calibrated.json");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_mesh-cli"))
            .arg("calibrate")
            .arg(root.join("examples/naca0012-les.json"))
            .arg("--wall-shear")
            .arg(root.join("examples/pilot-wall-shear.csv"))
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(data["les"]["solverWallShearStress"], 0.4);
    let bytes = fs::read(&output).unwrap();
    assert!(!run().status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    let audit = Command::new(env!("CARGO_BIN_EXE_mesh-cli"))
        .arg("audit")
        .arg(&output)
        .output()
        .unwrap();
    assert!(audit.status.success());
    let audit: serde_json::Value = serde_json::from_slice(&audit.stdout).unwrap();
    assert_eq!(audit["shearSource"], "SOLVER_FEEDBACK");
}

#[test]
fn invalid_and_infeasible_feedback_do_not_create_an_output() {
    let scratch = Scratch::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let csv = scratch.0.join("invalid.csv");
    fs::write(&csv, "wallShearStressPa\n8\nNaN").unwrap();
    let output = scratch.0.join("must-not-exist.json");
    let result = Command::new(env!("CARGO_BIN_EXE_mesh-cli"))
        .arg("calibrate")
        .arg(root.join("examples/naca0012-les.json"))
        .arg("--wall-shear")
        .arg(csv)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    fs::write(scratch.0.join("infeasible.csv"), "wallShearStressPa\n8").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_mesh-cli"))
        .arg("calibrate")
        .arg(root.join("examples/naca0012-les.json"))
        .arg("--wall-shear")
        .arg(scratch.0.join("infeasible.csv"))
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!output.exists());
}
