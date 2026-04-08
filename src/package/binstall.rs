//! cargo-binstall metadata generation.
//! Generates the [package.metadata.binstall] snippet and injects it into Cargo.toml.

use std::path::Path;

/// Generate the cargo-binstall metadata TOML snippet.
pub fn binstall_metadata(owner: &str, repo: &str, binary: &str) -> String {
    format!(
        r#"[package.metadata.binstall]
pkg-url = "https://github.com/{owner}/{repo}/releases/download/v{{{{ version }}}}/{binary}-{{{{ target }}}}{{{{ archive-suffix }}}}"
bin-dir = "{binary}{{{{ binary-ext }}}}"
pkg-fmt = "tgz"

[package.metadata.binstall.overrides.x86_64-pc-windows-msvc]
pkg-fmt = "zip"

[package.metadata.binstall.overrides.aarch64-pc-windows-msvc]
pkg-fmt = "zip"
"#
    )
}

/// Inject cargo-binstall metadata into Cargo.toml.
/// Returns Ok(true) if injected, Ok(false) if already present.
pub fn inject_into_cargo_toml(cargo_path: &Path, owner: &str, repo: &str, binary: &str) -> Result<bool, String> {
    let content = std::fs::read_to_string(cargo_path)
        .map_err(|e| format!("failed to read Cargo.toml: {e}"))?;

    if content.contains("[package.metadata.binstall]") {
        return Ok(false);
    }

    let metadata = binstall_metadata(owner, repo, binary);
    let new_content = format!("{}\n{}", content.trim_end(), metadata);

    std::fs::write(cargo_path, new_content)
        .map_err(|e| format!("failed to write Cargo.toml: {e}"))?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binstall_metadata_format() {
        let output = binstall_metadata("user", "my-tool", "my-tool");
        assert!(output.contains("[package.metadata.binstall]"));
        assert!(output.contains("user/my-tool/releases"));
        assert!(output.contains("pkg-fmt = \"tgz\""));
        assert!(output.contains("[package.metadata.binstall.overrides.x86_64-pc-windows-msvc]"));
        assert!(output.contains("pkg-fmt = \"zip\""));
    }

    #[test]
    fn test_binstall_metadata_contains_template_vars() {
        let output = binstall_metadata("owner", "repo", "binary");
        assert!(output.contains("{{ version }}"));
        assert!(output.contains("{{ target }}"));
        assert!(output.contains("{{ archive-suffix }}"));
        assert!(output.contains("{{ binary-ext }}"));
    }

    #[test]
    fn test_inject_into_cargo_toml() {
        let dir = std::env::temp_dir().join(format!("binstall-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cargo_path = dir.join("Cargo.toml");
        std::fs::write(&cargo_path, "[package]\nname = \"test\"\nversion = \"0.1.0\"\n").unwrap();

        let result = inject_into_cargo_toml(&cargo_path, "user", "test", "test");
        assert!(result.unwrap(), "should inject on first call");

        let content = std::fs::read_to_string(&cargo_path).unwrap();
        assert!(content.contains("[package.metadata.binstall]"));
        assert!(content.contains("user/test/releases"));

        // Second call should be idempotent
        let result = inject_into_cargo_toml(&cargo_path, "user", "test", "test");
        assert!(!result.unwrap(), "should skip when already present");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
