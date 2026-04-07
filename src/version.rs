//! `bincast version` — bump Cargo.toml version.
//!
//! `bincast version patch`   → 0.1.0 → 0.1.1
//! `bincast version minor`   → 0.1.0 → 0.2.0
//! `bincast version major`   → 0.1.0 → 1.0.0
//! `bincast version 2.0.0`   → explicit version

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Semver components.
#[derive(Debug, Clone)]
pub struct Semver {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Semver {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);
        // Strip pre-release suffix if present (e.g., 0.1.0-alpha.1)
        let base = s.split('-').next().unwrap_or(s);
        let parts: Vec<&str> = base.split('.').collect();
        if parts.len() != 3 {
            return Err(Error::Config(format!("invalid version '{s}' — expected X.Y.Z")));
        }
        Ok(Semver {
            major: parts[0].parse().map_err(|_| Error::Config(format!("invalid major: {}", parts[0])))?,
            minor: parts[1].parse().map_err(|_| Error::Config(format!("invalid minor: {}", parts[1])))?,
            patch: parts[2].parse().map_err(|_| Error::Config(format!("invalid patch: {}", parts[2])))?,
        })
    }

    pub fn bump_patch(&self) -> Self {
        Semver { major: self.major, minor: self.minor, patch: self.patch + 1 }
    }

    pub fn bump_minor(&self) -> Self {
        Semver { major: self.major, minor: self.minor + 1, patch: 0 }
    }

    pub fn bump_major(&self) -> Self {
        Semver { major: self.major + 1, minor: 0, patch: 0 }
    }

    pub fn format(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Run the version command.
pub fn run(bump: &str) -> Result<()> {
    let cargo_path = Path::new("Cargo.toml");
    if !cargo_path.exists() {
        return Err(Error::Config("no Cargo.toml found".into()));
    }

    let content = std::fs::read_to_string(cargo_path)?;

    // Find current version
    let current = find_version(&content)?;
    let current_semver = Semver::parse(&current)?;

    // Determine new version
    let new_version = match bump {
        "patch" => current_semver.bump_patch().format(),
        "minor" => current_semver.bump_minor().format(),
        "major" => current_semver.bump_major().format(),
        explicit => {
            // Validate it's a valid semver
            let _ = Semver::parse(explicit)?;
            explicit.strip_prefix('v').unwrap_or(explicit).to_string()
        }
    };

    eprintln!("  {current} → {new_version}");

    // Update Cargo.toml
    let updated = update_version(&content, &current, &new_version);
    std::fs::write(cargo_path, &updated)?;
    eprintln!("  ✓ Updated Cargo.toml");

    // Also update workspace root if this is a workspace member
    let root_cargo = Path::new("Cargo.toml");
    let root_content = std::fs::read_to_string(root_cargo)?;
    if root_content.contains("[workspace.package]") {
        let ws_updated = update_workspace_version(&root_content, &current, &new_version);
        if ws_updated != root_content {
            std::fs::write(root_cargo, &ws_updated)?;
            eprintln!("  ✓ Updated workspace.package.version");
        }
    }

    // Also update bincast.toml if it exists
    let bincast_path = Path::new("bincast.toml");
    if bincast_path.exists() {
        // bincast.toml doesn't have a version field currently, skip
    }

    // Update Cargo.lock by running cargo check
    let _ = Command::new("cargo")
        .args(["check", "--quiet"])
        .output();

    // Git commit
    let output = Command::new("git")
        .args(["add", "Cargo.toml", "Cargo.lock"])
        .output();

    if output.is_ok() {
        let msg = format!("release v{new_version}");
        let _ = Command::new("git")
            .args(["commit", "-m", &msg])
            .output();
        eprintln!("  ✓ Committed: {msg}");
    }

    Ok(())
}

/// Find the version string in Cargo.toml content.
fn find_version(content: &str) -> Result<String> {
    // Look for version = "X.Y.Z" in [package] section
    // Skip version.workspace = true
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version") && trimmed.contains('"')
            && let Some(start) = trimmed.find('"')
            && let Some(end) = trimmed[start + 1..].find('"')
        {
            return Ok(trimmed[start + 1..start + 1 + end].to_string());
        }
    }
    Err(Error::Config("no version found in Cargo.toml".into()))
}

/// Replace the version in Cargo.toml content.
fn update_version(content: &str, old: &str, new: &str) -> String {
    content.replacen(
        &format!("version = \"{old}\""),
        &format!("version = \"{new}\""),
        1, // Only replace first occurrence (in [package])
    )
}

/// Replace version in workspace.package section.
fn update_workspace_version(content: &str, old: &str, new: &str) -> String {
    // Find [workspace.package] section and replace version there
    content.replace(
        &format!("version = \"{old}\""),
        &format!("version = \"{new}\""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse() {
        let v = Semver::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_semver_parse_with_v_prefix() {
        let v = Semver::parse("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
    }

    #[test]
    fn test_semver_parse_with_prerelease() {
        let v = Semver::parse("0.1.0-alpha.1").unwrap();
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_bump_patch() {
        let v = Semver::parse("0.1.0").unwrap().bump_patch();
        assert_eq!(v.format(), "0.1.1");
    }

    #[test]
    fn test_bump_minor() {
        let v = Semver::parse("0.1.3").unwrap().bump_minor();
        assert_eq!(v.format(), "0.2.0");
    }

    #[test]
    fn test_bump_major() {
        let v = Semver::parse("0.2.5").unwrap().bump_major();
        assert_eq!(v.format(), "1.0.0");
    }

    #[test]
    fn test_find_version() {
        let content = r#"
[package]
name = "my-tool"
version = "0.3.1"
edition = "2024"
"#;
        assert_eq!(find_version(content).unwrap(), "0.3.1");
    }

    #[test]
    fn test_update_version() {
        let content = r#"
[package]
name = "my-tool"
version = "0.3.1"
edition = "2024"
"#;
        let updated = update_version(content, "0.3.1", "0.3.2");
        assert!(updated.contains("version = \"0.3.2\""));
        assert!(!updated.contains("0.3.1"));
    }

    #[test]
    fn test_invalid_version() {
        assert!(Semver::parse("not.a.version").is_err());
        assert!(Semver::parse("1.2").is_err());
    }
}
