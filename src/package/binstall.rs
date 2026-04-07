//! cargo-binstall metadata generation.
//! Generates the [package.metadata.binstall] snippet for Cargo.toml.

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
}
