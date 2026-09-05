//! A checker that cannot run must not report success.
use bun_runner::BunRunner;
use camino::Utf8PathBuf;
use std::fs;
use std::process::Command;

#[test]
fn missing_typescript_checker_fails_unless_explicitly_skipped() {
    // Outside the repository so an ancestor's installed checker cannot hide
    // the missing dependency. Sources are copied from the static fixture.
    let temp = tempfile::tempdir().unwrap();
    let workspace = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let fixture = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/projects/missing-checker");
    for name in [
        "package.json",
        "tsconfig.json",
        "Component.svelte",
        "error.ts",
    ] {
        fs::copy(fixture.join(name), workspace.join(name)).unwrap();
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bun = runtime
        .block_on(BunRunner::ensure_bun(Some(&workspace)))
        .unwrap();
    let install = Command::new(bun)
        .arg("install")
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    for format in ["human", "json", "machine"] {
        let output = Command::new(env!("CARGO_BIN_EXE_svelte-check-rs"))
            .args(["--workspace", workspace.as_str(), "--output", format])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "missing checker succeeded ({format})"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("not found"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("found 0 errors"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_svelte-check-rs"))
        .args([
            "--workspace",
            workspace.as_str(),
            "--skip-tsgo",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!([])
    );
}
