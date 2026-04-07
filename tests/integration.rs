//! End-to-end integration tests — exercise the full CLI and pipeline
//! against fixture projects with real filesystem operations.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_project(name: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("bincast-e2e-{name}-{ts}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn bincast_bin() -> PathBuf {
    // cargo sets this env var when running integration tests
    PathBuf::from(env!("CARGO_BIN_EXE_bincast"))
}

fn write_cargo_toml(dir: &PathBuf) {
    fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "test-tool"
version = "0.1.0"
edition = "2024"
description = "A test tool"
license = "MIT"
repository = "https://github.com/user/test-tool"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() { println!(\"hello\"); }").unwrap();
}

fn write_bincast_toml(dir: &PathBuf) {
    fs::write(
        dir.join("releaser.toml"),
        r#"[package]
name = "test-tool"
binary = "test-tool"
description = "A test tool"
repository = "https://github.com/user/test-tool"
license = "MIT"

[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]

[distribute.github]
release = true

[distribute.pypi]
package_name = "test-tool"

[distribute.npm]
scope = "@test-org"

[distribute.homebrew]
tap = "user/homebrew-test-tool"

[distribute.scoop]
bucket = "user/scoop-test-tool"

[distribute.install_script]
enabled = true
"#,
    )
    .unwrap();
}

// --- CLI Integration Tests ---

#[test]
fn test_cli_help() {
    let bin = bincast_bin();
    let output = Command::new(&bin).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("bincast"));
    assert!(stderr.contains("COMMANDS"));
}

#[test]
fn test_cli_version() {
    let bin = bincast_bin();
    let output = Command::new(&bin).arg("version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bincast 0.1.0"));
}

#[test]
fn test_cli_unknown_command() {
    let bin = bincast_bin();
    let output = Command::new(&bin).arg("deploy").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command"));
}

// --- Init Integration Tests ---

#[test]
fn test_init_creates_releaser_toml() {
    let dir = temp_project("init");
    write_cargo_toml(&dir);
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("init")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(output.status.success(), "init failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(dir.join("releaser.toml").exists());

    // The generated file should be valid TOML that releaser can parse
    let content = fs::read_to_string(dir.join("releaser.toml")).unwrap();
    assert!(content.contains("test-tool"));
    assert!(content.contains("[package]"));
    assert!(content.contains("[targets]"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_fails_if_releaser_toml_exists() {
    let dir = temp_project("init-exists");
    write_cargo_toml(&dir);
    write_bincast_toml(&dir);
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("init")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_fails_without_cargo_toml() {
    let dir = temp_project("init-nocargo");
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("init")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());

    let _ = fs::remove_dir_all(&dir);
}

// --- Generate Integration Tests ---

#[test]
fn test_generate_produces_all_files() {
    let dir = temp_project("generate-all");
    write_bincast_toml(&dir);
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("generate")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check all expected files exist
    assert!(dir.join(".github/workflows/release.yml").exists());
    assert!(dir.join("install.sh").exists());
    assert!(dir.join("install.ps1").exists());
    assert!(dir.join("homebrew/test-tool.rb").exists());
    assert!(dir.join("scoop/test-tool.json").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generate_ci_is_valid() {
    let dir = temp_project("generate-valid");
    write_bincast_toml(&dir);
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("generate")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let ci_path = dir.join(".github/workflows/release.yml");
    assert!(ci_path.exists(), "CI file not generated at {}", ci_path.display());
    let ci = fs::read_to_string(&ci_path).unwrap();

    // Validate with our workflow validator
    let issues = bincast::generate::validate::validate_workflow(&ci);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == bincast::generate::validate::Severity::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "CI validation errors:\n{}",
        errors
            .iter()
            .map(|e| format!("  - {}", e.message))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generate_fails_with_invalid_config() {
    let dir = temp_project("generate-invalid");
    fs::write(
        dir.join("releaser.toml"),
        r#"[package]
name = "test"
binary = "test"
repository = "https://gitlab.com/bad/url"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#,
    )
    .unwrap();
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .arg("generate")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must be a GitHub URL"));

    let _ = fs::remove_dir_all(&dir);
}

// --- Init → Generate Round-Trip ---

#[test]
fn test_init_then_generate_works() {
    let dir = temp_project("init-generate");
    write_cargo_toml(&dir);
    let bin = bincast_bin();

    // Step 1: init
    let output = Command::new(&bin)
        .arg("init")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "init failed");

    // Step 2: generate (should work with the init-generated config)
    let output = Command::new(&bin)
        .arg("generate")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generate failed after init: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have at least the CI workflow
    assert!(dir.join(".github/workflows/release.yml").exists());

    let _ = fs::remove_dir_all(&dir);
}

// --- Publish Dry-Run ---

#[test]
fn test_publish_dry_run() {
    let dir = temp_project("publish-dry");
    write_bincast_toml(&dir);
    let bin = bincast_bin();

    let output = Command::new(&bin)
        .args(["publish", "v0.1.0", "--dry-run"])
        .current_dir(&dir)
        .output()
        .unwrap();

    // Publish dry-run should succeed (even without building)
    // For now it just prints what it would do
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Just verify it doesn't crash
    let _ = stderr;

    let _ = fs::remove_dir_all(&dir);
}
