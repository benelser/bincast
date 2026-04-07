use super::types::ReleaserConfig;

/// Validate a parsed ReleaserConfig. Returns a list of validation errors.
pub fn validate(config: &ReleaserConfig) -> Vec<String> {
    let mut errors = Vec::new();

    // Package validation
    if config.package.name.is_empty() {
        errors.push("package.name must not be empty".into());
    }

    if config.package.binary.is_empty() {
        errors.push("package.binary must not be empty".into());
    }

    if config.package.repository.is_empty() {
        errors.push("package.repository must not be empty".into());
    } else if !config.package.repository.starts_with("https://github.com/") {
        errors.push(format!(
            "package.repository must be a GitHub URL (got '{}')",
            config.package.repository
        ));
    } else {
        // Validate owner/repo structure
        let path = config.package.repository.trim_start_matches("https://github.com/");
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            errors.push(format!(
                "package.repository must be https://github.com/owner/repo (got '{}')",
                config.package.repository
            ));
        }
    }

    // Targets validation
    if config.targets.platforms.is_empty() {
        errors.push("targets.platforms must contain at least one target".into());
    }

    // Check for duplicate targets
    let mut seen = std::collections::HashSet::new();
    for target in &config.targets.platforms {
        if !seen.insert(target.as_str()) {
            errors.push(format!("duplicate target: {target}"));
        }
    }

    // Distribution channel validation
    let has_any_channel = config.distribute.github.is_some()
        || config.distribute.pypi.is_some()
        || config.distribute.npm.is_some()
        || config.distribute.homebrew.is_some()
        || config.distribute.scoop.is_some()
        || config.distribute.cargo.is_some()
        || config.distribute.install_script.is_some();

    if !has_any_channel {
        errors.push("at least one distribution channel must be enabled".into());
    }

    // npm scope must start with @
    if let Some(npm) = &config.distribute.npm
        && !npm.scope.starts_with('@')
    {
        errors.push(format!(
            "distribute.npm.scope must start with '@' (got '{}')",
            npm.scope
        ));
    }

    // Homebrew tap must be owner/repo format
    if let Some(homebrew) = &config.distribute.homebrew {
        let parts: Vec<&str> = homebrew.tap.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            errors.push(format!(
                "distribute.homebrew.tap must be owner/repo format (got '{}')",
                homebrew.tap
            ));
        }
    }

    // Scoop bucket must be owner/repo format
    if let Some(scoop) = &config.distribute.scoop {
        let parts: Vec<&str> = scoop.bucket.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            errors.push(format!(
                "distribute.scoop.bucket must be owner/repo format (got '{}')",
                scoop.bucket
            ));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    fn valid_config() -> &'static str {
        r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"
license = "MIT"

[targets]
platforms = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#
    }

    #[test]
    fn test_valid_config_passes() {
        let config = parse(valid_config()).unwrap();
        let errors = validate(&config);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn test_invalid_npm_scope() {
        let input = r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.npm]
scope = "missing-at"
"#;
        let config = parse(input).unwrap();
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("must start with '@'")));
    }

    #[test]
    fn test_invalid_repo_url() {
        let input = r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://gitlab.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#;
        let config = parse(input).unwrap();
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("must be a GitHub URL")));
    }

    #[test]
    fn test_invalid_homebrew_tap() {
        let input = r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.homebrew]
tap = "bad-format"
"#;
        let config = parse(input).unwrap();
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("owner/repo format")));
    }

    #[test]
    fn test_no_channels_error() {
        let input = r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]
"#;
        let config = parse(input).unwrap();
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("at least one distribution channel")));
    }

    #[test]
    fn test_duplicate_targets() {
        let input = r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#;
        let config = parse(input).unwrap();
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("duplicate target")));
    }
}
