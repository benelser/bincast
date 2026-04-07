//! `bincast release` — tag the current Cargo.toml version and push.
//!
//! This is the "cut release" command. The version should already be
//! bumped (via `bincast version patch/minor/major`).
//!
//! Pre-flight checks:
//! - Must be on main/master
//! - Clean working tree
//! - Tag doesn't already exist
//! - CI workflow exists

use std::process::Command;

use crate::cargo;
use crate::error::{Error, Result};

/// Run the release command.
pub fn run(dry_run: bool) -> Result<()> {
    // Read version from Cargo.toml
    let meta = cargo::read(std::path::Path::new("Cargo.toml"))?;
    let version = format!("v{}", meta.version);

    eprintln!("  bincast release {version}");
    eprintln!();

    // Pre-flight checks
    preflight_checks(&version)?;

    if dry_run {
        eprintln!("  dry-run: would tag {version} and push");
        eprintln!("  dry-run complete — no tags created");
        return Ok(());
    }

    // Create tag
    eprintln!("  tagging {version}...");
    let output = Command::new("git")
        .args(["tag", &version, "-m", &format!("Release {version}")])
        .output()
        .map_err(|e| Error::Config(format!("git tag failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Config(format!("git tag failed: {stderr}")));
    }

    // Push commit + tag
    eprintln!("  pushing...");
    let output = Command::new("git")
        .args(["push"])
        .output()
        .map_err(|e| Error::Config(format!("git push failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Config(format!("git push failed: {stderr}")));
    }

    let output = Command::new("git")
        .args(["push", "origin", &version])
        .output()
        .map_err(|e| Error::Config(format!("git push tag failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Config(format!("git push tag failed: {stderr}")));
    }

    eprintln!("  ✓ tagged and pushed {version}");
    eprintln!();

    // Print CI link
    let gh_available = Command::new("gh").arg("--version").output().is_ok();

    if gh_available {
        // Try to get the repo URL for the Actions link
        if let Ok(url_output) = Command::new("gh")
            .args(["repo", "view", "--json", "url", "--jq", ".url"])
            .output()
        {
            let url = String::from_utf8_lossy(&url_output.stdout);
            let url = url.trim();
            if !url.is_empty() {
                eprintln!("  CI: {url}/actions");
                eprintln!("  Release: {url}/releases/tag/{version}");
            }
        }
    } else {
        if let Ok(output) = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
        {
            let url = String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace("git@github.com:", "https://github.com/")
                .replace(".git", "");
            eprintln!("  CI: {url}/actions");
        }
    }

    Ok(())
}

fn preflight_checks(version: &str) -> Result<()> {
    // Must be in a git repo
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|_| Error::Config("not a git repository".into()))?;
    if !output.status.success() {
        return Err(Error::Config("not a git repository".into()));
    }

    // Must be on main or master
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map_err(|e| Error::Config(format!("git branch failed: {e}")))?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch != "main" && branch != "master" {
        return Err(Error::Config(format!(
            "you're on branch '{branch}' — releases must be from 'main' or 'master'\n\n  \
             Either:\n    git checkout main && bincast release\n  \
             Or bump version for a PR:\n    bincast version patch"
        )));
    }

    // Clean working tree
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| Error::Config(format!("git status failed: {e}")))?;
    let status = String::from_utf8_lossy(&output.stdout);
    if !status.trim().is_empty() {
        return Err(Error::Config(
            "working tree is dirty — commit or stash changes before releasing".into(),
        ));
    }

    // Tag doesn't exist
    let output = Command::new("git")
        .args(["tag", "-l", version])
        .output()
        .map_err(|e| Error::Config(format!("git tag list failed: {e}")))?;
    let existing = String::from_utf8_lossy(&output.stdout);
    if !existing.trim().is_empty() {
        return Err(Error::Config(format!(
            "tag {version} already exists — version was not bumped since last release\n\n  \
             Bump it first:\n    bincast version patch\n    bincast version minor"
        )));
    }

    // CI workflow exists
    if !std::path::Path::new(".github/workflows/release.yml").exists() {
        return Err(Error::Config(
            "no .github/workflows/release.yml — run 'bincast generate' first".into(),
        ));
    }

    eprintln!("  ✓ pre-flight checks passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_from_cargo_toml() {
        // Just verify we can read the version
        let meta = cargo::read(std::path::Path::new("Cargo.toml")).unwrap();
        assert!(!meta.version.is_empty());
    }
}
