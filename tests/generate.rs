use std::fs;
use std::path::PathBuf;

fn fixture_config() -> &'static str {
    r#"
[package]
name = "my-tool"
binary = "my-tool"
description = "A great CLI tool"
repository = "https://github.com/user/my-tool"
license = "MIT"

[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "x86_64-pc-windows-msvc",
]

[distribute.github]
release = true

[distribute.pypi]
package_name = "my-tool"

[distribute.npm]
scope = "@my-org"

[distribute.homebrew]
tap = "user/homebrew-my-tool"

[distribute.cargo]
crate_name = "my-tool"

[distribute.install_script]
enabled = true
"#
}

fn setup_and_generate(config_toml: &str) -> (PathBuf, Vec<bincast::generate::GeneratedFile>) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("releaser-test-{}-{}", std::process::id(), ts));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bincast.toml"), config_toml).unwrap();

    let config = bincast::config::parse(config_toml).unwrap();
    let files = bincast::generate::run(&config, &dir).unwrap();
    (dir, files)
}

#[test]
fn test_generate_all_channels_produces_expected_files() {
    let (dir, files) = setup_and_generate(fixture_config());

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&".github/workflows/release.yml"));
    assert!(paths.contains(&"install.sh"));
    assert!(paths.contains(&"install.ps1"));
    assert!(paths.contains(&"homebrew/my-tool.rb"));

    // All files exist on disk
    for file in &files {
        assert!(
            dir.join(&file.path).exists(),
            "file not found: {}",
            file.path
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_ci_is_valid_yaml_structure() {
    let (dir, files) = setup_and_generate(fixture_config());

    let ci = files.iter().find(|f| f.path.ends_with("release.yml")).unwrap();

    // Must contain expected job names
    assert!(ci.content.contains("jobs:"), "missing jobs: key");
    assert!(ci.content.contains("build:"), "missing build job");
    assert!(ci.content.contains("release:"), "missing release job");
    assert!(ci.content.contains("publish-pypi:"), "missing pypi job");
    assert!(ci.content.contains("dispatch-homebrew:"), "missing homebrew dispatch");

    // Must contain all targets in matrix
    assert!(ci.content.contains("aarch64-apple-darwin"));
    assert!(ci.content.contains("x86_64-unknown-linux-gnu"));
    assert!(ci.content.contains("x86_64-pc-windows-msvc"));

    // Actions must be SHA-pinned (contain @sha, not @v4)
    for line in ci.content.lines() {
        if line.trim_start().starts_with("- uses:") || line.trim_start().starts_with("uses:") {
            let uses_part = line.split("uses:").nth(1).unwrap().trim();
            // Should contain an @ with a long hex SHA or be a well-known action
            if uses_part.contains('@') && !uses_part.contains("peter-evans") && !uses_part.contains("pypa") {
                let after_at = uses_part.split('@').nth(1).unwrap().split_whitespace().next().unwrap();
                assert!(
                    after_at.len() >= 40 || after_at.starts_with("release"),
                    "action not SHA-pinned: {uses_part}"
                );
            }
        }
    }

    // Must use OIDC permissions
    assert!(ci.content.contains("id-token: write"));
    assert!(ci.content.contains("attestations: write"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_install_sh_has_correct_values() {
    let (dir, files) = setup_and_generate(fixture_config());

    let sh = files.iter().find(|f| f.path == "install.sh").unwrap();

    assert!(sh.content.starts_with("#!/bin/sh"));
    assert!(sh.content.contains("REPO=\"user/my-tool\""));
    assert!(sh.content.contains("BINARY=\"my-tool\""));
    assert!(sh.content.contains("shasum -a 256"));
    assert!(sh.content.contains("detect_platform"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_install_ps1_has_correct_values() {
    let (dir, files) = setup_and_generate(fixture_config());

    let ps1 = files.iter().find(|f| f.path == "install.ps1").unwrap();

    assert!(ps1.content.contains("$Repo = \"user/my-tool\""));
    assert!(ps1.content.contains("$Binary = \"my-tool\""));
    assert!(ps1.content.contains("Get-FileHash"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_homebrew_formula() {
    let (dir, files) = setup_and_generate(fixture_config());

    let formula = files.iter().find(|f| f.path.ends_with(".rb")).unwrap();

    assert!(formula.content.contains("class MyTool < Formula"));
    assert!(formula.content.contains("desc \"A great CLI tool\""));
    assert!(formula.content.contains("license \"MIT\""));
    assert!(formula.content.contains("aarch64-apple-darwin"));
    assert!(formula.content.contains("x86_64-apple-darwin"));
    assert!(formula.content.contains("bin.install \"my-tool\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_github_only_config() {
    let config = r#"
[package]
name = "simple"
binary = "simple"
repository = "https://github.com/user/simple"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#;
    let (dir, files) = setup_and_generate(config);

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&".github/workflows/release.yml"));
    // Should NOT generate optional files
    assert!(!paths.contains(&"install.sh"));
    assert!(!paths.contains(&"install.ps1"));
    assert!(!paths.iter().any(|p| p.ends_with(".rb")));
    assert!(!paths.iter().any(|p| p.ends_with(".json")));

    let ci = &files[0].content;
    assert!(!ci.contains("publish-pypi:"));
    assert!(!ci.contains("dispatch-homebrew:"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_ci_passes_workflow_validation() {
    let (dir, files) = setup_and_generate(fixture_config());

    let ci = files.iter().find(|f| f.path.ends_with("release.yml")).unwrap();
    let issues = bincast::generate::validate::validate_workflow(&ci.content);

    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == bincast::generate::validate::Severity::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "generated CI has validation errors:\n{}",
        errors.iter().map(|e| format!("  - {}", e.message)).collect::<Vec<_>>().join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_generated_ci_has_correct_manylinux_per_target() {
    let (dir, files) = setup_and_generate(fixture_config());
    let ci = files.iter().find(|f| f.path.ends_with("release.yml")).unwrap();

    // aarch64-unknown-linux-gnu needs manylinux: 2_28 for cross-compilation container
    // Parse matrix entries — each target should have the right manylinux value
    let lines: Vec<&str> = ci.content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("- target: aarch64-unknown-linux-gnu") {
            let next = lines[i + 1..].iter().take(3).collect::<Vec<_>>();
            let manylinux_line = next.iter().find(|l| l.contains("manylinux:"));
            assert!(manylinux_line.is_some(), "aarch64-unknown-linux-gnu missing manylinux in matrix");
            assert!(
                manylinux_line.unwrap().contains("2_28"),
                "aarch64-unknown-linux-gnu should use manylinux 2_28, got: {}",
                manylinux_line.unwrap()
            );
        }
        if line.contains("- target: x86_64-unknown-linux-musl") {
            let next = lines[i + 1..].iter().take(3).collect::<Vec<_>>();
            let manylinux_line = next.iter().find(|l| l.contains("manylinux:"));
            assert!(manylinux_line.is_some(), "x86_64-unknown-linux-musl missing manylinux in matrix");
            assert!(
                manylinux_line.unwrap().contains("musllinux_1_2"),
                "x86_64-unknown-linux-musl should use musllinux_1_2, got: {}",
                manylinux_line.unwrap()
            );
        }
        if line.contains("- target: x86_64-unknown-linux-gnu") {
            let next = lines[i + 1..].iter().take(3).collect::<Vec<_>>();
            let manylinux_line = next.iter().find(|l| l.contains("manylinux:"));
            assert!(manylinux_line.is_some(), "x86_64-unknown-linux-gnu missing manylinux in matrix");
            assert!(
                manylinux_line.unwrap().contains("auto"),
                "x86_64-unknown-linux-gnu should use manylinux auto, got: {}",
                manylinux_line.unwrap()
            );
        }
    }

    // The maturin step should use matrix.manylinux, not hardcoded 'auto'
    assert!(
        ci.content.contains("manylinux: ${{ matrix.manylinux }}"),
        "maturin step should use matrix.manylinux"
    );

    // Linker env vars must NOT be in $GITHUB_ENV — they leak into maturin's Docker container
    // and reference a binary that doesn't exist inside the container
    assert!(
        !ci.content.contains(">> $GITHUB_ENV"),
        "linker env vars must not be written to $GITHUB_ENV (leaks into maturin Docker container)"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_github_only_ci_has_no_manylinux_in_matrix() {
    let config = r#"
[package]
name = "simple"
binary = "simple"
repository = "https://github.com/user/simple"

[targets]
platforms = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"]

[distribute.github]
release = true
"#;
    let (dir, files) = setup_and_generate(config);
    let ci = files.iter().find(|f| f.path.ends_with("release.yml")).unwrap();

    // Without PyPI, no manylinux should appear in matrix
    assert!(
        !ci.content.contains("manylinux:"),
        "github-only config should not have manylinux in matrix"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_github_only_ci_passes_workflow_validation() {
    let config = r#"
[package]
name = "simple"
binary = "simple"
repository = "https://github.com/user/simple"

[targets]
platforms = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"]

[distribute.github]
release = true
"#;
    let (dir, files) = setup_and_generate(config);
    let ci = files.iter().find(|f| f.path.ends_with("release.yml")).unwrap();
    let issues = bincast::generate::validate::validate_workflow(&ci.content);

    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == bincast::generate::validate::Severity::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "github-only CI has validation errors:\n{}",
        errors.iter().map(|e| format!("  - {}", e.message)).collect::<Vec<_>>().join("\n")
    );

    let _ = fs::remove_dir_all(&dir);
}
