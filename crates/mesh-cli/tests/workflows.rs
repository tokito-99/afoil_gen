#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
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
            std::env::temp_dir().join(format!("afoil-workflows-{}-{nonce}", std::process::id()));
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
fn isolated_gmsh_verifier_preserves_roundtrip_counts_and_refuses_reuse() {
    let scratch = Scratch::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = scratch.0.join("mock-gmsh");
    fs::write(
        &executable,
        "#!/bin/sh\ncp input.msh checked.msh\nprintf 'Done reading\\nDone writing\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    let output = scratch.0.join("verification");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_mesh-cli"))
            .arg("verify")
            .arg(root.join("examples/naca2412-rans.json"))
            .arg("--solver")
            .arg("gmsh")
            .arg("--output-dir")
            .arg(&output)
            .arg("--executable")
            .arg(&executable)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("verification.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "PASSED");
    assert!(
        report["evidence"][0]
            .as_str()
            .unwrap()
            .contains("115496 nodes")
    );

    let second = run();
    assert!(!second.status.success());
    let unchanged: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("verification.json")).unwrap()).unwrap();
    assert_eq!(unchanged, report);
}
