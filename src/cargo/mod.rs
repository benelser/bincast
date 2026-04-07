use crate::error::{Error, Result};
use crate::toml_parser;
use std::path::{Path, PathBuf};

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
    /// If this is a workspace member, the package name to use with -p
    pub package_flag: Option<String>,
    /// Whether this came from a workspace
    pub is_workspace: bool,
}

/// Information about a workspace member.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    pub path: PathBuf,
    pub name: String,
    pub has_binary: bool,
    pub binary_name: Option<String>,
}

/// Result of reading a project directory — either a single crate or a workspace.
#[derive(Debug)]
pub enum ProjectKind {
    SingleCrate(CargoMetadata),
    Workspace {
        root_meta: WorkspaceRoot,
        members: Vec<WorkspaceMember>,
    },
}

/// Metadata from the workspace root Cargo.toml.
#[derive(Debug, Clone)]
pub struct WorkspaceRoot {
    pub version: Option<String>,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub readme: Option<String>,
    pub member_globs: Vec<String>,
}

/// Smart project reader — detects workspace vs single crate, resolves the binary.
pub fn read_project(dir: &Path) -> Result<ProjectKind> {
    let cargo_path = dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Err(Error::Config("no Cargo.toml found".into()));
    }

    let content = std::fs::read_to_string(&cargo_path)?;
    let val = toml_parser::parse(&content)?;

    // Check if this is a workspace
    if val.get("workspace").is_some() {
        let root_meta = parse_workspace_root(&val);

        // Check if root also has [package] (non-virtual workspace)
        let has_package = val.get("package").is_some();

        let members = discover_members(dir, &root_meta.member_globs)?;

        if members.is_empty() && has_package {
            // Non-virtual workspace with no discoverable members — treat as single crate
            let meta = parse_with_inheritance(&content, &root_meta)?;
            return Ok(ProjectKind::SingleCrate(meta));
        }

        return Ok(ProjectKind::Workspace {
            root_meta,
            members,
        });
    }

    // Single crate
    let meta = parse(&content)?;
    Ok(ProjectKind::SingleCrate(meta))
}

/// Get all binary crate members from a workspace.
pub fn workspace_binaries(members: &[WorkspaceMember]) -> Vec<&WorkspaceMember> {
    members.iter().filter(|m| m.has_binary).collect()
}

/// Resolve the best CargoMetadata for a workspace, picking the binary crate
/// and inheriting workspace-level metadata.
pub fn resolve_workspace_binary(
    dir: &Path,
    root: &WorkspaceRoot,
    members: &[WorkspaceMember],
) -> Result<CargoMetadata> {
    let binaries = workspace_binaries(members);

    let member = match binaries.len() {
        0 => return Err(Error::Config(
            "no binary crates found in workspace — bincast needs a [[bin]] target or src/main.rs".into(),
        )),
        1 => binaries[0],
        _ => {
            eprintln!(
                "  ! Found {} binary crates: {}",
                binaries.len(),
                binaries.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(", ")
            );
            eprintln!("  Using: {}", binaries[0].name);
            binaries[0]
        }
    };

    // Read the member's Cargo.toml with workspace inheritance
    let member_cargo = dir.join(&member.path).join("Cargo.toml");
    let content = std::fs::read_to_string(&member_cargo)?;
    let mut meta = parse_with_inheritance(&content, root)?;

    // Set workspace-specific fields
    meta.is_workspace = true;
    meta.package_flag = Some(meta.name.clone());

    if meta.binary.is_none() {
        meta.binary = member.binary_name.clone();
    }

    Ok(meta)
}

/// Read and parse a Cargo.toml file (single crate, no workspace inheritance).
pub fn read(path: &Path) -> Result<CargoMetadata> {
    let content = std::fs::read_to_string(path)?;
    parse(&content)
}

/// Parse a Cargo.toml string into CargoMetadata (no workspace inheritance).
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
        package_flag: None,
        is_workspace: false,
    })
}

/// Parse a member Cargo.toml with workspace.package inheritance.
fn parse_with_inheritance(input: &str, root: &WorkspaceRoot) -> Result<CargoMetadata> {
    let val = toml_parser::parse(input)?;

    let pkg = val
        .get("package")
        .ok_or_else(|| Error::Config("Cargo.toml missing [package] section".into()))?;

    let name = pkg
        .get_str("name")
        .ok_or_else(|| Error::Config("Cargo.toml missing package.name".into()))?
        .to_string();

    // Version: use member's own or inherit from workspace
    let version = pkg
        .get_str("version")
        .map(|s| s.to_string())
        .or_else(|| {
            // Check for version.workspace = true pattern
            // Our TOML parser stores "workspace" as a string if it's `version = { workspace = true }`
            // but for `version.workspace = true` dotted key it would be under version.workspace
            root.version.clone()
        })
        .unwrap_or_else(|| "0.0.0".to_string());

    let binary = val
        .get("bin")
        .and_then(|v| v.as_array())
        .and_then(|bins| bins.first())
        .and_then(|b| b.get_str("name"))
        .map(|s| s.to_string());

    // Each field: use member's value, fall back to workspace
    let description = pkg
        .get_str("description")
        .map(|s| s.to_string())
        .or_else(|| root.description.clone());

    let repository = pkg
        .get_str("repository")
        .map(|s| s.to_string())
        .or_else(|| root.repository.clone());

    let license = pkg
        .get_str("license")
        .map(|s| s.to_string())
        .or_else(|| root.license.clone());

    let homepage = pkg
        .get_str("homepage")
        .map(|s| s.to_string())
        .or_else(|| root.homepage.clone());

    let readme = pkg
        .get_str("readme")
        .map(|s| s.to_string())
        .or_else(|| root.readme.clone());

    Ok(CargoMetadata {
        name,
        version,
        description,
        repository,
        license,
        homepage,
        readme,
        binary,
        package_flag: None,
        is_workspace: false,
    })
}

/// Parse workspace root metadata from [workspace] and [workspace.package].
fn parse_workspace_root(val: &toml_parser::Value) -> WorkspaceRoot {
    let ws = val.get("workspace");

    let member_globs = ws
        .and_then(|w| w.get_string_array("members"))
        .map(|v| v.into_iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let wp = val.get_path("workspace.package");

    WorkspaceRoot {
        version: wp.and_then(|p| p.get_str("version")).map(|s| s.to_string()),
        description: wp.and_then(|p| p.get_str("description")).map(|s| s.to_string()),
        repository: wp.and_then(|p| p.get_str("repository")).map(|s| s.to_string()),
        license: wp.and_then(|p| p.get_str("license")).map(|s| s.to_string()),
        homepage: wp.and_then(|p| p.get_str("homepage")).map(|s| s.to_string()),
        readme: wp.and_then(|p| p.get_str("readme")).map(|s| s.to_string()),
        member_globs,
    }
}

/// Discover workspace members from glob patterns.
/// Supports simple patterns like "crates/*" and literal paths like "cli".
fn discover_members(workspace_dir: &Path, globs: &[String]) -> Result<Vec<WorkspaceMember>> {
    let mut members = Vec::new();

    for glob in globs {
        if glob.contains('*') {
            // Simple glob: "crates/*" → list directories matching
            let (prefix, _) = glob.split_once('*').unwrap_or((glob, ""));
            let search_dir = workspace_dir.join(prefix);
            if search_dir.is_dir() {
                let entries = std::fs::read_dir(&search_dir)
                    .map_err(|e| Error::Config(format!("failed to read {}: {e}", search_dir.display())))?;
                for entry in entries {
                    let entry = entry.map_err(|e| Error::Config(e.to_string()))?;
                    let path = entry.path();
                    if path.is_dir() && path.join("Cargo.toml").exists()
                        && let Some(member) = read_member(workspace_dir, &path)?
                    {
                        members.push(member);
                    }
                }
            }
        } else {
            // Literal path
            let member_dir = workspace_dir.join(glob);
            if member_dir.join("Cargo.toml").exists()
                && let Some(member) = read_member(workspace_dir, &member_dir)?
            {
                members.push(member);
            }
        }
    }

    Ok(members)
}

/// Read a single workspace member directory.
fn read_member(workspace_dir: &Path, member_dir: &Path) -> Result<Option<WorkspaceMember>> {
    let cargo_path = member_dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_path)?;
    let val = toml_parser::parse(&content)?;

    let pkg = match val.get("package") {
        Some(p) => p,
        None => return Ok(None), // Skip non-package members
    };

    let name = match pkg.get_str("name") {
        Some(n) => n.to_string(),
        None => return Ok(None),
    };

    // Check for binary: [[bin]] section OR src/main.rs
    let has_bin_section = val
        .get("bin")
        .and_then(|v| v.as_array())
        .is_some_and(|bins| !bins.is_empty());

    let has_main_rs = member_dir.join("src/main.rs").exists();
    let has_binary = has_bin_section || has_main_rs;

    let binary_name = if has_bin_section {
        val.get("bin")
            .and_then(|v| v.as_array())
            .and_then(|bins| bins.first())
            .and_then(|b| b.get_str("name"))
            .map(|s| s.to_string())
    } else if has_main_rs {
        Some(name.clone())
    } else {
        None
    };

    let relative_path = member_dir
        .strip_prefix(workspace_dir)
        .unwrap_or(member_dir)
        .to_path_buf();

    Ok(Some(WorkspaceMember {
        path: relative_path,
        name,
        has_binary,
        binary_name,
    }))
}

/// Extract owner and repo from a GitHub repository URL.
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
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bincast-cargo-{name}-{ts}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_parse_minimal_cargo_toml() {
        let meta = parse(r#"
[package]
name = "my-tool"
version = "0.1.0"
edition = "2024"
"#).unwrap();
        assert_eq!(meta.name, "my-tool");
        assert_eq!(meta.version, "0.1.0");
        assert!(!meta.is_workspace);
        assert!(meta.package_flag.is_none());
    }

    #[test]
    fn test_parse_full_cargo_toml() {
        let meta = parse(r#"
[package]
name = "durable"
version = "0.2.0"
edition = "2024"
description = "The SQLite of durable agent execution"
license = "MIT"
repository = "https://github.com/benelser/durable"
homepage = "https://durable.dev"
readme = "README.md"
"#).unwrap();
        assert_eq!(meta.name, "durable");
        assert_eq!(meta.version, "0.2.0");
        assert_eq!(meta.description.as_deref(), Some("The SQLite of durable agent execution"));
    }

    #[test]
    fn test_parse_missing_package_fails() {
        assert!(parse("[dependencies]\nserde = \"1\"").is_err());
    }

    #[test]
    fn test_parse_cargo_toml_with_bin_section() {
        let meta = parse(r#"
[package]
name = "my-lib"
version = "0.1.0"
edition = "2024"
repository = "https://github.com/user/my-tool"

[[bin]]
name = "my-tool"
path = "src/main.rs"
"#).unwrap();
        assert_eq!(meta.name, "my-lib");
        assert_eq!(meta.binary.as_deref(), Some("my-tool"));
    }

    #[test]
    fn test_parse_workspace_root_fails_without_package() {
        assert!(parse("[workspace]\nmembers = [\"crates/*\"]").is_err());
    }

    #[test]
    fn test_parse_github_url() {
        assert_eq!(parse_github_url("https://github.com/benelser/durable"), Some(("benelser", "durable")));
        assert_eq!(parse_github_url("https://github.com/benelser/durable/"), Some(("benelser", "durable")));
        assert_eq!(parse_github_url("https://github.com/benelser/durable.git"), Some(("benelser", "durable")));
        assert_eq!(parse_github_url("https://gitlab.com/user/repo"), None);
        assert_eq!(parse_github_url("https://github.com/"), None);
    }

    #[test]
    fn test_workspace_detection() {
        let dir = temp_dir("ws-detect");

        // Create workspace root
        fs::write(dir.join("Cargo.toml"), r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.3.0"
license = "MIT"
repository = "https://github.com/user/my-project"
"#).unwrap();

        // Create binary member
        let cli_dir = dir.join("crates/cli");
        fs::create_dir_all(cli_dir.join("src")).unwrap();
        fs::write(cli_dir.join("Cargo.toml"), r#"
[package]
name = "my-tool"
version.workspace = true
edition = "2024"
"#).unwrap();
        fs::write(cli_dir.join("src/main.rs"), "fn main() {}").unwrap();

        // Create library member
        let core_dir = dir.join("crates/core");
        fs::create_dir_all(core_dir.join("src")).unwrap();
        fs::write(core_dir.join("Cargo.toml"), r#"
[package]
name = "my-core"
version.workspace = true
edition = "2024"
"#).unwrap();
        fs::write(core_dir.join("src/lib.rs"), "").unwrap();

        let project = read_project(&dir).unwrap();
        match project {
            ProjectKind::Workspace { root_meta, members } => {
                assert_eq!(root_meta.version.as_deref(), Some("0.3.0"));
                assert_eq!(root_meta.repository.as_deref(), Some("https://github.com/user/my-project"));
                assert_eq!(root_meta.license.as_deref(), Some("MIT"));
                assert_eq!(members.len(), 2);

                let bin_members: Vec<_> = members.iter().filter(|m| m.has_binary).collect();
                assert_eq!(bin_members.len(), 1);
                assert_eq!(bin_members[0].name, "my-tool");
                assert_eq!(bin_members[0].binary_name.as_deref(), Some("my-tool"));
            }
            _ => panic!("expected workspace"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_workspace_resolve_binary() {
        let dir = temp_dir("ws-resolve");

        fs::write(dir.join("Cargo.toml"), r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.5.0"
license = "MIT"
repository = "https://github.com/user/my-project"
description = "A great project"
"#).unwrap();

        let cli_dir = dir.join("crates/cli");
        fs::create_dir_all(cli_dir.join("src")).unwrap();
        fs::write(cli_dir.join("Cargo.toml"), r#"
[package]
name = "my-cli"
version.workspace = true
edition = "2024"
"#).unwrap();
        fs::write(cli_dir.join("src/main.rs"), "fn main() {}").unwrap();

        let project = read_project(&dir).unwrap();
        match project {
            ProjectKind::Workspace { root_meta, members } => {
                let meta = resolve_workspace_binary(&dir, &root_meta, &members).unwrap();
                assert_eq!(meta.name, "my-cli");
                assert_eq!(meta.version, "0.5.0"); // inherited
                assert_eq!(meta.license.as_deref(), Some("MIT")); // inherited
                assert_eq!(meta.repository.as_deref(), Some("https://github.com/user/my-project")); // inherited
                assert_eq!(meta.description.as_deref(), Some("A great project")); // inherited
                assert!(meta.is_workspace);
                assert_eq!(meta.package_flag.as_deref(), Some("my-cli"));
            }
            _ => panic!("expected workspace"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_workspace_no_binary_errors() {
        let dir = temp_dir("ws-nobin");

        fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = [\"lib\"]").unwrap();
        let lib_dir = dir.join("lib");
        fs::create_dir_all(lib_dir.join("src")).unwrap();
        fs::write(lib_dir.join("Cargo.toml"), "[package]\nname = \"mylib\"\nversion = \"0.1.0\"").unwrap();
        fs::write(lib_dir.join("src/lib.rs"), "").unwrap();

        let project = read_project(&dir).unwrap();
        match project {
            ProjectKind::Workspace { root_meta, members } => {
                let result = resolve_workspace_binary(&dir, &root_meta, &members);
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("no binary crates"));
            }
            _ => panic!("expected workspace"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_single_crate_detected() {
        let dir = temp_dir("single");
        fs::write(dir.join("Cargo.toml"), r#"
[package]
name = "simple"
version = "1.0.0"
repository = "https://github.com/user/simple"
"#).unwrap();

        let project = read_project(&dir).unwrap();
        match project {
            ProjectKind::SingleCrate(meta) => {
                assert_eq!(meta.name, "simple");
                assert!(!meta.is_workspace);
            }
            _ => panic!("expected single crate"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_workspace_with_explicit_bin() {
        let dir = temp_dir("ws-bin");

        fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = [\"app\"]").unwrap();
        let app_dir = dir.join("app");
        fs::create_dir_all(app_dir.join("src")).unwrap();
        fs::write(app_dir.join("Cargo.toml"), r#"
[package]
name = "my-app"
version = "0.1.0"

[[bin]]
name = "my-custom-binary"
path = "src/main.rs"
"#).unwrap();
        fs::write(app_dir.join("src/main.rs"), "fn main() {}").unwrap();

        let project = read_project(&dir).unwrap();
        match project {
            ProjectKind::Workspace { root_meta, members } => {
                assert_eq!(members.len(), 1);
                assert_eq!(members[0].binary_name.as_deref(), Some("my-custom-binary"));

                let meta = resolve_workspace_binary(&dir, &root_meta, &members).unwrap();
                assert_eq!(meta.binary.as_deref(), Some("my-custom-binary"));
            }
            _ => panic!("expected workspace"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_cargo_toml_with_features() {
        let meta = parse(r#"
[package]
name = "my-tool"
version = "0.3.0"
edition = "2024"
repository = "https://github.com/user/my-tool"

[features]
default = ["color"]
color = []
"#).unwrap();
        assert_eq!(meta.name, "my-tool");
        assert_eq!(meta.version, "0.3.0");
    }

    #[test]
    fn test_parse_cargo_toml_with_dependencies() {
        let meta = parse(r#"
[package]
name = "bincast"
version = "0.1.0"
edition = "2024"
description = "Ship your Rust binary to every package manager"
license = "MIT"
repository = "https://github.com/benelser/bincast"

[dependencies]

[dev-dependencies]
"#).unwrap();
        assert_eq!(meta.name, "bincast");
    }
}
