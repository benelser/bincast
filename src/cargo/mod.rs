use crate::error::{Error, Result};
use crate::toml_parser;
use std::path::Path;

/// Metadata extracted from a Cargo.toml file.
#[derive(Debug, Clone)]
pub struct CargoMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub readme: Option<String>,
    /// Explicit binary name from [[bin]] or package name
    pub binary: Option<String>,
}

/// Read and parse a Cargo.toml file.
pub fn read(path: &Path) -> Result<CargoMetadata> {
    let content = std::fs::read_to_string(path)?;
    parse(&content)
}

/// Parse a Cargo.toml string into CargoMetadata.
pub fn parse(input: &str) -> Result<CargoMetadata> {
    let val = toml_parser::parse(input)?;

    let pkg = val
        .get("package")
        .ok_or_else(|| Error::Config("Cargo.toml missing [package] section".into()))?;

    let name = pkg
        .get_str("name")
        .ok_or_else(|| Error::Config("Cargo.toml missing package.name".into()))?
        .to_string();

    let version = pkg
        .get_str("version")
        .ok_or_else(|| Error::Config("Cargo.toml missing package.version".into()))?
        .to_string();

    // Check for explicit [[bin]] section
    let binary = val
        .get("bin")
        .and_then(|v| v.as_array())
        .and_then(|bins| bins.first())
        .and_then(|b| b.get_str("name"))
        .map(|s| s.to_string());

    Ok(CargoMetadata {
        name,
        version,
        description: pkg.get_str("description").map(|s| s.to_string()),
        repository: pkg.get_str("repository").map(|s| s.to_string()),
        license: pkg.get_str("license").map(|s| s.to_string()),
        homepage: pkg.get_str("homepage").map(|s| s.to_string()),
        readme: pkg.get_str("readme").map(|s| s.to_string()),
        binary,
    })
}

/// Extract owner and repo from a GitHub repository URL.
/// e.g., "https://github.com/benelser/durable" -> ("benelser", "durable")
pub fn parse_github_url(url: &str) -> Option<(&str, &str)> {
    let path = url.strip_prefix("https://github.com/")?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let slash = path.find('/')?;
    let owner = &path[..slash];
    let repo = &path[slash + 1..];
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_cargo_toml() {
        let input = r#"
[package]
name = "my-tool"
version = "0.1.0"
edition = "2024"
"#;
        let meta = parse(input).unwrap();
        assert_eq!(meta.name, "my-tool");
        assert_eq!(meta.version, "0.1.0");
        assert!(meta.description.is_none());
        assert!(meta.repository.is_none());
        assert!(meta.binary.is_none());
    }

    #[test]
    fn test_parse_full_cargo_toml() {
        let input = r#"
[package]
name = "durable"
version = "0.2.0"
edition = "2024"
description = "The SQLite of durable agent execution"
license = "MIT"
repository = "https://github.com/benelser/durable"
homepage = "https://durable.dev"
readme = "README.md"
"#;
        let meta = parse(input).unwrap();
        assert_eq!(meta.name, "durable");
        assert_eq!(meta.version, "0.2.0");
        assert_eq!(meta.description.as_deref(), Some("The SQLite of durable agent execution"));
        assert_eq!(meta.license.as_deref(), Some("MIT"));
        assert_eq!(meta.repository.as_deref(), Some("https://github.com/benelser/durable"));
        assert_eq!(meta.homepage.as_deref(), Some("https://durable.dev"));
        assert_eq!(meta.readme.as_deref(), Some("README.md"));
    }

    #[test]
    fn test_parse_missing_package_fails() {
        let input = r#"
[dependencies]
serde = "1"
"#;
        let result = parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_github_url() {
        assert_eq!(
            parse_github_url("https://github.com/benelser/durable"),
            Some(("benelser", "durable"))
        );
        assert_eq!(
            parse_github_url("https://github.com/benelser/durable/"),
            Some(("benelser", "durable"))
        );
        assert_eq!(
            parse_github_url("https://github.com/benelser/durable.git"),
            Some(("benelser", "durable"))
        );
        assert_eq!(
            parse_github_url("https://gitlab.com/user/repo"),
            None
        );
        assert_eq!(
            parse_github_url("https://github.com/"),
            None
        );
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/extra"),
            None
        );
    }

    #[test]
    fn test_parse_cargo_toml_with_dependencies() {
        let input = r#"
[package]
name = "releaser"
version = "0.1.0"
edition = "2024"
description = "Ship your Rust binary to every package manager"
license = "MIT"
repository = "https://github.com/benelser/releaser"

[dependencies]

[dev-dependencies]
"#;
        let meta = parse(input).unwrap();
        assert_eq!(meta.name, "releaser");
        assert_eq!(meta.version, "0.1.0");
    }

    #[test]
    fn test_parse_cargo_toml_with_bin_section() {
        let input = r#"
[package]
name = "my-lib"
version = "0.1.0"
edition = "2024"
repository = "https://github.com/user/my-tool"

[[bin]]
name = "my-tool"
path = "src/main.rs"
"#;
        let meta = parse(input).unwrap();
        assert_eq!(meta.name, "my-lib");
        assert_eq!(meta.binary.as_deref(), Some("my-tool"));
    }

    #[test]
    fn test_parse_cargo_toml_workspace_root() {
        let input = r#"
[workspace]
members = ["crates/cli", "crates/core"]

[workspace.package]
version = "0.1.0"
"#;
        // Workspace root without [package] should fail gracefully
        let result = parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cargo_toml_with_features() {
        let input = r#"
[package]
name = "my-tool"
version = "0.3.0"
edition = "2024"
repository = "https://github.com/user/my-tool"

[features]
default = ["color"]
color = []
json = []

[dependencies]
"#;
        let meta = parse(input).unwrap();
        assert_eq!(meta.name, "my-tool");
        assert_eq!(meta.version, "0.3.0");
    }
}
