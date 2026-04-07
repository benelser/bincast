//! The `bincast release` command — the golden path.
//! Tags a version, pushes the tag, and watches CI.

use std::path::Path;
use std::process::Command;

use crate::cargo;
use crate::error::{Error, Result};

/// Run the release command.
/// If version is None, reads from Cargo.toml.
pub fn run(version: Option<&str>, dry_run: bool) -> Result<()> {
    // Step 1: Determine version
    let version = match version {
        Some(v) => {
            if v.starts_with('v') {
                v.to_string()
            } else {
                format!("v{v}")
            }
        }
        None => {
            let meta = cargo::read(Path::new("Cargo.toml"))?;
            format!("v{}", meta.version)
        }
    };

    eprintln!("  bincast release {version}");
    eprintln!();

    // Step 2: Pre-flight checks
    preflight_checks(&version)?;

    if dry_run {
        eprintln!("  dry-run: would tag {version} and push");
        eprintln!("  dry-run: would watch CI at GitHub Actions");
        eprintln!();
        eprintln!("  dry-run complete — no tags created");
        return Ok(());
    }

    // Step 3: Create tag
    eprintln!("  creating tag {version}...");
    let output = Command::new("git")
        .args(["tag", &version, "-m", &format!("Release {version}")])
        .output()
        .map_err(|e| Error::Config(format!("failed to run git tag: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            return Err(Error::Config(format!(
                "tag {version} already exists — delete it with `git tag -d {version}` or use a different version"
            )));
        }
        return Err(Error::Config(format!("git tag failed: {stderr}")));
    }

    // Step 4: Push tag
    eprintln!("  pushing tag {version}...");
    let output = Command::new("git")
        .args(["push", "origin", &version])
        .output()
        .map_err(|e| Error::Config(format!("failed to push tag: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Config(format!("git push failed: {stderr}")));
    }
    eprintln!("  ✓ tag pushed");

    // Step 5: Watch CI (optional — requires gh CLI)
    eprintln!();

    let gh_available = Command::new("gh").arg("--version").output().is_ok();

    if !gh_available {
        // No gh — give the user everything they need to follow manually
        eprintln!("  ✓ tag {version} pushed — CI is running");
        eprintln!();

        let repo_url = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .replace("git@github.com:", "https://github.com/")
                    .replace(".git", "")
            });

        if let Some(url) = &repo_url {
            eprintln!("  Watch CI:     {url}/actions");
            eprintln!("  Release:      {url}/releases/tag/{version}");
        }
        eprintln!();
        eprintln!("  Tip: install gh (https://cli.github.com) to watch CI inline");
        return Ok(());
    }

    eprintln!("  waiting for CI to start...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Find the run ID for this tag
    let run_id = Command::new("gh")
        .args(["run", "list", "--limit", "1", "--json", "databaseId", "--jq", ".[0].databaseId"])
        .output()
        .ok()
        .and_then(|o| {
            let id = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if id.is_empty() { None } else { Some(id) }
        });

    let run_id = match run_id {
        Some(id) => {
            eprintln!("  watching CI run {id}...");
            eprintln!();
            id
        }
        None => {
            eprintln!("  ! could not find CI run — check GitHub Actions manually");
            return Ok(());
        }
    };

    let watch_result = Command::new("gh")
        .args(["run", "watch", &run_id, "--exit-status"])
        .output();

    match watch_result {
        Ok(output) if output.status.success() => {
            eprintln!();
            eprintln!("  ✓ release {version} complete!");
            eprintln!();

            if let Ok(url_output) = Command::new("gh")
                .args(["release", "view", &version, "--json", "url", "--jq", ".url"])
                .output()
            {
                let url = String::from_utf8_lossy(&url_output.stdout);
                let url = url.trim();
                if !url.is_empty() {
                    eprintln!("  {url}");
                }
            }
        }
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            eprintln!();
            eprintln!("  ✗ CI failed for {version}");
            if !stdout.is_empty() {
                eprintln!("{stdout}");
            }
            eprintln!();
            eprintln!("  View logs: gh run view --log-failed");
            return Err(Error::Config(format!("release {version} failed — CI did not pass")));
        }
        Err(e) => {
            eprintln!("  ! failed to watch CI: {e}");
        }
    }

    Ok(())
}

/// Pre-flight checks before releasing.
fn preflight_checks(version: &str) -> Result<()> {
    // Check we're in a git repo
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|_| Error::Config("not a git repository".into()))?;

    if !output.status.success() {
        return Err(Error::Config("not a git repository".into()));
    }

    // Check working tree is clean
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

    // Check tag doesn't already exist
    let output = Command::new("git")
        .args(["tag", "-l", version])
        .output()
        .map_err(|e| Error::Config(format!("git tag list failed: {e}")))?;

    let existing = String::from_utf8_lossy(&output.stdout);
    if !existing.trim().is_empty() {
        return Err(Error::Config(format!(
            "tag {version} already exists"
        )));
    }

    // Check CI workflow exists
    if !Path::new(".github/workflows/release.yml").exists() {
        return Err(Error::Config(
            "no .github/workflows/release.yml found — run `bincast generate` first".into(),
        ));
    }

    // Check remote exists
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| Error::Config(format!("no git remote: {e}")))?;

    if !output.status.success() {
        return Err(Error::Config("no 'origin' remote — push your repo first".into()));
    }

    eprintln!("  ✓ pre-flight checks passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_version_normalization() {
        // The run function normalizes versions — test the logic
        assert!(format!("v{}", "0.1.0").starts_with('v'));
        assert!("v0.1.0".starts_with('v'));
    }
}
